//! Band reference from reference games (R2 Task 32, TS M2 Task 14): per-game analysis of both
//! colours, the per-band aggregation (Beta-binomial quartiles, per-player means, opponent
//! slope) and the idempotent, evidence-gated finalisation.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use analysis::eval_cache::MemoryEvalCache;
use chess_core::{Color, TimeClass};
use enrich::difficulty::{DifficultyOutcome, DifficultySource, PendingReason};
use enrich::enrich_game::EnrichDeps;
use enrich::store::MemoryEnrichmentStore;
use enrich::types::DifficultyInput;
use futures::future::BoxFuture;
use profile::reference_aggregate::{
    LOW_SAMPLE_PLAYERS, ReferenceSide, aggregate_band_reference, aggregate_band_reference_report,
    has_minimum_evidence,
};
use profile::reference_jobs::{
    FinalizeOptions, FinalizeOutcome, MAIA_GRACE, REFERENCE_JOB_KIND, ReferenceGameDeps,
    ReferenceOutcome, finalize_band_reference, finalize_band_reference_with, handle_reference_game,
    reenrich_reference_games,
};
use profile_core::stats::beta_binomial::fit_beta_binomial;
use profile_core::stats::priority::{EstimateInput, EstimateReason, estimate_weakness};
use profile_core::types::{
    Band, BaseMetric, BinomialObs, Difficulty, GameContext, MetricId, PLAYER_GAME_METRICS_VERSION,
    PlatformId, PlayerGameMetrics, Side, WeaknessKey,
};
use queue::{Job, KIND_REFERENCE_GAME, ReferenceGamePayload};
use refdata::band_reference::{BandReferenceRow, load_band_reference};
use refdata::builds::{BuildKind, BuildStatus, NewBuild, create_build, get_build, set_source_sha};
use refdata::reference_games::{
    NewReferenceGame, ObservationRow, insert_reference_games, set_reference_game_status,
    write_observations,
};
use serde_json::json;
use sqlx::PgPool;
use testkit::fake_engine::FakeAnalyser;
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const TEST_TIMEOUT: Duration = Duration::from_secs(120);

async fn within<T>(what: &str, fut: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(TEST_TIMEOUT, fut)
        .await
        .unwrap_or_else(|_| panic!("timed out after {TEST_TIMEOUT:?} waiting for {what}"))
}

// =============================================================================================
// Pure aggregation

fn ctx(game: Uuid, color: Color, own: i32, opp: i32, tc: TimeClass) -> GameContext {
    GameContext {
        game_id: game,
        platform_game_id: format!("lg{game}"),
        platform: PlatformId::Lichess,
        played_at: OffsetDateTime::UNIX_EPOCH,
        time_class: tc,
        base_seconds: Some(180),
        increment_seconds: Some(0),
        rated: true,
        player_color: color,
        player_rating: Some(own),
        opponent_rating: Some(opp),
        result: "1/2-1/2".to_owned(),
        score: Some(0.5),
        termination: None,
        accuracy: None,
        protocol: engine::ANALYSIS_PROTOCOL.to_owned(),
    }
}

fn metrics() -> PlayerGameMetrics {
    PlayerGameMetrics {
        version: PLAYER_GAME_METRICS_VERSION,
        moves: 30,
        ..PlayerGameMetrics::default()
    }
}

fn side(player: &str, band: Band, own: i32, opp: i32, m: PlayerGameMetrics) -> ReferenceSide {
    let tc = TimeClass::Blitz;
    ReferenceSide {
        player_hash: player.to_owned(),
        band,
        tc,
        player_rating: own,
        opponent_rating: opp,
        metrics: m,
        ctx: ctx(Uuid::new_v4(), Color::White, own, opp, tc),
    }
}

/// `games` W1 sides of `player`, splitting `events` and `opps` evenly over the games, with an
/// EPL of 0.5 per event.
fn w1_player(player: &str, events: u32, opps: u32, games: u32) -> Vec<ReferenceSide> {
    (0..games)
        .map(|i| {
            let e = events / games + u32::from(i < events % games);
            let mut m = metrics();
            m.weakness.insert(
                WeaknessKey::W1,
                BinomialObs {
                    events: e,
                    opportunities: opps / games,
                    epl: 0.5 * f64::from(e),
                    exemplar_plies: vec![],
                },
            );
            side(player, Band::B2, 1500, 1500, m)
        })
        .collect()
}

fn row(rows: &[BandReferenceRow], band: Band, metric: MetricId) -> &BandReferenceRow {
    rows.iter()
        .find(|r| {
            r.band == band
                && r.time_class == TimeClass::Blitz
                && r.side == Side::Both
                && r.stat.metric == metric
        })
        .unwrap_or_else(|| panic!("no {band:?} {metric} row in {rows:#?}"))
}

const W1: MetricId = MetricId::Weakness(WeaknessKey::W1);
const ACCURACY: MetricId = MetricId::Base(BaseMetric::Accuracy);

#[test]
fn binomial_rows_pool_the_rate_and_take_quartiles_from_the_beta_binomial_fit() {
    let mut sides = Vec::new();
    for p in 0..10 {
        sides.extend(w1_player(&format!("low{p}"), 6, 300, 10));
        sides.extend(w1_player(&format!("high{p}"), 12, 300, 10));
    }
    let rows = aggregate_band_reference(&sides);
    let s = &row(&rows, Band::B2, W1).stat;
    let pooled = s.pooled_rate.unwrap();
    assert!((pooled - 0.03).abs() < 1e-6, "pooled {pooled}");
    assert!(s.p25 < 0.03 && 0.03 < s.p75, "p25 {} p75 {}", s.p25, s.p75);
    assert!(s.p25 <= s.p40 && s.p40 <= s.median && s.median <= s.p75);
    assert_eq!(s.n_players, 20);
    assert_eq!(s.n_games, 200);
    assert!(!s.low_sample);
    assert!((s.opps_per_game.unwrap() - 30.0).abs() < 1e-12);
    assert!((s.epl_per_event.unwrap() - 0.5).abs() < 1e-12);

    // The quartiles are those of the fitted Beta, not of the 20 raw per-player rates (whose
    // p25 would be exactly 0.02).
    let pairs: Vec<(f64, f64)> = (0..10)
        .map(|_| (6.0, 300.0))
        .chain((0..10).map(|_| (12.0, 300.0)))
        .collect();
    let dist = fit_beta_binomial(&pairs).unwrap().rates().unwrap();
    assert!((s.median - dist.quantile(0.5).unwrap()).abs() < 1e-12);
    assert!((s.p40 - dist.quantile(0.4).unwrap()).abs() < 1e-12);
    assert!((s.p25 - 0.02).abs() > 1e-6);
}

#[test]
fn five_players_are_low_sample() {
    let sides: Vec<_> = (0..5)
        .flat_map(|p| w1_player(&format!("p{p}"), 6, 300, 10))
        .collect();
    let rows = aggregate_band_reference(&sides);
    let s = &row(&rows, Band::B2, W1).stat;
    assert_eq!(s.n_players, 5);
    assert!(s.low_sample);
    assert_eq!(LOW_SAMPLE_PLAYERS, 10);
    assert!(!has_minimum_evidence(&rows));
    let ten: Vec<_> = (0..10)
        .flat_map(|p| w1_player(&format!("p{p}"), 6, 300, 1))
        .collect();
    let rows = aggregate_band_reference(&ten);
    assert!(!row(&rows, Band::B2, W1).stat.low_sample);
    assert!(has_minimum_evidence(&rows));
}

fn accuracy_side(player: &str, own: i32, opp: i32, value: f64) -> ReferenceSide {
    let mut m = metrics();
    m.accuracy = Some(value);
    side(player, Band::B2, own, opp, m)
}

