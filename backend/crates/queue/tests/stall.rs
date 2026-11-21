//! Leases, renewal and stall recovery (Task 14). A worker that dies mid-job is recovered by lease
//! expiry and the reaper without using an attempt; more than `max_stalls` stalls dead-letter the
//! job (`error_code = stalled`), which ports BullMQ's "a job failed by maxStalledCount
//! exhaustion" from apps/worker/src/start-workers.ts (`recordFinalFailure`).

use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use queue::queue::Queue;
use queue::runner::{Handler, JobContext, Runner, RunnerConfig};
use queue::{EnqueueOptions, Job, JobError};
use serde_json::{Value, json};
use sqlx::PgPool;
use testkit::wait::wait_for;
use uuid::Uuid;

const KIND: &str = "analyse-game";
const POLL: Duration = Duration::from_millis(25);

type HandleFn =
    dyn Fn(Job, JobContext) -> BoxFuture<'static, Result<Value, JobError>> + Send + Sync;

struct TestHandler {
    f: Box<HandleFn>,
    dead: Mutex<Vec<(i64, String)>>,
}

impl Handler for TestHandler {
    fn kind(&self) -> &'static str {
        KIND
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

fn handler<F, Fut>(f: F) -> Arc<TestHandler>
where
    F: Fn(Job, JobContext) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Value, JobError>> + Send + 'static,
{
    Arc::new(TestHandler {
        f: Box::new(move |job, ctx| Box::pin(f(job, ctx))),
        dead: Mutex::new(Vec::new()),
    })
}

/// A handler that never finishes (a worker that hangs until it is killed).
fn hanging() -> Arc<TestHandler> {
    handler(|_, _| async {
        std::future::pending::<()>().await;
        Ok(json!({}))
    })
}

#[derive(Debug, sqlx::FromRow)]
struct Row {
    state: String,
    attempts: i32,
    stalls: i32,
    error_code: Option<String>,
    last_error: Option<String>,
    lease_token: Option<Uuid>,
    lease_owner: Option<String>,
    finished: bool,
}

async fn row(pool: &PgPool, id: i64) -> Row {
    sqlx::query_as(
        "SELECT state::text AS state, attempts, stalls, error_code, last_error, lease_token, \
           lease_owner, finished_at IS NOT NULL AS finished FROM jobs WHERE id = $1",
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

async fn enqueue(q: &Queue) -> i64 {
    q.enqueue(KIND, &json!({}), due()).await.unwrap().unwrap()
}

/// `fast_for_tests` (lease 4 s) with renewals every 200 ms, so a renew has 2.8 s before the
/// local lease deadline on a loaded test host.
fn fast() -> RunnerConfig {
    RunnerConfig {
        renew_every: Duration::from_millis(200),
        ..RunnerConfig::fast_for_tests()
    }
}

/// For a runner that must stay alive and never be reaped: `fast()` with a 6 s lease, more than
/// `renew_every` + 3 s, so a forward step of the VM clock cannot make its lease look expired
/// before the next renew (ADR 0032).
fn live() -> RunnerConfig {
    RunnerConfig {
        lease: Duration::from_secs(6),
        ..fast()
    }
}

/// The `lease_owner` a runner with `cfg` claims under: `<worker_id>/<kind>`.
fn owner_of(cfg: &RunnerConfig) -> String {
    format!("{}/{KIND}", cfg.worker_id)
}

/// Seconds until the job's lease runs out, by the database clock, mapped onto a local instant.
async fn lease_deadline<'e, E>(db: E, id: i64) -> Instant
where
    E: sqlx::PgExecutor<'e>,
{
    let remaining: f64 = sqlx::query_scalar(
        "SELECT extract(epoch FROM lease_until - clock_timestamp())::float8 FROM jobs \
         WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_one(db)
    .await
    .unwrap();
    Instant::now() + Duration::from_secs_f64(remaining.max(0.0))
}

/// Makes the leases of `ids` expired on the database side (instead of a tiny lease plus a sleep,
/// which a step of the VM clock can defeat).
async fn expire(pool: &PgPool, ids: &[i64]) {
    sqlx::query(
        "UPDATE jobs SET lease_until = now() - interval '1 minute' \
         WHERE id = ANY($1) AND state = 'running'",
    )
    .bind(ids)
    .execute(pool)
    .await
    .unwrap();
}

/// Waits until `owner` holds the job's lease.
async fn wait_owned(pool: &PgPool, id: i64, owner: &str) -> Row {
    wait_for(
        &format!("job {id} running under {owner}"),
        Duration::from_secs(15),
        POLL,
        || async {
            let r = row(pool, id).await;
            (r.state == "running" && r.lease_owner.as_deref() == Some(owner)).then_some(r)
        },
    )
    .await
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn renews_lease_while_running(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let id = enqueue(&q).await;
    // The handler outlives the lease more than twice over. The lease is 6 s, so a ±3 s step of
    // the VM clock cannot expire a lease that was just renewed (ADR 0032).
    let h = handler(|_, _| async {
        tokio::time::sleep(Duration::from_secs(13)).await;
        Ok(json!({}))
    });
    let cfg = RunnerConfig {
        lease: Duration::from_secs(6),
        renew_every: Duration::from_millis(200),
        ..RunnerConfig::fast_for_tests()
    };
    let runner = Runner::spawn(q.clone(), h, cfg);
    let mut done = false;
    for _ in 0..400 {
        // Another reaper keeps looking for an expired lease the whole time.
        let recovered = q.recover_expired(&[KIND]).await.unwrap();
        assert!(
            recovered.is_empty(),
            "recovered a renewed job: {recovered:?}"
        );
        if row(&pool, id).await.state == "done" {
            done = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(done, "the 13 s job completed");
    let r = row(&pool, id).await;
    assert_eq!((r.attempts, r.stalls), (1, 0));
    assert_eq!(runner.stats().stalled, 0);
    runner.stop();
    runner.wait_idle().await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn expired_lease_is_recovered_without_using_an_attempt(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let id = enqueue(&q).await;
    let cfg = fast();
    let owner = owner_of(&cfg);
    let crashed = Runner::spawn(q.clone(), hanging(), cfg);
    wait_owned(&pool, id, &owner).await;
    // The worker dies: no release, no renewals.
    crashed.abort();
    let recovered = wait_for("lease expiry", Duration::from_secs(20), POLL, || async {
        let r = q.recover_expired(&[KIND]).await.unwrap();
        (!r.is_empty()).then_some(r)
    })
    .await;
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].job.id, id);
    assert!(!recovered[0].dead);
    let r = row(&pool, id).await;
    assert_eq!(r.state, "queued");
    assert_eq!(r.stalls, 1);
    assert_eq!(r.attempts, 0, "the stalled attempt is given back");
    assert_eq!(r.error_code.as_deref(), Some("stalled"));
    assert_eq!(r.lease_token, None);
    assert_eq!(r.lease_owner, None);

    let h = handler(|_, _| async { Ok(json!({})) });
    let fresh = Runner::spawn(q.clone(), h, live());
    wait_for("done", Duration::from_secs(10), POLL, || async {
        (row(&pool, id).await.state == "done").then_some(())
    })
    .await;
    assert_eq!(row(&pool, id).await.attempts, 1);
    fresh.stop();
    fresh.wait_idle().await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn runner_reaper_recovers_a_dead_workers_job(pool: PgPool) {
    // The same recovery, driven by a live runner's own reaper every `reap_every`.
    let q = Queue::new(pool.clone());
    let id = enqueue(&q).await;
    let cfg = fast();
    let owner = owner_of(&cfg);
    let crashed = Runner::spawn(q.clone(), hanging(), cfg);
    wait_owned(&pool, id, &owner).await;
    crashed.abort();
    let h = handler(|_, _| async { Ok(json!({})) });
    let survivor = Runner::spawn(q.clone(), h, live());
    wait_for("done", Duration::from_secs(15), POLL, || async {
        (row(&pool, id).await.state == "done").then_some(())
    })
    .await;
    // Outcomes, not exact counts: a clock step may legally add a recovery.
    let r = row(&pool, id).await;
    assert_eq!(r.attempts, 1, "every recovery gave its attempt back");
    assert!(r.stalls >= 1, "recovered by the survivor's reaper");
    assert!(survivor.stats().stalled >= 1);
    survivor.stop();
    survivor.wait_idle().await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn stalled_more_than_max_stalls_is_dead_and_calls_on_dead(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let id = enqueue(&q).await; // max_stalls = 2
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let crashing = handler(move |_, _| {
        c.fetch_add(1, Ordering::SeqCst);
        async {
            std::future::pending::<()>().await;
            Ok(json!({}))
        }
    });
    let watching = hanging();
    // Three workers in turn claim the job and die. The counts are exact because the condition
    // is frozen: the crashing runners never reap (`reap_every` 60 s, and their first tick finds
    // the job queued), and the test recovers the first two stalls itself.
    let crashing_cfg = || RunnerConfig {
        reap_every: Duration::from_secs(60),
        ..fast()
    };
    for i in 0..3 {
        let cfg = crashing_cfg();
        let owner = owner_of(&cfg);
        let runner = Runner::spawn(q.clone(), crashing.clone(), cfg);
        let r = wait_owned(&pool, id, &owner).await;
        assert_eq!(r.stalls, i, "stall {i}");
        // The claim is visible before the job task has called the handler: wait for the call.
        wait_for("handler called", Duration::from_secs(10), POLL, || async {
            (calls.load(Ordering::SeqCst) > i as usize).then_some(())
        })
        .await;
        runner.abort();
        if i < 2 {
            let rec = wait_for("lease expiry", Duration::from_secs(15), POLL, || async {
                let r = q.recover_expired(&[KIND]).await.unwrap();
                (!r.is_empty()).then_some(r)
            })
            .await;
            assert_eq!(rec.len(), 1);
            assert!(!rec[0].dead, "stall {} of max 2 re-queues", i + 1);
        }
    }
    // A live runner's reaper finds the third stall: more than max_stalls, so dead.
    let watcher = Runner::spawn(q.clone(), watching.clone(), live());
    let r = wait_for("dead", Duration::from_secs(15), POLL, || async {
        let r = row(&pool, id).await;
        (r.state == "dead").then_some(r)
    })
    .await;
    assert_eq!(r.stalls, 3);
    assert_eq!(r.error_code.as_deref(), Some("stalled"));
    assert_eq!(
        r.last_error.as_deref(),
        Some("job stalled more than max_stalls times")
    );
    assert!(r.finished);
    assert_eq!(r.attempts, 1, "a dead stall keeps its attempt");
    assert_eq!(calls.load(Ordering::SeqCst), 3, "never claimed after dying");
    wait_for("on_dead", Duration::from_secs(5), POLL, || async {
        (!watching.dead.lock().unwrap().is_empty()).then_some(())
    })
    .await;
    watcher.stop();
    watcher.wait_idle().await;
    assert_eq!(
        *watching.dead.lock().unwrap(),
        vec![(id, "job stalled more than max_stalls times".to_owned())]
    );
    assert!(crashing.dead.lock().unwrap().is_empty());
    assert_eq!(watcher.stats().dead, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn expired_lease_holder_cannot_complete(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let id = enqueue(&q).await;
    // Runner 1 claims job A with a short lease and then stops renewing (paused, not dead).
    let stale = q
        .dequeue(&[KIND], "runner-1", 1, Duration::from_millis(500))
        .await
        .unwrap()
        .pop()
        .unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Job>();
    let h = handler(move |job, ctx| {
        let tx = tx.clone();
        async move {
            tx.send(job).unwrap();
            ctx.cancel.cancelled().await;
            Ok(json!({}))
        }
    });
    // Runner 2 must keep the job through a clock step: a 6 s lease.
    let cfg = live();
    let owner2 = owner_of(&cfg);
    let runner2 = Runner::spawn(q.clone(), h, cfg);
    // Runner 2's reaper re-queues the expired job and runner 2 claims it.
    let fresh = tokio::time::timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("runner 2 claimed the job")
        .unwrap();
    assert_eq!(fresh.id, id);
    assert_ne!(fresh.lease_token, stale.lease_token);

    assert!(
        !q.complete(&stale).await.unwrap(),
        "stale complete is a no-op"
    );
    assert!(!q.renew(&stale, Duration::from_secs(60)).await.unwrap());
    let err = JobError::Retryable {
        message: "late".into(),
        code: None,
    };
    assert_eq!(
        q.fail(&stale, &err, queue::queue::Backoff::default())
            .await
            .unwrap(),
        queue::queue::FailOutcome::LeaseLost
    );
    let r = row(&pool, id).await;
    assert_eq!(r.state, "running");
    assert_eq!(r.lease_token, Some(fresh.lease_token));
    assert_eq!(r.lease_owner.as_deref(), Some(owner2.as_str()));
    runner2.force().await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn lost_lease_cancels_the_handler_and_drops_its_result(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let id = enqueue(&q).await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Instant>();
    let h = handler(move |_, ctx| {
        let tx = tx.clone();
        async move {
            ctx.cancel.cancelled().await;
            tx.send(Instant::now()).unwrap();
            Ok(json!({ "late": true }))
        }
    });
    let cfg = RunnerConfig {
        // No reaper interference and a lease that outlives a clock step: the takeover alone
        // must end the job.
        reap_every: Duration::from_secs(60),
        lease: Duration::from_secs(6),
        renew_every: Duration::from_millis(200),
        ..RunnerConfig::fast_for_tests()
    };
    let owner = owner_of(&cfg);
    let runner = Runner::spawn(q.clone(), h, cfg);
    wait_owned(&pool, id, &owner).await;
    // Someone else now holds the lease (as after a reaper re-queue and a re-claim).
    let thief = Uuid::new_v4();
    let mut takeover = pool.begin().await.unwrap();
    let deadline = lease_deadline(&mut *takeover, id).await;
    sqlx::query("UPDATE jobs SET lease_token = $2, lease_owner = 'thief' WHERE id = $1")
        .bind(id)
        .bind(thief)
        .execute(&mut *takeover)
        .await
        .unwrap();
    takeover.commit().await.unwrap();
    let committed = Instant::now();
    let cancelled_at = tokio::time::timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("handler cancelled")
        .unwrap();
    assert!(
        cancelled_at <= deadline + Duration::from_secs(1),
        "cancelled {:?} after the old lease ran out",
        cancelled_at.saturating_duration_since(deadline)
    );
    // Within ~2 s of the takeover (renews every 200 ms), long before the 6 s lease deadline:
    // the renew's `false` cancelled it, not the deadline fallback.
    let after = cancelled_at.saturating_duration_since(committed);
    assert!(
        after < Duration::from_secs(2),
        "cancelled {after:?} after the takeover"
    );
    wait_for("lease lost", Duration::from_secs(10), POLL, || async {
        (runner.stats().lease_lost == 1).then_some(())
    })
    .await;
    runner.stop();
    runner.wait_idle().await;
    let r = row(&pool, id).await;
    assert_eq!(r.state, "running", "the result was dropped");
    assert_eq!(r.lease_token, Some(thief));
    assert_eq!(runner.stats().completed, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn reaper_ignores_live_leases_and_other_kinds(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let live = enqueue(&q).await;
    let other = q
        .enqueue("import-account", &json!({}), due())
        .await
        .unwrap()
        .unwrap();
    q.dequeue(&[KIND], "w", 1, Duration::from_secs(60))
        .await
        .unwrap();
    q.dequeue(&["import-account"], "w", 1, Duration::from_secs(60))
        .await
        .unwrap();
    expire(&pool, &[other]).await;
    assert!(q.recover_expired(&[KIND]).await.unwrap().is_empty());
    assert!(q.recover_expired(&[]).await.unwrap().is_empty());
    assert_eq!(row(&pool, live).await.state, "running");
    let rec = q.recover_expired(&["import-account"]).await.unwrap();
    assert_eq!(rec.len(), 1);
    assert_eq!(rec[0].job.id, other);
    assert_eq!(rec[0].job.kind, "import-account");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn release_owned_requeues_only_that_owners_jobs(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let mine = enqueue(&q).await;
    let theirs = enqueue(&q).await;
    let lease = Duration::from_secs(60);
    let a = q
        .dequeue(&[KIND], "me", 1, lease)
        .await
        .unwrap()
        .pop()
        .unwrap();
    q.dequeue(&[KIND], "them", 1, lease).await.unwrap();
    assert_eq!(a.id, mine);
    assert_eq!(q.release_owned("me").await.unwrap(), 1);
    let r = row(&pool, mine).await;
    assert_eq!((r.state.as_str(), r.attempts), ("queued", 0));
    assert_eq!((r.lease_token, r.lease_owner), (None, None));
    assert_eq!(r.error_code.as_deref(), Some("shutdown"));
    assert_eq!(row(&pool, theirs).await.state, "running");
    // Idempotent, and the old token is fenced out.
    assert_eq!(q.release_owned("me").await.unwrap(), 0);
    assert!(!q.complete(&a).await.unwrap());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn hung_renewal_cancels_the_handler_before_the_lease_runs_out(pool: PgPool) {
    // A renew that hangs (here: blocked on a row lock) must not keep a zombie handler running
    // beside a future owner: the token fires by the lease deadline, while the renew is pending.
    let q = Queue::new(pool.clone());
    let id = enqueue(&q).await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Instant>();
    let h = handler(move |_, ctx| {
        let tx = tx.clone();
        async move {
            ctx.cancel.cancelled().await;
            tx.send(Instant::now()).unwrap();
            // Ignores the cancellation: the runner must drop it after the grace period.
            std::future::pending::<()>().await;
            Ok(json!({}))
        }
    });
    let cfg = RunnerConfig {
        renew_every: Duration::from_millis(200),
        reap_every: Duration::from_secs(60),
        ..RunnerConfig::fast_for_tests()
    };
    let owner = owner_of(&cfg);
    let runner = Runner::spawn(q.clone(), h, cfg);
    wait_owned(&pool, id, &owner).await;
    // Hold the row lock: every renew from now on blocks. The value read is the latest lease.
    let mut lock = pool.begin().await.unwrap();
    let deadline = lease_deadline(&mut *lock, id).await;
    let cancelled_at = tokio::time::timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("handler cancelled while the renew hung")
        .unwrap();
    assert!(
        cancelled_at <= deadline + Duration::from_secs(1),
        "cancelled {:?} after the lease ran out",
        cancelled_at.saturating_duration_since(deadline)
    );
    wait_for("handler dropped", Duration::from_secs(5), POLL, || async {
        (runner.in_flight() == 0).then_some(())
    })
    .await;
    assert_eq!(runner.stats().lease_lost, 1);
    assert_eq!(runner.stats().completed, 0);
    lock.rollback().await.unwrap();
    // Nothing was written by the runner: the abandoned renew never committed a result.
    let r = row(&pool, id).await;
    assert_eq!(r.state, "running");
    assert_eq!(r.lease_owner.as_deref(), Some(owner.as_str()));
    runner.stop();
    runner.wait_idle().await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn concurrent_reapers_recover_each_job_once(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let mut ids = Vec::new();
    for _ in 0..10 {
        ids.push(enqueue(&q).await);
    }
    let claimed = q
        .dequeue(&[KIND], "w", 10, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(claimed.len(), 10);
    expire(&pool, &ids).await;
    let (a, b) = tokio::join!(q.recover_expired(&[KIND]), q.recover_expired(&[KIND]));
    let a: Vec<i64> = a.unwrap().iter().map(|r| r.job.id).collect();
    let b: Vec<i64> = b.unwrap().iter().map(|r| r.job.id).collect();
    let mut all: Vec<i64> = a.iter().chain(b.iter()).copied().collect();
    all.sort_unstable();
    assert_eq!(
        all, ids,
        "each expired job recovered exactly once: {a:?} {b:?}"
    );
    for id in ids {
        let r = row(&pool, id).await;
        assert_eq!(
            (r.state.as_str(), r.stalls, r.attempts),
            ("queued", 1, 0),
            "job {id}"
        );
    }
}

/// The `run_at` column of a job, as stored.
async fn run_at(pool: &PgPool, id: i64) -> time::OffsetDateTime {
    sqlx::query_scalar("SELECT run_at FROM jobs WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn re_queues_keep_their_run_at(pool: PgPool) {
    // A stall recovery and a shutdown release re-queue a job at once *in its old place*: the
    // `run_at` is kept, so the job neither falls behind a backlog nor looks delayed after the
    // clock steps back (an immediate job has `run_at <= created_at`).
    let q = Queue::new(pool.clone());
    let early = EnqueueOptions {
        run_at: Some(time::OffsetDateTime::now_utc() - time::Duration::hours(1)),
        ..EnqueueOptions::default()
    };
    let old = q.enqueue(KIND, &json!({}), early).await.unwrap().unwrap();
    let newer = enqueue(&q).await;
    let original = run_at(&pool, old).await;

    let a = q
        .dequeue(&[KIND], "w", 1, Duration::from_secs(60))
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(a.id, old);
    expire(&pool, &[old]).await;
    assert_eq!(q.recover_expired(&[KIND]).await.unwrap().len(), 1);
    assert_eq!(
        run_at(&pool, old).await,
        original,
        "the reaper keeps run_at"
    );
    let again = q
        .dequeue(&[KIND], "me", 1, Duration::from_secs(60))
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(again.id, old, "still ahead of the newer job");

    assert_eq!(q.release_owned("me").await.unwrap(), 1);
    assert_eq!(
        run_at(&pool, old).await,
        original,
        "release_owned keeps run_at"
    );
    let next: Vec<i64> = q
        .dequeue(&[KIND], "w", 2, Duration::from_secs(60))
        .await
        .unwrap()
        .iter()
        .map(|j| j.id)
        .collect();
    assert_eq!(next, vec![old, newer]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn reaper_ignores_a_lease_that_only_looks_expired(pool: PgPool) {
    // A forward step of the database clock makes a live lease look expired. A runner's reaper
    // confirms an expiry over two passes at least `stall_grace` apart on the monotonic clock,
    // with the lease token and heartbeat unchanged, so the live owner (renewing every 200 ms)
    // keeps its job.
    let q = Queue::new(pool.clone());
    let id = enqueue(&q).await;
    let release = Arc::new(tokio::sync::Notify::new());
    let r2 = release.clone();
    let h = handler(move |_, _| {
        let release = r2.clone();
        async move {
            release.notified().await;
            Ok(json!({}))
        }
    });
    let cfg = live();
    let owner = owner_of(&cfg);
    let owner_runner = Runner::spawn(q.clone(), h, cfg);
    wait_owned(&pool, id, &owner).await;
    // A second runner of the kind reaps every 250 ms too.
    let other = Runner::spawn(q.clone(), hanging(), live());
    let until = Instant::now() + Duration::from_secs(4);
    while Instant::now() < until {
        sqlx::query(
            "UPDATE jobs SET lease_until = now() - interval '10 seconds' \
             WHERE id = $1 AND state = 'running'",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let r = row(&pool, id).await;
    assert_eq!(r.state, "running", "a live lease is never reaped");
    assert_eq!(r.stalls, 0);
    assert_eq!(r.lease_owner.as_deref(), Some(owner.as_str()));
    assert_eq!(owner_runner.stats().stalled + other.stats().stalled, 0);

    release.notify_one();
    wait_for("done", Duration::from_secs(10), POLL, || async {
        (row(&pool, id).await.state == "done").then_some(())
    })
    .await;
    let r = row(&pool, id).await;
    assert_eq!((r.attempts, r.stalls), (1, 0));
    owner_runner.stop();
    other.stop();
    owner_runner.wait_idle().await;
    other.wait_idle().await;
}
