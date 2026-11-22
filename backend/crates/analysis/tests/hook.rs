//! The analyse-game hook seam (R2 Task 31) with a fake hook: where it runs, what it sees, and
//! that no hook outcome loses the game's analysis or breaks R1's fence and release rules.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use analysis::eval_cache::DbEvalCache;
use analysis::hook::{AnalyseHook, HookError, HookInput, PreparedHook};
use analysis::jobs::analyse::{AnalyseGameDeps, AnalyseOutcome, handle_analyse_game};
use db::games::{AnalysisStatus, get_game};
use futures::future::BoxFuture;
use queue::{AnalyseGamePayload, Job, JobError, KIND_ANALYSE};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use testkit::fake_engine::FakeAnalyser;
use testkit::{GameSeed, insert_account, insert_game};
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const TEST_TIMEOUT: Duration = Duration::from_secs(30);

async fn within<T>(what: &str, fut: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(TEST_TIMEOUT, fut)
        .await
        .unwrap_or_else(|_| panic!("timed out after {TEST_TIMEOUT:?} waiting for {what}"))
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    /// Prepares; persist writes `enrichment_protocol = 'fake-v1'`.
    Ok,
    /// `prepare` fails.
    Fail,
    /// `prepare` is cancelled.
    Cancel,
    /// `persist` writes, then errors.
    PersistErr,
    /// A newer attempt claims the game during `prepare`.
    Supersede,
    /// A newer attempt claims the game, then `prepare` waits for the run's token.
    SupersedeAndWait,
    /// `prepare` waits for the run's token (the deadline).
    WaitForCancel,
    /// `prepare` takes this long and records the heartbeat before and after.
    Slow(Duration),
}

type Log = Arc<Mutex<Vec<String>>>;

struct FakeHook {
    pool: PgPool,
    mode: Mode,
    log: Log,
    /// `(plies, evals, records)` seen by `prepare`.
    seen: Arc<Mutex<Option<(usize, usize, usize)>>>,
    /// Heartbeat before and after a slow `prepare`.
    beats: Arc<Mutex<Option<(OffsetDateTime, OffsetDateTime)>>>,
}

impl FakeHook {
    fn new(pool: &PgPool, mode: Mode) -> Arc<Self> {
        Arc::new(FakeHook {
            pool: pool.clone(),
            mode,
            log: Arc::default(),
            seen: Arc::default(),
            beats: Arc::default(),
        })
    }

    fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    async fn heartbeat(&self, id: Uuid) -> OffsetDateTime {
        sqlx::query_scalar("SELECT heartbeat_at FROM games WHERE id = $1")
            .bind(id)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn claim_newer_attempt(&self, id: Uuid) {
        sqlx::query("UPDATE games SET analysis_attempts = analysis_attempts + 1 WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .unwrap();
    }
}

impl AnalyseHook for FakeHook {
    fn prepare<'a>(
        &'a self,
        i: HookInput<'a>,
    ) -> BoxFuture<'a, Result<Box<dyn PreparedHook>, HookError>> {
        Box::pin(async move {
            self.log.lock().unwrap().push("prepare".to_owned());
            *self.seen.lock().unwrap() =
                Some((i.parsed.plies.len(), i.evals.len(), i.records.len()));
            let id = i.game.id;
            match self.mode {
                Mode::Fail => return Err(HookError::Failed("boom".to_owned())),
                Mode::Cancel => return Err(HookError::Cancelled),
                Mode::Supersede => self.claim_newer_attempt(id).await,
                Mode::SupersedeAndWait => {
                    self.claim_newer_attempt(id).await;
                    i.cancel.cancelled().await;
                    return Err(HookError::Cancelled);
                }
                Mode::WaitForCancel => {
                    i.cancel.cancelled().await;
                    return Err(HookError::Cancelled);
                }
                Mode::Slow(d) => {
                    let before = self.heartbeat(id).await;
                    tokio::time::sleep(d).await;
                    let after = self.heartbeat(id).await;
                    *self.beats.lock().unwrap() = Some((before, after));
                }
                Mode::Ok | Mode::PersistErr => {}
            }
            Ok(Box::new(FakePrepared {
                game_id: id,
                pool: self.pool.clone(),
                fail: self.mode == Mode::PersistErr,
                log: self.log.clone(),
            }) as Box<dyn PreparedHook>)
        })
    }
}

struct FakePrepared {
    game_id: Uuid,
    pool: PgPool,
    fail: bool,
    log: Log,
}

impl PreparedHook for FakePrepared {
    fn persist<'a>(&'a self, tx: &'a mut PgConnection) -> BoxFuture<'a, Result<(), sqlx::Error>> {
        Box::pin(async move {
            self.log.lock().unwrap().push("persist".to_owned());
            sqlx::query("UPDATE games SET enrichment_protocol = 'fake-v1' WHERE id = $1")
                .bind(self.game_id)
                .execute(&mut *tx)
                .await?;
            if self.fail {
                return Err(sqlx::Error::Protocol("hook persist failed".to_owned()));
            }
            Ok(())
        })
    }

