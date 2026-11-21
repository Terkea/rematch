//! Port of apps/worker/test/jobs/analyse-game.test.ts (BullMQ attempts become `Job.attempts` /
//! `max_attempts`), plus the R1 tests for shutdown releases, generations, metrics and `on_dead`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use analysis::eval_cache::{DbEvalCache, EvalCache, EvalCacheEntry};
use analysis::jobs::analyse::{
    AnalyseGameDeps, AnalyseGameHandler, AnalyseOutcome, enqueue_analyse, handle_analyse_game,
    request_reanalysis,
};
use chess_core::{EngineScore, PositionEval};
use db::games::{AnalysisStatus, GameRow, get_game};
use engine::{AnalyseOptions, Analyser, AnalysisLine, AnalysisResult, EngineError};
use futures::future::BoxFuture;
use queue::queue::{Backoff, FailOutcome, Queue};
use queue::runner::{Handler, JobContext};
use queue::{AnalyseGamePayload, Job, JobError, KIND_ANALYSE};
use serde_json::{Value, json};
use sqlx::PgPool;
use testkit::fake_engine::FakeAnalyser;
use testkit::{GameSeed, insert_account, insert_game};
use time::OffsetDateTime;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

// ---------------------------------------------------------------------------------------------
// Helpers

/// An analyser driven by a closure (the TS tests' inline `{ analyse: async () => ... }` pools).
struct FnAnalyser {
    calls: Mutex<Vec<String>>,
    f: Box<dyn Fn(String) -> BoxFuture<'static, Result<AnalysisResult, EngineError>> + Send + Sync>,
}

impl FnAnalyser {
    fn new(
        f: impl Fn(String) -> BoxFuture<'static, Result<AnalysisResult, EngineError>>
        + Send
        + Sync
        + 'static,
    ) -> Arc<Self> {
        Arc::new(FnAnalyser {
            calls: Mutex::new(Vec::new()),
            f: Box::new(f),
        })
    }

    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

impl Analyser for FnAnalyser {
    fn analyse<'a>(
        &'a self,
        fen: &'a str,
        _opts: AnalyseOptions,
    ) -> BoxFuture<'a, Result<AnalysisResult, EngineError>> {
        self.calls.lock().unwrap().push(fen.to_owned());
        (self.f)(fen.to_owned())
    }
}

fn line(cp: i32) -> AnalysisResult {
    AnalysisResult {
        lines: vec![AnalysisLine {
            multipv: 1,
            score: EngineScore::Cp(cp),
            pv: vec!["e2e4".to_owned()],
            depth: 1,
            nodes: 1,
        }],
        wall: Duration::from_millis(1),
        cpu: Some(Duration::from_millis(1)),
    }
}

/// The TS `pool()`: cp 20 everywhere, or `engine exploded` while `fail` is set.
fn pool_fake(fail: Arc<Mutex<bool>>) -> Arc<FnAnalyser> {
    FnAnalyser::new(move |_| {
        let fail = *fail.lock().unwrap();
        Box::pin(async move {
            if fail {
                Err(EngineError::Spawn("engine exploded".to_owned()))
            } else {
                Ok(line(20))
            }
        })
    })
}

fn ok_pool() -> Arc<FnAnalyser> {
    pool_fake(Arc::new(Mutex::new(false)))
}

/// The TS `cacheless`: never hits, stores nothing.
struct NoCache;

impl EvalCache for NoCache {
    fn get_many<'a>(
        &'a self,
        _fen_keys: &'a [String],
        _protocol: &'a str,
    ) -> BoxFuture<'a, Result<HashMap<String, PositionEval>, sqlx::Error>> {
        Box::pin(async { Ok(HashMap::new()) })
    }

    fn put_many<'a>(
        &'a self,
        _entries: &'a [EvalCacheEntry],
    ) -> BoxFuture<'a, Result<(), sqlx::Error>> {
        Box::pin(async { Ok(()) })
    }
}

fn deps(pool: &PgPool, analyser: Arc<dyn Analyser>) -> AnalyseGameDeps {
    AnalyseGameDeps::new(
        pool.clone(),
        analyser,
        Arc::new(DbEvalCache::new(pool.clone())),
    )
}

fn cacheless(pool: &PgPool, analyser: Arc<dyn Analyser>) -> AnalyseGameDeps {
    AnalyseGameDeps::new(pool.clone(), analyser, Arc::new(NoCache))
}

/// Every await on the handler, the engine or a gate is bounded, so a hang fails in seconds.
const TEST_TIMEOUT: Duration = Duration::from_secs(30);

async fn within<T>(what: &str, fut: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(TEST_TIMEOUT, fut)
        .await
        .unwrap_or_else(|_| panic!("timed out after {TEST_TIMEOUT:?} waiting for {what}"))
}

/// A chess.com account `username` with one pending game in which it plays white.
async fn add_game(pool: &PgPool, pgn: &str, username: &str) -> Uuid {
    within("add_game", async {
        let acct = insert_account(pool, "chesscom", username).await;
        insert_game(
            pool,
            acct,
            GameSeed {
                pgn: pgn.to_owned(),
                white: username.to_owned(),
                black: "x".to_owned(),
                ..GameSeed::default()
            },
        )
        .await
    })
    .await
}

async fn game(pool: &PgPool, id: Uuid) -> GameRow {
    within("get_game", get_game(pool, id))
        .await
        .unwrap()
        .expect("game row")
}

