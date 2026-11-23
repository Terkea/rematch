//! The chess.com peer benchmark (ADR 0080): the opponent side of every analysed game is a
//! reference observation, on the account's own rating scale.

use chess_core::{Color, TimeClass};
use profile::peer_benchmark::{
    PEER_RATING_WINDOW, PeerSide, aggregate_peer_benchmark, is_peer_comparable, opponent_context,
    peer_sides_of,
};
use profile::reference_aggregate::LOW_SAMPLE_PLAYERS;
use profile_core::kpi::catalogue::kpi_catalogue;
use profile_core::types::{
    BaseMetric, BinomialObs, GameContext, MetricId, PLAYER_GAME_METRICS_VERSION, PlatformId,
    PlayerGameMetrics, Side, WeaknessKey,
};
use time::OffsetDateTime;
use uuid::Uuid;

const TRAPS: MetricId = MetricId::Weakness(WeaknessKey::W7Traps);
const MISSED: MetricId = MetricId::Weakness(WeaknessKey::W4Missed);
const W1: MetricId = MetricId::Weakness(WeaknessKey::W1);
const ACCURACY: MetricId = MetricId::Base(BaseMetric::Accuracy);
/// The build's time; the unit tests' games are played at it unless said otherwise.
const NOW: OffsetDateTime = OffsetDateTime::UNIX_EPOCH;

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

