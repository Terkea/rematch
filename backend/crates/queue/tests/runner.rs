//! Queue runner (Task 14): concurrency per kind, NOTIFY wake-ups with a polling fallback, the
//! per-handler hard deadline, graceful and forced shutdown, and failure logging. Behavioural
//! reference: apps/worker/src/start-workers.ts and apps/worker/src/lifecycle/shutdown.ts.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use queue::queue::{Backoff, Queue};
use queue::runner::{Handler, JobContext, Runner, RunnerConfig};
use queue::{EnqueueOptions, Job, JobError};
use serde_json::{Value, json};
use sqlx::PgPool;
use testkit::wait::wait_for;

const KIND: &str = "analyse-game";
const POLL: Duration = Duration::from_millis(25);

// ---- test handler ----

type HandleFn =
    dyn Fn(Job, JobContext) -> BoxFuture<'static, Result<Value, JobError>> + Send + Sync;

struct TestHandler {
    kind: &'static str,
    f: Box<HandleFn>,
    dead: Mutex<Vec<(i64, String)>>,
}

impl Handler for TestHandler {
    fn kind(&self) -> &'static str {
        self.kind
    }

    fn handle(&self, job: Job, ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        (self.f)(job, ctx)
    }

    fn on_dead<'a>(&'a self, job: &'a Job, reason: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            self.dead.lock().unwrap().push((job.id, reason.to_owned()));
        })
    }
}