#[test]
fn opponent_slope_is_the_ols_slope_per_100_points() {
    let sides: Vec<_> = (0..12)
        .map(|i| {
            let own = 1500;
            let opp = 1300 + 37 * i;
            let gap = f64::from(opp - own) / 100.0;
            accuracy_side(&format!("p{i}"), own, opp, 1.0 + 0.5 * gap)
        })
        .collect();
    let rows = aggregate_band_reference(&sides);
    let slope = row(&rows, Band::B2, ACCURACY)
        .stat
        .opp_slope_per_100
        .unwrap();
    assert!((slope - 0.5).abs() < 1e-6, "slope {slope}");
}

#[test]
fn opponent_slope_is_unknown_when_the_gap_never_varies() {
    let sides: Vec<_> = (0..12)
        .map(|i| accuracy_side(&format!("p{i}"), 1500, 1600, 60.0 + f64::from(i)))
        .collect();
    let rows = aggregate_band_reference(&sides);
    let slope = row(&rows, Band::B2, ACCURACY).stat.opp_slope_per_100;
    // Not estimable: unknown (None, stored NULL), never NaN and never a made-up 0.
    assert_eq!(slope, None);
}

#[test]
fn continuous_rows_take_type_7_quantiles_of_per_player_means() {
    // Per-player means 1, 2, 3, 4 (each from two games m - 0.5 and m + 0.5).
    let sides: Vec<_> = (1..=4)
        .flat_map(|m| {
            let m = f64::from(m);
            [
                accuracy_side(&format!("p{m}"), 1500, 1500, m - 0.5),
                accuracy_side(&format!("p{m}"), 1500, 1500, m + 0.5),
            ]
        })
        .collect();
    let rows = aggregate_band_reference(&sides);
    let s = &row(&rows, Band::B2, ACCURACY).stat;
    assert_eq!((s.p25, s.p40, s.median, s.p75), (1.75, 2.2, 2.5, 3.25));
    assert_eq!((s.n_players, s.n_games), (4, 8));
    assert_eq!(
        (s.pooled_rate, s.epl_per_event, s.opps_per_game),
        (None, None, None)
    );
}

/// The colour split: `Both` pools every side; `White` and `Black` rows each come from their own
/// colour's sides, with their own player counts and low-sample flags.
#[test]
fn sides_are_aggregated_per_colour_and_pooled_for_both() {
    // Ten white players at 0.02 and ten black players at 0.06: each colour has ten players.
    let mut sides: Vec<_> = (0..10)
        .flat_map(|p| w1_player(&format!("w{p}"), 6, 300, 1))
        .collect();
    for p in 0..10 {
        for mut s in w1_player(&format!("b{p}"), 18, 300, 1) {
            s.ctx.player_color = Color::Black;
            sides.push(s);
        }
    }
    let rows = aggregate_band_reference(&sides);
    let of = |side: Side| {
        rows.iter()
            .find(|r| r.side == side && r.band == Band::B2 && r.stat.metric == W1)
            .unwrap_or_else(|| panic!("no {side} W1 row"))
            .stat
            .clone()
    };
    let (both, white, black) = (of(Side::Both), of(Side::White), of(Side::Black));
    assert_eq!(
        (both.n_players, white.n_players, black.n_players),
        (20, 10, 10)
    );
    assert!((both.pooled_rate.unwrap() - 0.04).abs() < 1e-12);
    assert!((white.pooled_rate.unwrap() - 0.02).abs() < 1e-12);
    assert!((black.pooled_rate.unwrap() - 0.06).abs() < 1e-12);
    assert!(!white.low_sample && !black.low_sample);
    // Rows are ordered band, time class, side, catalogue.
    let order: Vec<Side> = rows
        .iter()
        .filter(|r| r.stat.metric == W1)
        .map(|r| r.side)
        .collect();
    assert_eq!(order, [Side::Both, Side::White, Side::Black]);

    // Five black players only: the black row is low-sample (never served), the both row too.
    let few: Vec<_> = (0..5)
        .flat_map(|p| w1_player(&format!("b{p}"), 18, 300, 1))
        .map(|mut s| {
            s.ctx.player_color = Color::Black;
            s
        })
        .collect();
    let rows = aggregate_band_reference(&few);
    assert!(
        rows.iter().all(|r| r.side != Side::White),
        "no white sides, no white rows"
    );
    assert!(
        rows.iter()
            .all(|r| r.stat.low_sample || r.stat.metric != W1)
    );
}

#[test]
fn stale_metrics_versions_are_left_out() {
    let mut sides: Vec<_> = (0..10)
        .flat_map(|p| w1_player(&format!("p{p}"), 6, 300, 1))
        .collect();
    let mut stale = w1_player("stale", 300, 300, 1);
    stale[0].metrics.version = PLAYER_GAME_METRICS_VERSION - 1;
    sides.extend(stale);
    let report = aggregate_band_reference_report(&sides);
    let s = &row(&report.rows, Band::B2, W1).stat;
    assert_eq!(s.n_players, 10);
    assert!((s.pooled_rate.unwrap() - 0.02).abs() < 1e-12);
    assert_eq!(report.stale_sides, 1);
}

#[test]
fn players_without_opportunities_or_values_do_not_count() {
    let mut sides: Vec<_> = (0..10)
        .flat_map(|p| w1_player(&format!("p{p}"), 6, 300, 1))
        .collect();
    // Opportunity-free W1 and no accuracy: evidence for neither row.
    sides.extend(w1_player("idle", 0, 0, 1));
    let rows = aggregate_band_reference(&sides);
    let s = &row(&rows, Band::B2, W1).stat;
    assert_eq!(s.n_players, 10);
    assert_eq!(
        s.n_games, 11,
        "a zero-opportunity game still counts in opps_per_game"
    );
    assert!((s.opps_per_game.unwrap() - 3000.0 / 11.0).abs() < 1e-9);
    assert!(
        rows.iter().all(|r| r.stat.metric != ACCURACY),
        "no accuracy anywhere, so no accuracy row"
    );
}

#[test]
fn cells_split_by_band_and_time_class() {
    let mut sides: Vec<_> = (0..10)
        .flat_map(|p| w1_player(&format!("p{p}"), 6, 300, 1))
        .collect();
    let mut b4 = w1_player("strong", 3, 300, 1);
    b4[0].band = Band::B4;
    let mut rapid = w1_player("rapid", 9, 300, 1);
    rapid[0].tc = TimeClass::Rapid;
    rapid[0].ctx.time_class = TimeClass::Rapid;
    sides.extend(b4);
    sides.extend(rapid);
    let rows = aggregate_band_reference(&sides);
    assert_eq!(row(&rows, Band::B2, W1).stat.n_players, 10);
    let strong = &row(&rows, Band::B4, W1).stat;
    assert_eq!(strong.n_players, 1);
    assert!(strong.low_sample);
    let r = rows
        .iter()
        .find(|r| r.time_class == TimeClass::Rapid && r.stat.metric == W1)
        .unwrap();
    assert_eq!((r.band, r.stat.n_players), (Band::B2, 1));
}

