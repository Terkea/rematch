//! A synthetic profile seed (R2 Task 33, TS M2 Task 25 `seed.ts`; reused by Task 34).
//!
//! One Lichess account, `games` analysed blitz games (ratings 1500, so band B2) split over two
//! ISO weeks (2026-W46 and 2026-W47), each with a current-version `game_metrics` row and a `W1`
//! series point of 2 events in 50 opportunities (a rate of 0.04). With `reference` set, one
//! active band-reference build holds `weakness:W1` for B2 blitz at a median of 0.02.
//!
//! Every game is `done`, enriched (`enr-v1`) and analysed one hour after it was played, all
//! before [`SEED_NOW`], the fake clock of the build.
#![allow(dead_code)]

use std::collections::BTreeMap;

use chess_core::TimeClass;
use db::profile_rows::{
    GameMetricsRow, SeriesPointRow, insert_game_metrics, replace_series_points,
};
use profile_core::types::{
    Band, BandReferenceStat, BinomialObs, MetricId, PLAYER_GAME_METRICS_VERSION, PhaseValues,
    PlayerGameMetrics, WeaknessKey,
};
use refdata::band_reference::{BandReferenceRow, write_band_reference};
use refdata::builds::{BuildKind, NewBuild, activate_build, create_build, set_source_sha};
use serde_json::json;
use sqlx::PgPool;
use testkit::{GameSeed, insert_account, insert_game};
use time::macros::datetime;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

/// The build's fake clock: after every seeded game and its analysis.
pub const SEED_NOW: OffsetDateTime = datetime!(2026-11-20 12:00 UTC);
/// First game of each seed week (2026-W46 and 2026-W47).
pub const WEEK_STARTS: [OffsetDateTime; 2] = [
    datetime!(2026-11-10 10:00 UTC),
    datetime!(2026-11-17 10:00 UTC),
];
pub const SEED_WEEKS: [&str; 2] = ["2026-W46", "2026-W47"];
/// Per-game W1 counts: 2 events in 50 opportunities.
pub const W1_EVENTS: i32 = 2;
pub const W1_OPPORTUNITIES: i32 = 50;
/// The band median of W1 in the seeded reference.
pub const W1_REFERENCE: f64 = 0.02;
pub const PROTOCOL: &str = "sf19-n300k-v1";

/// [`seed_now`] as a plain `fn`, for `BuildProfileDeps::now`.
pub fn seed_now() -> OffsetDateTime {
    SEED_NOW
}

#[derive(Clone, Debug)]
pub struct SeedOptions {
    pub games: usize,
    pub reference: bool,
}

