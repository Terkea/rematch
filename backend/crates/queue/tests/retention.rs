//! Retention of finished jobs (Task 14): done rows for 24 h and at most the newest 1000 per kind,
//! failed and dead rows for 7 days, deleted in bounded batches. Ports the bounded
//! `removeOnComplete` / `removeOnFail` retention of the TS `DEFAULT_JOB_OPTIONS`.

use std::time::Duration;

use queue::queue::Queue;
use queue::retention::Retention;
use sqlx::PgPool;

/// Inserts `n` jobs of `kind` in `state`, finished `age` ago (`None` for live rows).
async fn insert(pool: &PgPool, kind: &str, state: &str, n: i32, age: Option<&str>) {
    sqlx::query(
        "INSERT INTO jobs (kind, payload, state, created_at, finished_at)          SELECT $1, jsonb_build_object('i', i), $2::job_state, now() - interval '30 days',            now() - $3::interval - i * interval '1 millisecond'          FROM generate_series(1, $4) AS i",
    )
    .bind(kind)
    .bind(state)
    .bind(age)
    .bind(n)
    .execute(pool)
    .await
    .unwrap();
}

async fn count(pool: &PgPool, kind: &str, state: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = $1 AND state = $2::job_state")
        .bind(kind)
        .bind(state)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[test]
fn retention_defaults() {
    let r = Retention::default();
    assert_eq!(r.done_max_age, Duration::from_secs(24 * 3600));
    assert_eq!(r.done_max_count_per_kind, 1000);
    assert_eq!(r.failed_max_age, Duration::from_secs(7 * 24 * 3600));
    assert!(r.batch_size > 0 && r.batch_size <= 10_000);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn cleanup_applies_retention(pool: PgPool) {
    let q = Queue::new(pool.clone());
    // analyse-game: 1005 recent done rows (5 beyond the newest 1000) and 3 older than 24 h.
    insert(&pool, "analyse-game", "done", 1005, Some("1 hour")).await;
    insert(&pool, "analyse-game", "done", 3, Some("25 hours")).await;
    // failed and dead: kept for 7 days.
    insert(&pool, "analyse-game", "failed", 2, Some("8 days")).await;
    insert(&pool, "analyse-game", "failed", 4, Some("6 days")).await;
    insert(&pool, "analyse-game", "dead", 1, Some("8 days")).await;
    insert(&pool, "analyse-game", "dead", 1, Some("2 days")).await;
    // Another kind has its own 1000.
    insert(&pool, "import-account", "done", 10, Some("1 hour")).await;
    // Live rows are never touched, however old.
    insert(&pool, "analyse-game", "queued", 2, None).await;
    insert(&pool, "analyse-game", "running", 1, None).await;

    let deleted = q.cleanup(&Retention::default()).await.unwrap();
    assert_eq!(deleted, 3 + 5 + 2 + 1);
    assert_eq!(count(&pool, "analyse-game", "done").await, 1000);
    assert_eq!(count(&pool, "analyse-game", "failed").await, 4);
    assert_eq!(count(&pool, "analyse-game", "dead").await, 1);
    assert_eq!(count(&pool, "import-account", "done").await, 10);
    assert_eq!(count(&pool, "analyse-game", "queued").await, 2);
    assert_eq!(count(&pool, "analyse-game", "running").await, 1);
    // The newest 1000 are the ones kept.
    let oldest_kept_age: f64 = sqlx::query_scalar(
        "SELECT extract(epoch FROM now() - min(finished_at))::float8 FROM jobs \
         WHERE kind = 'analyse-game' AND state = 'done'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(oldest_kept_age < 3700.0 + 1.0, "{oldest_kept_age}");
    let beyond_1000: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = 'analyse-game' AND state = 'done' \
         AND (payload->>'i')::int > 1000",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        beyond_1000, 0,
        "rows 1001..1005 are the oldest of the recent ones"
    );

    // Idempotent.
    assert_eq!(q.cleanup(&Retention::default()).await.unwrap(), 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn cleanup_deletes_in_bounded_batches(pool: PgPool) {
    let q = Queue::new(pool.clone());
    insert(&pool, "analyse-game", "done", 25, Some("2 days")).await;
    insert(&pool, "analyse-game", "dead", 13, Some("9 days")).await;
    insert(&pool, "analyse-game", "done", 30, Some("1 hour")).await;
    let r = Retention {
        done_max_count_per_kind: 10,
        batch_size: 7,
        ..Retention::default()
    };
    // Every batch deletes at most 7 rows; the loop keeps going until nothing is left.
    let deleted = q.cleanup(&r).await.unwrap();
    assert_eq!(deleted, 25 + 13 + 20);
    assert_eq!(count(&pool, "analyse-game", "done").await, 10);
    assert_eq!(count(&pool, "analyse-game", "dead").await, 0);
}
