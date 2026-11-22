//! The colour split (next round T1): build-profile writes estimates and KPI snapshots for every
//! game (`both`) and for the games the player had White or Black in, with the minimum evidence
//! counted per side and a side's own benchmark only. Rebuilds are idempotent, and profiles from
//! before the split are queued for one rebuild.

mod common {
    pub mod seed;
}

use std::time::Duration;

use common::seed::{GameShape, SeedOptions, insert_side_reference, seed, seed_now, w1_stat};
use profile::build_profile::{BuildProfileDeps, BuildProfileSummary, handle_build_profile};
use profile::requeue::requeue_colour_split;
use profile_core::types::{Side, WeaknessKey};
use queue::queue::Queue;
use queue::{BuildProfilePayload, KIND_BUILD_PROFILE, profile_key};
use sqlx::PgPool;
use uuid::Uuid;

const TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// 60 games: every third one with Black (20 games, W1 3/50, no blunders), the rest with White
/// (40 games, W1 2/50, two blunders each).
fn white_heavy_blunders(i: usize) -> GameShape {
    if i.is_multiple_of(3) {
        GameShape {
            color: "black",
            w1_events: 3,
            blunders: 0,
        }
    } else {
        GameShape {
            color: "white",
            w1_events: 2,
            blunders: 2,
        }
    }
}

fn fixture() -> SeedOptions {
    SeedOptions {
        games: 60,
        reference: false,
        shape: white_heavy_blunders,
    }
}

fn deps(pool: &PgPool) -> BuildProfileDeps {
    BuildProfileDeps {
        pool: pool.clone(),
        queue: Queue::new(pool.clone()),
        now: seed_now,
        benchmark: profile::peer_benchmark::BenchmarkSource::LichessBands,
    }
}

async fn build(pool: &PgPool, account_id: Uuid) -> BuildProfileSummary {
    let payload = BuildProfilePayload {
        account_id,
        time_class: "blitz".to_owned(),
    };
    tokio::time::timeout(TEST_TIMEOUT, handle_build_profile(&deps(pool), payload))
        .await
        .expect("build-profile timed out")
        .expect("build-profile")
}

fn binomial_keys() -> usize {
    WeaknessKey::ALL.iter().filter(|k| k.is_binomial()).count()
}

type EstimateCols = (f64, i32, Option<f64>, f64, f64, String, Option<f64>);

