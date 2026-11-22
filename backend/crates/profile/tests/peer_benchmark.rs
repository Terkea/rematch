//! The chess.com peer benchmark (ADR 0080): the opponent side of every analysed game is a
//! reference observation, on the account's own rating scale.

use chess_core::{Color, TimeClass};
use profile::peer_benchmark::{
    PEER_RATING_WINDOW, PeerSide, aggregate_peer_benchmark, is_peer_comparable, opponent_context,
};
use profile::reference_aggregate::LOW_SAMPLE_PLAYERS;
use profile_core::types::{
    BaseMetric, BinomialObs, GameContext, MetricId, PLAYER_GAME_METRICS_VERSION, PlatformId,
    PlayerGameMetrics, WeaknessKey,
};
use time::OffsetDateTime;
use uuid::Uuid;

const W5: MetricId = MetricId::Weakness(WeaknessKey::W5);
const W1: MetricId = MetricId::Weakness(WeaknessKey::W1);
const ACCURACY: MetricId = MetricId::Base(BaseMetric::Accuracy);

fn owner_ctx(color: Color, result: &str) -> GameContext {
    GameContext {
        game_id: Uuid::new_v4(),
        platform_game_id: "cc1".to_owned(),
        platform: PlatformId::Chesscom,
        played_at: OffsetDateTime::UNIX_EPOCH,
        time_class: TimeClass::Rapid,
        base_seconds: Some(600),
        increment_seconds: Some(0),
        rated: true,
        player_color: color,
        // Stored (post-game) ratings: the pre-game ones come from the metrics.
        player_rating: Some(812),
        opponent_rating: Some(790),
        result: result.to_owned(),
        score: Some(if result == "1-0" { 1.0 } else { 0.0 }),
        termination: None,
        accuracy: Some(71.0),
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

/// One game of `opponent` rated `rating`, with W5, W1 and accuracy observed.
fn side(opponent: &str, rating: i32, w5_events: u32, accuracy: f64) -> PeerSide {
    let mut m = metrics();
    m.accuracy = Some(accuracy);
    m.weakness.insert(
        WeaknessKey::W5,
        BinomialObs {
            events: w5_events,
            opportunities: 1,
            epl: 0.5 * f64::from(w5_events),
            exemplar_plies: vec![],
        },
    );
    m.weakness.insert(
        WeaknessKey::W1,
        BinomialObs {
            events: 0,
            opportunities: 20,
            epl: 0.0,
            exemplar_plies: vec![],
        },
    );
    let mut ctx = owner_ctx(Color::Black, "1-0");
    ctx.player_rating = Some(rating);
    ctx.opponent_rating = Some(800);
    PeerSide {
        opponent: opponent.to_owned(),
        opponent_rating: rating,
        player_rating: 800,
        metrics: m,
        ctx,
    }
}

fn players(n: usize, rating: i32) -> Vec<PeerSide> {
    (0..n)
        .map(|i| side(&format!("opp{i}"), rating, u32::from(i % 2 == 0), 60.0))
        .collect()
}

#[test]
fn opponent_context_is_the_other_colour_with_swapped_pre_game_ratings() {
    let owner = owner_ctx(Color::White, "1-0");
    let opp = opponent_context(&owner, (800, 780), Some(55.0));
    assert_eq!(opp.player_color, Color::Black);
    assert_eq!(opp.player_rating, Some(780));
    assert_eq!(opp.opponent_rating, Some(800));
    assert_eq!(opp.score, Some(0.0));
    assert_eq!(opp.accuracy, Some(55.0));
    assert_eq!(opp.result, "1-0");
    assert_eq!(opp.game_id, owner.game_id);
}

#[test]
fn only_metrics_the_unenriched_side_can_measure_are_compared() {
    // Decided from the main evals, the clock and the result alone.
    for m in [W5, ACCURACY, MetricId::Weakness(WeaknessKey::W9a)] {
        assert!(is_peer_comparable(m), "{m}");
    }
    // Need the enrichment (refutation, threat, best line, tablebase, Maia), which only the
    // player's side has: a missing line would read as a non-event.
    for m in [
        W1,
        MetricId::Weakness(WeaknessKey::W3Shallow),
        MetricId::Weakness(WeaknessKey::W10),
        MetricId::Base(BaseMetric::OneMoveBlundersPerGame),
        MetricId::Base(BaseMetric::SharpErrorRate),
    ] {
        assert!(!is_peer_comparable(m), "{m}");
    }
}

#[test]
fn the_window_is_the_player_rating_plus_or_minus_150() {
    let mut sides = players(12, 800);
    sides.push(side("far-above", 800 + PEER_RATING_WINDOW + 1, 1, 60.0));
    sides.push(side("far-below", 800 - PEER_RATING_WINDOW - 1, 1, 60.0));
    let b = aggregate_peer_benchmark(&sides, 800);
    assert_eq!((b.rating_low, b.rating_high), (650, 950));
    assert_eq!(b.n_players, 12);
    assert_eq!(b.n_games, 12);
}

#[test]
fn enrichment_dependent_rows_are_never_produced() {
    let b = aggregate_peer_benchmark(&players(12, 800), 800);
    assert!(b.stats.iter().any(|s| s.metric == W5));
    assert!(b.stats.iter().all(|s| s.metric != W1), "{:#?}", b.stats);
}

#[test]
fn players_are_distinct_opponents_and_low_sample_follows_them() {
    // Nine opponents, each faced three times: 27 games but nine players.
    let mut sides = Vec::new();
    for i in 0..9 {
        for _ in 0..3 {
            sides.push(side(&format!("opp{i}"), 800, 1, 60.0));
        }
    }
    let b = aggregate_peer_benchmark(&sides, 800);
    let w5 = b.stats.iter().find(|s| s.metric == W5).unwrap();
    assert_eq!(w5.n_players, 9);
    assert_eq!(w5.n_games, 27);
    assert!(w5.low_sample, "9 < {LOW_SAMPLE_PLAYERS}");
    assert!(b.reference().is_none(), "a low-sample row is never served");

    sides.push(side("opp9", 800, 1, 60.0));
    let b = aggregate_peer_benchmark(&sides, 800);
    let reference = b.reference().expect("ten players");
    assert!(!reference[&W5].low_sample);
}

#[test]
fn one_frequent_opponent_weighs_as_much_as_any_other() {
    // Ten opponents never fail to convert; one faced 40 times always does. Pooled by games it
    // would be 40/50; with each opponent weighted once it is 1/11.
    let mut sides: Vec<PeerSide> = (0..10)
        .map(|i| side(&format!("opp{i}"), 800, 0, 60.0))
        .collect();
    for _ in 0..40 {
        sides.push(side("regular", 800, 1, 90.0));
    }
    let b = aggregate_peer_benchmark(&sides, 800);
    let w5 = b.stats.iter().find(|s| s.metric == W5).unwrap();
    let pooled = w5.pooled_rate.unwrap();
    assert!((pooled - 1.0 / 11.0).abs() < 1e-9, "{pooled}");
    assert!((w5.opps_per_game.unwrap() - 1.0).abs() < 1e-9);
    assert!((w5.epl_per_event.unwrap() - 0.5).abs() < 1e-9);
    // Means are per opponent too: the median of eleven per-player means is 60.
    let acc = b.stats.iter().find(|s| s.metric == ACCURACY).unwrap();
    assert!((acc.median - 60.0).abs() < 1e-9, "{}", acc.median);
    assert_eq!(acc.n_players, 11);
}

#[test]
fn no_sides_give_an_empty_benchmark() {
    let b = aggregate_peer_benchmark(&[], 800);
    assert_eq!(b.n_players, 0);
    assert!(b.stats.is_empty());
    assert!(b.reference().is_none());
}

// =============================================================================================
// Storage and build-profile

mod common {
    pub mod moves;
    pub mod seed;
}

use common::moves::insert_evals;
use common::seed::{PROTOCOL, SEED_NOW, seed_now};
use db::profile_rows::{
    GameMetricsRow, SeriesPointRow, insert_game_metrics, replace_series_points,
};
use profile::build_profile::{BuildProfileDeps, handle_build_profile};
use profile::peer_benchmark::{BenchmarkSource, load_account_benchmark, refresh_opponent_metrics};
use queue::BuildProfilePayload;
use queue::queue::Queue;
use sqlx::PgPool;
use testkit::{GameSeed, insert_account, insert_game};

/// Player metrics of one rapid game: rated 800 against 800, a W5 opportunity with `w5_events`.
fn owner_metrics(w5_events: u32) -> PlayerGameMetrics {
    let mut m = metrics();
    m.player_rating = Some(800);
    m.opponent_rating = Some(800);
    m.score = Some(0.0);
    m.expected_score = Some(0.5);
    m.accuracy = Some(70.0);
    m.weakness.insert(
        WeaknessKey::W5,
        BinomialObs {
            events: w5_events,
            opportunities: 1,
            epl: 0.5 * f64::from(w5_events),
            exemplar_plies: vec![],
        },
    );
    m
}

/// A rapid chess.com game against `opponent`, analysed, with the player's metrics and W5 point.
async fn analysed_game(pool: &PgPool, account: Uuid, opponent: &str, i: i64) -> Uuid {
    let game = insert_game(
        pool,
        account,
        GameSeed {
            black: opponent.to_owned(),
            base_seconds: Some(600),
            increment_seconds: Some(0),
            ..GameSeed::default()
        },
    )
    .await;
    let at = SEED_NOW - time::Duration::days(30) + time::Duration::hours(i);
    sqlx::query(
        "UPDATE games SET played_at = $2, analysis_status = 'done', analysed_at = $2, \
         enrichment_protocol = 'enr-v1' WHERE id = $1",
    )
    .bind(game)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    insert_game_metrics(
        &mut tx,
        &GameMetricsRow {
            game_id: game,
            account_id: account,
            time_class: "rapid".to_owned(),
            played_at: at,
            protocol: PROTOCOL.to_owned(),
            enrichment_protocol: Some("enr-v1".to_owned()),
            difficulty_model: None,
            metrics: serde_json::to_value(owner_metrics(1)).unwrap(),
        },
    )
    .await
    .unwrap();
    replace_series_points(
        &mut tx,
        game,
        &[SeriesPointRow {
            account_id: account,
            time_class: "rapid".to_owned(),
            series_key: "W5".to_owned(),
            played_at: at,
            events: 1,
            opportunities: 1,
            epl: 0.5,
            protocol: PROTOCOL.to_owned(),
        }],
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    game
}

/// The opponent side of `game` as a refresh stores it: rated `rating`, W5 with `w5` events.
async fn opponent_row(
    pool: &PgPool,
    account: Uuid,
    game: Uuid,
    opponent: &str,
    rating: i32,
    w5: u32,
) {
    let mut m = owner_metrics(w5);
    m.player_rating = Some(rating);
    sqlx::query(
        "INSERT INTO opponent_game_metrics (game_id, account_id, time_class, opponent, \
           opponent_rating, player_rating, metrics, source_created_at) \
         SELECT $1, $2, 'rapid', $3, $4, 800, $5, created_at FROM game_metrics \
         WHERE game_id = $1",
    )
    .bind(game)
    .bind(account)
    .bind(opponent)
    .bind(rating)
    .bind(serde_json::to_value(m).unwrap())
    .execute(pool)
    .await
    .unwrap();
}

async fn build_rapid(pool: &PgPool, account: Uuid) {
    let deps = BuildProfileDeps {
        pool: pool.clone(),
        queue: Queue::new(pool.clone()),
        now: seed_now,
        benchmark: BenchmarkSource::Peers,
    };
    handle_build_profile(
        &deps,
        BuildProfilePayload {
            account_id: account,
            time_class: "rapid".to_owned(),
        },
    )
    .await
    .expect("build-profile");
}

/// `players` opponents in the window, one game each, plus one far above it.
async fn peer_seed(pool: &PgPool, players: usize) -> Uuid {
    let account = insert_account(pool, "chesscom", "PeerHero").await;
    for i in 0..players {
        let name = format!("Opp{i}");
        let game = analysed_game(pool, account, &name, i as i64).await;
        let w5 = u32::from(i % 3 == 0);
        opponent_row(pool, account, game, &name.to_lowercase(), 780, w5).await;
    }
    let game = analysed_game(pool, account, "Strong", players as i64).await;
    opponent_row(pool, account, game, "strong", 1400, 0).await;
    account
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn build_profile_compares_with_the_opponents_faced(pool: PgPool) {
    let account = peer_seed(&pool, 12).await;
    build_rapid(&pool, account).await;

    let (source, low, high, players, games): (String, i32, i32, i32, i32) = sqlx::query_as(
        "SELECT source, rating_low, rating_high, n_players, n_games FROM account_benchmark \
         WHERE account_id = $1 AND time_class = 'rapid'",
    )
    .bind(account)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((source.as_str(), low, high), ("peers", 650, 950));
    assert_eq!((players, games), (12, 12), "the 1400 opponent is outside the window");

    type Row = (String, Option<f64>, Option<f64>);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT series_key, band_median, excess FROM weakness_estimates \
         WHERE account_id = $1 AND time_class = 'rapid' ORDER BY series_key",
    )
    .bind(account)
    .fetch_all(&pool)
    .await
    .unwrap();
    let w5 = rows.iter().find(|r| r.0 == "W5").unwrap();
    assert!(w5.1.is_some() && w5.2.is_some(), "W5 is compared: {w5:?}");
    let w1 = rows.iter().find(|r| r.0 == "W1").unwrap();
    assert_eq!((w1.1, w1.2), (None, None), "W1 needs enrichment: never compared");

    let served = load_account_benchmark(&pool, account, TimeClass::Rapid)
        .await
        .unwrap()
        .expect("served rows");
    assert!(served.contains_key(&W5));
    assert!(!served.contains_key(&W1));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn too_few_opponents_store_the_band_but_serve_nothing(pool: PgPool) {
    let account = peer_seed(&pool, 9).await;
    build_rapid(&pool, account).await;
    let players: i32 = sqlx::query_scalar(
        "SELECT n_players FROM account_benchmark WHERE account_id = $1 AND time_class = 'rapid'",
    )
    .bind(account)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(players, 9);
    assert!(
        load_account_benchmark(&pool, account, TimeClass::Rapid)
            .await
            .unwrap()
            .is_none()
    );
    let compared: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM weakness_estimates WHERE account_id = $1 AND excess IS NOT NULL",
    )
    .bind(account)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(compared, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn refresh_computes_the_opponent_side_from_cached_evals_once(pool: PgPool) {
    insert_evals(&pool).await;
    let account = insert_account(&pool, "chesscom", "PeerHero").await;
    // Scholar's mate, the player White against "Villain"; every position's eval is cached.
    let game = analysed_game(&pool, account, "Villain", 0).await;
    let r = refresh_opponent_metrics(&pool, account, TimeClass::Rapid)
        .await
        .unwrap();
    assert_eq!((r.computed, r.skipped), (1, 0));
    let (opponent, rating, own, version): (String, i32, i32, i32) = sqlx::query_as(
        "SELECT opponent, opponent_rating, player_rating, metrics_version \
         FROM opponent_game_metrics WHERE game_id = $1",
    )
    .bind(game)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((opponent.as_str(), rating, own), ("villain", 800, 800));
    assert_eq!(version as u32, PLAYER_GAME_METRICS_VERSION);
    let moves: i64 = sqlx::query_scalar(
        "SELECT (metrics->>'moves')::bigint FROM opponent_game_metrics WHERE game_id = $1",
    )
    .bind(game)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(moves > 0, "the opponent's moves were measured");

    let again = refresh_opponent_metrics(&pool, account, TimeClass::Rapid)
        .await
        .unwrap();
    assert_eq!((again.computed, again.skipped), (0, 0), "a current row is kept");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn refresh_skips_a_game_whose_evals_are_not_cached(pool: PgPool) {
    let account = insert_account(&pool, "chesscom", "PeerHero").await;
    analysed_game(&pool, account, "Villain", 0).await;
    let r = refresh_opponent_metrics(&pool, account, TimeClass::Rapid)
        .await
        .unwrap();
    assert_eq!((r.computed, r.skipped), (0, 1), "no engine runs");
}