async fn move_count(pool: &PgPool, id: Uuid) -> i64 {
    within(
        "move count",
        sqlx::query_scalar("SELECT count(*) FROM moves WHERE game_id = $1")
            .bind(id)
            .fetch_one(pool),
    )
    .await
    .unwrap()
}

/// `(type, payload)` of every user event, oldest first.
async fn events(pool: &PgPool) -> Vec<(String, Value)> {
    within(
        "user events",
        sqlx::query_as("SELECT type, payload FROM user_events ORDER BY id").fetch_all(pool),
    )
    .await
    .unwrap()
}

/// Runs one SQL statement (test setup), bounded.
async fn exec(pool: &PgPool, sql: &'static str, id: Uuid) {
    within(sql, sqlx::query(sql).bind(id).execute(pool))
        .await
        .unwrap();
}

/// A job for `game_id` on attempt `attempts` of `max_attempts`.
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
        run_at: now - time::Duration::milliseconds(50),
        created_at: now - time::Duration::milliseconds(80),
        started_at: now,
    }
}

async fn run(
    d: &AnalyseGameDeps,
    id: Uuid,
    attempts: i32,
    max: i32,
) -> Result<AnalyseOutcome, JobError> {
    within(
        "handle_analyse_game",
        handle_analyse_game(
            d,
            &job(id, attempts, max),
            AnalyseGamePayload { game_id: id },
            CancellationToken::new(),
        ),
    )
    .await
}

fn done(outcome: &Result<AnalyseOutcome, JobError>) -> bool {
    matches!(outcome, Ok(AnalyseOutcome::Done { .. }))
}

/// The TS `gatedPool(cp)`: every call signals `started`, then waits for `release`.
struct Gate {
    started: Notify,
    released: tokio::sync::watch::Sender<bool>,
}

fn gated_pool(cp: i32, fail_after_gate: Option<&'static str>) -> (Arc<Gate>, Arc<FnAnalyser>) {
    let gate = Arc::new(Gate {
        started: Notify::new(),
        released: tokio::sync::watch::channel(false).0,
    });
    let g = gate.clone();
    let analyser = FnAnalyser::new(move |_| {
        let g = g.clone();
        Box::pin(async move {
            g.started.notify_one();
            let mut rx = g.released.subscribe();
            rx.wait_for(|r| *r).await.ok();
            match fail_after_gate {
                Some(msg) => Err(EngineError::Spawn(msg.to_owned())),
                None => Ok(line(cp)),
            }
        })
    });
    (gate, analyser)
}

