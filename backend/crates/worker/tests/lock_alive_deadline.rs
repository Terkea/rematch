//! Port of the lock-alive case of apps/worker/test/recovery/deadline.test.ts, through the
//! worker's analyse runner.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use analysis::jobs::analyse::enqueue_analyse;
use common::{add_game, deps, game, wait_until};
use db::games::AnalysisStatus;
use engine::{AnalyseOptions, Analyser, AnalysisResult, EngineError};
use futures::future::BoxFuture;
use queue::queue::Queue;
use queue::runner::RunnerConfig;
use sqlx::PgPool;
use testkit::fake_engine::FakeAnalyser;
use worker::start::spawn_analyse_runner;

/// The first call hangs for ever; every later one answers at once.
struct HangFirst {
    calls: AtomicUsize,
    inner: FakeAnalyser,
}

impl Analyser for HangFirst {
    fn analyse<'a>(
        &'a self,
        fen: &'a str,
        opts: AnalyseOptions,
    ) -> BoxFuture<'a, Result<AnalysisResult, EngineError>> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            Box::pin(futures::future::pending())
        } else {
            self.inner.analyse(fen, opts)
        }
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_worker_whose_lease_stays_alive_is_still_recovered_the_deadline_ends_the_job(
    pool: PgPool,
) {
    let (acct, id) = add_game(&pool, "1. c4 e5 2. Nc3 Nf6 *", "deadline-worker").await;
    let mut d = deps(
        &pool,
        Arc::new(HangFirst {
            calls: AtomicUsize::new(0),
            inner: FakeAnalyser::new(),
        }),
    );
    d.deadline = Duration::from_millis(800);
    let queue = Queue::new(pool.clone());
    let runner = spawn_analyse_runner(queue.clone(), d, &RunnerConfig::fast_for_tests(), 1);
    assert!(enqueue_analyse(&queue, id, acct).await.unwrap());
    let done = wait_until(
        "the retried analysis to finish",
        Duration::from_secs(15),
        || {
            let pool = pool.clone();
            async move {
                let g = game(&pool, id).await;
                (g.analysis_status == AnalysisStatus::Done).then_some(g)
            }
        },
    )
    .await;
    assert_eq!(done.analysis_attempts, 2);
    let stats = runner.stats();
    // Ended by the handler's deadline (one retry), not by stall detection or the runner's hard
    // timeout (the lease never expired).
    assert_eq!(stats.retried, 1, "{stats:?}");
    assert_eq!(stats.stalled, 0, "{stats:?}");
    assert_eq!(stats.timed_out, 0, "{stats:?}");
    runner.shutdown(Duration::from_secs(5)).await;
}
