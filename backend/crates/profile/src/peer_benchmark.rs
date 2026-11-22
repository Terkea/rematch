//! The peer benchmark (ADR 0080): the account's analysed games' **opponent sides** as the
//! reference group, on the account's own rating scale. It replaces the Lichess-dump band
//! reference in the default flow; nothing is downloaded and no engine runs.
//!
//! - **Observations.** Every analysed game already has main evals for both colours. The opponent
//!   side's [`PlayerGameMetrics`] are computed from the stored PGN, the cached main evals and no
//!   enrichment ([`compute_opponent_metrics`]), with the opponent's colour and pre-game rating
//!   ([`opponent_context`]), and cached in `opponent_game_metrics` per game
//!   ([`refresh_opponent_metrics`]). A game whose evals are not all cached is skipped (counted),
//!   never evaluated.
//! - **Comparable metrics only** ([`is_peer_comparable`]). The opponent side has no enrichment
//!   (refutation, threat and best lines, tablebase, Maia). Detectors that need it would read a
//!   missing line as a non-event (W1's non-decisive moves still count), so their rows are never
//!   produced: no benchmark, "not compared", rather than a biased one.
//! - **The band** is the player's latest pre-game rating ± [`PEER_RATING_WINDOW`] in the time
//!   class; opponents whose pre-game rating falls outside are left out.
//! - **Players** are distinct opponents (case-insensitive usernames). Each weighs once
//!   ([`Weighting::Players`]), and `low_sample` is the band reference's rule: fewer than
//!   [`LOW_SAMPLE_PLAYERS`](crate::reference_aggregate::LOW_SAMPLE_PLAYERS) opponents with evidence. Low-sample rows are stored, never served.
//! - **Bias.** These are only games against this player. The web label says so.

use std::collections::{BTreeMap, BTreeSet};

use analysis::eval_cache::DbEvalCache;
use analysis::evaluate::{EvaluateDeps, evaluate_game};
use chess_core::game_accuracy::game_accuracy;
use chess_core::move_record::build_move_records;
use chess_core::pgn::parse_pgn;
use chess_core::score::win_percent;
use chess_core::{ClockInfo, Color, TimeClass};
use db::games::get_game;
use engine::ANALYSIS_PROTOCOL;
use profile_core::bands::band_of;
use profile_core::kpi::catalogue::{KpiKind, kpi_catalogue};
use profile_core::types::{
    BandReference, BandReferenceStat, BaseMetric, GameContext, MetricId,
    PLAYER_GAME_METRICS_VERSION, PlayerGameMetrics, WeaknessKey,
};
use serde_json::Value;
use sqlx::{FromRow, PgConnection, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::analyse_features::{
    FeatureInput, compute_game_features, parse_color, parse_time_class, player_score,
};
use crate::hook_impl::load_game_explorer;
use crate::reference_aggregate::{
    AggregateReport, ReferenceSide, Weighting, aggregate_row,
};
use crate::requeue::NoEngine;

/// Half-width of the band, in rating points of the player's own platform.
pub const PEER_RATING_WINDOW: i32 = 150;

/// `account_benchmark.source` of a peer benchmark.
pub const SOURCE_PEERS: &str = "peers";
/// `account_benchmark.source` of the legacy Lichess band reference.
pub const SOURCE_LICHESS_BANDS: &str = "lichess_bands";

/// Which benchmark build-profile compares with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BenchmarkSource {
    /// The peer benchmark (the default).
    #[default]
    Peers,
    /// The legacy Lichess-dump band reference (opt-in, `BENCHMARK_SOURCE=lichess-bands`).
    LichessBands,
}

impl BenchmarkSource {
    /// From `BENCHMARK_SOURCE` (`peers`, the default, or `lichess-bands`). Anything else is the
    /// default, logged.
    pub fn from_env() -> Self {
        match std::env::var("BENCHMARK_SOURCE").as_deref() {
            Err(_) | Ok("" | "peers") => BenchmarkSource::Peers,
            Ok("lichess-bands") => BenchmarkSource::LichessBands,
            Ok(other) => {
                tracing::warn!(value = other, "unknown BENCHMARK_SOURCE; using peers");
                BenchmarkSource::Peers
            }
        }
    }
}