#[test]
fn zero_event_band_never_surfaces_a_weakness() {
    // T12 ruling: a band where nobody ever hung material has median 0 and no EPL per event.
    let sides: Vec<_> = (0..12)
        .flat_map(|p| w1_player(&format!("p{p}"), 0, 300, 10))
        .collect();
    let rows = aggregate_band_reference(&sides);
    let s = &row(&rows, Band::B2, W1).stat;
    assert_eq!((s.pooled_rate, s.median, s.p75), (Some(0.0), 0.0, 0.0));
    assert_eq!(
        s.epl_per_event, None,
        "EPL per event is unknown without events, never 0"
    );
    assert!(!s.low_sample);

    // A zero-event player against it: nothing is claimed.
    let points = [(0.0, 400.0, 0.0, 0.0)];
    let est = estimate_weakness(&EstimateInput {
        key: WeaknessKey::W1,
        points: &points,
        games_in_window: 40,
        band: Band::B2,
        reference: Some(s),
        fatigue: 0.0,
    });
    assert_ne!(est.reason, EstimateReason::Ok);
    assert_eq!(
        (est.priority, est.percentile, est.k_unweighted),
        (0.0, None, 0.0)
    );
}

// =============================================================================================
// Database: per-game analysis and finalisation

const MAIA_MODEL: &str = "maia2-blitz-0.11.0";

/// Maia that answers every input with one fixed difficulty while `up`.
struct FakeMaia {
    up: AtomicBool,
    /// Healthy, but every item times out (review I2).
    timeout: AtomicBool,
    /// Answers with [`OTHER_MODEL`] (a redeployed Maia).
    other_model: AtomicBool,
}

const OTHER_MODEL: &str = "maia2-rapid-0.10.0";

fn maia(up: bool) -> Arc<FakeMaia> {
    Arc::new(FakeMaia {
        up: AtomicBool::new(up),
        timeout: AtomicBool::new(false),
        other_model: AtomicBool::new(false),
    })
}

impl DifficultySource for FakeMaia {
    fn estimate<'a>(
        &'a self,
        inputs: &'a [DifficultyInput],
    ) -> BoxFuture<'a, Vec<DifficultyOutcome>> {
        Box::pin(async move {
            inputs
                .iter()
                .map(|_| {
                    if self.up.load(Ordering::SeqCst) && self.timeout.load(Ordering::SeqCst) {
                        DifficultyOutcome::PendingMaia {
                            reason: PendingReason::Timeout,
                        }
                    } else if self.up.load(Ordering::SeqCst) {
                        let other = self.other_model.load(Ordering::SeqCst);
                        DifficultyOutcome::Ok {
                            difficulty: Difficulty {
                                p_best: 0.25,
                                p_ok: 0.85,
                                model: if other { OTHER_MODEL } else { MAIA_MODEL }.to_owned(),
                            },
                        }
                    } else {
                        DifficultyOutcome::PendingMaia {
                            reason: PendingReason::MaiaUnavailable,
                        }
                    }
                })
                .collect()
        })
    }

    fn healthy(&self) -> BoxFuture<'_, bool> {
        Box::pin(async move { self.up.load(Ordering::SeqCst) })
    }
}

/// Ten legal 16-ply games (synthetic fixtures: no dump is read in tests).
const GAMES: [&str; 10] = [
    "1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 5. O-O Be7 6. Re1 b5 7. Bb3 d6 8. c3 O-O 1/2-1/2",
    "1. d4 d5 2. c4 e6 3. Nc3 Nf6 4. Bg5 Be7 5. e3 O-O 6. Nf3 h6 7. Bh4 b6 8. cxd5 Nxd5 1/2-1/2",
    "1. e4 c5 2. Nf3 d6 3. d4 cxd4 4. Nxd4 Nf6 5. Nc3 a6 6. Be2 e5 7. Nb3 Be7 8. O-O O-O 1/2-1/2",
    "1. c4 e5 2. Nc3 Nf6 3. Nf3 Nc6 4. g3 d5 5. cxd5 Nxd5 6. Bg2 Nb6 7. O-O Be7 8. d3 O-O 1/2-1/2",
    "1. e4 e6 2. d4 d5 3. Nc3 Nf6 4. Bg5 Be7 5. e5 Nfd7 6. Bxe7 Qxe7 7. f4 O-O 8. Nf3 c5 1/2-1/2",
    "1. d4 Nf6 2. c4 g6 3. Nc3 Bg7 4. e4 d6 5. Nf3 O-O 6. Be2 e5 7. O-O Nc6 8. d5 Ne7 1/2-1/2",
    "1. e4 c6 2. d4 d5 3. Nc3 dxe4 4. Nxe4 Bf5 5. Ng3 Bg6 6. h4 h6 7. Nf3 Nd7 8. h5 Bh7 1/2-1/2",
    "1. Nf3 d5 2. g3 Nf6 3. Bg2 e6 4. O-O Be7 5. d3 O-O 6. Nbd2 c5 7. e4 Nc6 8. Re1 b6 1/2-1/2",
    "1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 4. c3 Nf6 5. d3 d6 6. O-O O-O 7. Re1 a6 8. Bb3 Ba7 1/2-1/2",
    "1. d4 d5 2. Nf3 Nf6 3. e3 e6 4. Bd3 c5 5. c3 Nc6 6. Nbd2 Bd6 7. O-O O-O 8. dxc5 Bxc5 1/2-1/2",
];

fn hash(name: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(name.as_bytes()))
}

fn pgn(moves: &str) -> String {
    format!("[Event \"Rated Blitz game\"]\n[Result \"1/2-1/2\"]\n\n{moves}\n")
}

/// A reference game in B2 blitz (1500 vs 1550) between two players named by `tag`.
fn reference_game(tag: &str, moves: &str) -> NewReferenceGame {
    NewReferenceGame {
        lichess_game_id: format!("g-{tag}"),
        pgn: pgn(moves),
        time_class: TimeClass::Blitz,
        base_seconds: Some(180),
        increment_seconds: Some(0),
        white_rating: Some(1500),
        black_rating: Some(1550),
        white_hash: hash(&format!("white-{tag}")),
        black_hash: hash(&format!("black-{tag}")),
        white_selected: true,
        black_selected: true,
        result: "1/2-1/2".to_owned(),
        termination: Some("Normal".to_owned()),
        played_at: OffsetDateTime::UNIX_EPOCH,
    }
}

const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// A verified `building` band-reference build of `month` with `games` inserted (`pending`).
async fn build_with(pool: &PgPool, month: &str, games: &[NewReferenceGame]) -> (Uuid, Vec<Uuid>) {
    let id = create_build(
        pool,
        &NewBuild {
            kind: BuildKind::BandReference,
            source_month: month.to_owned(),
            source_url: None,
            params: json!({}),
            protocol: Some(engine::ANALYSIS_PROTOCOL.to_owned()),
            enrichment_protocol: Some(enrich::protocol::ENRICHMENT_PROTOCOL.to_owned()),
            difficulty_model: None,
        },
    )
    .await
    .unwrap();
    set_source_sha(pool, id, SHA, 1000).await.unwrap();
    let ids = insert_reference_games(pool, id, games).await.unwrap();
    (id, ids)
}

fn ten_games_in(prefix: &str, tc: TimeClass) -> Vec<NewReferenceGame> {
    ten_games(prefix)
        .into_iter()
        .map(|g| NewReferenceGame {
            time_class: tc,
            ..g
        })
        .collect()
}

fn ten_games(prefix: &str) -> Vec<NewReferenceGame> {
    GAMES
        .iter()
        .enumerate()
        .map(|(i, g)| reference_game(&format!("{prefix}{i}"), g))
        .collect()
}

fn deps(pool: &PgPool, maia: Option<Arc<FakeMaia>>) -> ReferenceGameDeps {
    let analyser = Arc::new(FakeAnalyser::new());
    ReferenceGameDeps {
        pool: pool.clone(),
        analyser: analyser.clone(),
        cache: Arc::new(MemoryEvalCache::default()),
        enrich: EnrichDeps {
            analyser,
            store: Arc::new(MemoryEnrichmentStore::default()),
            tb: None,
            difficulty: maia.map(|m| m as Arc<dyn DifficultySource>),
            cancel: CancellationToken::new(),
        },
    }
}

