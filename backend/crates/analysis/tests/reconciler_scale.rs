//! Port of apps/worker/test/recovery/reconciler-scale.test.ts.

use std::time::{Duration, Instant};

use analysis::jobs::reconcile::{ReconcileOptions, reconcile};
use queue::queue::Queue;
use queue::{EnqueueOptions, KIND_ANALYSE, analyse_key};
use serde_json::json;
use sqlx::PgPool;
use testkit::insert_account;
use uuid::Uuid;

const N: i64 = 2500;
/// Analysed (done, failed, skipped) games around the pending ones.
const FILLER: i64 = 20_000;

async fn live_analyse_jobs(pool: &PgPool) -> i64 {
    sqlx::query_scalar(
        "SELECT count(DISTINCT dedupe_key) FROM jobs WHERE kind = $1 \
         AND state IN ('queued', 'running', 'paused')",
    )
    .bind(KIND_ANALYSE)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn sweeps_2500_pending_games_in_pages_and_leaves_exactly_one_job_per_game(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = insert_account(&pool, "lichess", "bulk").await;
    sqlx::query(
        "INSERT INTO games (account_id, platform, platform_game_id, pgn, played_at, time_class, \
           rated, player_color, white_name, black_name, result, pending_since) \
         SELECT $1, 'lichess', 'bulk-' || i, '1. e4 *', \
           '2026-01-01T00:00:00Z'::timestamptz + make_interval(secs => i), 'blitz', true, \
           'white', 'bulk', 'x', '*', now() - interval '10 minutes' \
         FROM generate_series(1, $2) i",
    )
    .bind(acct)
    .bind(N)
    .execute(&pool)
    .await
    .unwrap();
    // Filler: an account's history is mostly analysed games, and the sweep must not wade
    // through them (nor touch them).
    sqlx::query(
        "INSERT INTO games (account_id, platform, platform_game_id, pgn, played_at, time_class, \
           rated, player_color, white_name, black_name, result, analysis_status, pending_since) \
         SELECT $1, 'lichess', 'done-' || i, '1. e4 *', \
           '2025-01-01T00:00:00Z'::timestamptz + make_interval(secs => i), 'blitz', true, \
           'white', 'bulk', 'x', '*', \
           (ARRAY['done', 'failed', 'skipped'])[1 + i % 3]::analysis_status, \
           now() - interval '1 day' \
         FROM generate_series(1, $2) i",
    )
    .bind(acct)
    .bind(FILLER)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("ANALYZE games").execute(&pool).await.unwrap();
    let ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM games WHERE analysis_status = 'pending' ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(ids.len() as i64, N);

    // A few already have a live job: they must not get a second one.
    for id in &ids[..10] {
        queue
            .enqueue(
                KIND_ANALYSE,
                &json!({ "gameId": id }),
                EnqueueOptions {
                    dedupe_key: Some(analyse_key(*id)),
                    user_key: Some(acct),
                    max_attempts: 3,
                    ..EnqueueOptions::default()
                },
            )
            .await
            .unwrap()
            .unwrap();
    }

    let opts = ReconcileOptions {
        page_size: 1000,
        ..ReconcileOptions::default()
    };
    let started = Instant::now();
    let res = reconcile(&pool, &queue, &opts).await.unwrap();
    let took = started.elapsed();
    assert_eq!(res.requeued_pending, (N - 10) as u64);
    assert!(took < Duration::from_secs(15), "sweep took {took:?}");

    assert_eq!(live_analyse_jobs(&pool).await, N);
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = $1")
        .bind(KIND_ANALYSE)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, N, "exactly one job per game");

    let again = reconcile(&pool, &queue, &opts).await.unwrap();
    assert_eq!(again.requeued_pending, 0);
    assert_eq!(live_analyse_jobs(&pool).await, N);
    let filler: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM games WHERE analysis_status IN ('done', 'failed', 'skipped') \
         AND analysis_error IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(filler, FILLER, "analysed games are left alone");
}