/// W1's `(k_unweighted, n_unweighted, band_median, confidence, priority, state, p_imp)`.
async fn w1(pool: &PgPool, a: Uuid, side: Side) -> EstimateCols {
    sqlx::query_as(
        "SELECT k_unweighted, n_unweighted, band_median, confidence, priority, state::text, \
         p_imp FROM weakness_estimates WHERE account_id = $1 AND time_class = 'blitz' \
         AND series_key = 'W1' AND side = $2::side_scope",
    )
    .bind(a)
    .bind(side.as_str())
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The latest `(value, n)` of a metric's `last50` row on a side.
async fn last50(pool: &PgPool, a: Uuid, side: Side, metric: &str) -> (Option<f64>, i32) {
    sqlx::query_as(
        "SELECT value, n FROM kpi_snapshot WHERE account_id = $1 AND time_class = 'blitz' \
         AND side = $2::side_scope AND metric = $3 AND window_key = 'last50' \
         ORDER BY id DESC LIMIT 1",
    )
    .bind(a)
    .bind(side.as_str())
    .bind(metric)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Rows per side of a table for the account.
async fn rows_per_side(pool: &PgPool, table: &str, a: Uuid) -> Vec<(String, i64)> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT side::text AS s, count(*) FROM {table} WHERE account_id = $1 GROUP BY side ORDER BY side"
    )))
    .bind(a)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn snapshots_and_estimates_are_split_by_colour(pool: PgPool) {
    let s = seed(&pool, fixture()).await;
    // Only a both benchmark: it is never applied to one colour.
    insert_side_reference(&pool, &[(Side::Both, w1_stat())]).await;
    let summary = build(&pool, s.account_id).await;
    assert_eq!(summary.estimates, 3 * binomial_keys());

    // KPIs: White blundered twice a game, Black never; each side counts its own games.
    let (white, white_n) = last50(&pool, s.account_id, Side::White, "blunders_per_game").await;
    let (black, black_n) = last50(&pool, s.account_id, Side::Black, "blunders_per_game").await;
    let (both, both_n) = last50(&pool, s.account_id, Side::Both, "blunders_per_game").await;
    assert_eq!((white, white_n), (Some(2.0), 40));
    assert_eq!((black, black_n), (Some(0.0), 20));
    assert_eq!(both_n, 50, "the last 50 games of both colours");
    let both = both.unwrap();
    assert!(both > 0.0 && both < 2.0, "both mixes the colours: {both}");

    // Estimates from each side's own series points.
    let (wk, wn, w_median, ..) = w1(&pool, s.account_id, Side::White).await;
    let (bk, bn, b_median, ..) = w1(&pool, s.account_id, Side::Black).await;
    let (ak, an, a_median, ..) = w1(&pool, s.account_id, Side::Both).await;
    assert_eq!((wk, wn), (80.0, 2000));
    assert_eq!((bk, bn), (60.0, 1000));
    assert_eq!((ak, an), (140.0, 3000));
    assert!(a_median.is_some(), "both has its benchmark");
    assert_eq!(
        (w_median, b_median),
        (None, None),
        "no colour rows means no colour benchmark"
    );

    // Area scores per side, in every window the side has games in.
    let areas: Vec<(String, i64)> = sqlx::query_as(
        "SELECT side::text AS s, count(*) FROM kpi_snapshot WHERE account_id = $1 \
         AND metric LIKE 'area:%' GROUP BY side ORDER BY side",
    )
    .bind(s.account_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        areas,
        [
            ("both".to_owned(), 7 * 3),
            ("white".to_owned(), 7 * 3),
            ("black".to_owned(), 7 * 3),
        ]
    );
    // The timeline stays both only: one W1 history, and colour rows carry its state, no P_imp.
    let (.., a_state, _) = w1(&pool, s.account_id, Side::Both).await;
    let (.., w_state, w_pimp) = w1(&pool, s.account_id, Side::White).await;
    assert_eq!(w_state, a_state);
    assert_eq!(w_pimp, None);

    let kpis = rows_per_side(&pool, "kpi_snapshot", s.account_id).await;
    let estimates = rows_per_side(&pool, "weakness_estimates", s.account_id).await;
    println!("fixture rows per side: kpi_snapshot {kpis:?}, weakness_estimates {estimates:?}");
    let per_key = i64::try_from(binomial_keys()).unwrap();
    assert_eq!(
        estimates,
        [
            ("both".to_owned(), per_key),
            ("white".to_owned(), per_key),
            ("black".to_owned(), per_key),
        ]
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn minimum_evidence_and_benchmarks_are_per_side(pool: PgPool) {
    let s = seed(&pool, fixture()).await;
    let mut black_ref = w1_stat();
    black_ref.median = 0.03;
    insert_side_reference(
        &pool,
        &[
            (Side::Both, w1_stat()),
            (Side::White, w1_stat()),
            (Side::Black, black_ref),
        ],
    )
    .await;
    build(&pool, s.account_id).await;

    let (_, _, a_median, a_conf, a_prio, ..) = w1(&pool, s.account_id, Side::Both).await;
    let (_, _, w_median, w_conf, w_prio, ..) = w1(&pool, s.account_id, Side::White).await;
    let (_, _, b_median, b_conf, b_prio, ..) = w1(&pool, s.account_id, Side::Black).await;
    assert!((a_median.unwrap() - 0.02).abs() < 1e-6);
    assert!((w_median.unwrap() - 0.02).abs() < 1e-6);
    assert!(
        (b_median.unwrap() - 0.03).abs() < 1e-6,
        "black's own benchmark"
    );
    // 60 and 40 games rank; Black's 20 games are below MIN_GAMES (30) for the side, so its
    // estimate is measured (confidence) but never ranked.
    assert!(a_prio > 0.0 && w_prio > 0.0, "{a_prio} {w_prio}");
    assert!(b_conf > 0.5, "black is measured: {b_conf}");
    assert_eq!(b_prio, 0.0, "black has 20 games, fewer than MIN_GAMES");
    assert!(a_conf > 0.5 && w_conf > 0.5);
}

/// The full estimate rows, side by side (no `updated_at`).
async fn estimate_dump(pool: &PgPool, a: Uuid) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT (to_jsonb(e) - 'updated_at')::text FROM weakness_estimates e \
         WHERE account_id = $1 ORDER BY side, series_key",
    )
    .bind(a)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn rebuilds_are_idempotent(pool: PgPool) {
    let s = seed(&pool, fixture()).await;
    insert_side_reference(&pool, &[(Side::Both, w1_stat()), (Side::White, w1_stat())]).await;
    let first = build(&pool, s.account_id).await;
    assert!(first.snapshots > 0);
    let snapshots = rows_per_side(&pool, "kpi_snapshot", s.account_id).await;
    let estimates = estimate_dump(&pool, s.account_id).await;
    let events: i64 = sqlx::query_scalar("SELECT count(*) FROM user_events WHERE account_id = $1")
        .bind(s.account_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    let second = build(&pool, s.account_id).await;
    assert_eq!((second.snapshots, second.transitions), (0, 0));
    assert_eq!(second.estimates, first.estimates);
    assert_eq!(
        rows_per_side(&pool, "kpi_snapshot", s.account_id).await,
        snapshots
    );
    assert_eq!(estimate_dump(&pool, s.account_id).await, estimates);
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM user_events WHERE account_id = $1")
        .bind(s.account_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, events, "a rerun publishes nothing");
}

/// Events come from the both side only: one `profile.updated` per changed area, as before.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn events_are_published_for_both_only(pool: PgPool) {
    let s = seed(&pool, fixture()).await;
    insert_side_reference(&pool, &[(Side::Both, w1_stat()), (Side::White, w1_stat())]).await;
    build(&pool, s.account_id).await;
    let areas: Vec<String> = sqlx::query_scalar(
        "SELECT payload->>'area' FROM user_events WHERE account_id = $1 \
         AND type = 'profile.updated' ORDER BY id",
    )
    .bind(s.account_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    let mut unique = areas.clone();
    unique.dedup();
    assert_eq!(areas, unique, "one event per area, not one per side");
    let w1_changes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM user_events WHERE account_id = $1 \
         AND type = 'weakness.changed' AND payload->>'id' = 'W1'",
    )
    .bind(s.account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(w1_changes <= 1, "{w1_changes}");
}

async fn live_builds(pool: &PgPool, a: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = $1 AND dedupe_key = $2 \
         AND state IN ('queued', 'running', 'paused')",
    )
    .bind(KIND_BUILD_PROFILE)
    .bind(profile_key(a, "blitz"))
    .fetch_one(pool)
    .await
    .unwrap()
}

/// A profile built before the split (both rows only) is queued for one rebuild at worker start;
/// once rebuilt it is never due again.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn profiles_from_before_the_split_are_rebuilt_once(pool: PgPool) {
    let s = seed(&pool, fixture()).await;
    build(&pool, s.account_id).await;
    let queue = Queue::new(pool.clone());
    sqlx::query("DELETE FROM jobs")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(requeue_colour_split(&pool, &queue).await.unwrap(), 0);

    // What the migration leaves behind: both rows only.
    sqlx::query("DELETE FROM weakness_estimates WHERE side <> 'both'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM kpi_snapshot WHERE side <> 'both'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(requeue_colour_split(&pool, &queue).await.unwrap(), 1);
    assert_eq!(live_builds(&pool, s.account_id).await, 1);
    // Twice is deduped onto the live build.
    assert_eq!(requeue_colour_split(&pool, &queue).await.unwrap(), 1);
    assert_eq!(live_builds(&pool, s.account_id).await, 1);

    let rebuilt = build(&pool, s.account_id).await;
    assert!(rebuilt.snapshots > 0, "the colour rows are written");
    assert_eq!(
        rows_per_side(&pool, "weakness_estimates", s.account_id)
            .await
            .len(),
        3
    );
    assert_eq!(requeue_colour_split(&pool, &queue).await.unwrap(), 0);
}

/// The CoachingInput's colour split (v2 `byColor`, ADR 0084): each colour from its own games,
/// snapshot rows, estimates and benchmark rows, with the minimum evidence kept per colour.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn coaching_input_splits_by_colour(pool: PgPool) {
    use chess_core::TimeClass;
    use coaching_input::{BenchmarkSource, InputVersion};

    let s = seed(&pool, fixture()).await;
    // White has its own rows; Black has none (so it is never compared).
    insert_side_reference(&pool, &[(Side::Both, w1_stat()), (Side::White, w1_stat())]).await;
    build(&pool, s.account_id).await;
    let input = profile::coaching::build_coaching_input(
        &pool,
        s.account_id,
        TimeClass::Blitz,
        common::seed::SEED_NOW,
    )
    .await
    .expect("coaching input");
    assert_eq!(input.input_version, InputVersion::V2);
    assert_eq!(input.validate(), Ok(()));
    let by = input.by_color.as_ref().expect("v2 has byColor");

    // 40 White games (all won), 20 Black games (all lost).
    let (w, b) = (&by.white, &by.black);
    assert_eq!((w.games, w.wins, w.draws, w.losses), (40, 40, 0, 0));
    assert_eq!((b.games, b.wins, b.losses), (20, 0, 20));
    assert!(w.sufficient && b.sufficient);
    assert_eq!(w.score.value, Some(1.0));
    assert_eq!(b.score.value, Some(0.0));
    assert!(b.score.high.unwrap() > 0.0, "an interval, not a point");
    assert_eq!(w.blunders_per_game.value, Some(2.0));
    assert_eq!(
        (b.blunders_per_game.value, b.blunders_per_game.n),
        (Some(0.0), 20)
    );

    // White's W1 against White's own peers; Black's 20 games are below MIN_GAMES (30).
    let w1 = w
        .weaknesses
        .iter()
        .find(|x| x.series_key == "W1")
        .expect("white W1");
    assert_eq!((w1.events, w1.opportunities), (80, 2000));
    assert!((w1.peer_rate.unwrap() - 0.02).abs() < 1e-9);
    assert!(b.weaknesses.is_empty(), "{:?}", b.weaknesses);

    // The Lichess band path stores its benchmark without a peer band.
    assert_eq!(
        input.benchmark.as_ref().map(|b| b.source),
        Some(BenchmarkSource::LichessBands)
    );
}

/// A colour below the minimum says so: no number, never 0.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn coaching_input_colour_below_minimum_is_insufficient(pool: PgPool) {
    use chess_core::TimeClass;

    fn few_black(i: usize) -> GameShape {
        GameShape {
            color: if i < 4 { "black" } else { "white" },
            w1_events: 2,
            blunders: 1,
        }
    }
    let s = seed(
        &pool,
        SeedOptions {
            games: 40,
            reference: false,
            shape: few_black,
        },
    )
    .await;
    build(&pool, s.account_id).await;
    let input = profile::coaching::build_coaching_input(
        &pool,
        s.account_id,
        TimeClass::Blitz,
        common::seed::SEED_NOW,
    )
    .await
    .expect("coaching input");
    let b = &input.by_color.as_ref().unwrap().black;
    assert_eq!((b.games, b.sufficient, b.need_more), (4, false, Some(6)));
    for m in [&b.score, &b.accuracy, &b.blunders_per_game] {
        assert_eq!(
            (m.value, m.low, m.high, m.sufficient),
            (None, None, None, false)
        );
    }
    assert!(b.weaknesses.is_empty());
    assert_eq!(input.benchmark, None, "no stored benchmark rows");
}
