//! Port of apps/worker/test/recovery/poison.test.ts.

mod common;

use std::sync::Arc;
use std::time::Duration;

use analysis::jobs::analyse::enqueue_analyse;
use common::{add_game, analyse_job, deps, game};
use db::games::AnalysisStatus;
use queue::queue::Queue;
use queue::runner::RunnerConfig;
use sqlx::PgPool;
use testkit::fake_engine::FakeAnalyser;
use worker::start::spawn_analyse_runner;

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_poison_game_fails_after_3_attempts(pool: PgPool) {
    // An illegal move: a parse error that is not an unsupported variant.
    let (acct, id) = add_game(&pool, "1. e4 e5 2. Ke9 *", "poison").await;
    let queue = Queue::new(pool.clone());
    let runner = spawn_analyse_runner(
        queue.clone(),
        deps(&pool, Arc::new(FakeAnalyser::new())),
        &RunnerConfig::fast_for_tests(),
        1,
    );
    assert!(enqueue_analyse(&queue, id, acct).await.unwrap());
    let mut seen = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let failed = loop {
        let g = game(&pool, id).await;
        seen.push(g.analysis_status);
        let (state, _) = analyse_job(&pool, id).await;
        if g.analysis_status == AnalysisStatus::Failed && state == "dead" {
            break g;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the job to fail for good"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert_eq!(failed.analysis_attempts, 3);
    assert!(failed.analysis_error.is_some());
    assert_eq!(analyse_job(&pool, id).await, ("dead".to_owned(), 3));
    // Between attempts the game is pending again, so nothing looks final too early.
    assert!(!seen.contains(&AnalysisStatus::Done));
    let events: Vec<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT type::text, payload FROM user_events WHERE account_id = $1 ORDER BY id",
    )
    .bind(acct)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].0, "job.failed");
    assert_eq!(events[0].1["kind"], "analyse-game");
    assert_eq!(events[0].1["gameId"], serde_json::json!(id));
    runner.shutdown(Duration::from_secs(5)).await;
}
