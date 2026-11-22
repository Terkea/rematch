//! The claim-selection hook (R2 Task 27): `RunnerConfig.strategy` defaults to [`PriorityFifo`],
//! which is R1's claim unchanged, and the runner claims through whatever strategy it is given.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures::future::BoxFuture;
use queue::queue::{Queue, QueueError};
use queue::runner::{Handler, JobContext, Runner, RunnerConfig};
use queue::strategy::{PriorityFifo, SelectStrategy};
use queue::{EnqueueOptions, Job, JobError};
use serde_json::{Value, json};
use sqlx::PgPool;
use testkit::wait::wait_for;
use time::OffsetDateTime;

const KIND: &str = "analyse-game";
const LEASE: Duration = Duration::from_secs(60);

async fn enqueue(q: &Queue, priority: i16, run_at: Option<OffsetDateTime>) -> i64 {
    q.enqueue(
        KIND,
        &json!({}),
        EnqueueOptions {
            priority,
            run_at,
            ..EnqueueOptions::default()
        },
    )
    .await
    .unwrap()
    .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn default_strategy_is_priority_fifo(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let now = OffsetDateTime::now_utc();
    let ago = |s: i64| Some(now - time::Duration::seconds(s));
    let late = enqueue(&q, 100, ago(10)).await;
    let early = enqueue(&q, 100, ago(20)).await;
    let urgent = enqueue(&q, 1, ago(1)).await;
    let tie_a = enqueue(&q, 50, ago(5)).await;
    let tie_b = enqueue(&q, 50, ago(5)).await;

    let cfg = RunnerConfig::default();
    assert_eq!(cfg.strategy.order_by(), PriorityFifo.order_by());
    assert_eq!(cfg.strategy.order_by(), "priority, run_at, id");

    // One at a time, so the claim order is observable (RETURNING has no order).
    let mut order = Vec::new();
    while let [job] = cfg
        .strategy
        .dequeue(&q, &[KIND], "w", 1, LEASE)
        .await
        .unwrap()
        .as_slice()
    {
        assert_eq!(job.attempts, 1);
        order.push(job.id);
    }
    assert_eq!(order, vec![urgent, tie_a, tie_b, early, late]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn priority_fifo_matches_queue_dequeue(pool: PgPool) {
    let q = Queue::new(pool.clone());
    for p in [30, 10, 20, 10] {
        enqueue(&q, p, None).await;
    }
    let jobs = PriorityFifo
        .dequeue(&q, &[KIND, "other"], "w", 3, LEASE)
        .await
        .unwrap();
    let prios: Vec<i16> = jobs.iter().map(|j| j.priority).collect();
    assert_eq!(prios, vec![10, 10, 20]);
    let rest = q.dequeue(&[KIND], "w", 10, LEASE).await.unwrap();
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].priority, 30);
}

/// A strategy that counts its calls and delegates to [`PriorityFifo`].
#[derive(Default)]
struct Counting(AtomicUsize);

impl SelectStrategy for Counting {
    fn dequeue<'a>(
        &'a self,
        queue: &'a Queue,
        kinds: &'a [&'a str],
        owner: &'a str,
        limit: i64,
        lease: Duration,
    ) -> BoxFuture<'a, Result<Vec<Job>, QueueError>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        PriorityFifo.dequeue(queue, kinds, owner, limit, lease)
    }

    fn order_by(&self) -> &'static str {
        "counting"
    }
}

struct Ok1;

impl Handler for Ok1 {
    fn kind(&self) -> &'static str {
        KIND
    }

    fn handle(&self, _job: Job, _ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        Box::pin(async { Ok(Value::Null) })
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn runner_claims_through_its_strategy(pool: PgPool) {
    let q = Queue::new(pool.clone());
    for _ in 0..3 {
        enqueue(&q, 100, None).await;
    }
    let strategy = Arc::new(Counting::default());
    let cfg = RunnerConfig {
        strategy: strategy.clone(),
        ..RunnerConfig::fast_for_tests()
    };
    assert!(format!("{cfg:?}").contains("counting"), "{cfg:?}");
    let handle = Runner::spawn(q.clone(), Arc::new(Ok1), cfg);
    wait_for(
        "all done",
        Duration::from_secs(20),
        Duration::from_millis(25),
        || async {
            let n: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE state = 'done'")
                .fetch_one(&pool)
                .await
                .unwrap();
            (n == 3).then_some(())
        },
    )
    .await;
    assert!(handle.shutdown(Duration::from_secs(5)).await);
    assert!(strategy.0.load(Ordering::SeqCst) >= 1);
}