    fn after_commit(self: Box<Self>) -> BoxFuture<'static, ()> {
        Box::pin(async move {
            let published: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM user_events WHERE type = 'game.analysed' \
                 AND payload->>'gameId' = $1::text",
            )
            .bind(self.game_id)
            .fetch_one(&self.pool)
            .await
            .unwrap();
            self.log
                .lock()
                .unwrap()
                .push(format!("after_commit events={published}"));
        })
    }
}

fn deps(pool: &PgPool, hook: Option<Arc<FakeHook>>) -> AnalyseGameDeps {
    let mut d = AnalyseGameDeps::new(
        pool.clone(),
        Arc::new(FakeAnalyser::new()),
        Arc::new(DbEvalCache::new(pool.clone())),
    );
    d.hook = hook.map(|h| h as Arc<dyn AnalyseHook>);
    d
}

async fn add_game(pool: &PgPool) -> Uuid {
    let acct = insert_account(pool, "chesscom", "hooked").await;
    insert_game(
        pool,
        acct,
        GameSeed {
            white: "hooked".to_owned(),
            ..GameSeed::default()
        },
    )
    .await
}

fn job(game_id: Uuid) -> Job {
    let now = OffsetDateTime::now_utc();
    Job {
        id: 1,
        kind: KIND_ANALYSE.to_owned(),
        payload: json!({ "gameId": game_id }),
        dedupe_key: None,
        user_key: None,
        priority: 100,
        attempts: 1,
        max_attempts: 3,
        lease_token: Uuid::new_v4(),
        run_at: now,
        created_at: now,
        started_at: now,
    }
}

async fn run(d: &AnalyseGameDeps, id: Uuid) -> Result<AnalyseOutcome, JobError> {
    within(
        "handle_analyse_game",
        handle_analyse_game(
            d,
            &job(id),
            AnalyseGamePayload { game_id: id },
            CancellationToken::new(),
        ),
    )
    .await
}