fn job(id: Uuid) -> Job {
    let now = OffsetDateTime::now_utc();
    Job {
        id: 7,
        kind: KIND_REFERENCE_GAME.to_owned(),
        payload: json!({ "referenceGameId": id }),
        dedupe_key: None,
        user_key: None,
        priority: 200,
        attempts: 1,
        max_attempts: 3,
        lease_token: Uuid::new_v4(),
        run_at: now,
        created_at: now,
        started_at: now,
    }
}

async fn run(d: &ReferenceGameDeps, id: Uuid) -> ReferenceOutcome {
    within(
        "handle_reference_game",
        handle_reference_game(
            d,
            &job(id),
            ReferenceGamePayload {
                reference_game_id: id,
            },
            CancellationToken::new(),
        ),
    )
    .await
    .unwrap_or_else(|e| panic!("reference game {id}: {e}"))
}

async fn count(pool: &PgPool, sql: &str, build: Uuid) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .bind(build)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn status(pool: &PgPool, build: Uuid) -> BuildStatus {
    get_build(pool, build).await.unwrap().unwrap().status
}

const OBSERVATIONS: &str = "SELECT count(*) FROM reference_observations WHERE build_id = $1";
const BAND_ROWS: &str = "SELECT count(*) FROM band_reference WHERE build_id = $1";

fn finalize_of(o: &ReferenceOutcome) -> Option<&FinalizeOutcome> {
    match o {
        ReferenceOutcome::Done { finalize, .. }
        | ReferenceOutcome::AlreadyFinished { finalize }
        | ReferenceOutcome::Skipped { finalize, .. }
        | ReferenceOutcome::Failed { finalize, .. } => finalize.as_ref(),
        ReferenceOutcome::Missing | ReferenceOutcome::NotBuilding { .. } => None,
    }
}

/// T21 review I1: only the sampled side of a reference game is a reference player. The
/// one-game opponent of a sampled player is analysed (enrichment needs both colours) but gives
/// no observation, so it never enters a cell.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn an_unsampled_opponent_contributes_nothing_to_a_cell(pool: PgPool) {
    let games: Vec<NewReferenceGame> = ten_games("u")
        .into_iter()
        .map(|g| NewReferenceGame {
            black_selected: false,
            ..g
        })
        .collect();
    let blacks: Vec<String> = games.iter().map(|g| g.black_hash.clone()).collect();
    let (build, ids) = build_with(&pool, "2026-09", &games).await;
    let d = deps(&pool, None);
    for id in &ids {
        let out = run(&d, *id).await;
        assert!(
            matches!(
                out,
                ReferenceOutcome::Done {
                    observations: 1,
                    ..
                }
            ),
            "{out:?}"
        );
    }
    let colours: Vec<String> =
        sqlx::query_scalar("SELECT color::text FROM reference_observations WHERE build_id = $1")
            .bind(build)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(colours, vec!["white".to_owned(); 10]);
    let leaked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM reference_observations WHERE build_id = $1 \
         AND player_hash = ANY($2)",
    )
    .bind(build)
    .bind(&blacks)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(leaked, 0);
    let w1: (i32, i32) = sqlx::query_as(
        "SELECT n_players, n_games FROM band_reference \
         WHERE build_id = $1 AND band = 'B2' AND time_class = 'blitz' AND metric = 'weakness:W1' \
         AND side = 'both'",
    )
    .bind(build)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(w1, (10, 10), "only the ten sampled white players");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn ten_reference_games_give_twenty_observations_and_an_active_build(pool: PgPool) {
    let (build, ids) = build_with(&pool, "2026-09", &ten_games("a")).await;
    let d = deps(&pool, None);
    for (i, id) in ids.iter().enumerate() {
        let out = run(&d, *id).await;
        assert!(
            matches!(
                out,
                ReferenceOutcome::Done {
                    observations: 2,
                    ..
                }
            ),
            "game {i}: {out:?}"
        );
        let fin = finalize_of(&out);
        if i + 1 < ids.len() {
            assert_eq!(fin, None, "no finalisation while games are outstanding");
            assert_eq!(status(&pool, build).await, BuildStatus::Building);
        } else {
            assert!(
                matches!(fin, Some(FinalizeOutcome::Activated { .. })),
                "last game finalises: {fin:?}"
            );
        }
    }
    assert_eq!(count(&pool, OBSERVATIONS, build).await, 20);
    let colours: Vec<(String, i64)> = sqlx::query_as(
        "SELECT color::text, count(*) FROM reference_observations WHERE build_id = $1 \
         GROUP BY color ORDER BY color",
    )
    .bind(build)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        colours,
        [("black".to_owned(), 10), ("white".to_owned(), 10)]
    );

    let w1: (i32, i32, bool) = sqlx::query_as(
        "SELECT n_players, n_games, low_sample FROM band_reference \
         WHERE build_id = $1 AND band = 'B2' AND time_class = 'blitz' AND metric = 'weakness:W1' \
         AND side = 'both'",
    )
    .bind(build)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(w1, (20, 20, false));
    assert_eq!(status(&pool, build).await, BuildStatus::Active);

    let loaded = load_band_reference(&pool, Band::B2, TimeClass::Blitz)
        .await
        .unwrap()
        .expect("the active build serves B2 blitz");
    assert_eq!(loaded.build_id, build);
    assert_eq!(loaded.served_from(W1), Some(Band::B2));

    let metrics_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job_metrics WHERE kind = $1 AND game_id = ANY($2)",
    )
    .bind(REFERENCE_JOB_KIND)
    .bind(&ids)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(metrics_rows, 10, "one job_metrics row per reference game");

    // A duplicate delivery after activation does nothing.
    let again = run(&d, ids[0]).await;
    assert!(
        matches!(again, ReferenceOutcome::NotBuilding { .. }),
        "{again:?}"
    );
    assert_eq!(count(&pool, OBSERVATIONS, build).await, 20);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_second_build_supersedes_the_first(pool: PgPool) {
    let d = deps(&pool, None);
    let (first, first_ids) = build_with(&pool, "2026-08", &ten_games("a")).await;
    for id in &first_ids {
        run(&d, *id).await;
    }
    assert_eq!(status(&pool, first).await, BuildStatus::Active);

    let (second, second_ids) = build_with(&pool, "2026-09", &ten_games("b")).await;
    for id in &second_ids {
        run(&d, *id).await;
    }
    assert_eq!(status(&pool, second).await, BuildStatus::Active);
    assert_eq!(status(&pool, first).await, BuildStatus::Superseded);
    assert_eq!(count(&pool, BAND_ROWS, first).await, 0);
    assert_eq!(count(&pool, OBSERVATIONS, first).await, 0);
    let loaded = load_band_reference(&pool, Band::B2, TimeClass::Blitz)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.build_id, second);

    // A queued job of the superseded build finds no row: a no-op, never an error.
    let stale = run(&d, first_ids[3]).await;
    assert_eq!(stale, ReferenceOutcome::Missing);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn reference_rows_keep_the_difficulty_model(pool: PgPool) {
    let maia = maia(true);
    let d = deps(&pool, Some(maia));
    let (build, ids) = build_with(&pool, "2026-09", &ten_games("m")).await;
    for id in &ids {
        run(&d, *id).await;
    }
    let row = get_build(&pool, build).await.unwrap().unwrap();
    assert_eq!(row.status, BuildStatus::Active);
    assert_eq!(row.difficulty_model.as_deref(), Some(MAIA_MODEL));
}

const W11: MetricId = MetricId::Weakness(WeaknessKey::W11);
const FLAGGED: &str = "SELECT count(*) FROM reference_games WHERE build_id = $1 AND difficulty_pending_at IS NOT NULL";