/// True for the metrics an unenriched side measures exactly as the enriched player's side:
/// decided from the main evals, the clocks, the phases and the result. Score vs expected is
/// left out too: against the player it is the player's own result mirrored.
pub fn is_peer_comparable(metric: MetricId) -> bool {
    use BaseMetric as B;
    use WeaknessKey as W;
    match metric {
        MetricId::Weakness(k) => matches!(
            k,
            W::W4Missed | W::W4Allowed | W::W5 | W::W6 | W::W7Traps | W::W7Prep | W::W9a | W::W9b
        ),
        MetricId::Base(m) => matches!(
            m,
            B::Accuracy
                | B::AccuracyOpening
                | B::AccuracyMiddlegame
                | B::AccuracyEndgame
                | B::BlundersPerGame
                | B::BlunderRateLowClock
                | B::ConversionRate
                | B::ResilienceRate
                | B::MissedPunishRate
                | B::EvalTrend1535
                | B::TimeTroubleRate
                | B::LossesOnTimeRate
                | B::TimePerMoveOpening
                | B::TimePerMoveMiddlegame
                | B::TimePerMoveEndgame
                | B::EvalAfterOpening
                | B::OpeningOutWorseRate
                | B::BookExitPly
                | B::TimeInOpening
                | B::EndgameConversionRate
                | B::EndgameHoldRate
                | B::SaveableResignationRate
        ),
        MetricId::MotifMissRate(_) | MetricId::Area(_) => false,
    }
}

fn other(c: Color) -> Color {
    match c {
        Color::White => Color::Black,
        Color::Black => Color::White,
    }
}

/// The opponent's context of a game: the other colour, the pre-game ratings `(own, opponent)`
/// of the player swapped, the score mirrored, and the opponent's game accuracy.
pub fn opponent_context(
    owner: &GameContext,
    owner_pre: (i32, i32),
    accuracy: Option<f64>,
) -> GameContext {
    let color = other(owner.player_color);
    GameContext {
        player_color: color,
        player_rating: Some(owner_pre.1),
        opponent_rating: Some(owner_pre.0),
        score: player_score(&owner.result, color),
        accuracy,
        ..owner.clone()
    }
}

/// One opponent side.
#[derive(Clone, Debug, PartialEq)]
pub struct PeerSide {
    /// The opponent's lower-case username.
    pub opponent: String,
    /// Pre-game ratings of the opponent and of the player.
    pub opponent_rating: i32,
    pub player_rating: i32,
    pub metrics: PlayerGameMetrics,
    /// The opponent's point of view ([`opponent_context`]).
    pub ctx: GameContext,
}

/// The aggregated peer benchmark of one account × time class.
#[derive(Clone, Debug, PartialEq)]
pub struct PeerBenchmark {
    pub center_rating: i32,
    pub rating_low: i32,
    pub rating_high: i32,
    /// Distinct opponents in the band, and their games.
    pub n_players: u32,
    pub n_games: u32,
    /// Every comparable row with evidence, low-sample ones included (stored, never served).
    pub stats: Vec<BandReferenceStat>,
}

