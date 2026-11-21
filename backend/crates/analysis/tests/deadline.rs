//! Port of the analysis cases of apps/worker/test/recovery/deadline.test.ts.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use analysis::deadline::ANALYSE_DEADLINE;
use analysis::eval_cache::DbEvalCache;
use analysis::jobs::analyse::{
    AnalyseGameDeps, AnalyseGameHandler, enqueue_analyse, handle_analyse_game,
};
use db::games::{AnalysisStatus, get_game};
use engine::{AnalyseOptions, Analyser, AnalysisResult, EngineError};
use futures::future::BoxFuture;
use queue::queue::Queue;
use queue::runner::{Runner, RunnerConfig};
use queue::{AnalyseGamePayload, Job, JobError, KIND_ANALYSE};
use serde_json::json;
use sqlx::PgPool;
use testkit::fake_engine::FakeAnalyser;
use testkit::{GameSeed, insert_account, insert_game};
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

async fn add_game(pool: &PgPool, pgn: &str, username: &str) -> (Uuid, Uuid) {
    let acct = insert_account(pool, "chesscom", username).await;
    let id = insert_game(
        pool,
        acct,
        GameSeed {
            pgn: pgn.to_owned(),
            white: username.to_owned(),
            ..GameSeed::default()
        },
    )
    .await;
    (acct, id)
}

fn job(game_id: Uuid, attempts: i32, max_attempts: i32) -> Job {
    let now = OffsetDateTime::now_utc();
    Job {
        id: 1,
        kind: KIND_ANALYSE.to_owned(),
        payload: json!({ "gameId": game_id }),
        dedupe_key: None,
        user_key: None,
        priority: 100,
        attempts,
        max_attempts,
        lease_token: Uuid::new_v4(),
        run_at: now,
        created_at: now,
        started_at: now,
    }
}

#[test]
fn uses_the_brief_s_analysis_deadline_of_10_minutes() {
    assert_eq!(ANALYSE_DEADLINE, Duration::from_secs(10 * 60));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn an_analysis_hung_on_a_never_resolving_await_ends_with_job_deadline_error(pool: PgPool) {
    let (_, id) = add_game(&pool, "1. e4 d5 2. exd5 Qxd5 *", "deadline-unit").await;
    let mut hanging = FakeAnalyser::new();
    hanging.hang = true;
    let mut d = AnalyseGameDeps::new(
        pool.clone(),
        Arc::new(hanging),
        Arc::new(DbEvalCache::new(pool.clone())),
    );
    d.deadline = Duration::from_millis(200);
    let started = Instant::now();
    let res = tokio::time::timeout(
        Duration::from_secs(10),
        handle_analyse_game(
            &d,
            &job(id, 1, 3),
            AnalyseGamePayload { game_id: id },
            CancellationToken::new(),
        ),
    )
    .await
    .expect("the deadline ends the analysis");
    // Generous bound: well over 3x the 200 ms deadline.
    assert!(started.elapsed() < Duration::from_secs(5));
    match &res {
        Err(JobError::Retryable { code, message }) => {
            assert_eq!(code.as_deref(), Some("timeout"));
            assert!(message.contains("deadline"), "{message}");
        }
        other => panic!("expected a retryable timeout, got {other:?}"),
    }
    let g = get_game(&pool, id).await.unwrap().unwrap();
    assert_eq!(g.analysis_status, AnalysisStatus::Pending);
    assert!(g.analysis_error.unwrap().contains("deadline"));
}

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
    let analyser = HangFirst {
        calls: AtomicUsize::new(0),
        inner: FakeAnalyser::new(),
    };
    let mut d = AnalyseGameDeps::new(
        pool.clone(),
        Arc::new(analyser),
        Arc::new(DbEvalCache::new(pool.clone())),
    );
    d.deadline = Duration::from_millis(800);
    d.concurrency = 1;
    let queue = Queue::new(pool.clone());
    assert!(enqueue_analyse(&queue, id, acct).await.unwrap());
    let runner = Runner::spawn(
        queue.clone(),
        Arc::new(AnalyseGameHandler(Arc::new(d))),
        RunnerConfig::fast_for_tests(),
    );
    let done = testkit::wait_for(
        "the retried analysis to finish",
        Duration::from_secs(15),
        Duration::from_millis(50),
        || {
            let pool = pool.clone();
            async move {
                let g = get_game(&pool, id).await.unwrap().unwrap();
                (g.analysis_status == AnalysisStatus::Done).then_some(g)
            }
        },
    )
    .await;
    assert_eq!(done.analysis_attempts, 2);
    let stats = runner.stats();
    // Ended by the deadline (one retry), not by stall detection (the lease never expired).
    assert_eq!(stats.retried, 1, "{stats:?}");
    assert_eq!(stats.stalled, 0, "{stats:?}");
    tokio::time::timeout(
        Duration::from_secs(30),
        runner.shutdown(Duration::from_secs(5)),
    )
    .await
    .expect("runner shutdown");
}