/// Total W11 (easy-move errors, Maia `p_ok` based) opportunities over the build's observations.
async fn w11_opportunities(pool: &PgPool, build: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COALESCE(sum((metrics->'weakness'->'W11'->>'opportunities')::bigint), 0)::bigint \
         FROM reference_observations WHERE build_id = $1",
    )
    .bind(build)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn w11_row(pool: &PgPool, build: Uuid) -> Option<i32> {
    sqlx::query_scalar(
        "SELECT n_players FROM band_reference WHERE build_id = $1 AND band = 'B2' \
         AND time_class = 'blitz' AND metric = $2 AND side = 'both'",
    )
    .bind(build)
    .bind(W11.to_string())
    .fetch_optional(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_build_waits_for_maia_and_finalising_before_it_returns_excludes_pending_moves(
    pool: PgPool,
) {
    // Maia configured but down: difficulty is pending, never estimated, and nothing is recorded.
    let maia = maia(false);
    let d = deps(&pool, Some(maia));
    let (build, ids) = build_with(&pool, "2026-09", &ten_games("n")).await;
    let mut last = None;
    for id in &ids {
        last = Some(run(&d, *id).await);
    }
    // Within the grace the build waits for Maia instead of freezing pending moves into it.
    let fin = finalize_of(last.as_ref().unwrap()).cloned();
    assert_eq!(fin, Some(FinalizeOutcome::WaitingForMaia { games: 10 }));
    assert_eq!(status(&pool, build).await, BuildStatus::Building);
    assert_eq!(count(&pool, FLAGGED, build).await, 10);
    assert!(MAIA_GRACE >= Duration::from_secs(3600));

    // Finalised before Maia returns (grace over): pending moves are out of W11's n.
    let out = finalize_band_reference_with(
        &pool,
        build,
        &FinalizeOptions {
            maia_grace: Duration::ZERO,
        },
    )
    .await
    .unwrap();
    assert!(
        matches!(
            out,
            FinalizeOutcome::Activated {
                maia_pending_games: 10,
                ..
            }
        ),
        "{out:?}"
    );
    assert_eq!(w11_opportunities(&pool, build).await, 0);
    assert_eq!(
        w11_row(&pool, build).await,
        None,
        "no W11 evidence, so no W11 row"
    );
    let row = get_build(&pool, build).await.unwrap().unwrap();
    assert_eq!(row.status, BuildStatus::Active);
    assert_eq!(row.difficulty_model, None);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn reference_games_are_re_enriched_once_maia_returns(pool: PgPool) {
    let maia = maia(false);
    let d = deps(&pool, Some(maia.clone()));
    let (build, ids) = build_with(&pool, "2026-09", &ten_games("r")).await;
    for id in &ids {
        run(&d, *id).await;
    }
    assert_eq!(w11_opportunities(&pool, build).await, 0);

    // Still down: the sweep changes nothing.
    let report = within(
        "sweep",
        reenrich_reference_games(&d, 100, &CancellationToken::new()),
    )
    .await
    .unwrap();
    assert!(!report.maia_healthy);
    assert_eq!(report.resolved, 0);
    assert_eq!(count(&pool, FLAGGED, build).await, 10);
    assert_eq!(status(&pool, build).await, BuildStatus::Building);

    // Maia is back: every game is re-enriched (pages of 3 until none is left), then the build
    // is finalised with W11 evidence.
    maia.up.store(true, Ordering::SeqCst);
    let report = within(
        "sweep",
        reenrich_reference_games(&d, 3, &CancellationToken::new()),
    )
    .await
    .unwrap();
    assert!(report.maia_healthy);
    assert_eq!((report.resolved, report.still_pending), (10, 0));
    assert_eq!(count(&pool, FLAGGED, build).await, 0);
    assert_eq!(count(&pool, OBSERVATIONS, build).await, 20);
    // 3 eligible plies past ply 10 per side.
    assert_eq!(w11_opportunities(&pool, build).await, 60);
    assert!(
        report
            .finalized
            .iter()
            .any(|(b, f)| *b == build && matches!(f, FinalizeOutcome::Activated { .. })),
        "{report:?}"
    );
    let row = get_build(&pool, build).await.unwrap().unwrap();
    assert_eq!(row.status, BuildStatus::Active);
    assert_eq!(row.difficulty_model.as_deref(), Some(MAIA_MODEL));
    assert_eq!(w11_row(&pool, build).await, Some(20));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn the_wait_for_maia_ends_after_the_grace(pool: PgPool) {
    let maia = maia(false);
    let d = deps(&pool, Some(maia));
    let (build, ids) = build_with(&pool, "2026-09", &ten_games("e")).await;
    for id in &ids {
        run(&d, *id).await;
    }
    assert_eq!(status(&pool, build).await, BuildStatus::Building);
    sqlx::query(
        "UPDATE reference_games SET difficulty_pending_at = now() - interval '25 hours' \
         WHERE build_id = $1",
    )
    .bind(build)
    .execute(&pool)
    .await
    .unwrap();
    // Maia stays down; the recurring sweep still finalises the expired build.
    let report = within(
        "sweep",
        reenrich_reference_games(&d, 100, &CancellationToken::new()),
    )
    .await
    .unwrap();
    assert!(!report.maia_healthy);
    assert_eq!(status(&pool, build).await, BuildStatus::Active);
    assert_eq!(w11_opportunities(&pool, build).await, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn an_unknown_slope_is_stored_null_and_nan_is_refused(pool: PgPool) {
    let (build, _) = build_with(&pool, "2026-09", &[]).await;
    let sides: Vec<_> = (0..12)
        .map(|i| accuracy_side(&format!("p{i}"), 1500, 1600, 60.0 + f64::from(i)))
        .collect();
    let rows = aggregate_band_reference(&sides);
    let mut tx = pool.begin().await.unwrap();
    refdata::band_reference::write_band_reference(&mut tx, build, &rows)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let null: bool = sqlx::query_scalar(
        "SELECT opp_slope_per_100 IS NULL FROM band_reference WHERE build_id = $1 AND metric = $2 \
         AND side = 'both'",
    )
    .bind(build)
    .bind(ACCURACY.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(null, "an unknown slope is NULL");
    for bad in ["NaN", "Infinity", "-Infinity"] {
        let res = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE band_reference SET opp_slope_per_100 = '{bad}'::real WHERE build_id = $1"
        )))
        .bind(build)
        .execute(&pool)
        .await;
        assert!(res.is_err(), "{bad} must be refused");
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_second_difficulty_model_fails_the_game_not_the_build(pool: PgPool) {
    let maia = maia(true);
    let d = deps(&pool, Some(maia.clone()));
    let mut games = ten_games("x");
    games.push(reference_game("extra", GAMES[0]));
    let (build, ids) = build_with(&pool, "2026-09", &games).await;
    for id in &ids[..10] {
        run(&d, *id).await;
    }
    assert_eq!(status(&pool, build).await, BuildStatus::Building);
    // Maia is redeployed with another model before the last game.
    maia.other_model.store(true, Ordering::SeqCst);
    let out = run(&d, ids[10]).await;
    assert!(
        matches!(&out, ReferenceOutcome::Failed { reason, .. } if reason.contains("difficulty model")),
        "{out:?}"
    );
    // The other games still finalise the build, with the first model.
    assert!(
        matches!(finalize_of(&out), Some(FinalizeOutcome::Activated { .. })),
        "{out:?}"
    );
    assert_eq!(count(&pool, OBSERVATIONS, build).await, 20);
    let failed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM reference_games WHERE build_id = $1 AND status = 'failed'",
    )
    .bind(build)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(failed, 1);
    let row = get_build(&pool, build).await.unwrap().unwrap();
    assert_eq!(row.status, BuildStatus::Active);
    assert_eq!(row.difficulty_model.as_deref(), Some(MAIA_MODEL));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn finalize_runs_once(pool: PgPool) {
    let d = deps(&pool, None);
    let (build, ids) = build_with(&pool, "2026-09", &ten_games("c")).await;
    for id in &ids[..8] {
        run(&d, *id).await;
    }
    // Two workers finish the last two games at the same time.
    let (a, b) = tokio::join!(run(&d, ids[8]), run(&d, ids[9]));
    let finals: Vec<_> = [&a, &b].into_iter().filter_map(finalize_of).collect();
    let activated = finals
        .iter()
        .filter(|f| matches!(f, FinalizeOutcome::Activated { .. }))
        .count();
    assert_eq!(
        activated, 1,
        "exactly one finaliser activates: {a:?} / {b:?}"
    );
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM reference_builds WHERE kind = 'band_reference' AND status = 'active'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active, 1);
    assert_eq!(status(&pool, build).await, BuildStatus::Active);
    let rows = count(&pool, BAND_ROWS, build).await;
    assert!(rows > 0);

    // Idempotent: finalising again changes nothing.
    let again = finalize_band_reference(&pool, build).await.unwrap();
    assert_eq!(again, FinalizeOutcome::AlreadyActive);
    assert_eq!(count(&pool, BAND_ROWS, build).await, rows);

    // Concurrent direct calls on an already active build are no-ops too.
    let (x, y) = tokio::join!(
        finalize_band_reference(&pool, build),
        finalize_band_reference(&pool, build)
    );
    assert_eq!(x.unwrap(), FinalizeOutcome::AlreadyActive);
    assert_eq!(y.unwrap(), FinalizeOutcome::AlreadyActive);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn concurrent_finalisers_of_a_finished_build_activate_it_once(pool: PgPool) {
    let d = deps(&pool, None);
    let (build, ids) = build_with(&pool, "2026-09", &ten_games("k")).await;
    // Hold the last game back so no handler finalises, then finish it by hand.
    for id in &ids[..9] {
        run(&d, *id).await;
    }
    let mut conn = pool.acquire().await.unwrap();
    set_reference_game_status(&mut conn, ids[9], db::games::AnalysisStatus::Skipped, None)
        .await
        .unwrap();
    drop(conn);
    let (x, y) = tokio::join!(
        finalize_band_reference(&pool, build),
        finalize_band_reference(&pool, build)
    );
    let (x, y) = (x.unwrap(), y.unwrap());
    let outcomes = [&x, &y];
    assert_eq!(
        outcomes
            .iter()
            .filter(|o| matches!(o, FinalizeOutcome::Activated { .. }))
            .count(),
        1,
        "{x:?} / {y:?}"
    );
    assert!(
        outcomes.contains(&&FinalizeOutcome::AlreadyActive),
        "{x:?} / {y:?}"
    );
    assert_eq!(status(&pool, build).await, BuildStatus::Active);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn finalize_waits_for_outstanding_games(pool: PgPool) {
    let d = deps(&pool, None);
    let (build, ids) = build_with(&pool, "2026-09", &ten_games("w")).await;
    run(&d, ids[0]).await;
    let out = finalize_band_reference(&pool, build).await.unwrap();
    assert_eq!(out, FinalizeOutcome::Pending { outstanding: 9 });
    assert_eq!(status(&pool, build).await, BuildStatus::Building);
    assert_eq!(count(&pool, BAND_ROWS, build).await, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_build_without_minimum_evidence_is_never_activated(pool: PgPool) {
    let d = deps(&pool, None);
    // A good active build first.
    let (good, good_ids) = build_with(&pool, "2026-08", &ten_games("g")).await;
    for id in &good_ids {
        run(&d, *id).await;
    }
    assert_eq!(status(&pool, good).await, BuildStatus::Active);
    let good_rows = count(&pool, BAND_ROWS, good).await;

    // A newer build with 2 games: 4 players per cell, every row low-sample.
    let (thin, thin_ids) = build_with(&pool, "2026-09", &ten_games("t")[..2]).await;
    let mut last = None;
    for id in &thin_ids {
        last = Some(run(&d, *id).await);
    }
    let fin = finalize_of(last.as_ref().unwrap()).cloned();
    assert!(
        matches!(fin, Some(FinalizeOutcome::InsufficientEvidence { .. })),
        "{fin:?}"
    );
    assert_eq!(status(&pool, thin).await, BuildStatus::Failed);
    assert_eq!(status(&pool, good).await, BuildStatus::Active);
    assert_eq!(count(&pool, BAND_ROWS, good).await, good_rows);
    let loaded = load_band_reference(&pool, Band::B2, TimeClass::Blitz)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.build_id, good);

    // Finalising the failed build again is a no-op.
    let again = finalize_band_reference(&pool, thin).await.unwrap();
    assert_eq!(
        again,
        FinalizeOutcome::NotBuilding {
            status: BuildStatus::Failed.as_str().to_owned()
        }
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_bad_game_fails_alone_and_does_not_block_finalisation(pool: PgPool) {
    let d = deps(&pool, None);
    let mut games = ten_games("p");
    // An illegal move: the game is poison (never retried into a stuck build).
    games.push(reference_game("bad", "1. e4 e5 2. Ke3 Nc6 1/2-1/2"));
    let (build, ids) = build_with(&pool, "2026-09", &games).await;
    let bad = run(&d, ids[10]).await;
    assert!(matches!(bad, ReferenceOutcome::Failed { .. }), "{bad:?}");
    for id in &ids[..10] {
        run(&d, *id).await;
    }
    assert_eq!(status(&pool, build).await, BuildStatus::Active);
    assert_eq!(count(&pool, OBSERVATIONS, build).await, 20);
    let failed: String =
        sqlx::query_scalar("SELECT status::text FROM reference_games WHERE id = $1")
            .bind(ids[10])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(failed, "failed");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn stale_observations_are_left_out_of_the_build(pool: PgPool) {
    let d = deps(&pool, None);
    let mut games = ten_games("s");
    games.push(reference_game("stale", GAMES[1]));
    let (build, ids) = build_with(&pool, "2026-09", &games).await;
    // The 11th game holds an observation written by an older metrics version.
    let mut stale = PlayerGameMetrics {
        version: PLAYER_GAME_METRICS_VERSION - 1,
        moves: 8,
        ..PlayerGameMetrics::default()
    };
    stale.weakness.insert(
        WeaknessKey::W1,
        BinomialObs {
            events: 8,
            opportunities: 8,
            epl: 4.0,
            exemplar_plies: vec![],
        },
    );
    let mut tx = pool.begin().await.unwrap();
    write_observations(
        &mut tx,
        build,
        &[ObservationRow {
            reference_game_id: ids[10],
            color: Color::White,
            player_hash: hash("stale-player"),
            band: Band::B2,
            time_class: TimeClass::Blitz,
            player_rating: 1500,
            opponent_rating: 1550,
            metrics: stale,
        }],
    )
    .await
    .unwrap();
    set_reference_game_status(&mut tx, ids[10], db::games::AnalysisStatus::Done, None)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let mut last = None;
    for id in &ids[..10] {
        last = Some(run(&d, *id).await);
    }
    let fin = finalize_of(last.as_ref().unwrap()).cloned();
    assert!(
        matches!(
            fin,
            Some(FinalizeOutcome::Activated {
                stale_observations: 1,
                ..
            })
        ),
        "{fin:?}"
    );
    let (n_players, pooled): (i32, Option<f64>) = sqlx::query_as(
        "SELECT n_players, pooled_rate::float8 FROM band_reference \
         WHERE build_id = $1 AND band = 'B2' AND time_class = 'blitz' AND metric = 'weakness:W1' \
         AND side = 'both'",
    )
    .bind(build)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n_players, 20, "the stale side is not evidence");
    assert_eq!(pooled, Some(0.0), "its 8 events are not counted");
}

// =============================================================================================
// Fix round 2 (review I1, I2, m4)

#[test]
fn only_a_usable_weakness_row_is_minimum_evidence() {
    // Twelve players give a usable accuracy row, but the only weakness row has 3 players.
    let mut sides: Vec<_> = (0..12)
        .map(|i| accuracy_side(&format!("a{i}"), 1500, 1500, 60.0))
        .collect();
    sides.extend((0..3).flat_map(|p| w1_player(&format!("w{p}"), 6, 300, 1)));
    let rows = aggregate_band_reference(&sides);
    assert!(!row(&rows, Band::B2, ACCURACY).stat.low_sample);
    assert!(row(&rows, Band::B2, W1).stat.low_sample);
    assert!(!has_minimum_evidence(&rows));
}

async fn analyse_all(d: &ReferenceGameDeps, ids: &[Uuid]) -> Option<ReferenceOutcome> {
    let mut last = None;
    for id in ids {
        last = Some(run(d, *id).await);
    }
    last
}

async fn usable(pool: &PgPool, build: Uuid) -> i64 {
    count(
        pool,
        "SELECT count(*) FROM band_reference WHERE build_id = $1 AND side = 'both' \
         AND NOT low_sample",
        build,
    )
    .await
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_thinner_build_never_supersedes_a_fuller_one(pool: PgPool) {
    let d = deps(&pool, None);
    // The active build covers blitz and rapid.
    let mut full = ten_games("fb");
    full.extend(ten_games_in("fr", TimeClass::Rapid));
    let (active, ids) = build_with(&pool, "2026-08", &full).await;
    analyse_all(&d, &ids).await;
    assert_eq!(status(&pool, active).await, BuildStatus::Active);
    let active_rows = count(&pool, BAND_ROWS, active).await;
    let active_usable = usable(&pool, active).await;

    // A newer build with only the blitz cell healthy passes the evidence gate but is thinner.
    let (thin, ids) = build_with(&pool, "2026-09", &ten_games("tb")).await;
    let last = analyse_all(&d, &ids).await.unwrap();
    let fin = finalize_of(&last).cloned();
    match fin {
        Some(FinalizeOutcome::ThinnerThanActive {
            active_build,
            usable_cells,
            active_usable_cells,
        }) => {
            assert_eq!(active_build, active);
            assert_eq!(active_usable_cells, active_usable);
            assert!(usable_cells > 0 && usable_cells < active_usable_cells);
        }
        other => panic!("expected ThinnerThanActive, got {other:?}"),
    }
    let thin_row = get_build(&pool, thin).await.unwrap().unwrap();
    assert_eq!(thin_row.status, BuildStatus::Failed);
    let error = thin_row.error.unwrap_or_default();
    assert!(
        error.contains(&format!("thinner than active build {active}"))
            && error.contains("usable cells"),
        "{error}"
    );
    // The active build is untouched and still served for both time classes.
    assert_eq!(status(&pool, active).await, BuildStatus::Active);
    assert_eq!(count(&pool, BAND_ROWS, active).await, active_rows);
    for tc in [TimeClass::Blitz, TimeClass::Rapid] {
        let loaded = load_band_reference(&pool, Band::B2, tc)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.build_id, active);
    }

    // An equally covering build supersedes it.
    let mut equal = ten_games("eb");
    equal.extend(ten_games_in("er", TimeClass::Rapid));
    let (same, ids) = build_with(&pool, "2026-10", &equal).await;
    analyse_all(&d, &ids).await;
    assert_eq!(status(&pool, same).await, BuildStatus::Active);
    assert_eq!(status(&pool, active).await, BuildStatus::Superseded);
    assert_eq!(usable(&pool, same).await, active_usable);

    // A larger one does too.
    let mut larger = ten_games("lb");
    larger.extend(ten_games_in("lr", TimeClass::Rapid));
    larger.extend(ten_games_in("lc", TimeClass::Classical));
    let (big, ids) = build_with(&pool, "2026-11", &larger).await;
    analyse_all(&d, &ids).await;
    assert_eq!(status(&pool, big).await, BuildStatus::Active);
    assert_eq!(status(&pool, same).await, BuildStatus::Superseded);
    assert!(usable(&pool, big).await > active_usable);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn the_grace_starts_at_the_first_pending_time(pool: PgPool) {
    // Maia answers /healthz but every item times out: each sweep re-persists the games as still
    // pending. The wait must still end 24 h after the first pending time.
    let maia = maia(true);
    maia.timeout.store(true, Ordering::SeqCst);
    let d = deps(&pool, Some(maia));
    let (build, ids) = build_with(&pool, "2026-09", &ten_games("q")).await;
    let last = analyse_all(&d, &ids).await.unwrap();
    assert_eq!(
        finalize_of(&last).cloned(),
        Some(FinalizeOutcome::WaitingForMaia { games: 10 })
    );
    sqlx::query(
        "UPDATE reference_games SET difficulty_pending_at = now() - interval '25 hours' \
         WHERE build_id = $1",
    )
    .bind(build)
    .execute(&pool)
    .await
    .unwrap();
    let stamps = |pool: PgPool| async move {
        sqlx::query_scalar::<_, OffsetDateTime>(
            "SELECT difficulty_pending_at FROM reference_games WHERE build_id = $1 ORDER BY id",
        )
        .bind(build)
        .fetch_all(&pool)
        .await
        .unwrap()
    };
    let before = stamps(pool.clone()).await;

    let report = within(
        "sweep",
        reenrich_reference_games(&d, 100, &CancellationToken::new()),
    )
    .await
    .unwrap();
    assert!(report.maia_healthy);
    assert_eq!((report.resolved, report.still_pending), (0, 10));
    assert_eq!(
        stamps(pool.clone()).await,
        before,
        "the first pending time is kept"
    );
    assert!(
        report.finalized.iter().any(|(b, f)| *b == build
            && matches!(
                f,
                FinalizeOutcome::Activated {
                    maia_pending_games: 10,
                    ..
                }
            )),
        "{report:?}"
    );
    assert_eq!(status(&pool, build).await, BuildStatus::Active);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn observations_of_a_failed_game_are_not_aggregated(pool: PgPool) {
    // Review m1: a racing duplicate delivery can fail a game after its observations were stored.
    let d = deps(&pool, None);
    let mut games = ten_games("f");
    games.push(reference_game("failed", GAMES[2]));
    let (build, ids) = build_with(&pool, "2026-09", &games).await;
    let mut m = PlayerGameMetrics {
        version: PLAYER_GAME_METRICS_VERSION,
        moves: 8,
        ..PlayerGameMetrics::default()
    };
    m.weakness.insert(
        WeaknessKey::W1,
        BinomialObs {
            events: 8,
            opportunities: 8,
            epl: 4.0,
            exemplar_plies: vec![],
        },
    );
    let mut tx = pool.begin().await.unwrap();
    write_observations(
        &mut tx,
        build,
        &[ObservationRow {
            reference_game_id: ids[10],
            color: Color::White,
            player_hash: hash("failed-player"),
            band: Band::B2,
            time_class: TimeClass::Blitz,
            player_rating: 1500,
            opponent_rating: 1550,
            metrics: m,
        }],
    )
    .await
    .unwrap();
    set_reference_game_status(&mut tx, ids[10], db::games::AnalysisStatus::Failed, None)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    analyse_all(&d, &ids[..10]).await;
    assert_eq!(status(&pool, build).await, BuildStatus::Active);
    let (n_players, pooled): (i32, Option<f64>) = sqlx::query_as(
        "SELECT n_players, pooled_rate::float8 FROM band_reference \
         WHERE build_id = $1 AND band = 'B2' AND time_class = 'blitz' AND metric = 'weakness:W1' \
         AND side = 'both'",
    )
    .bind(build)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((n_players, pooled), (20, Some(0.0)));
}

// =============================================================================================
// Fix round 3 (the I1 locking gap, m2)

/// Analyses every game but the last, then marks the last one skipped: the build is finished,
/// but no handler has finalised it.
async fn finished_unfinalised(d: &ReferenceGameDeps, pool: &PgPool, ids: &[Uuid]) {
    let (last, rest) = ids.split_last().unwrap();
    analyse_all(d, rest).await;
    let mut conn = pool.acquire().await.unwrap();
    set_reference_game_status(&mut conn, *last, db::games::AnalysisStatus::Skipped, None)
        .await
        .unwrap();
}

/// Sessions of this test database waiting on an advisory lock.
async fn advisory_waiters(pool: &PgPool) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM pg_stat_activity \
         WHERE datname = current_database() AND wait_event_type = 'Lock' \
           AND wait_event = 'advisory'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn wait_for_waiters(pool: &PgPool, n: i64) {
    testkit::wait_for(
        "advisory lock waiters",
        Duration::from_secs(30),
        Duration::from_millis(20),
        || async {
            let w = advisory_waiters(pool).await;
            (w >= n).then_some(w)
        },
    )
    .await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn concurrent_finalisers_compare_against_each_other(pool: PgPool) {
    let d = deps(&pool, None);
    // X: an active blitz-only build.
    let (x, ids) = build_with(&pool, "2026-09", &ten_games("x")).await;
    analyse_all(&d, &ids).await;
    assert_eq!(status(&pool, x).await, BuildStatus::Active);
    // A (blitz + rapid) and a newer B (blitz only): each is at least as full as X, but B is
    // thinner than A.
    let mut full = ten_games("ab");
    full.extend(ten_games_in("ar", TimeClass::Rapid));
    let (a, a_ids) = build_with(&pool, "2026-10", &full).await;
    let (b, b_ids) = build_with(&pool, "2026-11", &ten_games("bb")).await;
    finished_unfinalised(&d, &pool, &a_ids).await;
    finished_unfinalised(&d, &pool, &b_ids).await;

    // Hold the activation's kind lock, so both finalisers get as far as they can before either
    // activates. Without one lock around the comparison and the activation, both compare
    // against X, and B (newer) then supersedes A.
    let mut holder = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('refdata:activate:band_reference'))")
        .execute(&mut *holder)
        .await
        .unwrap();
    let fa = tokio::spawn({
        let pool = pool.clone();
        async move { finalize_band_reference(&pool, a).await }
    });
    wait_for_waiters(&pool, 1).await;
    let fb = tokio::spawn({
        let pool = pool.clone();
        async move { finalize_band_reference(&pool, b).await }
    });
    wait_for_waiters(&pool, 2).await;
    holder.rollback().await.unwrap();
    let (ra, rb) = within("finalisers", async {
        (fa.await.unwrap(), fb.await.unwrap())
    })
    .await;
    let (ra, rb) = (ra.unwrap(), rb.unwrap());

    assert!(matches!(ra, FinalizeOutcome::Activated { .. }), "A: {ra:?}");
    assert!(
        matches!(rb, FinalizeOutcome::ThinnerThanActive { active_build, .. } if active_build == a),
        "B: {rb:?}"
    );
    assert_eq!(status(&pool, a).await, BuildStatus::Active);
    assert_eq!(status(&pool, b).await, BuildStatus::Failed);
    assert_eq!(status(&pool, x).await, BuildStatus::Superseded);
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM reference_builds WHERE kind = 'band_reference' AND status = 'active'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn an_older_dump_is_failed_not_retried(pool: PgPool) {
    let d = deps(&pool, None);
    let (newer, ids) = build_with(&pool, "2026-10", &ten_games("n")).await;
    analyse_all(&d, &ids).await;
    assert_eq!(status(&pool, newer).await, BuildStatus::Active);
    let (older, ids) = build_with(&pool, "2026-09", &ten_games("o")).await;
    let last = analyse_all(&d, &ids).await.unwrap();
    assert_eq!(
        finalize_of(&last).cloned(),
        Some(FinalizeOutcome::OlderThanActive {
            month: "2026-09".to_owned(),
            active_month: "2026-10".to_owned(),
        })
    );
    assert_eq!(status(&pool, older).await, BuildStatus::Failed);
    assert_eq!(status(&pool, newer).await, BuildStatus::Active);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn one_finaliser_error_never_stops_the_sweep(pool: PgPool) {
    // Review m2: two finished builds, the first (by start time) fails to write its rows.
    let d = deps(&pool, None);
    let (bad, bad_ids) = build_with(&pool, "2026-09", &ten_games("bad")).await;
    let (good, good_ids) = build_with(&pool, "2026-10", &ten_games("good")).await;
    finished_unfinalised(&d, &pool, &bad_ids).await;
    finished_unfinalised(&d, &pool, &good_ids).await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "CREATE FUNCTION t32_refuse() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected failure'; END $$; \
         CREATE TRIGGER t32_refuse BEFORE INSERT ON band_reference FOR EACH ROW \
           WHEN (NEW.build_id = '{bad}') EXECUTE FUNCTION t32_refuse();"
    )))
    .execute(&pool)
    .await
    .unwrap();
    let report = within(
        "sweep",
        reenrich_reference_games(&d, 100, &CancellationToken::new()),
    )
    .await
    .unwrap();
    assert_eq!(report.finalize_errors, 1, "{report:?}");
    assert!(
        report
            .finalized
            .iter()
            .any(|(b, f)| *b == good && matches!(f, FinalizeOutcome::Activated { .. })),
        "{report:?}"
    );
    assert_eq!(status(&pool, good).await, BuildStatus::Active);
    assert_eq!(
        status(&pool, bad).await,
        BuildStatus::Building,
        "retried by the next run"
    );
}

/// R2 Task 35 (T21 hand-off): `lichess-db` creates band-reference builds with NULL protocols (it
/// has no engine or enrich dependency). The finaliser, which does, records the analysis and
/// enrichment protocols the games were analysed under when it activates the build.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn activation_records_the_analysis_and_enrichment_protocols(pool: PgPool) {
    let (build, ids) = build_with(&pool, "2026-08", &ten_games("p")).await;
    // As lichess-db creates it.
    sqlx::query(
        "UPDATE reference_builds SET protocol = NULL, enrichment_protocol = NULL WHERE id = $1",
    )
    .bind(build)
    .execute(&pool)
    .await
    .unwrap();
    let before: (Option<String>, Option<String>) =
        sqlx::query_as("SELECT protocol, enrichment_protocol FROM reference_builds WHERE id = $1")
            .bind(build)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, (None, None), "lichess-db leaves them NULL");
    let d = deps(&pool, None);
    let last = analyse_all(&d, &ids).await.expect("games ran");
    assert!(
        matches!(finalize_of(&last), Some(FinalizeOutcome::Activated { .. })),
        "{last:?}"
    );
    let after: (Option<String>, Option<String>) =
        sqlx::query_as("SELECT protocol, enrichment_protocol FROM reference_builds WHERE id = $1")
            .bind(build)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        after,
        (
            Some(engine::ANALYSIS_PROTOCOL.to_owned()),
            Some("enr-v1".to_owned())
        )
    );
}