impl PeerBenchmark {
    /// The served rows (not low-sample); `None` when there is none.
    pub fn reference(&self) -> Option<BandReference> {
        let r: BandReference = self
            .stats
            .iter()
            .filter(|s| !s.low_sample)
            .map(|s| (s.metric, s.clone()))
            .collect();
        (!r.is_empty()).then_some(r)
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// See the module docs. `center_rating` is the player's latest pre-game rating.
pub fn aggregate_peer_benchmark(sides: &[PeerSide], center_rating: i32) -> PeerBenchmark {
    let rating_low = center_rating - PEER_RATING_WINDOW;
    let rating_high = center_rating + PEER_RATING_WINDOW;
    let band = band_of(f64::from(center_rating));
    let cell: Vec<ReferenceSide> = sides
        .iter()
        .filter(|s| s.metrics.version == PLAYER_GAME_METRICS_VERSION)
        .filter(|s| (rating_low..=rating_high).contains(&s.opponent_rating))
        .map(|s| ReferenceSide {
            player_hash: s.opponent.clone(),
            band,
            tc: s.ctx.time_class,
            player_rating: s.opponent_rating,
            opponent_rating: s.player_rating,
            metrics: s.metrics.clone(),
            ctx: s.ctx.clone(),
        })
        .collect();
    let players: BTreeSet<&str> = cell.iter().map(|s| s.player_hash.as_str()).collect();
    let refs: Vec<&ReferenceSide> = cell.iter().collect();
    let mut report = AggregateReport::default();
    let stats = kpi_catalogue()
        .iter()
        .filter(|d| d.kind != KpiKind::Last && is_peer_comparable(d.metric))
        .filter_map(|d| aggregate_row(d, &refs, Weighting::Players, &mut report))
        .collect();
    PeerBenchmark {
        center_rating,
        rating_low,
        rating_high,
        n_players: count(players.len()),
        n_games: count(cell.len()),
        stats,
    }
}

// ---------------------------------------------------------------------------------- storage

/// What [`refresh_opponent_metrics`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RefreshReport {
    pub computed: u32,
    /// Games left out: unparsable, without ratings or with uncached evals.
    pub skipped: u32,
}

#[derive(FromRow)]
struct MissingRow {
    game_id: Uuid,
    source_created_at: OffsetDateTime,
    metrics: Value,
}

/// Current-version player metrics whose opponent row is missing, stale or older than them.
const MISSING_SQL: &str = "SELECT gm.game_id, gm.created_at AS source_created_at, gm.metrics \
     FROM game_metrics gm \
     LEFT JOIN opponent_game_metrics o ON o.game_id = gm.game_id \
     WHERE gm.account_id = $1 AND gm.time_class = $2::time_class AND gm.metrics_version = $3 \
       AND (o.game_id IS NULL OR o.metrics_version <> $3 \
            OR o.source_created_at <> gm.created_at) \
     ORDER BY gm.played_at, gm.game_id";

fn version() -> i32 {
    i32::try_from(PLAYER_GAME_METRICS_VERSION).unwrap_or(i32::MAX)
}

/// The opponent's metrics of one game from its stored PGN and cached main evals (no engine, no
/// enrichment). `None` when the game cannot be computed that way.
pub async fn compute_opponent_metrics(
    pool: &PgPool,
    game_id: Uuid,
    owner_metrics: &PlayerGameMetrics,
) -> Result<Option<(String, i32, i32, PlayerGameMetrics)>, sqlx::Error> {
    let Some(game) = get_game(pool, game_id).await? else {
        return Ok(None);
    };
    let (Some(own_pre), Some(opp_pre)) =
        (owner_metrics.player_rating, owner_metrics.opponent_rating)
    else {
        return Ok(None);
    };
    let Some(owner_color) = parse_color(&game.player_color) else {
        return Ok(None);
    };
    let Ok(parsed) = parse_pgn(&game.pgn) else {
        return Ok(None);
    };
    let cache = DbEvalCache::new(pool.clone());
    let deps = EvaluateDeps {
        analyser: &NoEngine,
        cache: &cache,
        concurrency: 1,
        on_progress: None,
        cancel: None,
        deadline: None,
    };
    let Ok(evals) = evaluate_game(&deps, &parsed, None).await.map(|e| e.evals) else {
        return Ok(None);
    };
    let clock = match (game.base_seconds, game.increment_seconds) {
        (Some(base), Some(inc)) => Some(ClockInfo {
            base_seconds: f64::from(base),
            increment_seconds: f64::from(inc),
        }),
        _ => None,
    };
    let Ok(records) = build_move_records(&parsed, &evals, clock) else {
        return Ok(None);
    };
    let Some(tc) = parse_time_class(&game.time_class) else {
        return Ok(None);
    };
    let Ok(platform) = game.platform.parse() else {
        return Ok(None);
    };
    let secs = |v: Option<i32>| v.and_then(|s| u32::try_from(s).ok());
    let owner = GameContext {
        game_id: game.id,
        platform_game_id: game.platform_game_id.clone(),
        platform,
        played_at: game.played_at,
        time_class: tc,
        base_seconds: secs(game.base_seconds),
        increment_seconds: secs(game.increment_seconds),
        rated: game.rated,
        player_color: owner_color,
        player_rating: Some(own_pre),
        opponent_rating: Some(opp_pre),
        result: game.result.clone(),
        score: player_score(&game.result, owner_color),
        termination: game.termination.clone(),
        accuracy: owner_metrics.accuracy,
        protocol: ANALYSIS_PROTOCOL.to_owned(),
    };
    let wins: Vec<f64> = evals.iter().map(|e| win_percent(e.score)).collect();
    let opp_color = other(owner_color);
    let accuracy = (!records.is_empty()).then(|| game_accuracy(&wins, opp_color));
    let ctx = opponent_context(&owner, (own_pre, opp_pre), accuracy);
    let explorer = load_game_explorer(pool, game.account_id, tc, &parsed).await;
    let raw = vec![None; records.len()];
    let features = compute_game_features(FeatureInput {
        ctx: &ctx,
        parsed: &parsed,
        evals: &evals,
        records: &records,
        raw: &raw,
        explorer: explorer.as_ref(),
    });
    let name = match opp_color {
        Color::White => &game.white_name,
        Color::Black => &game.black_name,
    };
    Ok(Some((
        name.to_lowercase(),
        opp_pre,
        own_pre,
        features.metrics,
    )))
}

/// Computes the missing or stale `opponent_game_metrics` rows of the account × time class.
pub async fn refresh_opponent_metrics(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
) -> Result<RefreshReport, sqlx::Error> {
    let rows: Vec<MissingRow> = sqlx::query_as(MISSING_SQL)
        .bind(account_id)
        .bind(tc.as_str())
        .bind(version())
        .fetch_all(pool)
        .await?;
    let mut report = RefreshReport::default();
    for r in rows {
        let Ok(owner) = serde_json::from_value::<PlayerGameMetrics>(r.metrics) else {
            report.skipped += 1;
            continue;
        };
        let Some((opponent, opp_rating, own_rating, metrics)) =
            compute_opponent_metrics(pool, r.game_id, &owner).await?
        else {
            report.skipped += 1;
            continue;
        };
        let metrics = serde_json::to_value(&metrics)
            .map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        sqlx::query(
            "INSERT INTO opponent_game_metrics (game_id, account_id, time_class, opponent, \
               opponent_rating, player_rating, metrics, source_created_at) \
             VALUES ($1, $2, $3::time_class, $4, $5, $6, $7, $8) \
             ON CONFLICT (game_id) DO UPDATE SET account_id = EXCLUDED.account_id, \
               time_class = EXCLUDED.time_class, opponent = EXCLUDED.opponent, \
               opponent_rating = EXCLUDED.opponent_rating, \
               player_rating = EXCLUDED.player_rating, metrics = EXCLUDED.metrics, \
               source_created_at = EXCLUDED.source_created_at, created_at = now()",
        )
        .bind(r.game_id)
        .bind(account_id)
        .bind(tc.as_str())
        .bind(&opponent)
        .bind(opp_rating)
        .bind(own_rating)
        .bind(&metrics)
        .bind(r.source_created_at)
        .execute(pool)
        .await?;
        report.computed += 1;
    }
    if report.skipped > 0 {
        tracing::info!(%account_id, tc = tc.as_str(), skipped = report.skipped,
            "opponent sides left out of the peer benchmark");
    }
    Ok(report)
}

#[derive(FromRow)]
struct SideRow {
    game_id: Uuid,
    opponent: String,
    opponent_rating: i32,
    player_rating: i32,
    metrics: Value,
    platform: String,
    platform_game_id: String,
    played_at: OffsetDateTime,
    base_seconds: Option<i32>,
    increment_seconds: Option<i32>,
    rated: bool,
    player_color: String,
    result: String,
    termination: Option<String>,
}

/// The opponent sides of the account × time class's current games (whose player metrics are
/// current and unchanged since the side was computed), on the caller's connection.
pub async fn load_peer_sides(
    conn: &mut PgConnection,
    account_id: Uuid,
    tc: TimeClass,
) -> Result<Vec<PeerSide>, sqlx::Error> {
    let rows: Vec<SideRow> = sqlx::query_as(
        "SELECT o.game_id, o.opponent, o.opponent_rating, o.player_rating, o.metrics, \
           g.platform::text AS platform, g.platform_game_id, g.played_at, g.base_seconds, \
           g.increment_seconds, g.rated, g.player_color::text AS player_color, \
           g.result::text AS result, g.termination \
         FROM opponent_game_metrics o \
         JOIN game_metrics gm ON gm.game_id = o.game_id \
           AND gm.created_at = o.source_created_at AND gm.metrics_version = $3 \
         JOIN games g ON g.id = o.game_id \
         WHERE o.account_id = $1 AND o.time_class = $2::time_class AND o.metrics_version = $3 \
         ORDER BY g.played_at, g.id",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .bind(version())
    .fetch_all(conn)
    .await?;
    let secs = |v: Option<i32>| v.and_then(|s| u32::try_from(s).ok());
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let metrics: PlayerGameMetrics = serde_json::from_value(r.metrics).ok()?;
            let owner = parse_color(&r.player_color)?;
            let color = other(owner);
            let ctx = GameContext {
                game_id: r.game_id,
                platform_game_id: r.platform_game_id,
                platform: r.platform.parse().ok()?,
                played_at: r.played_at,
                time_class: tc,
                base_seconds: secs(r.base_seconds),
                increment_seconds: secs(r.increment_seconds),
                rated: r.rated,
                player_color: color,
                player_rating: Some(r.opponent_rating),
                opponent_rating: Some(r.player_rating),
                score: player_score(&r.result, color),
                result: r.result,
                termination: r.termination,
                accuracy: metrics.accuracy,
                protocol: ANALYSIS_PROTOCOL.to_owned(),
            };
            Some(PeerSide {
                opponent: r.opponent,
                opponent_rating: r.opponent_rating,
                player_rating: r.player_rating,
                metrics,
                ctx,
            })
        })
        .collect())
}