impl Default for SeedOptions {
    fn default() -> Self {
        Self {
            games: 40,
            reference: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Seed {
    pub account_id: Uuid,
    /// Game ids, oldest first.
    pub games: Vec<Uuid>,
    pub build_id: Option<Uuid>,
}

/// When the seed's game `i` is played: half the games in each week, one hour apart.
pub fn played_at(i: usize, total: usize) -> OffsetDateTime {
    let half = total.div_ceil(2).max(1);
    let (week, j) = (i / half, i % half);
    WEEK_STARTS[week.min(1)] + Duration::hours(i64::try_from(j).unwrap_or(0))
}

/// The metrics of every seeded game (version-current, W1 2/50).
pub fn seed_metrics() -> PlayerGameMetrics {
    let mut weakness = BTreeMap::new();
    weakness.insert(
        WeaknessKey::W1,
        BinomialObs {
            events: W1_EVENTS as u32,
            opportunities: W1_OPPORTUNITIES as u32,
            epl: 0.1,
            exemplar_plies: Vec::new(),
        },
    );
    PlayerGameMetrics {
        version: PLAYER_GAME_METRICS_VERSION,
        score: Some(1.0),
        expected_score: Some(0.5),
        player_rating: Some(1500),
        opponent_rating: Some(1500),
        accuracy: Some(82.5),
        accuracy_by_phase: PhaseValues {
            opening: Some(90.0),
            middlegame: Some(80.0),
            endgame: None,
        },
        moves: 30,
        blunders: 1,
        mistakes: 1,
        inaccuracies: 2,
        weakness,
        ..PlayerGameMetrics::default()
    }
}

/// Inserts one analysed blitz game of the seed's shape played at `at` and analysed at
/// `analysed_at`, with its metrics and W1 series point. Returns the game id.
pub async fn insert_analysed_game(
    pool: &PgPool,
    account_id: Uuid,
    at: OffsetDateTime,
    analysed_at: OffsetDateTime,
) -> Uuid {
    let game_id = insert_unanalysed_game(pool, account_id, at).await;
    write_game_rows(pool, account_id, game_id, at, analysed_at).await;
    game_id
}

/// A blitz game played at `at`, still without metrics or series (`pending`).
pub async fn insert_unanalysed_game(pool: &PgPool, account_id: Uuid, at: OffsetDateTime) -> Uuid {
    let game_id = insert_game(
        pool,
        account_id,
        GameSeed {
            base_seconds: Some(180),
            increment_seconds: Some(0),
            ..GameSeed::default()
        },
    )
    .await;
    sqlx::query("UPDATE games SET played_at = $2 WHERE id = $1")
        .bind(game_id)
        .bind(at)
        .execute(pool)
        .await
        .expect("set played_at");
    game_id
}

/// Marks the game analysed and writes its `game_metrics` row and W1 series point.
pub async fn write_game_rows(
    pool: &PgPool,
    account_id: Uuid,
    game_id: Uuid,
    at: OffsetDateTime,
    analysed_at: OffsetDateTime,
) {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE games SET analysis_status = 'done', analysed_at = $2, \
         enrichment_protocol = 'enr-v1' WHERE id = $1",
    )
    .bind(game_id)
    .bind(analysed_at)
    .execute(&mut *tx)
    .await
    .expect("mark analysed");
    insert_game_metrics(
        &mut tx,
        &GameMetricsRow {
            game_id,
            account_id,
            time_class: TimeClass::Blitz.as_str().to_owned(),
            played_at: at,
            protocol: PROTOCOL.to_owned(),
            enrichment_protocol: Some("enr-v1".to_owned()),
            difficulty_model: None,
            metrics: serde_json::to_value(seed_metrics()).unwrap(),
        },
    )
    .await
    .expect("insert game_metrics");
    replace_series_points(
        &mut tx,
        game_id,
        &[SeriesPointRow {
            account_id,
            time_class: TimeClass::Blitz.as_str().to_owned(),
            series_key: "W1".to_owned(),
            played_at: at,
            events: W1_EVENTS,
            opportunities: W1_OPPORTUNITIES,
            epl: 0.1,
            protocol: PROTOCOL.to_owned(),
        }],
    )
    .await
    .expect("insert series point");
    tx.commit().await.unwrap();
}

/// The W1 band statistics of the seeded reference.
pub fn w1_stat() -> BandReferenceStat {
    BandReferenceStat {
        metric: MetricId::Weakness(WeaknessKey::W1),
        median: W1_REFERENCE,
        p25: 0.01,
        p40: 0.016,
        p75: 0.03,
        pooled_rate: Some(W1_REFERENCE),
        epl_per_event: Some(0.1),
        opps_per_game: Some(50.0),
        opp_slope_per_100: Some(0.0),
        n_players: 200,
        n_games: 2000,
        low_sample: false,
    }
}

/// Creates and activates a band-reference build holding `stats` for B2 blitz.
pub async fn insert_reference(pool: &PgPool, stats: &[BandReferenceStat]) -> Uuid {
    let id = create_build(
        pool,
        &NewBuild {
            kind: BuildKind::BandReference,
            source_month: "2026-10".into(),
            source_url: None,
            params: json!({}),
            protocol: Some(PROTOCOL.into()),
            enrichment_protocol: Some("enr-v1".into()),
            difficulty_model: None,
        },
    )
    .await
    .expect("create build");
    let rows: Vec<BandReferenceRow> = stats
        .iter()
        .map(|s| BandReferenceRow {
            band: Band::B2,
            time_class: TimeClass::Blitz,
            stat: s.clone(),
        })
        .collect();
    let mut tx = pool.begin().await.unwrap();
    write_band_reference(&mut tx, id, &rows)
        .await
        .expect("write band reference");
    tx.commit().await.unwrap();
    set_source_sha(pool, id, &"ab".repeat(32), 10)
        .await
        .expect("set sha");
    activate_build(pool, id).await.expect("activate build");
    id
}

/// The seed (module docs).
pub async fn seed(pool: &PgPool, opts: SeedOptions) -> Seed {
    let account_id = insert_account(pool, "lichess", "seedplayer").await;
    let mut games = Vec::with_capacity(opts.games);
    for i in 0..opts.games {
        let at = played_at(i, opts.games);
        games.push(insert_analysed_game(pool, account_id, at, at + Duration::hours(1)).await);
    }
    let build_id = if opts.reference {
        Some(insert_reference(pool, &[w1_stat()]).await)
    } else {
        None
    };
    Seed {
        account_id,
        games,
        build_id,
    }
}
