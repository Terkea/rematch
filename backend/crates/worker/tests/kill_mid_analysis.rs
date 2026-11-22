//! Port of apps/worker/test/recovery/kill-mid-analysis.test.ts.

mod common;

use std::sync::Arc;
use std::time::Duration;

use analysis::jobs::analyse::enqueue_analyse;
use common::{HangAfter, RUY, add_game, analyse_job, deps, event_types, game, wait_until};
use db::games::AnalysisStatus;
use queue::queue::Queue;
use queue::runner::RunnerConfig;
use sqlx::PgPool;
use testkit::fake_engine::FakeAnalyser;
use worker::start::spawn_analyse_runner;

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn kill_mid_analysis_still_completes(pool: PgPool) {
    let (acct, id) = add_game(&pool, RUY, "killed").await;
    let queue = Queue::new(pool.clone());
    let cfg = RunnerConfig::fast_for_tests();
    // Position 4 never returns: the worker "dies" while it is in flight.
    let hanging = HangAfter::new(3);
    let crashed = spawn_analyse_runner(queue.clone(), deps(&pool, hanging.clone()), &cfg, 1);
    assert!(enqueue_analyse(&queue, id, acct).await.unwrap());
    wait_until("the 4th engine call", Duration::from_secs(15), || {
        let n = hanging.calls();
        async move { (n >= 4).then_some(()) }
    })
    .await;
    assert_eq!(
        game(&pool, id).await.analysis_status,
        AnalysisStatus::Analysing
    );

    // Simulated crash: stop without finishing or releasing the job.
    crashed.abort();

    let healthy = Arc::new(FakeAnalyser::new());
    let next = spawn_analyse_runner(queue.clone(), deps(&pool, healthy.clone()), &cfg, 1);
    let done = wait_until("the game to be done", Duration::from_secs(30), || {
        let pool = pool.clone();
        async move {
            let g = game(&pool, id).await;
            (g.analysis_status == AnalysisStatus::Done).then_some(g)
        }
    })
    .await;
    assert_eq!(done.analysis_attempts, 2);
    assert!(done.analysis_started_at.is_some());
    let plies: Vec<i32> =
        sqlx::query_scalar("SELECT ply FROM moves WHERE game_id = $1 ORDER BY ply")
            .bind(id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(plies, (1..=10).collect::<Vec<i32>>());
    // The 4th position (in flight at the crash) went to the next engine.
    let first = hanging.fens.lock().unwrap().clone();
    assert!(healthy.called_fens().contains(&first[3]));
    next.shutdown(Duration::from_secs(5)).await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_job_failed_by_max_stalls_exhaustion_leaves_the_game_failed(pool: PgPool) {
    let (acct, id) = add_game(&pool, "1. d4 Nf6 2. c4 g6 *", "stalls").await;
    let queue = Queue::new(pool.clone());
    let cfg = RunnerConfig::fast_for_tests();
    let hanging = HangAfter::new(0);
    let crashed = spawn_analyse_runner(queue.clone(), deps(&pool, hanging.clone()), &cfg, 1);
    assert!(enqueue_analyse(&queue, id, acct).await.unwrap());
    // No stalls allowed: the first stall exhausts the job.
    sqlx::query("UPDATE jobs SET max_stalls = 0 WHERE dedupe_key = $1")
        .bind(queue::analyse_key(id))
        .execute(&pool)
        .await
        .unwrap();
    wait_until("the first engine call", Duration::from_secs(15), || {
        let n = hanging.calls();
        async move { (n > 0).then_some(()) }
    })
    .await;
    crashed.abort();

    // The next worker's reaper dead-letters the job; its handler never runs. Its heartbeat
    // period is short, so the stalled run's heartbeat (at least a lease old) counts as stale.
    let mut d = deps(&pool, hanging.clone());
    d.heartbeat_every = Duration::from_millis(200);
    let next = spawn_analyse_runner(queue.clone(), d, &cfg, 1);
    let failed = wait_until("the game to be failed", Duration::from_secs(30), || {
        let pool = pool.clone();
        async move {
            let g = game(&pool, id).await;
            (g.analysis_status == AnalysisStatus::Failed).then_some(g)
        }
    })
    .await;
    assert!(
        failed
            .analysis_error
            .as_deref()
            .unwrap_or("")
            .contains("stalled"),
        "{:?}",
        failed.analysis_error
    );
    assert_eq!(analyse_job(&pool, id).await.0, "dead");
    assert_eq!(hanging.calls(), 1);
    // `job.failed` is published right after the failed write commits.
    let events = wait_until("job.failed", Duration::from_secs(10), || {
        let pool = pool.clone();
        async move {
            let events = event_types(&pool, acct).await;
            (!events.is_empty()).then_some(events)
        }
    })
    .await;
    assert_eq!(events, ["job.failed"]);
    next.shutdown(Duration::from_secs(5)).await;
}