/// The player's latest pre-game rating among the loaded games (the band's centre).
pub fn center_rating(games: &[(GameContext, PlayerGameMetrics)]) -> Option<i32> {
    games
        .iter()
        .filter(|(_, m)| m.player_rating.is_some())
        .max_by_key(|(c, _)| (c.played_at, c.game_id))
        .and_then(|(_, m)| m.player_rating)
}

/// What the build stored as the account × time class's benchmark.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredBenchmark<'a> {
    pub source: &'static str,
    /// The peer band, `None` for the Lichess bands.
    pub peers: Option<&'a PeerBenchmark>,
    /// The rows: every peer row (low-sample ones flagged), or the served Lichess ones.
    pub stats: Vec<BandReferenceStat>,
}

/// Replaces the account × time class's stored benchmark (in the build's transaction). `None`
/// removes it.
pub async fn write_account_benchmark(
    conn: &mut PgConnection,
    account_id: Uuid,
    tc: TimeClass,
    b: Option<&StoredBenchmark<'_>>,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM account_benchmark WHERE account_id = $1 AND time_class = $2::time_class")
        .bind(account_id)
        .bind(tc.as_str())
        .execute(&mut *conn)
        .await?;
    let Some(b) = b else {
        return Ok(());
    };
    let n = |v: u32| i32::try_from(v).unwrap_or(i32::MAX);
    sqlx::query(
        "INSERT INTO account_benchmark (account_id, time_class, source, center_rating, \
           rating_low, rating_high, n_players, n_games) \
         VALUES ($1, $2::time_class, $3, $4, $5, $6, $7, $8)",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .bind(b.source)
    .bind(b.peers.map(|p| p.center_rating))
    .bind(b.peers.map(|p| p.rating_low))
    .bind(b.peers.map(|p| p.rating_high))
    .bind(b.peers.map(|p| n(p.n_players)))
    .bind(b.peers.map(|p| n(p.n_games)))
    .execute(&mut *conn)
    .await?;
    if b.stats.is_empty() {
        return Ok(());
    }
    let f = |g: fn(&BandReferenceStat) -> f64| b.stats.iter().map(g).collect::<Vec<_>>();
    let o = |g: fn(&BandReferenceStat) -> Option<f64>| {
        b.stats
            .iter()
            .map(|s| g(s).filter(|x| x.is_finite()))
            .collect::<Vec<_>>()
    };
    sqlx::query(
        "INSERT INTO account_benchmark_stat (account_id, time_class, metric, median, p25, p40, \
           p75, pooled_rate, epl_per_event, opps_per_game, opp_slope_per_100, n_players, \
           n_games, low_sample) \
         SELECT $1, $2::time_class, m, med, q25, q40, q75, pr, epl, opg, slope, np, ng, low \
         FROM UNNEST($3::text[], $4::float8[], $5::float8[], $6::float8[], $7::float8[], \
                     $8::float8[], $9::float8[], $10::float8[], $11::float8[], $12::int4[], \
                     $13::int4[], $14::bool[]) \
           AS x(m, med, q25, q40, q75, pr, epl, opg, slope, np, ng, low)",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .bind(b.stats.iter().map(|s| s.metric.to_string()).collect::<Vec<_>>())
    .bind(f(|s| s.median))
    .bind(f(|s| s.p25))
    .bind(f(|s| s.p40))
    .bind(f(|s| s.p75))
    .bind(o(|s| s.pooled_rate))
    .bind(o(|s| s.epl_per_event))
    .bind(o(|s| s.opps_per_game))
    .bind(o(|s| s.opp_slope_per_100))
    .bind(b.stats.iter().map(|s| n(s.n_players)).collect::<Vec<_>>())
    .bind(b.stats.iter().map(|s| n(s.n_games)).collect::<Vec<_>>())
    .bind(b.stats.iter().map(|s| s.low_sample).collect::<Vec<_>>())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

#[derive(FromRow)]
struct StatRow {
    metric: String,
    median: f64,
    p25: f64,
    p40: f64,
    p75: f64,
    pooled_rate: Option<f64>,
    epl_per_event: Option<f64>,
    opps_per_game: Option<f64>,
    opp_slope_per_100: Option<f64>,
    n_players: i32,
    n_games: i32,
    low_sample: bool,
}

/// The served rows (not low-sample) of the benchmark the account × time class's last build
/// stored; `None` without one. The CoachingInput builder reads this, so it compares with what
/// build-profile compared with.
pub async fn load_account_benchmark(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
) -> Result<Option<BandReference>, sqlx::Error> {
    let rows: Vec<StatRow> = sqlx::query_as(
        "SELECT metric, median, p25, p40, p75, pooled_rate, epl_per_event, opps_per_game, \
           opp_slope_per_100, n_players, n_games, low_sample \
         FROM account_benchmark_stat \
         WHERE account_id = $1 AND time_class = $2::time_class AND NOT low_sample",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_all(pool)
    .await?;
    let u = |v: i32| u32::try_from(v).unwrap_or(0);
    let r: BandReference = rows
        .into_iter()
        .filter_map(|r| {
            let metric: MetricId = r.metric.parse().ok()?;
            Some((
                metric,
                BandReferenceStat {
                    metric,
                    median: r.median,
                    p25: r.p25,
                    p40: r.p40,
                    p75: r.p75,
                    pooled_rate: r.pooled_rate,
                    epl_per_event: r.epl_per_event,
                    opps_per_game: r.opps_per_game,
                    opp_slope_per_100: r.opp_slope_per_100,
                    n_players: u(r.n_players),
                    n_games: u(r.n_games),
                    low_sample: r.low_sample,
                },
            ))
        })
        .collect::<BTreeMap<_, _>>();
    Ok((!r.is_empty()).then_some(r))
}
