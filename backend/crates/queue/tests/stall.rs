//! Leases, renewal and stall recovery (Task 14). A worker that dies mid-job is recovered by lease
//! expiry and the reaper without using an attempt; more than `max_stalls` stalls dead-letter the
//! job (`error_code = stalled`), which ports BullMQ's "a job failed by maxStalledCount
//! exhaustion" from apps/worker/src/start-workers.ts (`recordFinalFailure`).

use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

async fn enqueue(q: &Queue) -> i64 {
    q.enqueue(KIND, &json!({}), EnqueueOptions::default())
        .await
        .unwrap()
        .unwrap()
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
    let h = handler(|_, _| async {
        tokio::time::sleep(Duration::from_secs(5)).await;
        Ok(json!({}))
    });
    let cfg = RunnerConfig::fast_for_tests(); // lease 2 s, renew 500 ms
    assert_eq!(cfg.lease, Duration::from_secs(2));
    let runner = Runner::spawn(q.clone(), h, cfg);
    let mut done = false;
    for _ in 0..200 {
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
    assert!(done, "the 5 s job completed");
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
    let cfg = RunnerConfig::fast_for_tests();
    let owner = cfg.worker_id.clone();
    let crashed = Runner::spawn(q.clone(), hanging(), cfg);
    wait_owned(&pool, id, &owner).await;
    // The worker dies: no release, no renewals.
    crashed.abort();
    let recovered = wait_for("lease expiry", Duration::from_secs(10), POLL, || async {
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
    let fresh = Runner::spawn(q.clone(), h, RunnerConfig::fast_for_tests());
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
    let cfg = RunnerConfig::fast_for_tests();
    let owner = cfg.worker_id.clone();
    let crashed = Runner::spawn(q.clone(), hanging(), cfg);
    wait_owned(&pool, id, &owner).await;
    crashed.abort();
    let h = handler(|_, _| async { Ok(json!({})) });
    let survivor = Runner::spawn(q.clone(), h, RunnerConfig::fast_for_tests());
    wait_for("done", Duration::from_secs(15), POLL, || async {
        (row(&pool, id).await.state == "done").then_some(())
    })
    .await;
    let r = row(&pool, id).await;
    assert_eq!((r.attempts, r.stalls), (1, 1));
    assert_eq!(survivor.stats().stalled, 1);
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
    // Three workers in turn claim the job and die; each successor's reaper recovers it.
    for i in 0..3 {
        let cfg = RunnerConfig::fast_for_tests();
        let owner = cfg.worker_id.clone();
        let runner = Runner::spawn(q.clone(), crashing.clone(), cfg);
        let r = wait_owned(&pool, id, &owner).await;
        assert_eq!(r.stalls, i, "stall {i}");
        runner.abort();
    }
    // The fourth runner's reaper finds the third stall: more than max_stalls, so dead.
    let watcher = Runner::spawn(q.clone(), watching.clone(), RunnerConfig::fast_for_tests());
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
    let cfg = RunnerConfig::fast_for_tests();
    let owner2 = cfg.worker_id.clone();
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
    let cancelled = Arc::new(AtomicUsize::new(0));
    let c = cancelled.clone();
    let h = handler(move |_, ctx| {
        let c = c.clone();
        async move {
            ctx.cancel.cancelled().await;
            c.fetch_add(1, Ordering::SeqCst);
            Ok(json!({ "late": true }))
        }
    });
    let cfg = RunnerConfig::fast_for_tests();
    let owner = cfg.worker_id.clone();
    let runner = Runner::spawn(q.clone(), h, cfg);
    wait_owned(&pool, id, &owner).await;
    // Someone else now holds the lease (as after a reaper re-queue and a re-claim).
    let thief = Uuid::new_v4();
    sqlx::query("UPDATE jobs SET lease_token = $2, lease_owner = 'thief' WHERE id = $1")
        .bind(id)
        .bind(thief)
        .execute(&pool)
        .await
        .unwrap();
    wait_for("lease lost", Duration::from_secs(10), POLL, || async {
        (runner.stats().lease_lost == 1).then_some(())
    })
    .await;
    wait_for(
        "handler cancelled",
        Duration::from_secs(5),
        POLL,
        || async { (cancelled.load(Ordering::SeqCst) == 1).then_some(()) },
    )
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
        .enqueue("import-account", &json!({}), EnqueueOptions::default())
        .await
        .unwrap()
        .unwrap();
    q.dequeue(&[KIND], "w", 1, Duration::from_secs(60))
        .await
        .unwrap();
    q.dequeue(&["import-account"], "w", 1, Duration::from_millis(1))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
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
