//! Loading analysed games (R2 Task 31, TS M2 Task 24 `loadAnalysedGames`, `loadSeriesPoints`,
//! `loadExemplarMoves`).
//!
//! Version gate (global constraints): a `game_metrics` row whose `metrics_version` is not
//! [`PLAYER_GAME_METRICS_VERSION`] is stale. [`load_analysed_games`] leaves it out (and logs the
//! count): it is no evidence, never zero evidence.

use std::collections::{BTreeMap, HashMap};

use chess_core::{Color, TimeClass};
use profile_core::timeline::series::SeriesPoint;
use profile_core::types::{
    GameContext, LineAnalysis, PLAYER_GAME_METRICS_VERSION, PlatformId, PlayerGameMetrics,
    SeriesKey,
};
use sqlx::types::Json;
use sqlx::{FromRow, PgConnection, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::analyse_features::{parse_color, parse_time_class, player_score};

#[derive(thiserror::Error, Debug)]
pub enum LoadError {
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
    #[error("undecodable row: {0}")]
    Decode(String),
}

pub type Result<T> = std::result::Result<T, LoadError>;

#[derive(FromRow)]
struct AnalysedRow {
    game_id: Uuid,
    platform: String,
    platform_game_id: String,
    played_at: OffsetDateTime,
    base_seconds: Option<i32>,
    increment_seconds: Option<i32>,
    rated: bool,
    player_color: String,
    white_rating: Option<i32>,
    black_rating: Option<i32>,
    result: String,
    termination: Option<String>,
    white_accuracy: Option<f32>,
    black_accuracy: Option<f32>,
    protocol: String,
    metrics: serde_json::Value,
}

/// Served by `game_metrics_account_tc_played_idx` (account, time class, played_at).
const ANALYSED_SQL: &str = "\
SELECT gm.game_id, g.platform::text AS platform, g.platform_game_id, gm.played_at, \
  g.base_seconds, g.increment_seconds, g.rated, g.player_color::text AS player_color, \
  g.white_rating, g.black_rating, g.result::text AS result, g.termination, g.white_accuracy, \
  g.black_accuracy, gm.protocol, gm.metrics \
FROM game_metrics gm JOIN games g ON g.id = gm.game_id \
WHERE gm.account_id = $1 AND gm.time_class = $2::time_class AND gm.metrics_version = $3 \
ORDER BY gm.played_at, gm.game_id";

/// The account × time class games with current-version metrics, oldest first, each with its
/// context from the player's point of view (score 1 / 0.5 / 0 for the player).
pub async fn load_analysed_games(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
) -> Result<Vec<(GameContext, PlayerGameMetrics)>> {
    let mut conn = pool.acquire().await?;
    load_analysed_games_in(&mut conn, account_id, tc).await
}

/// [`load_analysed_games`] on the caller's connection or transaction.
pub async fn load_analysed_games_in(
    conn: &mut PgConnection,
    account_id: Uuid,
    tc: TimeClass,
) -> Result<Vec<(GameContext, PlayerGameMetrics)>> {
    let version = i32::try_from(PLAYER_GAME_METRICS_VERSION).unwrap_or(i32::MAX);
    let rows: Vec<AnalysedRow> = sqlx::query_as(ANALYSED_SQL)
        .bind(account_id)
        .bind(tc.as_str())
        .bind(version)
        .fetch_all(&mut *conn)
        .await?;
    let stale: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM game_metrics WHERE account_id = $1 AND time_class = $2::time_class \
         AND metrics_version <> $3",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .bind(version)
    .fetch_one(&mut *conn)
    .await?;
    if stale > 0 {
        tracing::info!(%account_id, tc = tc.as_str(), stale, "stale game_metrics rows left out");
    }
    rows.into_iter()
        .map(|r| {
            let player = parse_color(&r.player_color)
                .ok_or_else(|| LoadError::Decode(format!("player_color {:?}", r.player_color)))?;
            let platform: PlatformId = r
                .platform
                .parse()
                .map_err(|_| LoadError::Decode(format!("platform {:?}", r.platform)))?;
            let metrics: PlayerGameMetrics = serde_json::from_value(r.metrics)
                .map_err(|e| LoadError::Decode(format!("metrics of {}: {e}", r.game_id)))?;
            if metrics.version != PLAYER_GAME_METRICS_VERSION {
                return Err(LoadError::Decode(format!(
                    "metrics of {} have version {}",
                    r.game_id, metrics.version
                )));
            }
            let (own, opp, accuracy) = match player {
                Color::White => (r.white_rating, r.black_rating, r.white_accuracy),
                Color::Black => (r.black_rating, r.white_rating, r.black_accuracy),
            };
            let secs = |v: Option<i32>| v.and_then(|s| u32::try_from(s).ok());
            let ctx = GameContext {
                game_id: r.game_id,
                platform_game_id: r.platform_game_id,
                platform,
                played_at: r.played_at,
                time_class: tc,
                base_seconds: secs(r.base_seconds),
                increment_seconds: secs(r.increment_seconds),
                rated: r.rated,
                player_color: player,
                player_rating: own,
                opponent_rating: opp,
                score: player_score(&r.result, player),
                result: r.result,
                termination: r.termination,
                accuracy: accuracy.map(f64::from),
                protocol: r.protocol,
            };
            Ok((ctx, metrics))
        })
        .collect()
}

#[derive(FromRow)]
struct PointRow {
    series_key: String,
    game_id: Uuid,
    played_at: OffsetDateTime,
    events: i32,
    opportunities: i32,
    protocol: String,
}

/// Every series of the account × time class, points oldest first. A stored key that does not
/// parse is skipped and logged (it would name no series).
pub async fn load_series_points(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
) -> Result<BTreeMap<SeriesKey, Vec<SeriesPoint>>> {
    let mut conn = pool.acquire().await?;
    load_series_points_in(&mut conn, account_id, tc).await
}

/// [`load_series_points`] on the caller's connection or transaction.
pub async fn load_series_points_in(
    conn: &mut PgConnection,
    account_id: Uuid,
    tc: TimeClass,
) -> Result<BTreeMap<SeriesKey, Vec<SeriesPoint>>> {
    let rows: Vec<PointRow> = sqlx::query_as(
        "SELECT series_key, game_id, played_at, events, opportunities, protocol \
         FROM weakness_series_point WHERE account_id = $1 AND time_class = $2::time_class \
         ORDER BY series_key, played_at, game_id",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_all(conn)
    .await?;
    let mut out: BTreeMap<SeriesKey, Vec<SeriesPoint>> = BTreeMap::new();
    let mut invalid = 0u32;
    for r in rows {
        let (Ok(key), Ok(events), Ok(opportunities)) = (
            r.series_key.parse::<SeriesKey>(),
            u32::try_from(r.events),
            u32::try_from(r.opportunities),
        ) else {
            invalid += 1;
            continue;
        };
        out.entry(key).or_default().push(SeriesPoint {
            game_id: r.game_id,
            played_at: r.played_at,
            events,
            opportunities,
            protocol: r.protocol,
        });
    }
    if invalid > 0 {
        tracing::warn!(%account_id, invalid, "undecodable weakness_series_point rows skipped");
    }
    Ok(out)
}

/// One exemplar move: the move, its enrichment and its game.
#[derive(Clone, Debug, PartialEq)]
pub struct ExemplarRow {
    pub game_id: Uuid,
    /// The platform's game id (contract `ExemplarPosition.gameId`, R-7).
    pub platform_game_id: String,
    pub platform: String,
    pub played_at: OffsetDateTime,
    pub time_class: Option<TimeClass>,
    pub player_color: Color,
    pub ply: u32,
    pub color: Color,
    pub san: String,
    pub uci: String,
    pub fen_before: String,
    pub best_uci: Option<String>,
    pub win_before: f64,
    pub win_after: f64,
    pub delta_w: f64,
    pub judgement: Option<String>,
    pub phase: String,
    /// Empty without enrichment.
    pub best_pv: Vec<String>,
    pub refutation_pv: Vec<String>,
    pub best: Option<LineAnalysis>,
    pub refutation: Option<LineAnalysis>,
    pub threat: Option<LineAnalysis>,
}

#[derive(FromRow)]
struct ExemplarDbRow {
    game_id: Uuid,
    platform_game_id: String,
    platform: String,
    played_at: OffsetDateTime,
    time_class: String,
    player_color: String,
    ply: i32,
    color: String,
    san: String,
    uci: String,
    fen_before: String,
    best_uci: Option<String>,
    win_before: f64,
    win_after: f64,
    delta_w: f64,
    judgement: Option<String>,
    phase: String,
    best_pv: Option<Vec<String>>,
    refutation_pv: Option<Vec<String>>,
    best_line: Option<Json<LineAnalysis>>,
    refutation_line: Option<Json<LineAnalysis>>,
    threat_line: Option<Json<LineAnalysis>>,
}

/// The moves `refs` names (`(game id, ply)`), keyed the same way; missing ones are absent.
pub async fn load_exemplar_moves(
    pool: &PgPool,
    refs: &[(Uuid, u32)],
) -> Result<HashMap<(Uuid, u32), ExemplarRow>> {
    if refs.is_empty() {
        return Ok(HashMap::new());
    }
    let games: Vec<Uuid> = refs.iter().map(|r| r.0).collect();
    let plies: Vec<i32> = refs
        .iter()
        .map(|r| i32::try_from(r.1).unwrap_or(i32::MAX))
        .collect();
    let rows: Vec<ExemplarDbRow> = sqlx::query_as(
        "SELECT DISTINCT ON (m.game_id, m.ply) m.game_id, g.platform_game_id, \
           g.platform::text AS platform, g.played_at, g.time_class::text AS time_class, \
           g.player_color::text AS player_color, m.ply, m.color::text AS color, m.san, m.uci, \
           m.fen_before, m.best_uci, m.win_before, m.win_after, m.delta_w, \
           m.judgement::text AS judgement, m.phase::text AS phase, mf.best_pv, \
           mf.refutation_pv, mf.best_line, mf.refutation_line, mf.threat_line \
         FROM UNNEST($1::uuid[], $2::int4[]) AS r(game_id, ply) \
         JOIN moves m ON m.game_id = r.game_id AND m.ply = r.ply \
         JOIN games g ON g.id = m.game_id \
         LEFT JOIN move_features mf ON mf.game_id = m.game_id AND mf.ply = m.ply \
         ORDER BY m.game_id, m.ply",
    )
    .bind(&games)
    .bind(&plies)
    .fetch_all(pool)
    .await?;
    let mut out = HashMap::with_capacity(rows.len());
    for r in rows {
        let decode = |what: &str, v: &str| LoadError::Decode(format!("{what} {v:?}"));
        let ply = u32::try_from(r.ply).map_err(|_| decode("ply", &r.ply.to_string()))?;
        let row = ExemplarRow {
            game_id: r.game_id,
            platform_game_id: r.platform_game_id,
            platform: r.platform,
            played_at: r.played_at,
            time_class: parse_time_class(&r.time_class),
            player_color: parse_color(&r.player_color)
                .ok_or_else(|| decode("player_color", &r.player_color))?,
            ply,
            color: parse_color(&r.color).ok_or_else(|| decode("color", &r.color))?,
            san: r.san,
            uci: r.uci,
            fen_before: r.fen_before,
            best_uci: r.best_uci,
            win_before: r.win_before,
            win_after: r.win_after,
            delta_w: r.delta_w,
            judgement: r.judgement,
            phase: r.phase,
            best_pv: r.best_pv.unwrap_or_default(),
            refutation_pv: r.refutation_pv.unwrap_or_default(),
            best: r.best_line.map(|j| j.0),
            refutation: r.refutation_line.map(|j| j.0),
            threat: r.threat_line.map(|j| j.0),
        };
        out.insert((row.game_id, ply), row);
    }
    Ok(out)
}