fn handler<F, Fut>(kind: &'static str, f: F) -> Arc<TestHandler>
where
    F: Fn(Job, JobContext) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Value, JobError>> + Send + 'static,
{
    Arc::new(TestHandler {
        kind,
        f: Box::new(move |job, ctx| Box::pin(f(job, ctx))),
        dead: Mutex::new(Vec::new()),
    })
}

/// `fast_for_tests` with a 6 s lease (renewed every 500 ms): none of these tests wants a lease
/// to expire, and a runner that must stay alive keeps its jobs through a loaded host and a
/// ±3 s step of the VM clock (ADR 0032).
fn cfg(concurrency: usize) -> RunnerConfig {
    RunnerConfig {
        concurrency,
        lease: Duration::from_secs(6),
        ..RunnerConfig::fast_for_tests()
    }
}

#[derive(Debug, sqlx::FromRow)]
struct Row {
    state: String,
    attempts: i32,
    stalls: i32,
    error_code: Option<String>,
    lease_token: Option<uuid::Uuid>,
    lease_owner: Option<String>,
    /// `run_at - now()` in seconds, measured by the database.
    run_in: f64,
}

async fn row(pool: &PgPool, id: i64) -> Row {
    sqlx::query_as(
        "SELECT state::text AS state, attempts, stalls, error_code, lease_token, lease_owner, \
           extract(epoch FROM run_at - now())::float8 AS run_in \
         FROM jobs WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("job row")
}

/// Options for a job that is due at once with a `run_at` a minute in the past, so a step back of
/// the VM clock (Postgres included) between enqueue and claim cannot make it look delayed.
fn due() -> EnqueueOptions {
    EnqueueOptions {
        run_at: Some(time::OffsetDateTime::now_utc() - time::Duration::minutes(1)),
        ..EnqueueOptions::default()
    }
}

async fn enqueue(q: &Queue, kind: &str, payload: Value) -> i64 {
    q.enqueue(kind, &payload, due()).await.unwrap().unwrap()
}

async fn count_in(pool: &PgPool, state: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM jobs WHERE state = $1::job_state")
        .bind(state)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn wait_state(pool: &PgPool, id: i64, state: &str, within: Duration) -> Row {
    wait_for(&format!("job {id} {state}"), within, POLL, || async {
        let r = row(pool, id).await;
        (r.state == state).then_some(r)
    })
    .await
}

/// Waits until a runner's listener session has issued `LISTEN "jobs"` (its last query).
async fn wait_listening(pool: &PgPool, sessions: i64) {
    wait_for("LISTEN jobs", Duration::from_secs(10), POLL, || async {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity \
             WHERE datname = current_database() AND query = 'LISTEN \"jobs\"'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        (n >= sessions).then_some(())
    })
    .await
}

/// Tracks how many handler calls run at once, and the maximum seen.
#[derive(Default)]
struct Gauge {
    now: AtomicUsize,
    max: AtomicUsize,
    calls: AtomicUsize,
}

impl Gauge {
    async fn run_for(&self, d: Duration) {
        let n = self.now.fetch_add(1, Ordering::SeqCst) + 1;
        self.max.fetch_max(n, Ordering::SeqCst);
        self.calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(d).await;
        self.now.fetch_sub(1, Ordering::SeqCst);
    }
}

// ---- concurrency ----

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn runs_jobs_up_to_concurrency(pool: PgPool) {
    let q = Queue::new(pool.clone());
    for i in 0..10 {
        enqueue(&q, KIND, json!({ "i": i })).await;
    }
    let gauge = Arc::new(Gauge::default());
    let g = gauge.clone();
    let h = handler(KIND, move |_, _| {
        let g = g.clone();
        async move {
            g.run_for(Duration::from_millis(200)).await;
            Ok(json!({}))
        }
    });
    let runner = Runner::spawn(q.clone(), h, cfg(3));
    wait_for("10 done", Duration::from_secs(20), POLL, || async {
        (count_in(&pool, "done").await == 10).then_some(())
    })
    .await;
    assert_eq!(gauge.max.load(Ordering::SeqCst), 3, "at most 3 at once");
    assert_eq!(gauge.calls.load(Ordering::SeqCst), 10);
    runner.stop();
    runner.wait_idle().await;
    assert_eq!(runner.in_flight(), 0);
    assert_eq!(runner.stats().completed, 10);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn concurrency_is_per_job_kind(pool: PgPool) {
    let q = Queue::new(pool.clone());
    for i in 0..4 {
        enqueue(&q, "import-account", json!({ "i": i })).await;
        enqueue(&q, KIND, json!({ "i": i })).await;
    }
    let (imports, analyses) = (Arc::new(Gauge::default()), Arc::new(Gauge::default()));
    let gi = imports.clone();
    let import = handler("import-account", move |job, _| {
        let g = gi.clone();
        async move {
            assert_eq!(job.kind, "import-account");
            g.run_for(Duration::from_millis(150)).await;
            Ok(json!({}))
        }
    });
    let ga = analyses.clone();
    let analyse = handler(KIND, move |job, _| {
        let g = ga.clone();
        async move {
            assert_eq!(job.kind, KIND);
            g.run_for(Duration::from_millis(150)).await;
            Ok(json!({}))
        }
    });
    let r1 = Runner::spawn(q.clone(), import, cfg(1));
    let r2 = Runner::spawn(q.clone(), analyse, cfg(2));
    wait_for("8 done", Duration::from_secs(20), POLL, || async {
        (count_in(&pool, "done").await == 8).then_some(())
    })
    .await;
    assert_eq!(imports.max.load(Ordering::SeqCst), 1);
    assert_eq!(analyses.max.load(Ordering::SeqCst), 2);
    assert_eq!(imports.calls.load(Ordering::SeqCst), 4);
    assert_eq!(analyses.calls.load(Ordering::SeqCst), 4);
    r1.stop();
    r2.stop();
    r1.wait_idle().await;
    r2.wait_idle().await;
}

// ---- wake-ups ----

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn notify_wakes_idle_runner_quickly(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Instant>();
    let h = handler(KIND, move |_, _| {
        let tx = tx.clone();
        async move {
            tx.send(Instant::now()).unwrap();
            Ok(json!({}))
        }
    });
    let runner = Runner::spawn(
        q.clone(),
        h,
        RunnerConfig {
            idle_poll: Duration::from_secs(5),
            // No reap ticks either: only the notification can wake the loop.
            reap_every: Duration::from_secs(60),
            ..cfg(1)
        },
    );
    wait_listening(&pool, 1).await;
    // Let the runner finish its first (empty) claim and go idle.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let enqueued = Instant::now();
    let past = EnqueueOptions {
        run_at: Some(time::OffsetDateTime::now_utc() - time::Duration::seconds(10)),
        ..EnqueueOptions::default()
    };
    q.enqueue(KIND, &json!({}), past).await.unwrap().unwrap();
    let started = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("job started")
        .unwrap();
    let latency = started - enqueued;
    // Woken by NOTIFY, not by the 5 s poll.
    assert!(latency < Duration::from_secs(1), "{latency:?}");
    runner.stop();
    runner.wait_idle().await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn polls_when_no_notification_arrives(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let h = handler(KIND, |_, _| async { Ok(json!({})) });
    let runner = Runner::spawn(
        q.clone(),
        h,
        RunnerConfig {
            idle_poll: Duration::from_millis(200),
            // No reap ticks either: only the poll can find the job.
            reap_every: Duration::from_secs(60),
            ..cfg(1)
        },
    );
    wait_listening(&pool, 1).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    // A plain INSERT sends no NOTIFY: only the poll can find it.
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO jobs (kind, payload, run_at) \
             VALUES ($1, '{}', now() - interval '10 seconds') RETURNING id",
    )
    .bind(KIND)
    .fetch_one(&pool)
    .await
    .unwrap();
    wait_state(&pool, id, "done", Duration::from_secs(10)).await;
    runner.stop();
    runner.wait_idle().await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn ignores_notifications_for_other_kinds(pool: PgPool) {
    // A notification for another kind does not make the runner claim anything of that kind.
    let q = Queue::new(pool.clone());
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let h = handler(KIND, move |_, _| {
        let c = c.clone();
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(json!({}))
        }
    });
    let runner = Runner::spawn(q.clone(), h, cfg(1));
    wait_listening(&pool, 1).await;
    let other = enqueue(&q, "import-account", json!({})).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(row(&pool, other).await.state, "queued");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    runner.stop();
    runner.wait_idle().await;
}

// ---- hard deadline ----

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn hard_timeout_fails_as_retryable_timeout(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let id = enqueue(&q, KIND, json!({})).await;
    let cancelled = Arc::new(AtomicUsize::new(0));
    let c = cancelled.clone();
    let h = handler(KIND, move |_, ctx| {
        let c = c.clone();
        async move {
            ctx.cancel.cancelled().await;
            c.fetch_add(1, Ordering::SeqCst);
            // Ignores the cancellation and sleeps forever: the runner must still give up.
            std::future::pending::<()>().await;
            Ok(json!({}))
        }
    });
    let runner = Runner::spawn(
        q.clone(),
        h,
        RunnerConfig {
            hard_timeout: Some(Duration::from_millis(200)),
            // A long backoff keeps the retry queued so the row can be inspected.
            backoff: Backoff {
                base: Duration::from_secs(60),
            },
            // A lease far beyond the deadline and no reaper: only the hard timeout acts.
            lease: Duration::from_secs(10),
            renew_every: Duration::from_secs(1),
            stall_grace: Duration::from_secs(3),
            reap_every: Duration::from_secs(60),
            ..cfg(1)
        },
    );
    // `queued` before the claim too: wait for the retry itself.
    let r = wait_for("timeout retry", Duration::from_secs(10), POLL, || async {
        let r = row(&pool, id).await;
        (r.error_code.is_some()).then_some(r)
    })
    .await;
    assert_eq!(r.state, "queued");
    assert_eq!(r.error_code.as_deref(), Some("timeout"));
    assert_eq!(r.attempts, 1);
    assert!(r.run_in > 30.0, "retried with backoff: {}", r.run_in);
    assert_eq!(cancelled.load(Ordering::SeqCst), 1, "ctx.cancel fired");
    assert_eq!(runner.stats().timed_out, 1);
    runner.stop();
    runner.wait_idle().await;
}

// ---- shutdown ----

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn stop_then_wait_idle_drains_in_flight(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let mut ids = Vec::new();
    for i in 0..3 {
        ids.push(enqueue(&q, KIND, json!({ "i": i })).await);
    }
    let gauge = Arc::new(Gauge::default());
    let g = gauge.clone();
    let h = handler(KIND, move |_, _| {
        let g = g.clone();
        async move {
            g.run_for(Duration::from_millis(800)).await;
            Ok(json!({}))
        }
    });
    let runner = Runner::spawn(q.clone(), h, cfg(3));
    wait_for("3 running", Duration::from_secs(10), POLL, || async {
        (runner.in_flight() == 3).then_some(())
    })
    .await;
    runner.stop();
    let late = enqueue(&q, KIND, json!({ "late": true })).await;
    tokio::time::timeout(Duration::from_secs(10), runner.wait_idle())
        .await
        .expect("drained");
    assert_eq!(runner.in_flight(), 0);
    for id in ids {
        assert_eq!(row(&pool, id).await.state, "done");
    }
    // Stopped runners claim nothing more.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let r = row(&pool, late).await;
    assert_eq!((r.state.as_str(), r.attempts), ("queued", 0));
    assert_eq!(gauge.calls.load(Ordering::SeqCst), 3);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn force_cancels_handlers_and_releases_leases(pool: PgPool) {
    let q = Queue::new(pool.clone());
    // One handler ignores cancellation, one turns it into a retryable error: neither may use
    // an attempt.
    let stubborn = enqueue(&q, KIND, json!({ "mode": "ignore" })).await;
    let erroring = enqueue(&q, KIND, json!({ "mode": "error" })).await;
    let cancelled = Arc::new(AtomicUsize::new(0));
    let c = cancelled.clone();
    let h = handler(KIND, move |job, ctx| {
        let c = c.clone();
        async move {
            ctx.cancel.cancelled().await;
            c.fetch_add(1, Ordering::SeqCst);
            if job.payload["mode"] == "ignore" {
                std::future::pending::<()>().await;
            }
            Err(JobError::Retryable {
                message: "interrupted".into(),
                code: Some("engine".into()),
            })
        }
    });
    let runner = Runner::spawn(q.clone(), h, cfg(2));
    wait_for("2 running", Duration::from_secs(10), POLL, || async {
        (runner.in_flight() == 2).then_some(())
    })
    .await;
    let started = Instant::now();
    tokio::time::timeout(Duration::from_secs(10), runner.force())
        .await
        .expect("force is bounded");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(cancelled.load(Ordering::SeqCst), 2);
    assert_eq!(runner.in_flight(), 0);
    for id in [stubborn, erroring] {
        let r = row(&pool, id).await;
        assert_eq!(r.state, "queued", "job {id}");
        assert_eq!(r.attempts, 0, "job {id}: no attempt used");
        assert_eq!(r.lease_token, None);
        assert_eq!(r.lease_owner, None);
        // Ready at once; a clock step between the write and this read can add up to ~3 s.
        assert!(r.run_in <= 5.0, "ready at once: {}", r.run_in);
        assert_eq!(r.error_code.as_deref(), Some("shutdown"));
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn force_releases_retryable_failures_but_keeps_unrecoverable_ones(pool: PgPool) {
    // Every handler stops promptly, so `force` sees their own writes (not `release_owned`):
    // a retryable error and a panic after force are releases, an unrecoverable verdict is kept.
    // The runner gets its own pool so its writes never wait for the shared test-pool permits
    // and land within the force grace period.
    let own = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect_with((*pool.connect_options()).clone())
        .await
        .unwrap();
    let q = Queue::new(own);
    let erroring = enqueue(&q, KIND, json!({ "mode": "error" })).await;
    let panicking = enqueue(&q, KIND, json!({ "mode": "panic" })).await;
    let verdict = enqueue(&q, KIND, json!({ "mode": "unrecoverable" })).await;
    let h = handler(KIND, |job, ctx| async move {
        ctx.cancel.cancelled().await;
        match job.payload["mode"].as_str().unwrap() {
            "panic" => panic!("cancelled mid-write"),
            "unrecoverable" => Err(JobError::Unrecoverable {
                message: "account gone".into(),
                code: Some("not_found".into()),
            }),
            _ => Err(JobError::Retryable {
                message: "interrupted".into(),
                code: Some("engine".into()),
            }),
        }
    });
    let runner = Runner::spawn(q.clone(), h, cfg(3));
    wait_for("3 running", Duration::from_secs(10), POLL, || async {
        (runner.in_flight() == 3).then_some(())
    })
    .await;
    runner.force().await;
    let stats = runner.stats();
    assert_eq!((stats.released, stats.failed, stats.retried), (2, 1, 0));
    let r = row(&pool, verdict).await;
    assert_eq!(r.state, "failed", "an unrecoverable verdict stays failed");
    assert_eq!(r.error_code.as_deref(), Some("not_found"));
    for id in [erroring, panicking] {
        let r = row(&pool, id).await;
        assert_eq!((r.state.as_str(), r.attempts), ("queued", 0), "job {id}");
        assert_eq!(r.error_code.as_deref(), Some("shutdown"));
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn force_releases_only_this_runners_jobs(pool: PgPool) {
    // Two runners of one process share a worker_id; forcing one must not hand the other's
    // running jobs back.
    let q = Queue::new(pool.clone());
    let analyse = enqueue(&q, KIND, json!({})).await;
    let import = enqueue(&q, "import-account", json!({})).await;
    let hang = |_: Job, _: JobContext| async {
        std::future::pending::<()>().await;
        Ok(json!({}))
    };
    let shared = RunnerConfig {
        worker_id: "shared-worker".to_owned(),
        // The other runner's job must survive the ~2 s force on a busy host.
        lease: Duration::from_secs(10),
        renew_every: Duration::from_secs(1),
        stall_grace: Duration::from_secs(3),
        ..cfg(1)
    };
    let ra = Runner::spawn(q.clone(), handler(KIND, hang), shared.clone());
    let rb = Runner::spawn(q.clone(), handler("import-account", hang), shared);
    assert_eq!(ra.lease_owner(), "shared-worker/analyse-game");
    assert_eq!(rb.lease_owner(), "shared-worker/import-account");
    wait_for("both running", Duration::from_secs(10), POLL, || async {
        (ra.in_flight() == 1 && rb.in_flight() == 1).then_some(())
    })
    .await;
    let before = wait_state(&pool, import, "running", Duration::from_secs(5)).await;
    assert_eq!(
        before.lease_owner.as_deref(),
        Some("shared-worker/import-account")
    );

    ra.force().await;
    let r = row(&pool, analyse).await;
    assert_eq!((r.state.as_str(), r.attempts), ("queued", 0));
    let after = row(&pool, import).await;
    assert_eq!(
        after.state, "running",
        "the other runner's job is untouched"
    );
    assert_eq!(after.lease_token, before.lease_token);
    assert_eq!(after.lease_owner, before.lease_owner);
    assert_eq!(rb.in_flight(), 1);

    rb.force().await;
    let r = row(&pool, import).await;
    assert_eq!((r.state.as_str(), r.attempts), ("queued", 0));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn shutdown_drains_within_grace_or_releases(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let quick = enqueue(&q, KIND, json!({ "ms": 100 })).await;
    let slow = enqueue(&q, KIND, json!({ "ms": 60_000 })).await;
    let h = handler(KIND, |job, _| async move {
        let ms = job.payload["ms"].as_u64().unwrap();
        tokio::time::sleep(Duration::from_millis(ms)).await;
        Ok(json!({}))
    });
    let runner = Runner::spawn(
        q.clone(),
        h,
        RunnerConfig {
            // The slow job must still hold its lease when the grace period ends.
            lease: Duration::from_secs(10),
            renew_every: Duration::from_secs(1),
            stall_grace: Duration::from_secs(3),
            ..cfg(2)
        },
    );
    wait_for("2 running", Duration::from_secs(10), POLL, || async {
        (runner.in_flight() == 2).then_some(())
    })
    .await;
    let drained = tokio::time::timeout(
        Duration::from_secs(15),
        runner.shutdown(Duration::from_millis(1500)),
    )
    .await
    .expect("shutdown is bounded");
    assert!(
        !drained,
        "the slow job did not finish within the grace period"
    );
    assert_eq!(row(&pool, quick).await.state, "done");
    let r = row(&pool, slow).await;
    assert_eq!((r.state.as_str(), r.attempts), ("queued", 0));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn shutdown_is_re_entrant(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let id = enqueue(&q, KIND, json!({})).await;
    let h = handler(KIND, |_, _| async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        Ok(json!({}))
    });
    let runner = Runner::spawn(q.clone(), h, cfg(1));
    wait_for("running", Duration::from_secs(10), POLL, || async {
        (runner.in_flight() == 1).then_some(())
    })
    .await;
    runner.stop();
    runner.stop();
    let (a, b) = tokio::join!(
        runner.shutdown(Duration::from_secs(10)),
        runner.shutdown(Duration::from_secs(10))
    );
    assert!(a && b, "both callers see a clean drain");
    runner.wait_idle().await;
    runner.force().await;
    runner.force().await;
    runner.abort();
    assert!(runner.shutdown(Duration::from_secs(1)).await);
    let r = row(&pool, id).await;
    assert_eq!((r.state.as_str(), r.attempts), ("done", 1));
}

// ---- failures and logging ----

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn final_failure_is_dead_and_calls_on_dead_once(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let id = q
        .enqueue(
            KIND,
            &json!({}),
            EnqueueOptions {
                max_attempts: 2,
                ..due()
            },
        )
        .await
        .unwrap()
        .unwrap();
    let h = handler(KIND, |_, _| async {
        Err(JobError::Retryable {
            message: "engine crashed".into(),
            code: Some("engine".into()),
        })
    });
    let runner = Runner::spawn(q.clone(), h.clone(), cfg(1));
    let r = wait_state(&pool, id, "dead", Duration::from_secs(10)).await;
    assert_eq!(r.attempts, 2);
    runner.stop();
    runner.wait_idle().await;
    assert_eq!(
        *h.dead.lock().unwrap(),
        vec![(id, "engine crashed".to_owned())]
    );
    let stats = runner.stats();
    assert_eq!((stats.retried, stats.dead), (1, 1));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn on_dead_reason_is_truncated_to_2000_characters(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let id = q
        .enqueue(
            KIND,
            &json!({}),
            EnqueueOptions {
                max_attempts: 1,
                ..due()
            },
        )
        .await
        .unwrap()
        .unwrap();
    let h = handler(KIND, |_, _| async {
        Err(JobError::Retryable {
            message: "é".repeat(3000),
            code: None,
        })
    });
    let runner = Runner::spawn(q.clone(), h.clone(), cfg(1));
    wait_state(&pool, id, "dead", Duration::from_secs(10)).await;
    runner.stop();
    runner.wait_idle().await;
    let dead = h.dead.lock().unwrap().clone();
    assert_eq!(dead.len(), 1);
    assert_eq!(dead[0].1.chars().count(), 2000);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn release_requeues_without_an_attempt_and_is_metered(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let id = enqueue(&q, KIND, json!({})).await;
    let h = handler(KIND, |_, _| async {
        Err(JobError::Release {
            delay: Duration::from_secs(30),
            reason: "circuit_open".into(),
        })
    });
    let runner = Runner::spawn(q.clone(), h.clone(), cfg(1));
    wait_for("released", Duration::from_secs(10), POLL, || async {
        (runner.stats().released == 1).then_some(())
    })
    .await;
    runner.stop();
    runner.wait_idle().await;
    let r = row(&pool, id).await;
    assert_eq!((r.state.as_str(), r.attempts), ("queued", 0));
    assert_eq!(r.error_code.as_deref(), Some("circuit_open"));
    assert!(h.dead.lock().unwrap().is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_panicking_handler_is_a_retryable_failure(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let id = enqueue(&q, KIND, json!({})).await;
    let h = handler(KIND, |_, _| async {
        if true {
            panic!("boom");
        }
        Ok(json!({}))
    });
    let runner = Runner::spawn(
        q.clone(),
        h,
        RunnerConfig {
            backoff: Backoff {
                base: Duration::from_secs(60),
            },
            ..cfg(1)
        },
    );
    let r = wait_for("panic recorded", Duration::from_secs(10), POLL, || async {
        let r = row(&pool, id).await;
        (r.error_code.is_some()).then_some(r)
    })
    .await;
    assert_eq!(r.state, "queued");
    assert_eq!(r.error_code.as_deref(), Some("panic"));
    assert_eq!(r.attempts, 1);
    assert_eq!(r.stalls, 0);
    runner.stop();
    runner.wait_idle().await;
}

/// A minimal tracing subscriber that records the level and fields of warnings and errors.
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<Captured>>>);

type Captured = (tracing::Level, HashMap<String, String>);

struct Fields<'a>(&'a mut HashMap<String, String>);

impl tracing::field::Visit for Fields<'_> {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }
}

impl tracing::Subscriber for Capture {
    fn enabled(&self, meta: &tracing::Metadata<'_>) -> bool {
        // Warnings and errors only: the subscriber is global and sees every test's events.
        *meta.level() <= tracing::Level::WARN
    }

    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let mut fields = HashMap::new();
        event.record(&mut Fields(&mut fields));
        self.0
            .lock()
            .unwrap()
            .push((*event.metadata().level(), fields));
    }

    fn enter(&self, _: &tracing::span::Id) {}

    fn exit(&self, _: &tracing::span::Id) {}
}

/// The process-wide capturing subscriber, installed once. A thread-scoped dispatcher misses
/// events when other tests in this binary register the same callsites concurrently, so it is
/// global; tests filter on a kind only they use.
fn capture() -> &'static Capture {
    static CAPTURE: OnceLock<Capture> = OnceLock::new();
    CAPTURE.get_or_init(|| {
        let c = Capture::default();
        tracing::subscriber::set_global_default(c.clone()).expect("no other global subscriber");
        c
    })
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn failed_jobs_are_logged_with_id_kind_and_attempt(pool: PgPool) {
    const LOGGED: &str = "logged-kind";
    let capture = capture();
    let q = Queue::new(pool.clone());
    let id = q
        .enqueue(
            LOGGED,
            &json!({}),
            EnqueueOptions {
                max_attempts: 1,
                ..due()
            },
        )
        .await
        .unwrap()
        .unwrap();
    let h = handler(LOGGED, |_, _| async {
        Err(JobError::Retryable {
            message: "engine crashed".into(),
            code: Some("engine".into()),
        })
    });
    let runner = Runner::spawn(q.clone(), h, cfg(1));
    wait_state(&pool, id, "dead", Duration::from_secs(10)).await;
    runner.stop();
    runner.wait_idle().await;
    let events = capture.0.lock().unwrap().clone();
    let failed = events
        .iter()
        .find(|(_, f)| {
            f.get("message").map(String::as_str) == Some("job failed")
                && f.get("kind").map(String::as_str) == Some(LOGGED)
        })
        .unwrap_or_else(|| panic!("no 'job failed' event in {events:?}"));
    assert_eq!(failed.0, tracing::Level::ERROR);
    let f = &failed.1;
    assert_eq!(f.get("job_id"), Some(&id.to_string()));
    assert_eq!(f.get("kind").map(String::as_str), Some(LOGGED));
    assert_eq!(f.get("attempt").map(String::as_str), Some("1"));
    assert_eq!(f.get("max_attempts").map(String::as_str), Some("1"));
    assert_eq!(f.get("error").map(String::as_str), Some("engine crashed"));
    assert_eq!(f.get("error_code").map(String::as_str), Some("engine"));
    assert_eq!(f.get("outcome").map(String::as_str), Some("dead"));
}