async fn scalar_i64(pool: &PgPool, sql: &'static str, id: Uuid) -> i64 {
    sqlx::query_scalar(sql)
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn moves(pool: &PgPool, id: Uuid) -> i64 {
    scalar_i64(pool, "SELECT count(*) FROM moves WHERE game_id = $1", id).await
}

async fn previews(pool: &PgPool, id: Uuid) -> i64 {
    scalar_i64(
        pool,
        "SELECT count(*) FROM game_previews WHERE game_id = $1",
        id,
    )
    .await
}

async fn enrichment_protocol(pool: &PgPool, id: Uuid) -> Option<String> {
    sqlx::query_scalar("SELECT enrichment_protocol FROM games WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn analysed_events(pool: &PgPool) -> Vec<Value> {
    sqlx::query_scalar("SELECT payload FROM user_events WHERE type = 'game.analysed' ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
}

/// A stale preview row and an `enr-v1` mark from an earlier generation.
async fn seed_preview_and_enrichment(pool: &PgPool, id: Uuid) {
    sqlx::query(
        "INSERT INTO game_previews (game_id, protocol, moves) \
         VALUES ($1, 'sf19-n30k-preview-v1', '[]'::jsonb)",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE games SET enrichment_protocol = 'enr-v1' WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

fn event_keys(v: &Value) -> Vec<String> {
    let mut keys: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn hook_none_keeps_r1_behaviour(pool: PgPool) {
    let id = add_game(&pool).await;
    seed_preview_and_enrichment(&pool, id).await;
    let out = run(&deps(&pool, None), id).await;
    assert!(matches!(out, Ok(AnalyseOutcome::Done { .. })), "{out:?}");
    assert_eq!(moves(&pool, id).await, 7);
    // R1 never touches the R2 columns or tables.
    assert_eq!(
        enrichment_protocol(&pool, id).await.as_deref(),
        Some("enr-v1")
    );
    assert_eq!(previews(&pool, id).await, 1);
    let events = analysed_events(&pool).await;
    assert_eq!(events.len(), 1);
    assert_eq!(event_keys(&events[0]), ["accuracy", "gameId"]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn hook_rows_commit_with_the_analysis_and_after_commit_follows_the_publish(pool: PgPool) {
    let id = add_game(&pool).await;
    let hook = FakeHook::new(&pool, Mode::Ok);
    let out = run(&deps(&pool, Some(hook.clone())), id).await;
    assert!(matches!(out, Ok(AnalyseOutcome::Done { .. })), "{out:?}");
    // Scholar's mate: 7 plies, 8 evals, 7 records.
    assert_eq!(*hook.seen.lock().unwrap(), Some((7, 8, 7)));
    assert_eq!(
        hook.log(),
        ["prepare", "persist", "after_commit events=1"],
        "persist inside the analysis, after_commit after the publish"
    );
    assert_eq!(moves(&pool, id).await, 7);
    assert_eq!(
        enrichment_protocol(&pool, id).await.as_deref(),
        Some("fake-v1")
    );
    let game = get_game(&pool, id).await.unwrap().unwrap();
    assert_eq!(game.analysis_status, AnalysisStatus::Done);
    // The final event is the full pass's: no preview flag.
    let events = analysed_events(&pool).await;
    assert_eq!(events.len(), 1);
    assert_eq!(event_keys(&events[0]), ["accuracy", "gameId"]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn hook_failure_still_marks_done_without_features(pool: PgPool) {
    let id = add_game(&pool).await;
    seed_preview_and_enrichment(&pool, id).await;
    let hook = FakeHook::new(&pool, Mode::Fail);
    let out = run(&deps(&pool, Some(hook.clone())), id).await;
    assert!(matches!(out, Ok(AnalyseOutcome::Done { .. })), "{out:?}");
    assert_eq!(hook.log(), ["prepare"]);
    assert_eq!(moves(&pool, id).await, 7);
    // Left for `requeue_unenriched`; the preview is gone with the done transition.
    assert_eq!(enrichment_protocol(&pool, id).await, None);
    assert_eq!(previews(&pool, id).await, 0);
    assert_eq!(analysed_events(&pool).await.len(), 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn hook_persist_error_rolls_back_only_the_hook_rows(pool: PgPool) {
    let id = add_game(&pool).await;
    seed_preview_and_enrichment(&pool, id).await;
    let hook = FakeHook::new(&pool, Mode::PersistErr);
    let out = run(&deps(&pool, Some(hook.clone())), id).await;
    assert!(matches!(out, Ok(AnalyseOutcome::Done { .. })), "{out:?}");
    assert_eq!(hook.log(), ["prepare", "persist"], "no after_commit");
    assert_eq!(moves(&pool, id).await, 7);
    assert_eq!(
        enrichment_protocol(&pool, id).await,
        None,
        "fake-v1 rolled back"
    );
    assert_eq!(previews(&pool, id).await, 0);
    let game = get_game(&pool, id).await.unwrap().unwrap();
    assert_eq!(game.analysis_status, AnalysisStatus::Done);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn hook_cancel_releases_job(pool: PgPool) {
    let id = add_game(&pool).await;
    let hook = FakeHook::new(&pool, Mode::Cancel);
    let out = run(&deps(&pool, Some(hook)), id).await;
    match out {
        Err(JobError::Release { reason, delay }) => {
            assert_eq!(reason, "shutdown");
            assert_eq!(delay, Duration::ZERO);
        }
        other => panic!("expected a shutdown release, got {other:?}"),
    }
    let game = get_game(&pool, id).await.unwrap().unwrap();
    assert_eq!(game.analysis_status, AnalysisStatus::Pending);
    assert_eq!(moves(&pool, id).await, 0);
    assert!(analysed_events(&pool).await.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn hook_deadline_is_a_retryable_timeout(pool: PgPool) {
    let id = add_game(&pool).await;
    let hook = FakeHook::new(&pool, Mode::WaitForCancel);
    let mut d = deps(&pool, Some(hook));
    d.deadline = Duration::from_millis(300);
    let out = run(&d, id).await;
    match out {
        Err(JobError::Retryable { code, .. }) => assert_eq!(code.as_deref(), Some("timeout")),
        other => panic!("expected a timeout, got {other:?}"),
    }
    let game = get_game(&pool, id).await.unwrap().unwrap();
    assert_eq!(game.analysis_status, AnalysisStatus::Pending);
    assert_eq!(moves(&pool, id).await, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn superseded_during_prepare_writes_nothing(pool: PgPool) {
    let id = add_game(&pool).await;
    let hook = FakeHook::new(&pool, Mode::Supersede);
    let out = run(&deps(&pool, Some(hook.clone())), id).await;
    assert_eq!(out.unwrap(), AnalyseOutcome::Superseded);
    assert_eq!(hook.log(), ["prepare"], "no persist, no after_commit");
    assert_eq!(moves(&pool, id).await, 0);
    assert!(analysed_events(&pool).await.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn heartbeats_continue_during_prepare(pool: PgPool) {
    let id = add_game(&pool).await;
    let hook = FakeHook::new(&pool, Mode::Slow(Duration::from_millis(400)));
    let mut d = deps(&pool, Some(hook.clone()));
    d.heartbeat_every = Duration::from_millis(25);
    let out = run(&d, id).await;
    assert!(matches!(out, Ok(AnalyseOutcome::Done { .. })), "{out:?}");
    let (before, after) = hook.beats.lock().unwrap().expect("slow prepare ran");
    assert!(after > before, "heartbeat {before} not renewed by {after}");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn lost_heartbeat_during_prepare_cancels_and_supersedes(pool: PgPool) {
    let id = add_game(&pool).await;
    let hook = FakeHook::new(&pool, Mode::SupersedeAndWait);
    let mut d = deps(&pool, Some(hook.clone()));
    d.heartbeat_every = Duration::from_millis(25);
    let out = run(&d, id).await;
    assert_eq!(out.unwrap(), AnalyseOutcome::Superseded);
    assert_eq!(moves(&pool, id).await, 0);
}