// ---------------------------------------------------------------------------------------------
// Ported from analyse-game.test.ts

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn analyses_scholar_s_mate_into_7_moves(pool: PgPool) {
    let id = add_game(&pool, testkit::SCHOLARS, "scholar").await;
    let res = run(&deps(&pool, ok_pool()), id, 1, 3).await;
    assert!(done(&res), "{res:?}");
    let g = game(&pool, id).await;
    assert_eq!(g.analysis_status, AnalysisStatus::Done);
    assert_eq!(g.analysis_protocol.as_deref(), Some("sf19-n300k-v1"));
    assert!(g.white_accuracy.is_some());
    assert!(g.black_accuracy.is_some());
    assert_eq!(move_count(&pool, id).await, 7);
    assert_eq!(
        serde_json::to_value(res.unwrap()).unwrap(),
        json!({ "status": "done" })
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn skips_games_with_fewer_than_2_plies_without_calling_the_engine(pool: PgPool) {
    let id = add_game(&pool, "1. e4 *", "short").await;
    let p = ok_pool();
    let res = run(&deps(&pool, p.clone()), id, 1, 3).await;
    assert!(matches!(res, Ok(AnalyseOutcome::Skipped)), "{res:?}");
    assert_eq!(
        serde_json::to_value(res.unwrap()).unwrap(),
        json!({ "status": "skipped" })
    );
    assert_eq!(p.call_count(), 0);
    assert_eq!(
        game(&pool, id).await.analysis_status,
        AnalysisStatus::Skipped
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn skips_unsupported_variants(pool: PgPool) {
    let id = add_game(
        &pool,
        "[Variant \"Crazyhouse\"]\n\n1. e4 e5 2. Nf3 *",
        "variant",
    )
    .await;
    let p = ok_pool();
    let res = run(&deps(&pool, p.clone()), id, 1, 3).await;
    assert!(matches!(res, Ok(AnalyseOutcome::Skipped)), "{res:?}");
    assert_eq!(p.call_count(), 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn marks_failed_and_rethrows_on_engine_error_a_re_run_succeeds_without_duplicate_moves(
    pool: PgPool,
) {
    let id = add_game(&pool, "1. d4 d5 2. c4 e6 *", "flaky").await;
    let fail = Arc::new(Mutex::new(true));
    let d = deps(&pool, pool_fake(fail.clone()));
    // The TS default (no attempt info) is the final attempt.
    let res = run(&d, id, 1, 1).await;
    match &res {
        Err(JobError::Retryable { message, .. }) => assert!(message.contains("engine exploded")),
        other => panic!("expected a retryable engine error, got {other:?}"),
    }
    let failed = game(&pool, id).await;
    assert_eq!(failed.analysis_status, AnalysisStatus::Failed);
    assert!(failed.analysis_error.unwrap().contains("engine exploded"));

    *fail.lock().unwrap() = false;
    assert!(done(&run(&d, id, 1, 3).await));
    assert_eq!(game(&pool, id).await.analysis_error, None);
    assert_eq!(move_count(&pool, id).await, 4);
    // Re-running a finished game never duplicates its moves.
    assert!(done(&run(&d, id, 1, 3).await));
    assert_eq!(move_count(&pool, id).await, 4);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_missing_game_is_unrecoverable(pool: PgPool) {
    let res = run(&deps(&pool, ok_pool()), Uuid::nil(), 1, 3).await;
    assert!(
        matches!(res, Err(JobError::Unrecoverable { .. })),
        "{res:?}"
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn records_the_start_counts_attempts_and_heartbeats_from_inside_the_evaluation(pool: PgPool) {
    let id = add_game(&pool, "1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 *", "beating").await;
    let heartbeats: Arc<Mutex<Vec<Option<OffsetDateTime>>>> = Arc::default();
    let slow = {
        let (pool, heartbeats) = (pool.clone(), heartbeats.clone());
        FnAnalyser::new(move |_| {
            let (pool, heartbeats) = (pool.clone(), heartbeats.clone());
            Box::pin(async move {
                let beat = get_game(&pool, id).await.unwrap().unwrap().heartbeat_at;
                heartbeats.lock().unwrap().push(beat);
                tokio::time::sleep(Duration::from_millis(5)).await;
                Ok(line(20))
            })
        })
    };
    let mut d = deps(&pool, slow);
    d.heartbeat_every = Duration::ZERO;
    d.concurrency = 1;
    let before = OffsetDateTime::now_utc();
    assert!(done(&run(&d, id, 1, 3).await));
    let g = game(&pool, id).await;
    assert_eq!(g.analysis_attempts, 1);
    assert!(g.analysis_started_at.unwrap() >= before - time::Duration::seconds(1));
    let times: Vec<OffsetDateTime> = heartbeats
        .lock()
        .unwrap()
        .iter()
        .map(|h| h.expect("heartbeat set when the attempt starts"))
        .collect();
    // Refreshed while evaluating.
    assert!(times.iter().collect::<HashSet<_>>().len() > 3, "{times:?}");
    assert!(g.heartbeat_at.unwrap() >= *times.last().unwrap());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn goes_back_to_pending_not_failed_when_the_queue_will_retry(pool: PgPool) {
    let id = add_game(&pool, "1. e4 c6 2. d4 d5 *", "retrying").await;
    exec(
        &pool,
        "UPDATE games SET pending_since = NULL WHERE id = $1",
        id,
    )
    .await;
    let d = deps(&pool, pool_fake(Arc::new(Mutex::new(true))));
    let res = run(&d, id, 1, 3).await;
    match &res {
        Err(JobError::Retryable { message, .. }) => assert!(message.contains("engine exploded")),
        other => panic!("expected a retryable engine error, got {other:?}"),
    }
    let g = game(&pool, id).await;
    assert_eq!(g.analysis_status, AnalysisStatus::Pending);
    assert!(g.analysis_error.unwrap().contains("engine exploded"));
    assert_eq!(g.analysis_attempts, 1);
    assert!(g.pending_since.is_some());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_closed_engine_pool_shutdown_leaves_the_game_pending_even_on_the_last_attempt(
    pool: PgPool,
) {
    let id = add_game(&pool, "1. e4 e6 2. d4 d5 *", "shutdown").await;
    let closed = FnAnalyser::new(|_| Box::pin(async { Err(EngineError::PoolClosed) }));
    let res = run(&deps(&pool, closed), id, 3, 3).await;
    assert!(matches!(res, Err(JobError::Release { .. })), "{res:?}");
    assert_eq!(
        game(&pool, id).await.analysis_status,
        AnalysisStatus::Pending
    );
    assert_eq!(events(&pool).await, vec![]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn publishes_game_analysed_with_the_player_s_accuracy_after_the_commit(pool: PgPool) {
    let id = add_game(&pool, "1. d4 Nf6 2. c4 e6 *", "announced").await;
    let res = run(&deps(&pool, ok_pool()), id, 1, 3).await;
    let g = game(&pool, id).await;
    let white = f64::from(g.white_accuracy.unwrap());
    match res {
        Ok(AnalyseOutcome::Done { accuracy: Some(a) }) => assert!((a - white).abs() < 1e-4),
        other => panic!("expected done with an accuracy, got {other:?}"),
    }
    let evs = events(&pool).await;
    assert_eq!(evs.len(), 1, "{evs:?}");
    assert_eq!(evs[0].0, "game.analysed");
    assert_eq!(evs[0].1["gameId"], json!(id));
    let published = evs[0].1["accuracy"].as_f64().unwrap();
    assert!((published - white).abs() < 1e-4, "{published} vs {white}");
    let account: Uuid = sqlx::query_scalar("SELECT account_id FROM user_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(account, g.account_id);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn fences_the_final_write_a_stale_duplicate_run_cannot_overwrite_a_newer_done(pool: PgPool) {
    let id = add_game(&pool, "1. e4 g6 2. d4 Bg7 *", "fenced").await;
    let (gate, stale) = gated_pool(-400, None);
    let stale_deps = Arc::new(cacheless(&pool, stale));
    let started = gate.started.notified();
    let stale_run = tokio::spawn({
        let d = stale_deps.clone();
        async move { run(&d, id, 1, 3).await }
    });
    within("the stale run to reach the engine", started).await;
    // A second delivery of the same game (for example after a lost lease) runs to the end.
    assert!(done(&run(&cacheless(&pool, ok_pool()), id, 1, 3).await));
    let after_fresh = game(&pool, id).await;
    assert_eq!(after_fresh.analysis_attempts, 2);

    gate.released.send_replace(true);
    let res = within("the stale run", stale_run).await.unwrap();
    assert!(matches!(res, Ok(AnalyseOutcome::Superseded)), "{res:?}");
    assert_eq!(
        serde_json::to_value(res.unwrap()).unwrap(),
        json!({ "status": "superseded" })
    );
    let g = game(&pool, id).await;
    assert_eq!(g.analysis_status, AnalysisStatus::Done);
    assert_eq!(g.white_accuracy, after_fresh.white_accuracy);
    assert_eq!(g.black_accuracy, after_fresh.black_accuracy);
    assert_eq!(move_count(&pool, id).await, 4);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn fences_the_failure_write_too(pool: PgPool) {
    let id = add_game(&pool, "1. e4 b6 2. d4 Bb7 *", "fenced-fail").await;
    let (gate, failing) = gated_pool(0, Some("late failure"));
    let stale_deps = Arc::new(cacheless(&pool, failing));
    let started = gate.started.notified();
    let stale_run = tokio::spawn({
        let d = stale_deps.clone();
        // The stale run is on its final attempt, so it would write `failed`.
        async move { run(&d, id, 3, 3).await }
    });
    within("the stale run to reach the engine", started).await;
    assert!(done(&run(&cacheless(&pool, ok_pool()), id, 1, 3).await));
    gate.released.send_replace(true);
    match within("the stale run", stale_run).await.unwrap() {
        Err(JobError::Retryable { message, .. }) => assert!(message.contains("late failure")),
        other => panic!("expected the late failure, got {other:?}"),
    }
    let g = game(&pool, id).await;
    assert_eq!(g.analysis_status, AnalysisStatus::Done);
    assert_eq!(g.analysis_error, None);
    // The fenced failure hit no row, so no job.failed was published.
    assert!(events(&pool).await.iter().all(|(t, _)| t != "job.failed"));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn skips_a_game_already_done_at_the_current_protocol_without_touching_it(pool: PgPool) {
    let id = add_game(&pool, "1. Nf3 d5 2. g3 c5 *", "already").await;
    assert!(done(&run(&deps(&pool, ok_pool()), id, 1, 3).await));
    let before = game(&pool, id).await;
    let p = ok_pool();
    assert!(done(&run(&deps(&pool, p.clone()), id, 1, 3).await));
    assert_eq!(p.call_count(), 0);
    let after = game(&pool, id).await;
    assert_eq!(after.analysis_attempts, before.analysis_attempts);
    assert_eq!(after.analysis_started_at, before.analysis_started_at);
}

// ---------------------------------------------------------------------------------------------
// R1 additions

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn pool_closed_on_final_attempt_releases_without_attempt(pool: PgPool) {
    let id = add_game(&pool, "1. c4 e5 2. Nc3 Nf6 *", "closing").await;
    exec(
        &pool,
        "UPDATE games SET pending_since = NULL WHERE id = $1",
        id,
    )
    .await;
    let closed = FnAnalyser::new(|_| Box::pin(async { Err(EngineError::PoolClosed) }));
    let res = run(&deps(&pool, closed), id, 3, 3).await;
    match res {
        Err(JobError::Release { delay, reason }) => {
            assert_eq!(delay, Duration::ZERO);
            assert_eq!(reason, "shutdown");
        }
        other => panic!("expected a release, got {other:?}"),
    }
    let g = game(&pool, id).await;
    assert_eq!(g.analysis_status, AnalysisStatus::Pending);
    assert!(g.pending_since.is_some());
    assert!(g.analysis_error.is_some());
    assert!(events(&pool).await.iter().all(|(t, _)| t != "job.failed"));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_cancelled_job_token_releases_without_attempt(pool: PgPool) {
    let id = add_game(&pool, "1. c4 c5 2. Nc3 Nc6 *", "cancelled").await;
    let mut hang = FakeAnalyser::new();
    hang.hang = true;
    let d = deps(&pool, Arc::new(hang));
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        trigger.cancel();
    });
    let res = tokio::time::timeout(
        Duration::from_secs(10),
        handle_analyse_game(
            &d,
            &job(id, 3, 3),
            AnalyseGamePayload { game_id: id },
            cancel,
        ),
    )
    .await
    .expect("the cancel ends the analysis");
    assert!(
        matches!(&res, Err(JobError::Release { delay, .. }) if delay.is_zero()),
        "{res:?}"
    );
    let g = game(&pool, id).await;
    assert_eq!(g.analysis_status, AnalysisStatus::Pending);
    assert!(events(&pool).await.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn request_reanalysis_resets_attempts_and_bypasses_done(pool: PgPool) {
    let id = add_game(&pool, "1. e4 e5 2. Nf3 Nf6 *", "again").await;
    let d = deps(&pool, ok_pool());
    assert!(done(&run(&d, id, 1, 3).await));
    exec(
        &pool,
        "UPDATE games SET analysis_attempts = 6 WHERE id = $1",
        id,
    )
    .await;
    let queue = Queue::new(pool.clone());
    assert!(request_reanalysis(&pool, &queue, id).await.unwrap());
    let g = game(&pool, id).await;
    assert_eq!(g.analysis_attempts, 0);
    assert_eq!(g.analysis_generation, 1);
    assert_eq!(g.analysed_generation, Some(0));
    assert_eq!(g.analysis_status, AnalysisStatus::Pending);
    assert_eq!(queue.live_count(&[KIND_ANALYSE], None).await.unwrap(), 1);

    // The game is done at the current protocol, but not at the current generation. No cache,
    // so the engine calls prove the run did not return early.
    let p = ok_pool();
    assert!(done(&run(&cacheless(&pool, p.clone()), id, 1, 3).await));
    assert!(p.call_count() > 0, "the re-analysis must not return early");
    let g = game(&pool, id).await;
    assert_eq!(g.analysis_status, AnalysisStatus::Done);
    assert_eq!(g.analysed_generation, Some(1));
    assert_eq!(g.analysis_attempts, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn request_reanalysis_refuses_a_game_mid_analysis(pool: PgPool) {
    let id = add_game(&pool, "1. e4 e5 2. Nf3 Nf6 *", "busy").await;
    exec(
        &pool,
        "UPDATE games SET analysis_status = 'analysing' WHERE id = $1",
        id,
    )
    .await;
    let queue = Queue::new(pool.clone());
    assert!(!request_reanalysis(&pool, &queue, id).await.unwrap());
    assert_eq!(game(&pool, id).await.analysis_generation, 0);
    assert_eq!(queue.live_count(&[KIND_ANALYSE], None).await.unwrap(), 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn enqueue_analyse_dedupes_live_jobs_per_game(pool: PgPool) {
    let acct = insert_account(&pool, "chesscom", "dedupe").await;
    let id = insert_game(&pool, acct, GameSeed::default()).await;
    let queue = Queue::new(pool.clone());
    assert!(enqueue_analyse(&queue, id, acct).await.unwrap());
    assert!(!enqueue_analyse(&queue, id, acct).await.unwrap());
    let (key, user, max): (Option<String>, Option<Uuid>, i32) =
        sqlx::query_as("SELECT dedupe_key, user_key, max_attempts FROM jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(key, Some(format!("analyse:{id}")));
    assert_eq!(user, Some(acct));
    assert_eq!(max, 3);
}

#[derive(sqlx::FromRow, Debug)]
struct MetricsRow {
    job_id: Option<i64>,
    kind: String,
    game_id: Option<Uuid>,
    account_id: Option<Uuid>,
    queue_wait_ms: Option<f64>,
    parse_ms: Option<f64>,
    cache_ms: Option<f64>,
    engine_wall_ms: Option<f64>,
    engine_cpu_ms: Option<f64>,
    persist_ms: Option<f64>,
    total_ms: f64,
    positions: Option<i32>,
    engine_positions: Option<i32>,
    cache_hits: Option<i32>,
    lichess_used: Option<i32>,
}

async fn metrics_rows(pool: &PgPool) -> Vec<MetricsRow> {
    within(
        "job_metrics rows",
        sqlx::query_as(
            "SELECT job_id, kind, game_id, account_id, queue_wait_ms, parse_ms, cache_ms, \
             engine_wall_ms, engine_cpu_ms, persist_ms, total_ms, positions, engine_positions, \
             cache_hits, lichess_used FROM job_metrics ORDER BY id",
        )
        .fetch_all(pool),
    )
    .await
    .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn records_job_metrics_after_commit(pool: PgPool) {
    // 6 plies, no terminal position: 7 positions, all distinct, all from the engine.
    let id = add_game(&pool, "1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 *", "measured").await;
    let fake = Arc::new(FakeAnalyser::new());
    assert!(done(&run(&deps(&pool, fake.clone()), id, 1, 3).await));
    let rows = metrics_rows(&pool).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    let m = &rows[0];
    let g = game(&pool, id).await;
    assert_eq!(m.job_id, Some(1));
    assert_eq!(m.kind, "analyse-game");
    assert_eq!(m.game_id, Some(id));
    assert_eq!(m.account_id, Some(g.account_id));
    assert_eq!(m.positions, Some(7));
    assert_eq!(m.engine_positions, Some(fake.call_count() as i32));
    assert_eq!(m.cache_hits, Some(0));
    assert_eq!(m.lichess_used, Some(0));
    assert!(m.total_ms > 0.0);
    // started_at - max(run_at, created_at) = 50 ms in `job()`.
    assert!((m.queue_wait_ms.unwrap() - 50.0).abs() < 1.0, "{m:?}");
    assert!(m.parse_ms.unwrap() >= 0.0);
    assert!(m.cache_ms.unwrap() >= 0.0);
    assert!(m.persist_ms.unwrap() > 0.0);
    // Sums of the fake's reported 5 ms wall and 3 ms CPU per call.
    assert!((m.engine_wall_ms.unwrap() - 35.0).abs() < 1e-6);
    assert!((m.engine_cpu_ms.unwrap() - 21.0).abs() < 1e-6);
    assert!(m.total_ms >= m.persist_ms.unwrap());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn records_partial_job_metrics_for_a_failed_analysis(pool: PgPool) {
    let id = add_game(&pool, "1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 *", "measured-fail").await;
    let mut fake = FakeAnalyser::new();
    fake.fail_after = Some(2);
    let mut d = deps(&pool, Arc::new(fake));
    d.concurrency = 1;
    assert!(run(&d, id, 1, 3).await.is_err());
    let rows = metrics_rows(&pool).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].positions, Some(7));
    assert_eq!(rows[0].engine_positions, Some(2));
    assert_eq!(rows[0].persist_ms, None);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn on_dead_marks_failed_once_and_publishes_once(pool: PgPool) {
    let id = add_game(&pool, "1. e4 e5 2. Nf3 Nc6 *", "dead").await;
    exec(
        &pool,
        "UPDATE games SET analysis_status = 'analysing', analysis_attempts = 3 WHERE id = $1",
        id,
    )
    .await;
    let handler = AnalyseGameHandler(Arc::new(deps(&pool, ok_pool())));
    assert_eq!(handler.kind(), KIND_ANALYSE);
    let j = job(id, 3, 3);
    within("on_dead", handler.on_dead(&j, "stalled too often")).await;
    within("on_dead", handler.on_dead(&j, "stalled too often")).await;
    let g = game(&pool, id).await;
    assert_eq!(g.analysis_status, AnalysisStatus::Failed);
    assert_eq!(g.analysis_error.as_deref(), Some("stalled too often"));
    let evs = events(&pool).await;
    assert_eq!(evs.len(), 1, "{evs:?}");
    assert_eq!(evs[0].0, "job.failed");
    assert_eq!(
        evs[0].1,
        json!({ "kind": "analyse-game", "gameId": id, "reason": "stalled too often" })
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn final_attempt_failure_publishes_job_failed_once_even_after_on_dead(pool: PgPool) {
    let id = add_game(&pool, "1. d4 d5 2. Nf3 Nf6 *", "final").await;
    let d = Arc::new(deps(&pool, pool_fake(Arc::new(Mutex::new(true)))));
    let res = run(&d, id, 3, 3).await;
    assert!(matches!(res, Err(JobError::Retryable { .. })), "{res:?}");
    // The runner dead-letters the job and calls on_dead: it must not publish a second event.
    let handler = AnalyseGameHandler(d.clone());
    within(
        "on_dead",
        handler.on_dead(&job(id, 3, 3), "engine exploded"),
    )
    .await;
    let failed: Vec<_> = events(&pool)
        .await
        .into_iter()
        .filter(|(t, _)| t == "job.failed")
        .collect();
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(failed[0].1["kind"], json!("analyse-game"));
    assert_eq!(failed[0].1["gameId"], json!(id));
    assert_eq!(
        game(&pool, id).await.analysis_status,
        AnalysisStatus::Failed
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn the_handler_parses_the_payload_and_returns_the_outcome_json(pool: PgPool) {
    let id = add_game(&pool, testkit::SCHOLARS, "handled").await;
    let handler = AnalyseGameHandler(Arc::new(deps(&pool, ok_pool())));
    let out = within(
        "the handler",
        handler.handle(
            job(id, 1, 3),
            JobContext {
                cancel: CancellationToken::new(),
            },
        ),
    )
    .await
    .unwrap();
    assert_eq!(out, json!({ "status": "done" }));
}

// ---------------------------------------------------------------------------------------------
// Fix round 1: generation safety

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn on_dead_after_a_reanalysis_leaves_the_new_generation_pending(pool: PgPool) {
    let id = add_game(&pool, "1. d4 d5 2. c4 c6 *", "dead-again").await;
    let d = Arc::new(deps(&pool, pool_fake(Arc::new(Mutex::new(true)))));
    // The final attempt writes failed (and publishes job.failed once).
    assert!(matches!(
        run(&d, id, 3, 3).await,
        Err(JobError::Retryable { .. })
    ));
    assert_eq!(
        game(&pool, id).await.analysis_status,
        AnalysisStatus::Failed
    );
    // The user asks for a re-analysis before the runner dead-letters the job.
    let queue = Queue::new(pool.clone());
    assert!(
        within("request_reanalysis", request_reanalysis(&pool, &queue, id))
            .await
            .unwrap()
    );
    // Then the runner dead-letters the old job.
    let handler = AnalyseGameHandler(d.clone());
    within(
        "on_dead",
        handler.on_dead(&job(id, 3, 3), "engine exploded"),
    )
    .await;
    let g = game(&pool, id).await;
    assert_eq!(g.analysis_status, AnalysisStatus::Pending);
    assert_eq!(g.analysis_generation, 1);
    assert_eq!(g.analysis_attempts, 0);
    let failed = events(&pool)
        .await
        .into_iter()
        .filter(|(t, _)| t == "job.failed")
        .count();
    assert_eq!(failed, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_reanalysis_landing_after_the_done_commit_is_run_by_the_same_job(pool: PgPool) {
    let id = add_game(&pool, "1. e4 e5 2. Nf3 d6 *", "late-request").await;
    let account = game(&pool, id).await.account_id;
    // Stands in for `request_reanalysis` landing between the handler's done commit and its
    // completion: the game.analysed insert (after the commit) runs request_reanalysis's UPDATE.
    // Its enqueue would dedupe onto the running job, as the job is live.
    for sql in [
        "CREATE FUNCTION test_reanalyse() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN \
           IF NEW.type = 'game.analysed' THEN \
             UPDATE games SET analysis_generation = analysis_generation + 1, \
               analysis_attempts = 0, analysis_status = 'pending', analysis_error = NULL, \
               pending_since = now() \
             WHERE id = (NEW.payload->>'gameId')::uuid AND analysis_generation = 0 \
               AND analysis_status <> 'analysing'; \
           END IF; RETURN NEW; END $$",
        "CREATE TRIGGER test_reanalyse AFTER INSERT ON user_events \
           FOR EACH ROW EXECUTE FUNCTION test_reanalyse()",
    ] {
        within(sql, sqlx::query(sql).execute(&pool)).await.unwrap();
    }
    let queue = Queue::new(pool.clone());
    assert!(
        within("enqueue", enqueue_analyse(&queue, id, account))
            .await
            .unwrap()
    );
    within(
        "backdate",
        sqlx::query("UPDATE jobs SET run_at = now() - interval '1 minute'").execute(&pool),
    )
    .await
    .unwrap();
    let dequeue = || async {
        within(
            "dequeue",
            queue.dequeue(&[KIND_ANALYSE], "test", 1, Duration::from_secs(60)),
        )
        .await
        .unwrap()
        .pop()
        .expect("a ready analyse job")
    };
    let d = deps(&pool, ok_pool());
    let first = dequeue().await;
    let res = within(
        "first run",
        handle_analyse_game(
            &d,
            &first,
            AnalyseGamePayload { game_id: id },
            CancellationToken::new(),
        ),
    )
    .await;
    let err = match res {
        Err(e @ JobError::Release { .. }) => e,
        other => panic!("expected a zero-delay release for the new generation, got {other:?}"),
    };
    assert!(
        matches!(&err, JobError::Release { delay, reason } if delay.is_zero() && reason == "reanalysis")
    );
    let g = game(&pool, id).await;
    assert_eq!(g.analysis_status, AnalysisStatus::Pending);
    assert_eq!((g.analysis_generation, g.analysed_generation), (1, Some(0)));

    let released = within("release", queue.fail(&first, &err, Backoff::default()))
        .await
        .unwrap();
    assert!(
        matches!(released, FailOutcome::Released { .. }),
        "{released:?}"
    );
    let second = dequeue().await;
    assert_eq!(second.id, first.id, "the same job runs again");
    let res = within(
        "second run",
        handle_analyse_game(
            &d,
            &second,
            AnalyseGamePayload { game_id: id },
            CancellationToken::new(),
        ),
    )
    .await;
    assert!(done(&res), "{res:?}");
    let g = game(&pool, id).await;
    assert_eq!(g.analysis_status, AnalysisStatus::Done);
    assert_eq!(g.analysed_generation, Some(1));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn the_generation_fence_stops_a_stale_run_holding_the_same_attempt_number(pool: PgPool) {
    let id = add_game(&pool, "1. e4 a6 2. d4 b5 *", "stale-generation").await;
    let (gate, stale) = gated_pool(-400, None);
    let stale_deps = Arc::new(cacheless(&pool, stale));
    let started = gate.started.notified();
    // Generation 0, attempt 1.
    let stale_run = tokio::spawn({
        let d = stale_deps.clone();
        async move { run(&d, id, 1, 3).await }
    });
    within("the stale run to reach the engine", started).await;
    // Its lease is lost and the game reset, then a re-analysis resets attempts to 0.
    exec(
        &pool,
        "UPDATE games SET analysis_status = 'pending' WHERE id = $1",
        id,
    )
    .await;
    let queue = Queue::new(pool.clone());
    assert!(
        within("request_reanalysis", request_reanalysis(&pool, &queue, id))
            .await
            .unwrap()
    );
    // Generation 1 claims attempt 1 too, and finishes.
    assert!(done(&run(&cacheless(&pool, ok_pool()), id, 1, 3).await));
    let fresh = game(&pool, id).await;
    assert_eq!((fresh.analysis_attempts, fresh.analysis_generation), (1, 1));

    gate.released.send_replace(true);
    let res = within("the stale run", stale_run).await.unwrap();
    assert!(matches!(res, Ok(AnalyseOutcome::Superseded)), "{res:?}");
    let g = game(&pool, id).await;
    assert_eq!(g.analysis_status, AnalysisStatus::Done);
    assert_eq!(g.analysed_generation, Some(1));
    assert_eq!(g.white_accuracy, fresh.white_accuracy);
    assert_eq!(g.black_accuracy, fresh.black_accuracy);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_superseded_run_stops_at_its_next_heartbeat(pool: PgPool) {
    let id = add_game(&pool, "1. e4 h6 2. d4 g5 *", "beat-lost").await;
    // The first call waits for the gate; every later call hangs until cancelled.
    let gate = Arc::new(Gate {
        started: Notify::new(),
        released: tokio::sync::watch::channel(false).0,
    });
    let calls = Arc::new(Mutex::new(0usize));
    let stale = {
        let (gate, calls) = (gate.clone(), calls.clone());
        FnAnalyser::new(move |_| {
            let (gate, calls) = (gate.clone(), calls.clone());
            Box::pin(async move {
                let n = {
                    let mut c = calls.lock().unwrap();
                    *c += 1;
                    *c
                };
                if n > 1 {
                    futures::future::pending::<()>().await;
                }
                gate.started.notify_one();
                let mut rx = gate.released.subscribe();
                rx.wait_for(|r| *r).await.ok();
                Ok(line(10))
            })
        })
    };
    let mut stale_deps = cacheless(&pool, stale);
    stale_deps.heartbeat_every = Duration::ZERO;
    stale_deps.concurrency = 1;
    let stale_deps = Arc::new(stale_deps);
    let started = gate.started.notified();
    let stale_run = tokio::spawn({
        let d = stale_deps.clone();
        async move { run(&d, id, 1, 3).await }
    });
    within("the stale run to reach the engine", started).await;
    assert!(done(&run(&cacheless(&pool, ok_pool()), id, 1, 3).await));
    gate.released.send_replace(true);
    // Its heartbeat after the first result misses the fence and cancels the hung second search
    // (without that it would hang until the 10-minute deadline).
    let res = within("the superseded run", stale_run).await.unwrap();
    assert!(matches!(res, Ok(AnalyseOutcome::Superseded)), "{res:?}");
    assert_eq!(game(&pool, id).await.analysis_status, AnalysisStatus::Done);
}

// ---------------------------------------------------------------------------------------------
// Integration (M2): the re-analysis handoff after skipped and after a final failure

/// Stands in for `request_reanalysis` landing right after this run writes `status`: an AFTER
/// UPDATE trigger runs its UPDATE once (generation 0 only). Its enqueue would dedupe onto the
/// running job.
async fn reanalyse_after_status(pool: &PgPool, status: &'static str) {
    let function = "CREATE FUNCTION test_reanalyse_after() RETURNS trigger LANGUAGE plpgsql AS $$ \
        BEGIN \
          IF NEW.analysis_status::text = TG_ARGV[0] AND NEW.analysis_generation = 0 THEN \
            UPDATE games SET analysis_generation = analysis_generation + 1, \
              analysis_attempts = 0, analysis_status = 'pending', analysis_error = NULL, \
              pending_since = now() \
            WHERE id = NEW.id; \
          END IF; RETURN NULL; END $$";
    within("trigger function", sqlx::query(function).execute(pool))
        .await
        .unwrap();
    let trigger = format!(
        "CREATE TRIGGER test_reanalyse_after AFTER UPDATE ON games \
         FOR EACH ROW EXECUTE FUNCTION test_reanalyse_after('{status}')"
    );
    within(
        "trigger",
        sqlx::query(sqlx::AssertSqlSafe(trigger)).execute(pool),
    )
    .await
    .unwrap();
}

fn is_reanalysis_release(res: &Result<AnalyseOutcome, JobError>) -> bool {
    matches!(res, Err(JobError::Release { delay, reason }) if delay.is_zero() && reason == "reanalysis")
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_reanalysis_landing_after_a_skipped_write_is_run_by_the_same_job(pool: PgPool) {
    let id = add_game(&pool, "1. e4 *", "skip-again").await;
    reanalyse_after_status(&pool, "skipped").await;
    let p = ok_pool();
    let res = run(&deps(&pool, p.clone()), id, 1, 3).await;
    assert!(is_reanalysis_release(&res), "{res:?}");
    assert_eq!(p.call_count(), 0);
    let g = game(&pool, id).await;
    assert_eq!(g.analysis_status, AnalysisStatus::Pending);
    assert_eq!((g.analysis_generation, g.analysis_attempts), (1, 0));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_reanalysis_landing_after_the_final_failed_write_is_run_by_the_same_job(pool: PgPool) {
    let id = add_game(&pool, "1. d4 e5 2. dxe5 d6 *", "fail-again").await;
    reanalyse_after_status(&pool, "failed").await;
    let fail = Arc::new(Mutex::new(true));
    let d = deps(&pool, pool_fake(fail.clone()));
    // The final attempt writes failed (and publishes job.failed), then sees the new generation.
    let res = run(&d, id, 3, 3).await;
    assert!(is_reanalysis_release(&res), "{res:?}");
    let g = game(&pool, id).await;
    assert_eq!(g.analysis_status, AnalysisStatus::Pending);
    assert_eq!((g.analysis_generation, g.analysis_attempts), (1, 0));
    let failed = events(&pool)
        .await
        .into_iter()
        .filter(|(t, _)| t == "job.failed")
        .count();
    assert_eq!(failed, 1);
    // The released job runs again and completes the new generation.
    *fail.lock().unwrap() = false;
    assert!(done(&run(&d, id, 3, 3).await));
    assert_eq!(game(&pool, id).await.analysed_generation, Some(1));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_non_final_failure_does_not_hand_off(pool: PgPool) {
    let id = add_game(&pool, "1. c4 e5 2. g3 d5 *", "retry-anyway").await;
    reanalyse_after_status(&pool, "pending").await;
    let d = deps(&pool, pool_fake(Arc::new(Mutex::new(true))));
    // A non-final failure is retried by the queue anyway; its next claim takes generation 1.
    let res = run(&d, id, 1, 3).await;
    assert!(matches!(res, Err(JobError::Retryable { .. })), "{res:?}");
    assert_eq!(game(&pool, id).await.analysis_generation, 1);
}

// ---------------------------------------------------------------------------------------------
// Task 22: the on_dead generation fence (T19 re-review M4)

/// Between the dead commit and `on_dead`, a re-analysis can start a new generation whose first
/// attempt is already running (`analysing`, fresh heartbeat). `on_dead` must leave it alone.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn on_dead_leaves_a_fresh_analysing_run_of_a_new_generation_alone(pool: PgPool) {
    let id = add_game(&pool, "1. e4 c5 2. Nf3 d6 *", "fresh-gen").await;
    exec(
        &pool,
        "UPDATE games SET analysis_status = 'analysing', analysis_attempts = 1, \
         analysis_generation = 1, heartbeat_at = now(), analysis_started_at = now() \
         WHERE id = $1",
        id,
    )
    .await;
    let handler = AnalyseGameHandler(Arc::new(deps(&pool, ok_pool())));
    within("on_dead", handler.on_dead(&job(id, 3, 3), "stalled")).await;
    let g = game(&pool, id).await;
    assert_eq!(g.analysis_status, AnalysisStatus::Analysing);
    assert!(events(&pool).await.is_empty());

    // A stalled run's heartbeat is old: that one is the dead job's own, and it is failed.
    exec(
        &pool,
        "UPDATE games SET heartbeat_at = now() - interval '1 hour' WHERE id = $1",
        id,
    )
    .await;
    within("on_dead", handler.on_dead(&job(id, 3, 3), "stalled")).await;
    assert_eq!(
        game(&pool, id).await.analysis_status,
        AnalysisStatus::Failed
    );
}