/// One game of `opponent` rated `rating`, with W7.traps, W1 and accuracy observed.
fn side(opponent: &str, rating: i32, traps_events: u32, accuracy: f64) -> PeerSide {
    let mut m = metrics();
    m.accuracy = Some(accuracy);
    m.weakness.insert(
        WeaknessKey::W7Traps,
        BinomialObs {
            events: traps_events,
            opportunities: 1,
            epl: 0.5 * f64::from(traps_events),
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
    for m in [
        TRAPS,
        MISSED,
        ACCURACY,
        MetricId::Weakness(WeaknessKey::W9a),
        MetricId::Weakness(WeaknessKey::W4Allowed),
    ] {
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
    let b = aggregate_peer_benchmark(&sides, 800, NOW);
    assert_eq!((b.rating_low, b.rating_high), (650, 950));
    assert_eq!(b.n_players, 12);
    assert_eq!(b.n_games, 12);
}

#[test]
fn enrichment_dependent_rows_are_never_produced() {
    let b = aggregate_peer_benchmark(&players(12, 800), 800, NOW);
    assert!(b.stats.iter().any(|s| s.metric == TRAPS));
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
    let b = aggregate_peer_benchmark(&sides, 800, NOW);
    let traps = b.stats.iter().find(|s| s.metric == TRAPS).unwrap();
    assert_eq!(traps.n_players, 9);
    assert_eq!(traps.n_games, 27);
    assert!(traps.low_sample, "9 < {LOW_SAMPLE_PLAYERS}");
    assert!(b.reference().is_none(), "a low-sample row is never served");

    // A tenth opponent, faced 13 times: ten players and 40 chances (W7.traps' minimum).
    for _ in 0..13 {
        sides.push(side("opp9", 800, 1, 60.0));
    }
    let b = aggregate_peer_benchmark(&sides, 800, NOW);
    let reference = b.reference().expect("ten players");
    assert!(!reference[&TRAPS].low_sample);
}

#[test]
fn a_rate_row_needs_its_minimum_chances_as_well_as_ten_players() {
    // A12: twelve opponents but 12 W4.missed chances, under the KPI's own minimum of 40 (the
    // number of chances the player's own estimate needs): stored, never served.
    let min = kpi_catalogue()
        .iter()
        .find(|d| d.metric == MISSED)
        .unwrap()
        .min_n;
    assert_eq!(min, 40);
    let sides: Vec<PeerSide> = (0..12)
        .map(|i| missed_side(&format!("opp{i}"), u32::from(i % 4 == 0), 1, 0))
        .collect();
    let b = aggregate_peer_benchmark(&sides, 800, NOW);
    let r = b.stats.iter().find(|s| s.metric == MISSED).unwrap();
    assert_eq!((r.n_players, b.chances[&MISSED]), (12, 12));
    assert!(r.low_sample, "12 chances < {min}");
    assert!(!b.reference().unwrap().contains_key(&MISSED));
    // A mean row keeps the player rule: accuracy has twelve players and is served.
    assert!(!b.reference().unwrap()[&ACCURACY].low_sample);

    // The same twelve opponents with 4 chances each: 48 chances, served.
    let sides: Vec<PeerSide> = (0..12)
        .map(|i| missed_side(&format!("opp{i}"), u32::from(i % 4 == 0), 4, 0))
        .collect();
    let b = aggregate_peer_benchmark(&sides, 800, NOW);
    assert_eq!(b.chances[&MISSED], 48);
    assert!(!b.reference().unwrap()[&MISSED].low_sample);
}

/// One game against the player: `opponent` had `color`, W4.missed `k` of `n`.
fn coloured_missed_side(opponent: &str, color: Color, k: u32, n: u32) -> PeerSide {
    let mut s = missed_side(opponent, k, n, 0);
    s.ctx.player_color = color;
    s
}

#[test]
fn both_weighs_an_opponent_once_however_many_colours_they_had() {
    // A13 (the owner's rapid `losses_on_time_rate`: Both 0.0016, White 0.0063, Black 0): the
    // only event is by an opponent faced 8 times, 4 with each colour. Each opponent weighs one
    // game's worth in each pool (ADR 0081): in White their event is 1/4 of a unit, in Both 1/8,
    // and Both has about twice White's units. So Both is not the chance-weighted mean of the
    // colours; that is the per-opponent weighting, not a different denominator or decay.
    let mut sides = Vec::new();
    for i in 0..10 {
        sides.push(coloured_missed_side(&format!("w{i}"), Color::White, 0, 1));
        sides.push(coloured_missed_side(&format!("b{i}"), Color::Black, 0, 1));
    }
    for g in 0..4 {
        sides.push(coloured_missed_side("both", Color::White, u32::from(g == 0), 1));
        sides.push(coloured_missed_side("both", Color::Black, 0, 1));
    }
    let pooled = |side: Side| {
        let b = aggregate_peer_benchmark(&peer_sides_of(&sides, side), 800, NOW);
        let r = b.stats.iter().find(|s| s.metric == MISSED).unwrap().clone();
        (r.pooled_rate.unwrap(), r.n_players, b.chances[&MISSED])
    };
    let (both, both_players, both_chances) = pooled(Side::Both);
    let (white, white_players, white_chances) = pooled(Side::White);
    let (black, black_players, black_chances) = pooled(Side::Black);
    // Units: White 10 + 1 (the dual opponent's 4 games), event 1/4; Both 20 + 1, event 1/8.
    assert!((white - 0.25 / 11.0).abs() < 1e-12, "{white}");
    assert!((both - 0.125 / 21.0).abs() < 1e-12, "{both}");
    assert_eq!(black, 0.0);
    assert_eq!((both_players, white_players, black_players), (21, 11, 11));
    assert_eq!((both_chances, white_chances, black_chances), (28, 14, 14));
    let by_chances = (white * f64::from(white_chances) + black * f64::from(black_chances))
        / f64::from(both_chances);
    // Half the event's weight over 21/11 of White's units: (1/4 / 22) / (1/8 / 21) = 42/22.
    assert!(
        (by_chances / both - 42.0 / 22.0).abs() < 1e-9,
        "{both} vs {by_chances}"
    );

    // Without an opponent in both pools, Both is exactly the unit-weighted mean of the colours
    // (the same denominators and decay).
    let mut single: Vec<PeerSide> = sides
        .iter()
        .filter(|s| s.opponent != "both")
        .cloned()
        .collect();
    single.push(coloured_missed_side("solo", Color::White, 1, 1));
    single[0].ctx.played_at = NOW - time::Duration::days(60);
    let rate = |side: Side| {
        let b = aggregate_peer_benchmark(&peer_sides_of(&single, side), 800, NOW);
        b.stats
            .iter()
            .find(|s| s.metric == MISSED)
            .unwrap()
            .pooled_rate
            .unwrap()
    };
    // White units: 9 × 1 + 0.5 (the 60-day-old game) + 1 (solo, an event); Black: 10.
    let (w_units, b_units) = (10.5, 10.0);
    let mean = (rate(Side::White) * w_units + rate(Side::Black) * b_units) / (w_units + b_units);
    assert!((rate(Side::Both) - mean).abs() < 1e-12, "{} vs {mean}", rate(Side::Both));
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
    let b = aggregate_peer_benchmark(&sides, 800, NOW);
    let traps = b.stats.iter().find(|s| s.metric == TRAPS).unwrap();
    let pooled = traps.pooled_rate.unwrap();
    assert!((pooled - 1.0 / 11.0).abs() < 1e-9, "{pooled}");
    assert!((traps.opps_per_game.unwrap() - 1.0).abs() < 1e-9);
    assert!((traps.epl_per_event.unwrap() - 0.5).abs() < 1e-9);
    // Means are per opponent too: the median of eleven per-player means is 60.
    let acc = b.stats.iter().find(|s| s.metric == ACCURACY).unwrap();
    assert!((acc.median - 60.0).abs() < 1e-9, "{}", acc.median);
    assert_eq!(acc.n_players, 11);
}

#[test]
fn no_sides_give_an_empty_benchmark() {
    let b = aggregate_peer_benchmark(&[], 800, NOW);
    assert_eq!(b.n_players, 0);
    assert!(b.stats.is_empty());
    assert!(b.reference().is_none());
}

/// One game of `opponent`, `age_days` before [`NOW`], with W4.missed `k` of `n`.
fn missed_side(opponent: &str, k: u32, n: u32, age_days: i64) -> PeerSide {
    let mut s = side(opponent, 800, 0, 60.0);
    s.metrics.weakness.insert(
        WeaknessKey::W4Missed,
        BinomialObs {
            events: k,
            opportunities: n,
            epl: 0.3 * f64::from(k),
            exemplar_plies: vec![],
        },
    );
    s.ctx.played_at = NOW - time::Duration::days(age_days);
    s
}

#[test]
fn the_mirrors_of_the_players_own_episodes_are_not_compared() {
    // In a game against the player the opponent's conversion (W5) is the player's resilience
    // (W6) mirrored, and the reverse; W7.prep is the opening's eval seen from the other side.
    // The base rates and means built on the same episodes mirror them too.
    for m in [
        MetricId::Weakness(WeaknessKey::W5),
        MetricId::Weakness(WeaknessKey::W6),
        MetricId::Weakness(WeaknessKey::W7Prep),
        MetricId::Base(BaseMetric::ConversionRate),
        MetricId::Base(BaseMetric::ResilienceRate),
        MetricId::Base(BaseMetric::OpeningOutWorseRate),
        MetricId::Base(BaseMetric::EvalAfterOpening),
        MetricId::Base(BaseMetric::EvalTrend1535),
    ] {
        assert!(!is_peer_comparable(m), "{m}");
    }
}

#[test]
fn a_peer_row_compares_with_the_pooled_rate_not_a_skewed_fit_median() {
    // One game per opponent, chances clustered in a few games: 30 opponents with 2 chances and
    // no miss, 6 with 8 chances and 5 misses. The pooled rate is 30 / 108 = 27.8%; a
    // Beta-binomial fit of these pairs is U-shaped and its median far lower.
    let mut sides: Vec<PeerSide> = (0..30)
        .map(|i| missed_side(&format!("quiet{i}"), 0, 2, 0))
        .collect();
    sides.extend((0..6).map(|i| missed_side(&format!("sharp{i}"), 5, 8, 0)));
    let b = aggregate_peer_benchmark(&sides, 800, NOW);
    let r = b.stats.iter().find(|s| s.metric == MISSED).unwrap();
    let pooled = 30.0 / 108.0;
    assert!((r.pooled_rate.unwrap() - pooled).abs() < 1e-9, "{r:?}");
    assert!(
        (r.median - pooled).abs() < 1e-9,
        "the compared rate is the pooled one: {r:?}"
    );
    // The spread is the pooled rate's own uncertainty (a bootstrap by opponent), around it.
    assert!(
        r.p25 <= r.p40 && r.p40 <= r.median && r.median <= r.p75,
        "{r:?}"
    );
    assert!(r.p25 < pooled && r.p75 > pooled, "{r:?}");
    assert!(r.p25 > 0.1 && r.p75 < 0.5, "{r:?}");
    assert_eq!(b.chances[&MISSED], 30 * 2 + 6 * 8);
    // Seeded: the same sides give the same rows.
    assert_eq!(aggregate_peer_benchmark(&sides, 800, NOW), b);
}

#[test]
fn opponent_sides_use_the_players_window_and_decay() {
    // Ten opponents today never miss; ten 120 days ago (two half-lives, weight 1/4) always do;
    // ten 200 days ago, outside the 183-day window, always do too.
    let mut sides: Vec<PeerSide> = (0..10)
        .map(|i| missed_side(&format!("now{i}"), 0, 1, 0))
        .collect();
    sides.extend((0..10).map(|i| missed_side(&format!("then{i}"), 1, 1, 120)));
    sides.extend((0..10).map(|i| missed_side(&format!("old{i}"), 1, 1, 200)));
    let b = aggregate_peer_benchmark(&sides, 800, NOW);
    assert_eq!(
        (b.n_players, b.n_games),
        (20, 20),
        "the old games are outside"
    );
    let r = b.stats.iter().find(|s| s.metric == MISSED).unwrap();
    // 10 × 1/4 misses over 10 + 10 × 1/4 chances.
    assert!((r.pooled_rate.unwrap() - 0.2).abs() < 1e-9, "{r:?}");
    assert!((r.median - 0.2).abs() < 1e-9, "{r:?}");
    assert_eq!(r.n_players, 20);
    assert_eq!(b.chances[&MISSED], 20, "raw chances in the window");
    assert!(
        (r.opps_per_game.unwrap() - 1.0).abs() < 1e-9,
        "unweighted: {r:?}"
    );
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

/// Player metrics of one rapid game: rated 800 against 800, a W7.traps opportunity with `w5_events`.
fn owner_metrics(w5_events: u32) -> PlayerGameMetrics {
    let mut m = metrics();
    m.player_rating = Some(800);
    m.opponent_rating = Some(800);
    m.score = Some(0.0);
    m.expected_score = Some(0.5);
    m.accuracy = Some(70.0);
    m.weakness.insert(
        WeaknessKey::W7Traps,
        BinomialObs {
            events: w5_events,
            opportunities: 1,
            epl: 0.5 * f64::from(w5_events),
            exemplar_plies: vec![],
        },
    );
    m
}

/// A rapid chess.com game against `opponent`, analysed, with the player's metrics and W7.traps point.
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
            series_key: "W7.traps".to_owned(),
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

/// The opponent side of `game` as a refresh stores it: rated `rating`, W7.traps with `w5` events.
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
    assert_eq!(
        (players, games),
        (12, 12),
        "the 1400 opponent is outside the window"
    );

    type Row = (String, Option<f64>, Option<f64>);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT series_key, band_median, excess FROM weakness_estimates \
         WHERE account_id = $1 AND time_class = 'rapid' AND side = 'both' ORDER BY series_key",
    )
    .bind(account)
    .fetch_all(&pool)
    .await
    .unwrap();
    let w5 = rows.iter().find(|r| r.0 == "W7.traps").unwrap();
    assert!(
        w5.1.is_some() && w5.2.is_some(),
        "W7.traps is compared: {w5:?}"
    );
    let w1 = rows.iter().find(|r| r.0 == "W1").unwrap();
    assert_eq!(
        (w1.1, w1.2),
        (None, None),
        "W1 needs enrichment: never compared"
    );

    let served = load_account_benchmark(&pool, account, TimeClass::Rapid)
        .await
        .unwrap()
        .expect("served rows");
    assert!(served.contains_key(&TRAPS));
    assert!(!served.contains_key(&W1));

    // The web names the pooled rate with its raw chance count.
    let (chances, median, pooled): (Option<i32>, f64, Option<f64>) = sqlx::query_as(
        "SELECT n_chances, median, pooled_rate FROM account_benchmark_stat \
         WHERE account_id = $1 AND time_class = 'rapid' AND metric = 'weakness:W7.traps'",
    )
    .bind(account)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(chances, Some(12));
    assert_eq!(
        Some(median),
        pooled,
        "a peer row's median is its pooled rate"
    );
}

/// The colour split (ADR 0082): a colour's peers are the opponents who had that colour. The
/// player had White in every seeded game, so every opponent had Black: Black has peer rows,
/// White has none, and the both rows are never used for White.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn colour_peers_are_the_opponents_who_had_that_colour(pool: PgPool) {
    let account = peer_seed(&pool, 12).await;
    build_rapid(&pool, account).await;
    let sides: Vec<(String, i64)> = sqlx::query_as(
        "SELECT side::text AS s, count(*) FROM account_benchmark_stat \
         WHERE account_id = $1 AND time_class = 'rapid' GROUP BY side ORDER BY side",
    )
    .bind(account)
    .fetch_all(&pool)
    .await
    .unwrap();
    let names: Vec<&str> = sides.iter().map(|(s, _)| s.as_str()).collect();
    assert_eq!(names, ["both", "black"], "{sides:?}");
    assert_eq!(sides[0].1, sides[1].1, "the same opponents, the same rows");
    // A colour's rows carry the same pooled rate and chance count (the same opponents here).
    let traps: Vec<(String, Option<i32>, Option<f64>)> = sqlx::query_as(
        "SELECT side::text AS s, n_chances, pooled_rate FROM account_benchmark_stat \
         WHERE account_id = $1 AND time_class = 'rapid' AND metric = $2 ORDER BY side",
    )
    .bind(account)
    .bind(TRAPS.to_string())
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(traps.len(), 2, "{traps:?}");
    assert_eq!(traps[0].1, Some(12));
    assert_eq!((traps[0].1, traps[0].2), (traps[1].1, traps[1].2));

    let traps_median = |side: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, Option<f64>>(
                "SELECT band_median FROM weakness_estimates WHERE account_id = $1 \
                 AND time_class = 'rapid' AND series_key = 'W7.traps' \
                 AND side = $2::side_scope",
            )
            .bind(account)
            .bind(side)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert!(traps_median("both").await.is_some());
    assert!(
        traps_median("black").await.is_some(),
        "black is compared with black peers"
    );
    assert_eq!(
        traps_median("white").await,
        None,
        "no white peers: no white benchmark"
    );
    // The served (coaching) benchmark is the both one.
    let served = load_account_benchmark(&pool, account, TimeClass::Rapid)
        .await
        .unwrap()
        .expect("served rows");
    assert!(served.contains_key(&TRAPS));
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
        "SELECT count(*) FROM weakness_estimates WHERE account_id = $1 \
         AND excess IS NOT NULL",
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
    assert_eq!(
        (again.computed, again.skipped),
        (0, 0),
        "a current row is kept"
    );
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
