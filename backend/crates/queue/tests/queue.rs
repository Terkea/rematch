//! Postgres job queue core (Task 13). Ports of apps/worker/test/jobs/queues.test.ts plus the
//! claim, fencing, retry and release rules of ADR 0032.

use std::collections::HashSet;
use std::time::Duration;

use queue::queue::{
    Backoff, FailOutcome, JOBS_CHANNEL, PriorityFifo, Queue, QueueError, SelectStrategy,
};
use queue::{EnqueueOptions, Job, JobError, JobState};
use serde_json::json;
use sqlx::PgPool;
use sqlx::postgres::PgListener;
use time::OffsetDateTime;
use uuid::Uuid;

const LEASE: Duration = Duration::from_secs(60);
const KIND: &str = "analyse-game";

fn keyed(key: &str) -> EnqueueOptions {
    EnqueueOptions {
        dedupe_key: Some(key.to_owned()),
        ..EnqueueOptions::default()
    }
}

#[derive(Debug, sqlx::FromRow)]
struct Row {
    state: String,
    attempts: i32,
    /// `run_at - now()` in seconds, measured by the database.
    run_in: f64,
    error_code: Option<String>,
    last_error: Option<String>,
    lease_token: Option<Uuid>,
    finished: bool,
}

async fn row(pool: &PgPool, id: i64) -> Row {
    sqlx::query_as(
        "SELECT state::text AS state, attempts, \
           extract(epoch FROM run_at - now())::float8 AS run_in, error_code, last_error, \
           lease_token, finished_at IS NOT NULL AS finished \
         FROM jobs WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("job row")
}

async fn claim_one(q: &Queue) -> Job {
    let mut jobs = q.dequeue(&[KIND], "w1", 1, LEASE).await.unwrap();
    assert_eq!(jobs.len(), 1, "expected one claimable job");
    jobs.pop().unwrap()
}

fn retryable(msg: &str) -> JobError {
    JobError::Retryable {
        message: msg.to_owned(),
        code: Some("engine".to_owned()),
    }
}

/// How far the database clock may step back between a write of `now() + x` and a later read of
/// `x - now()` (ADR 0032; up to 3.2 s measured in the Docker Desktop VM). A step back only makes
/// the remaining time look longer, so it widens the upper bound alone.
const CLOCK_STEP: f64 = 3.5;

fn assert_near(actual: f64, expected: f64, tolerance: f64) {
    let (low, high) = (expected - tolerance, expected + tolerance + CLOCK_STEP);
    assert!(
        (low..=high).contains(&actual),
        "expected {expected} in [{low}, {high}], got {actual}"
    );
}

// ---- ports of queues.test.ts ----

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn enqueueing_the_same_game_twice_creates_one_job(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let first = q
        .enqueue(KIND, &json!({"gameId": "g1"}), keyed("analyse:g1"))
        .await
        .unwrap();
    let second = q
        .enqueue(KIND, &json!({"gameId": "g1"}), keyed("analyse:g1"))
        .await
        .unwrap();
    let other = q
        .enqueue(KIND, &json!({"gameId": "g2"}), keyed("analyse:g2"))
        .await
        .unwrap();
    assert!(first.is_some());
    assert_eq!(second, None);
    assert!(other.is_some());
    let live: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = $1")
        .bind(KIND)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(live, 2);
    let (payload, max_attempts): (serde_json::Value, i32) =
        sqlx::query_as("SELECT payload, max_attempts FROM jobs WHERE id = $1")
            .bind(first.unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(payload, json!({"gameId": "g1"}));
    assert_eq!(max_attempts, 3);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn re_enqueues_a_game_whose_previous_job_is_done(pool: PgPool) {
    let q = Queue::new(pool.clone());
    q.enqueue(KIND, &json!({"gameId": "again"}), keyed("analyse:again"))
        .await
        .unwrap();
    let job = claim_one(&q).await;
    assert!(q.complete(&job).await.unwrap());
    assert_eq!(row(&pool, job.id).await.state, "done");
    // Otherwise the retained row would silently swallow the re-analysis.
    let again = q
        .enqueue(KIND, &json!({"gameId": "again"}), keyed("analyse:again"))
        .await
        .unwrap();
    assert!(again.is_some());
    assert_eq!(row(&pool, again.unwrap()).await.state, "queued");
    // A live job is not duplicated.
    let dup = q
        .enqueue(KIND, &json!({"gameId": "again"}), keyed("analyse:again"))
        .await
        .unwrap();
    assert_eq!(dup, None);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn checks_liveness_for_many_games_in_one_query(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let keys: Vec<String> = (0..1000).map(|i| format!("analyse:g{i}")).collect();
    for key in &keys[..4] {
        q.enqueue(KIND, &json!({}), keyed(key)).await.unwrap();
    }
    // g0 queued; g1 running; g2 paused; g3 done; the rest never enqueued.
    for (key, state) in [
        ("analyse:g1", "running"),
        ("analyse:g2", "paused"),
        ("analyse:g3", "done"),
    ] {
        sqlx::query("UPDATE jobs SET state = $2::job_state WHERE dedupe_key = $1")
            .bind(key)
            .bind(state)
            .execute(&pool)
            .await
            .unwrap();
    }

    let states = q.live_states(KIND, &keys).await.unwrap();
    assert_eq!(states.len(), 3);
    assert_eq!(states.get("analyse:g0"), Some(&JobState::Queued));
    assert_eq!(states.get("analyse:g1"), Some(&JobState::Running));
    assert_eq!(states.get("analyse:g2"), Some(&JobState::Paused));
    assert_eq!(states.get("analyse:g3"), None, "done is not live");
    assert_eq!(states.get("analyse:g999"), None);
    assert!(q.live_states(KIND, &[]).await.unwrap().is_empty());
    // Scoped to the kind.
    assert!(
        q.live_states("import-account", &keys)
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn uses_the_default_job_options(pool: PgPool) {
    let opts = EnqueueOptions::default();
    assert_eq!(opts.max_attempts, 3);
    assert_eq!(opts.priority, 100);
    assert_eq!(Backoff::default().base, Duration::from_secs(5));
    assert_eq!(JOBS_CHANNEL, "jobs");
    let q = Queue::new(pool.clone());
    let id = q.enqueue(KIND, &json!({}), opts).await.unwrap().unwrap();
    let (priority, max_attempts, max_stalls, attempts): (i16, i32, i32, i32) = sqlx::query_as(
        "SELECT priority, max_attempts, max_stalls, attempts FROM jobs WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (priority, max_attempts, max_stalls, attempts),
        (100, 3, 2, 0)
    );
}

// ---- new tests ----

#[test]
fn backoff_doubles_per_attempt() {
    let b = Backoff::default();
    assert_eq!(b.delay(1), Duration::from_secs(5));
    assert_eq!(b.delay(2), Duration::from_secs(10));
    assert_eq!(b.delay(3), Duration::from_secs(20));
    assert_eq!(b.delay(0), Duration::from_secs(5), "never below base");
    // Huge attempt counts saturate instead of overflowing.
    assert!(b.delay(10_000) >= b.delay(30));
}

#[test]
fn priority_fifo_is_the_only_strategy() {
    assert_eq!(PriorityFifo.order_by(), "priority, run_at, id");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn dequeue_orders_by_priority_then_run_at(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let now = OffsetDateTime::now_utc();
    let ago = |s: i64| Some(now - time::Duration::seconds(s));
    let mk = |priority: i16, run_at| EnqueueOptions {
        priority,
        run_at,
        ..EnqueueOptions::default()
    };
    let late_low = q
        .enqueue(KIND, &json!({"n": "late_low"}), mk(100, ago(10)))
        .await
        .unwrap()
        .unwrap();
    let early_low = q
        .enqueue(KIND, &json!({"n": "early_low"}), mk(100, ago(20)))
        .await
        .unwrap()
        .unwrap();
    let high = q
        .enqueue(KIND, &json!({"n": "high"}), mk(10, ago(5)))
        .await
        .unwrap()
        .unwrap();
    let mut order = Vec::new();
    for _ in 0..3 {
        order.push(claim_one(&q).await.id);
    }
    assert_eq!(order, vec![high, early_low, late_low]);

    // A batch claim is returned in the same order.
    let ids: Vec<i64> = vec![
        q.enqueue(KIND, &json!({}), mk(50, ago(10)))
            .await
            .unwrap()
            .unwrap(),
        q.enqueue(KIND, &json!({}), mk(5, ago(5)))
            .await
            .unwrap()
            .unwrap(),
    ];
    let batch: Vec<i64> = q
        .dequeue(&[KIND], "w", 10, LEASE)
        .await
        .unwrap()
        .iter()
        .map(|j| j.id)
        .collect();
    assert_eq!(batch, vec![ids[1], ids[0]]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn dequeue_counts_the_attempt_and_sets_a_lease(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let user = Uuid::new_v4();
    let id = q
        .enqueue(
            KIND,
            &json!({"gameId": "x"}),
            EnqueueOptions {
                user_key: Some(user),
                ..keyed("k")
            },
        )
        .await
        .unwrap()
        .unwrap();
    let job = claim_one(&q).await;
    assert_eq!(job.id, id);
    assert_eq!(job.attempts, 1, "the attempt is counted at claim");
    assert_eq!(job.user_key, Some(user));
    assert_eq!(job.payload, json!({"gameId": "x"}));
    let (owner, lease_in): (String, f64) = sqlx::query_as(
        "SELECT lease_owner, extract(epoch FROM lease_until - now())::float8 FROM jobs WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(owner, "w1");
    assert_near(lease_in, 60.0, 5.0);
    let r = row(&pool, id).await;
    assert_eq!(r.state, "running");
    assert_eq!(r.lease_token, Some(job.lease_token));
    assert!(q.dequeue(&[KIND], "w2", 5, LEASE).await.unwrap().is_empty());
    assert!(q.dequeue(&[], "w2", 5, LEASE).await.unwrap().is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn dequeue_skips_locked_rows_concurrently(pool: PgPool) {
    let q = Queue::new(pool.clone());
    for i in 0..10 {
        q.enqueue(KIND, &json!({"i": i}), EnqueueOptions::default())
            .await
            .unwrap();
    }
    let (a, b) = tokio::join!(
        q.dequeue(&[KIND], "a", 5, LEASE),
        q.dequeue(&[KIND], "b", 5, LEASE)
    );
    let a: HashSet<i64> = a.unwrap().iter().map(|j| j.id).collect();
    let b: HashSet<i64> = b.unwrap().iter().map(|j| j.id).collect();
    assert_eq!(a.len(), 5);
    assert_eq!(b.len(), 5);
    assert!(a.is_disjoint(&b));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn many_concurrent_claimers_claim_each_job_exactly_once(pool: PgPool) {
    // The sqlx::test pool has several connections, so the claimers really race in Postgres.
    assert!(pool.options().get_max_connections() >= 4);
    let q = Queue::new(pool.clone());
    const JOBS: usize = 300;
    for i in 0..JOBS {
        q.enqueue(KIND, &json!({"i": i}), EnqueueOptions::default())
            .await
            .unwrap();
    }
    let mut tasks = Vec::new();
    for w in 0..20 {
        let q = q.clone();
        tasks.push(tokio::spawn(async move {
            let owner = format!("w{w}");
            let mut mine = Vec::new();
            let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
            loop {
                let jobs = q.dequeue(&[KIND], &owner, 3, LEASE).await.unwrap();
                if jobs.is_empty() {
                    // Empty is not "no work" (ADR 0032): SKIP LOCKED passes over rows that
                    // in-flight claims hold. Stop only once no live job is left.
                    if q.live_count(&[KIND], None).await.unwrap() == 0 {
                        break;
                    }
                    assert!(
                        tokio::time::Instant::now() < deadline,
                        "live jobs never drained"
                    );
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    continue;
                }
                for job in jobs {
                    assert!(
                        q.complete(&job).await.unwrap(),
                        "owner completes its own claim"
                    );
                    mine.push(job.id);
                }
            }
            mine
        }));
    }
    let mut all = Vec::new();
    for t in tasks {
        all.extend(t.await.unwrap());
    }
    let unique: HashSet<i64> = all.iter().copied().collect();
    assert_eq!(all.len(), JOBS, "every job claimed exactly once");
    assert_eq!(unique.len(), JOBS);
    let attempts: Vec<i32> = sqlx::query_scalar("SELECT DISTINCT attempts FROM jobs")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(attempts, vec![1], "no job was claimed twice");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn concurrent_enqueues_of_one_key_create_one_live_job(pool: PgPool) {
    // The sqlx::test pool has several connections, so the claimers really race in Postgres.
    assert!(pool.options().get_max_connections() >= 4);
    let q = Queue::new(pool.clone());
    let mut tasks = Vec::new();
    for _ in 0..20 {
        let q = q.clone();
        tasks.push(tokio::spawn(async move {
            q.enqueue(KIND, &json!({}), keyed("analyse:same"))
                .await
                .unwrap()
        }));
    }
    let mut created = 0;
    for t in tasks {
        if t.await.unwrap().is_some() {
            created += 1;
        }
    }
    assert_eq!(created, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn dedupe_keys_are_unique_among_live_jobs_only(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let one = || keyed("import:acc");
    q.enqueue(KIND, &json!({}), one()).await.unwrap().unwrap();
    let job = claim_one(&q).await;
    // Running still holds the key.
    assert_eq!(q.enqueue(KIND, &json!({}), one()).await.unwrap(), None);
    // The same key under another kind is independent.
    assert!(
        q.enqueue("import-account", &json!({}), one())
            .await
            .unwrap()
            .is_some()
    );
    let unrecoverable = JobError::Unrecoverable {
        message: "gone".into(),
        code: Some("not_found".into()),
    };
    assert_eq!(
        q.fail(&job, &unrecoverable, Backoff::default())
            .await
            .unwrap(),
        FailOutcome::Failed
    );
    // Failed frees it.
    assert!(q.enqueue(KIND, &json!({}), one()).await.unwrap().is_some());
    // Jobs without a key never dedupe.
    assert!(
        q.enqueue(KIND, &json!({}), EnqueueOptions::default())
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        q.enqueue(KIND, &json!({}), EnqueueOptions::default())
            .await
            .unwrap()
            .is_some()
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn future_run_at_is_not_dequeued(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let later = OffsetDateTime::now_utc() + time::Duration::hours(1);
    let id = q
        .enqueue(
            KIND,
            &json!({}),
            EnqueueOptions {
                run_at: Some(later),
                ..EnqueueOptions::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert!(q.dequeue(&[KIND], "w", 10, LEASE).await.unwrap().is_empty());
    sqlx::query("UPDATE jobs SET run_at = now() - interval '1 minute' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(claim_one(&q).await.id, id);
}

/// The database wall clock can step backwards (NTP, VM resync; the Docker Desktop VM steps back
/// about 2.7 s every 27 s). A job enqueued to run at once must stay claimable, while a delayed
/// job still waits for its `run_at`. A step back of `d` after the enqueue is simulated by moving
/// the row's times `d` ahead of the clock. The immediate row is moved 1 h and the control row
/// (delayed by 1 s) only 3 s, so no "due within a tolerance" rule can pass both: immediacy is
/// `run_at <= created_at`, not closeness to `now()`.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn an_immediate_job_stays_claimable_after_the_clock_steps_back(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let enqueue = |run_at| {
        let q = q.clone();
        async move {
            let opts = EnqueueOptions {
                run_at,
                ..EnqueueOptions::default()
            };
            q.enqueue(KIND, &json!({}), opts).await.unwrap().unwrap()
        }
    };
    let now_id = enqueue(None).await;
    let control = enqueue(None).await;
    enqueue(Some(OffsetDateTime::now_utc() + time::Duration::hours(1))).await;
    let shift = |id: i64, sql: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query(sql).bind(id).execute(&pool).await.unwrap();
        }
    };
    shift(
        now_id,
        "UPDATE jobs SET created_at = created_at + interval '1 hour', \
           run_at = run_at + interval '1 hour' WHERE id = $1",
    )
    .await;
    shift(
        control,
        "UPDATE jobs SET created_at = created_at + interval '3 seconds', \
           run_at = created_at + interval '4 seconds' WHERE id = $1",
    )
    .await;
    let ids: Vec<i64> = q
        .dequeue(&[KIND], "w", 10, LEASE)
        .await
        .unwrap()
        .iter()
        .map(|j| j.id)
        .collect();
    assert_eq!(ids, vec![now_id], "only the immediate job is due");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn dequeue_claims_several_kinds_in_one_call(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let mut want = Vec::new();
    for kind in [KIND, "import-account", "reconcile"] {
        let id = q.enqueue(kind, &json!({}), EnqueueOptions::default()).await;
        want.push(id.unwrap().unwrap());
    }
    let got: Vec<i64> = q
        .dequeue(&[KIND, "import-account"], "w", 10, LEASE)
        .await
        .unwrap()
        .iter()
        .map(|j| j.id)
        .collect();
    assert_eq!(got, want[..2], "both named kinds, in claim order");
    assert_eq!(
        row(&pool, want[2]).await.state,
        "queued",
        "other kinds wait"
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn retryable_failure_backs_off_exponentially(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let id = q
        .enqueue(KIND, &json!({}), EnqueueOptions::default())
        .await
        .unwrap()
        .unwrap();
    let job = claim_one(&q).await;
    let out = q
        .fail(&job, &retryable("boom"), Backoff::default())
        .await
        .unwrap();
    assert!(matches!(out, FailOutcome::Retried { .. }), "{out:?}");
    let r = row(&pool, id).await;
    assert_eq!((r.state.as_str(), r.attempts), ("queued", 1));
    assert_near(r.run_in, 5.0, 1.0);
    assert_eq!(r.error_code.as_deref(), Some("engine"));
    assert_eq!(r.last_error.as_deref(), Some("boom"));
    assert_eq!(r.lease_token, None, "the lease is cleared");

    sqlx::query("UPDATE jobs SET run_at = now() - interval '1 minute' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let job = claim_one(&q).await;
    assert_eq!(job.attempts, 2);
    q.fail(&job, &retryable("boom again"), Backoff::default())
        .await
        .unwrap();
    let r = row(&pool, id).await;
    assert_eq!((r.state.as_str(), r.attempts), ("queued", 2));
    assert_near(r.run_in, 10.0, 1.0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn final_retryable_failure_is_dead(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let id = q
        .enqueue(KIND, &json!({}), keyed("analyse:dead"))
        .await
        .unwrap()
        .unwrap();
    for attempt in 1..=3 {
        sqlx::query("UPDATE jobs SET run_at = now() - interval '1 minute' WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        let job = claim_one(&q).await;
        assert_eq!(job.attempts, attempt);
        assert_eq!(job.is_final_attempt(), attempt == 3);
        let out = q
            .fail(&job, &retryable("boom"), Backoff::default())
            .await
            .unwrap();
        if attempt < 3 {
            assert!(matches!(out, FailOutcome::Retried { .. }), "{out:?}");
        } else {
            assert_eq!(out, FailOutcome::Dead);
        }
    }
    let r = row(&pool, id).await;
    assert_eq!((r.state.as_str(), r.attempts), ("dead", 3));
    assert!(r.finished);
    assert!(
        q.live_states(KIND, &["analyse:dead".to_owned()])
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn unrecoverable_is_failed_immediately(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let id = q
        .enqueue(KIND, &json!({}), EnqueueOptions::default())
        .await
        .unwrap()
        .unwrap();
    let job = claim_one(&q).await;
    let err = JobError::Unrecoverable {
        message: "account not found".into(),
        code: Some("not_found".into()),
    };
    assert_eq!(
        q.fail(&job, &err, Backoff::default()).await.unwrap(),
        FailOutcome::Failed
    );
    let r = row(&pool, id).await;
    assert_eq!((r.state.as_str(), r.attempts), ("failed", 1));
    assert_eq!(r.error_code.as_deref(), Some("not_found"));
    assert_eq!(r.last_error.as_deref(), Some("account not found"));
    assert!(r.finished);
    assert!(q.dequeue(&[KIND], "w", 10, LEASE).await.unwrap().is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn release_does_not_use_an_attempt(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let id = q
        .enqueue(KIND, &json!({}), EnqueueOptions::default())
        .await
        .unwrap()
        .unwrap();
    let job = claim_one(&q).await;
    let open = JobError::Release {
        delay: Duration::from_secs(30),
        reason: "circuit_open".into(),
    };
    let out = q.fail(&job, &open, Backoff::default()).await.unwrap();
    assert!(matches!(out, FailOutcome::Released { .. }), "{out:?}");
    let r = row(&pool, id).await;
    assert_eq!((r.state.as_str(), r.attempts), ("queued", 0));
    assert_near(r.run_in, 30.0, 1.0);
    assert_eq!(r.error_code.as_deref(), Some("circuit_open"));
    assert_eq!(r.lease_token, None);
    assert!(!r.finished);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn release_on_the_final_attempt_is_not_dead(pool: PgPool) {
    // Shutdown or a closed engine pool on attempt 3 of 3 must not dead-letter the job.
    let q = Queue::new(pool.clone());
    let id = q
        .enqueue(KIND, &json!({}), EnqueueOptions::default())
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE jobs SET attempts = 2 WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let job = claim_one(&q).await;
    assert!(job.is_final_attempt());
    let shutdown = JobError::Release {
        delay: Duration::ZERO,
        reason: "shutdown".into(),
    };
    let out = q.fail(&job, &shutdown, Backoff::default()).await.unwrap();
    assert!(matches!(out, FailOutcome::Released { .. }), "{out:?}");
    // A zero-delay release keeps the job's place: `run_at` is unchanged (ADR 0032).
    assert_eq!(out, FailOutcome::Released { run_at: job.run_at });
    let r = row(&pool, id).await;
    assert_eq!((r.state.as_str(), r.attempts), ("queued", 2));
    // A fresh worker gets the final attempt again and can complete it.
    let again = claim_one(&q).await;
    assert_eq!(again.attempts, 3);
    assert!(q.complete(&again).await.unwrap());
    assert_eq!(row(&pool, id).await.state, "done");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn recurring_job_requeues_on_complete_and_failure(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let opts = EnqueueOptions {
        recur_every: Some(Duration::from_secs(300)),
        max_attempts: 1,
        ..keyed("reconcile")
    };
    let id = q
        .enqueue("reconcile", &json!({}), opts)
        .await
        .unwrap()
        .unwrap();
    let take = || async {
        q.dequeue(&["reconcile"], "w", 1, LEASE)
            .await
            .unwrap()
            .pop()
            .unwrap()
    };

    let job = take().await;
    assert!(q.complete(&job).await.unwrap());
    let r = row(&pool, id).await;
    assert_eq!((r.state.as_str(), r.attempts), ("queued", 0));
    assert_near(r.run_in, 300.0, 2.0);
    assert!(!r.finished);

    sqlx::query("UPDATE jobs SET run_at = now() - interval '1 minute' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let job = take().await;
    // max_attempts 1: a one-shot job would be dead now, a recurring one is rescheduled.
    let out = q
        .fail(&job, &retryable("db down"), Backoff::default())
        .await
        .unwrap();
    assert!(matches!(out, FailOutcome::Retried { .. }), "{out:?}");
    let r = row(&pool, id).await;
    assert_eq!((r.state.as_str(), r.attempts), ("queued", 0));
    assert_near(r.run_in, 300.0, 2.0);
    assert_eq!(r.last_error.as_deref(), Some("db down"));

    sqlx::query("UPDATE jobs SET run_at = now() - interval '1 minute' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let job = take().await;
    let err = JobError::Unrecoverable {
        message: "bad".into(),
        code: None,
    };
    assert!(matches!(
        q.fail(&job, &err, Backoff::default()).await.unwrap(),
        FailOutcome::Retried { .. }
    ));
    let r = row(&pool, id).await;
    assert_eq!((r.state.as_str(), r.attempts), ("queued", 0));
    assert_near(r.run_in, 300.0, 2.0);
    // Still one row: the recurring job is the same live job.
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn stale_lease_token_cannot_renew_complete_or_fail(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let id = q
        .enqueue(KIND, &json!({}), EnqueueOptions::default())
        .await
        .unwrap()
        .unwrap();
    let stale = claim_one(&q).await;
    assert!(
        q.renew(&stale, LEASE).await.unwrap(),
        "the holder can renew"
    );
    // The lease is lost (as the T14 reaper would do) and another worker claims the job.
    sqlx::query("UPDATE jobs SET state = 'queued', attempts = attempts - 1, lease_token = NULL WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let fresh = claim_one(&q).await;
    assert_ne!(fresh.lease_token, stale.lease_token);

    assert!(!q.renew(&stale, LEASE).await.unwrap());
    assert!(!q.complete(&stale).await.unwrap());
    assert_eq!(
        q.fail(&stale, &retryable("late"), Backoff::default())
            .await
            .unwrap(),
        FailOutcome::LeaseLost
    );
    let release = JobError::Release {
        delay: Duration::ZERO,
        reason: "shutdown".into(),
    };
    assert_eq!(
        q.fail(&stale, &release, Backoff::default()).await.unwrap(),
        FailOutcome::LeaseLost
    );
    let r = row(&pool, id).await;
    assert_eq!((r.state.as_str(), r.attempts), ("running", 1));
    assert_eq!(r.lease_token, Some(fresh.lease_token));

    assert!(q.complete(&fresh).await.unwrap());
    // Once done, nothing more can change it.
    assert!(!q.complete(&fresh).await.unwrap());
    assert!(!q.renew(&fresh, LEASE).await.unwrap());
    assert_eq!(row(&pool, id).await.state, "done");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn renew_extends_the_lease(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let id = q
        .enqueue(KIND, &json!({}), EnqueueOptions::default())
        .await
        .unwrap()
        .unwrap();
    let job = q
        .dequeue(&[KIND], "w", 1, Duration::from_secs(2))
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert!(q.renew(&job, Duration::from_secs(600)).await.unwrap());
    let lease_in: f64 = sqlx::query_scalar(
        "SELECT extract(epoch FROM lease_until - now())::float8 FROM jobs WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_near(lease_in, 600.0, 10.0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn last_error_is_truncated_to_2000_characters(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let id = q
        .enqueue(KIND, &json!({}), EnqueueOptions::default())
        .await
        .unwrap()
        .unwrap();
    let job = claim_one(&q).await;
    let long = "é".repeat(5000);
    q.fail(&job, &retryable(&long), Backoff::default())
        .await
        .unwrap();
    let r = row(&pool, id).await;
    assert_eq!(r.last_error.unwrap().chars().count(), 2000);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn enqueue_notifies_jobs_channel(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let mut listener = PgListener::connect_with(&pool).await.unwrap();
    listener.listen(JOBS_CHANNEL).await.unwrap();
    q.enqueue(KIND, &json!({}), keyed("k"))
        .await
        .unwrap()
        .unwrap();
    let n = tokio::time::timeout(Duration::from_secs(10), listener.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(n.channel(), "jobs");
    assert_eq!(n.payload(), KIND);
    // A deduped enqueue sends nothing: the next notification is the other kind's.
    assert_eq!(q.enqueue(KIND, &json!({}), keyed("k")).await.unwrap(), None);
    q.enqueue("import-account", &json!({}), EnqueueOptions::default())
        .await
        .unwrap()
        .unwrap();
    let n = tokio::time::timeout(Duration::from_secs(10), listener.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(n.payload(), "import-account");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn immediate_release_notifies_jobs_channel(pool: PgPool) {
    let q = Queue::new(pool.clone());
    q.enqueue(KIND, &json!({}), EnqueueOptions::default())
        .await
        .unwrap()
        .unwrap();
    let job = claim_one(&q).await;
    let mut listener = PgListener::connect_with(&pool).await.unwrap();
    listener.listen(JOBS_CHANNEL).await.unwrap();
    let release = JobError::Release {
        delay: Duration::ZERO,
        reason: "shutdown".into(),
    };
    q.fail(&job, &release, Backoff::default()).await.unwrap();
    let n = tokio::time::timeout(Duration::from_secs(10), listener.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(n.payload(), KIND);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn live_count_scopes_to_user_key(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let alice = Uuid::new_v4();
    let bob = Uuid::new_v4();
    let as_user = |u: Uuid| EnqueueOptions {
        user_key: Some(u),
        ..EnqueueOptions::default()
    };
    q.enqueue(KIND, &json!({}), as_user(alice)).await.unwrap();
    q.enqueue("import-account", &json!({}), as_user(alice))
        .await
        .unwrap();
    q.enqueue(KIND, &json!({}), as_user(bob)).await.unwrap();
    q.enqueue("reconcile", &json!({}), EnqueueOptions::default())
        .await
        .unwrap();
    let done = q
        .enqueue(KIND, &json!({}), as_user(alice))
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE jobs SET state = 'done' WHERE id = $1")
        .bind(done)
        .execute(&pool)
        .await
        .unwrap();

    let both = [KIND, "import-account"];
    assert_eq!(q.live_count(&both, Some(alice)).await.unwrap(), 2);
    assert_eq!(q.live_count(&[KIND], Some(alice)).await.unwrap(), 1);
    assert_eq!(q.live_count(&both, Some(bob)).await.unwrap(), 1);
    assert_eq!(q.live_count(&both, None).await.unwrap(), 3);
    assert_eq!(
        q.live_count(&[KIND, "import-account", "reconcile"], None)
            .await
            .unwrap(),
        4
    );
    assert_eq!(q.live_count(&both, Some(Uuid::new_v4())).await.unwrap(), 0);
    assert_eq!(q.live_count(&[], None).await.unwrap(), 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn every_query_has_a_timeout(pool: PgPool) {
    let q = Queue::new(pool.clone()).with_query_timeout(Duration::from_millis(300));
    let id = q
        .enqueue(KIND, &json!({}), EnqueueOptions::default())
        .await
        .unwrap()
        .unwrap();
    let job = claim_one(&q).await;
    // Another transaction holds the row lock, so the fenced complete blocks on it.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM jobs WHERE id = $1 FOR UPDATE")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    let started = std::time::Instant::now();
    let err = q.complete(&job).await.unwrap_err();
    assert!(
        matches!(err, QueueError::Timeout { op: "complete", .. }),
        "{err:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "bounded by the timeout"
    );
    tx.rollback().await.unwrap();
    // The job is untouched and still completes once the lock is gone.
    assert!(q.complete(&job).await.unwrap());
}
