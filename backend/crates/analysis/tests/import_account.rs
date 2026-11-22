//! Port of apps/worker/test/jobs/import-account.test.ts, plus the R1 failure-mapping tests
//! (`not_found`, breaker re-delays reusing one paused run, shutdown).

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use analysis::jobs::import::{
    ACCOUNT_BUSY_DELAY, ImportAccountDeps, ImportSummary, account_lock_key, enqueue_import,
    handle_import_account,
};
use db::accounts::{ImportCursor, get_account};
use db::import_runs::{ImportRunStatus, list_runs};
use futures::StreamExt;
use futures::stream::BoxStream;
use platforms::breaker::{BreakerOptions, CircuitBreaker, CircuitState};
use platforms::chesscom::ChesscomSource;
use platforms::lichess::{LichessSource, lichess_games_url};
use platforms::{
    FetchOptions, GameSource, NormalizedGame, Platform, PlatformClient, PlatformError, Player,
};
use queue::queue::{Backoff, FailOutcome, Queue};
use queue::{
    ImportAccountPayload, Job, JobError, KIND_ANALYSE, KIND_IMPORT, KIND_SCHEDULE, schedule_key,
};
use sqlx::PgPool;
use testkit::fake_platform::FakeClient;
use time::OffsetDateTime;
use time::macros::datetime;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

// ---------------------------------------------------------------------------------------------
// Helpers (mirroring apps/worker/test/jobs/helpers.ts)

type GameStream = BoxStream<'static, Result<NormalizedGame, PlatformError>>;

fn make_game(white: &str, black: &str, played_at: OffsetDateTime) -> NormalizedGame {
    NormalizedGame {
        platform: Platform::Lichess,
        platform_game_id: Uuid::new_v4().simple().to_string()[..8].to_owned(),
        pgn: testkit::SCHOLARS.to_owned(),
        played_at,
        time_class: chess_core::TimeClass::Blitz,
        base_seconds: Some(300),
        increment_seconds: Some(0),
        rated: true,
        white: Player {
            name: white.to_owned(),
            rating: Some(1),
        },
        black: Player {
            name: black.to_owned(),
            rating: Some(1),
        },
        result: "1-0".to_owned(),
        termination: None,
        eco: None,
        opening_name: None,
        lichess_evals: None,
    }
}

/// A game source driven by a closure; records the options of every fetch.
struct FnSource {
    calls: Mutex<Vec<FetchOptions>>,
    f: Box<dyn Fn(&FetchOptions) -> GameStream + Send + Sync>,
}

impl FnSource {
    fn new(f: impl Fn(&FetchOptions) -> GameStream + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(FnSource {
            calls: Mutex::new(Vec::new()),
            f: Box::new(f),
        })
    }

    /// Streams `list`, then never ends when `hang` is set.
    fn listing(list: Vec<NormalizedGame>, hang: bool) -> Arc<Self> {
        Self::new(move |_| {
            let items = futures::stream::iter(list.clone().into_iter().map(Ok));
            if hang {
                items.chain(futures::stream::pending()).boxed()
            } else {
                items.boxed()
            }
        })
    }
}

impl GameSource for FnSource {
    fn fetch<'a>(
        &'a self,
        _client: Arc<dyn PlatformClient>,
        _username: &'a str,
        opts: FetchOptions,
    ) -> BoxStream<'a, Result<NormalizedGame, PlatformError>> {
        self.calls.lock().unwrap().push(opts.clone());
        (self.f)(&opts)
    }
}

fn deps_with(
    pool: &PgPool,
    source: Arc<dyn GameSource>,
    client: Arc<dyn PlatformClient>,
) -> ImportAccountDeps {
    let mut deps = ImportAccountDeps::new(pool.clone(), Queue::new(pool.clone()));
    deps.sources = HashMap::from([
        (Platform::Lichess, source.clone()),
        (Platform::Chesscom, source),
    ]);
    deps.clients = HashMap::from([
        (Platform::Lichess, client.clone()),
        (Platform::Chesscom, client),
    ]);
    deps
}

fn deps(pool: &PgPool, source: Arc<dyn GameSource>) -> ImportAccountDeps {
    deps_with(pool, source, Arc::new(FakeClient::default()))
}

/// Enqueues (or finds) the account's import job, makes it ready, and claims it.
async fn claim(queue: &Queue, account_id: Uuid) -> (Job, ImportAccountPayload) {
    enqueue_import(queue, account_id, None).await.unwrap();
    sqlx::query(
        "UPDATE jobs SET run_at = now() - interval '1 minute' WHERE kind = $1 AND state = 'queued'",
    )
    .bind(KIND_IMPORT)
    .execute(queue.pool())
    .await
    .unwrap();
    let job = queue
        .dequeue(&[KIND_IMPORT], "test", 1, Duration::from_secs(60))
        .await
        .unwrap()
        .pop()
        .expect("an import job to claim");
    let payload = serde_json::from_value(job.payload.clone()).unwrap();
    (job, payload)
}

/// Settles the job like the runner would.
async fn settle(queue: &Queue, job: &Job, res: &Result<ImportSummary, JobError>) -> FailOutcome {
    match res {
        Ok(_) => {
            assert!(queue.complete(job).await.unwrap());
            FailOutcome::Retried {
                run_at: OffsetDateTime::now_utc(),
            }
        }
        Err(e) => queue.fail(job, e, Backoff::default()).await.unwrap(),
    }
}

/// One import of the account, settled through the queue.
async fn run(deps: &ImportAccountDeps, account_id: Uuid) -> Result<ImportSummary, JobError> {
    let (job, payload) = claim(&deps.queue, account_id).await;
    let res = handle_import_account(deps, &job, payload, CancellationToken::new()).await;
    settle(&deps.queue, &job, &res).await;
    res
}

async fn game_rows(pool: &PgPool, account_id: Uuid) -> Vec<(Uuid, String, String)> {
    sqlx::query_as(
        "SELECT id, player_color::text, analysis_status::text FROM games WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn live_analyse_keys(pool: &PgPool) -> Vec<String> {
    let mut keys: Vec<String> = sqlx::query_scalar(
        "SELECT dedupe_key FROM jobs WHERE kind = $1 AND state IN ('queued','running','paused')",
    )
    .bind(KIND_ANALYSE)
    .fetch_all(pool)
    .await
    .unwrap();
    keys.sort();
    keys
}

/// Live schedule jobs: `(dedupe_key, payload)`.
async fn live_schedule_jobs(pool: &PgPool) -> Vec<(String, serde_json::Value)> {
    sqlx::query_as(
        "SELECT dedupe_key, payload FROM jobs WHERE kind = $1          AND state IN ('queued','running','paused') ORDER BY id",
    )
    .bind(KIND_SCHEDULE)
    .fetch_all(pool)
    .await
    .unwrap()
}

fn json_account(account_id: Uuid) -> serde_json::Value {
    serde_json::json!({ "accountId": account_id })
}

async fn events(pool: &PgPool, ty: &str) -> Vec<serde_json::Value> {
    sqlx::query_scalar("SELECT payload FROM user_events WHERE type = $1 ORDER BY id")
        .bind(ty)
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn attempts_of(pool: &PgPool, job_id: i64) -> i32 {
    sqlx::query_scalar("SELECT attempts FROM jobs WHERE id = $1")
        .bind(job_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Simulates a lost lease: the reaper re-queues the running job, so it can be claimed again
/// while the old handler is still running (the attempt is given back, as the reaper does).
async fn lose_lease(pool: &PgPool, job_id: i64) {
    sqlx::query(
        "UPDATE jobs SET state = 'queued', attempts = greatest(attempts - 1, 0), \
         lease_owner = NULL, lease_token = NULL, lease_until = NULL WHERE id = $1",
    )
    .bind(job_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn held_import_locks(pool: &PgPool) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND classid = 7262021 \
         AND objsubid = 2 \
         AND database = (SELECT oid FROM pg_database WHERE datname = current_database())",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The archive-list URL `ChesscomSource` builds for `user`, read from a probe run against a
/// fake that knows no URLs (so the test never hand-writes it).
async fn chesscom_archives_url(user: &str) -> String {
    let probe = Arc::new(FakeClient::default());
    let client: Arc<dyn PlatformClient> = probe.clone();
    let _: Vec<_> = ChesscomSource::new()
        .fetch(client, user, FetchOptions::default())
        .collect()
        .await;
    probe
        .calls()
        .first()
        .expect("the archive list request")
        .0
        .clone()
}

const JAN_1: OffsetDateTime = datetime!(2026-01-01 00:00 UTC);

// ---------------------------------------------------------------------------------------------
// Ported from import-account.test.ts

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn import_twice_yields_same_rows_imported_with_one_schedule_job(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "lichess", "twice").await;
    let list = vec![
        make_game("Twice", "Villain", JAN_1),
        make_game("Hero", "TWICE", JAN_1),
    ];
    let d = deps(&pool, FnSource::listing(list.clone(), false));
    assert_eq!(
        run(&d, acct).await.unwrap(),
        ImportSummary {
            imported: 2,
            skipped: 0
        }
    );
    // R2 (ADR-R2-16): imported games wait for the scheduler; no per-game analyse job.
    assert!(live_analyse_keys(&pool).await.is_empty());
    assert_eq!(
        live_schedule_jobs(&pool).await,
        vec![(schedule_key(acct), json_account(acct))]
    );
    // Reset the cursor so the second import streams the same games again.
    sqlx::query("UPDATE linked_accounts SET import_cursor = NULL WHERE id = $1")
        .bind(acct)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        run(&d, acct).await.unwrap(),
        ImportSummary {
            imported: 0,
            skipped: 0
        }
    );
    let rows = game_rows(&pool, acct).await;
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|(_, _, status)| status == "imported"));
    assert!(live_analyse_keys(&pool).await.is_empty());
    // The second import's schedule enqueue deduped onto the live job.
    assert_eq!(live_schedule_jobs(&pool).await.len(), 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn imported_games_store_their_ply_count(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "lichess", "plies").await;
    let mut annotated = make_game("Plies", "Villain", JAN_1);
    annotated.pgn = "[Event \"x\"]\n[White \"Plies\"]\n\n1. e4 {[%clk 0:03:00]} 1... e5 \
        2. Nf3 $1 (2. Qh5 Nc6) 2... Nc6 ; a comment\n3. Bb5 a6 1/2-1/2"
        .to_owned();
    let empty = {
        let mut g = make_game("Plies", "Villain", JAN_1 - time::Duration::days(1));
        g.pgn = "[Event \"x\"]\n\n*".to_owned();
        g
    };
    let d = deps(
        &pool,
        FnSource::listing(
            vec![make_game("Plies", "Villain", JAN_1), annotated, empty],
            false,
        ),
    );
    run(&d, acct).await.unwrap();
    let mut plies: Vec<Option<i32>> =
        sqlx::query_scalar("SELECT ply_count FROM games WHERE account_id = $1")
            .bind(acct)
            .fetch_all(&pool)
            .await
            .unwrap();
    plies.sort();
    assert_eq!(plies, vec![Some(0), Some(6), Some(7)]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_payload_without_max_has_no_plan_limit_in_the_beta(pool: PgPool) {
    // Owner ruling (ADR-R2-16): no plan limits until M7; only an explicit `max` bounds a run.
    let acct = testkit::insert_account(&pool, "lichess", "defmax").await;
    let source = FnSource::listing(vec![make_game("Defmax", "Villain", JAN_1)], false);
    let d = deps(&pool, source.clone());
    assert_eq!(d.default_max, None);
    run(&d, acct).await.unwrap();
    let calls = source.calls.lock().unwrap().clone();
    assert!(!calls.is_empty());
    assert!(calls.iter().all(|c| c.max.is_none()), "{calls:?}");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_legacy_pending_game_is_not_re_enqueued_by_the_import(pool: PgPool) {
    // R1 stored games as `pending` and enqueued each one; the reconciler now owns lost jobs.
    let acct = testkit::insert_account(&pool, "lichess", "legacyimp").await;
    let list = vec![make_game("Legacyimp", "Villain", JAN_1)];
    let d = deps(&pool, FnSource::listing(list, false));
    run(&d, acct).await.unwrap();
    sqlx::query("UPDATE games SET analysis_status = 'pending' WHERE account_id = $1")
        .bind(acct)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE linked_accounts SET import_cursor = NULL WHERE id = $1")
        .bind(acct)
        .execute(&pool)
        .await
        .unwrap();
    run(&d, acct).await.unwrap();
    assert!(live_analyse_keys(&pool).await.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn sets_player_color_case_insensitively_and_skips_games_without_the_user(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "lichess", "colour").await;
    let list = vec![
        make_game("COLOUR", "Villain", JAN_1),
        make_game("Hero", "Colour", JAN_1),
        make_game("Hero", "Villain", JAN_1),
    ];
    let res = run(&deps(&pool, FnSource::listing(list, false)), acct)
        .await
        .unwrap();
    assert_eq!(
        res,
        ImportSummary {
            imported: 2,
            skipped: 1
        }
    );
    let mut colours: Vec<String> = game_rows(&pool, acct)
        .await
        .into_iter()
        .map(|(_, c, _)| c)
        .collect();
    colours.sort();
    assert_eq!(colours, ["black", "white"]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn updates_last_synced_at(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "lichess", "synced").await;
    let before = get_account(&pool, acct).await.unwrap().unwrap();
    assert_eq!(before.last_synced_at, None);
    run(&deps(&pool, FnSource::listing(vec![], false)), acct)
        .await
        .unwrap();
    let after = get_account(&pool, acct).await.unwrap().unwrap();
    assert!(after.last_synced_at.is_some());
    // And it finishes the run with a final progress event.
    let runs = list_runs(&pool, acct).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, ImportRunStatus::Done);
    let progress = events(&pool, "import.progress").await;
    assert_eq!(
        progress.last().unwrap(),
        &serde_json::json!({"runId": runs[0].id, "imported": 0, "done": true})
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_missing_account_throws_unrecoverable_error(pool: PgPool) {
    let d = deps(&pool, FnSource::listing(vec![], false));
    let missing = Uuid::parse_str("00000000-0000-4000-8000-000000000000").unwrap();
    let (job, payload) = claim(&d.queue, missing).await;
    let res = handle_import_account(&d, &job, payload, CancellationToken::new()).await;
    assert!(
        matches!(res, Err(JobError::Unrecoverable { .. })),
        "{res:?}"
    );
    assert_eq!(settle(&d.queue, &job, &res).await, FailOutcome::Failed);
    assert!(events(&pool, "job.failed").await.is_empty());
}

// ---------------------------------------------------------------------------------------------
// R1: failure mapping (ADR 0035)

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn not_found_fails_immediately_without_retry(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "chesscom", "ghost").await;
    // Every URL is unknown to the fake client, so the archive list is a typed NotFound.
    let client = Arc::new(FakeClient::default());
    let d = deps_with(&pool, Arc::new(ChesscomSource::new()), client.clone());
    let (job, payload) = claim(&d.queue, acct).await;
    let res = handle_import_account(&d, &job, payload, CancellationToken::new()).await;
    match &res {
        Err(JobError::Unrecoverable { code, .. }) => assert_eq!(code.as_deref(), Some("not_found")),
        other => panic!("expected Unrecoverable not_found, got {other:?}"),
    }
    assert_eq!(client.calls().len(), 1);

    let runs = list_runs(&pool, acct).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, ImportRunStatus::Failed);
    assert_eq!(runs[0].error_code.as_deref(), Some("not_found"));

    let failed = events(&pool, "job.failed").await;
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0]["kind"], "import-account");
    assert_eq!(failed[0]["runId"], runs[0].id.to_string());
    assert!(failed[0]["reason"].as_str().unwrap().contains("Not found"));

    assert_eq!(settle(&d.queue, &job, &res).await, FailOutcome::Failed);
    let (state, attempts): (String, i32) =
        sqlx::query_as("SELECT state::text, attempts FROM jobs WHERE id = $1")
            .bind(job.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((state.as_str(), attempts), ("failed", 1));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn redelays_reuse_one_paused_run(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "lichess", "paused").await;
    let clock = Arc::new(Mutex::new(Instant::now()));
    let now = {
        let clock = clock.clone();
        Arc::new(move || *clock.lock().unwrap())
    };
    let breaker = Arc::new(CircuitBreaker::new(BreakerOptions {
        failure_threshold: 1,
        cooldown: Duration::from_secs(60),
        now,
        ..BreakerOptions::default()
    }));
    let _ = breaker
        .exec(|| async { Err::<(), _>(PlatformError::Transport("503".into())) })
        .await;
    assert_eq!(breaker.state(), CircuitState::Open);

    let client = Arc::new(FakeClient::default());
    let mut d = deps_with(
        &pool,
        Arc::new(LichessSource { token: None }),
        client.clone(),
    );
    d.breakers = HashMap::from([(Platform::Lichess, breaker.clone())]);

    let mut run_ids = Vec::new();
    for _ in 0..3 {
        let (job, payload) = claim(&d.queue, acct).await;
        // Every claim is the job's first attempt again: a release gives the attempt back.
        assert_eq!(job.attempts, 1);
        let res = handle_import_account(&d, &job, payload, CancellationToken::new()).await;
        match &res {
            Err(JobError::Release { delay, reason }) => {
                assert_eq!(reason, "circuit_open");
                assert!(*delay >= Duration::from_secs(5), "{delay:?}");
            }
            other => panic!("expected Release, got {other:?}"),
        }
        assert!(matches!(
            settle(&d.queue, &job, &res).await,
            FailOutcome::Released { .. }
        ));
        assert_eq!(attempts_of(&pool, job.id).await, 0);
        let runs = list_runs(&pool, acct).await.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, ImportRunStatus::Paused);
        assert_eq!(runs[0].error_code.as_deref(), Some("circuit_open"));
        run_ids.push(runs[0].id);
    }
    assert!(run_ids.windows(2).all(|w| w[0] == w[1]));
    // The open breaker never let a request through.
    assert!(client.calls().is_empty());

    // The cooldown passes: the next run probes, the platform answers, and the run completes.
    *clock.lock().unwrap() += Duration::from_secs(61);
    assert_eq!(breaker.state(), CircuitState::HalfOpen);
    run(&d, acct).await.unwrap();
    let runs = list_runs(&pool, acct).await.unwrap();
    let statuses: Vec<_> = runs.iter().map(|r| r.status.as_str()).collect();
    assert_eq!(statuses, ["done"]);
    assert_eq!(runs[0].id, run_ids[0]);
    assert_eq!(runs[0].error_code, None);
    assert_eq!(runs[0].last_error, None);
    assert_eq!(client.calls().len(), 1);
    assert_eq!(breaker.state(), CircuitState::Closed);
    assert!(events(&pool, "job.failed").await.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn shutdown_cancel_pauses_the_run_and_releases(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "lichess", "stopper").await;
    let list = vec![
        make_game("Stopper", "Villain", datetime!(2026-03-04 12:00 UTC)),
        make_game("Stopper", "Villain", datetime!(2026-03-03 12:00 UTC)),
    ];
    let d = Arc::new(deps(&pool, FnSource::listing(list.clone(), true)));
    let (job, payload) = claim(&d.queue, acct).await;
    let cancel = CancellationToken::new();
    let task = {
        let (d, job, cancel) = (d.clone(), job.clone(), cancel.clone());
        tokio::spawn(async move { handle_import_account(&d, &job, payload, cancel).await })
    };
    testkit::wait_for(
        "two imported games",
        Duration::from_secs(10),
        Duration::from_millis(20),
        || {
            let pool = pool.clone();
            async move { (game_rows(&pool, acct).await.len() == 2).then_some(()) }
        },
    )
    .await;
    cancel.cancel();
    let res = tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .expect("the handler stops after cancel")
        .unwrap();
    match &res {
        Err(JobError::Release { delay, reason }) => {
            assert_eq!(*delay, Duration::ZERO);
            assert_eq!(reason, "shutdown");
        }
        other => panic!("expected Release, got {other:?}"),
    }
    let runs = list_runs(&pool, acct).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, ImportRunStatus::Paused);
    assert_eq!(runs[0].error_code.as_deref(), Some("shutdown"));
    let cursor = get_account(&pool, acct)
        .await
        .unwrap()
        .unwrap()
        .import_cursor;
    assert_eq!(
        cursor,
        Some(ImportCursor {
            oldest_reached: Some(list[1].played_at),
            newest_seen_at: None,
            backfill: false,
        })
    );
    assert!(matches!(
        settle(&d.queue, &job, &res).await,
        FailOutcome::Released { .. }
    ));
    let attempts: i32 = sqlx::query_scalar("SELECT attempts FROM jobs WHERE id = $1")
        .bind(job.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(attempts, 0);
    assert!(events(&pool, "job.failed").await.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn enqueue_import_dedupes_one_live_job_per_account(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = testkit::insert_account(&pool, "lichess", "dedupe").await;
    assert!(enqueue_import(&queue, acct, Some(10)).await.unwrap());
    assert!(!enqueue_import(&queue, acct, None).await.unwrap());
    let (key, user_key, max_attempts, payload): (String, Uuid, i32, serde_json::Value) =
        sqlx::query_as("SELECT dedupe_key, user_key, max_attempts, payload FROM jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(key, format!("import:{acct}"));
    assert_eq!(user_key, acct);
    assert_eq!(max_attempts, 3);
    assert_eq!(
        payload,
        serde_json::json!({"accountId": acct.to_string(), "max": 10})
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn handler_runs_the_import_and_rejects_a_bad_payload(pool: PgPool) {
    use analysis::jobs::import::ImportAccountHandler;
    let acct = testkit::insert_account(&pool, "lichess", "handled").await;
    let list = vec![make_game("Handled", "Villain", JAN_1)];
    let handler = ImportAccountHandler::new(Arc::new(deps(&pool, FnSource::listing(list, false))));
    assert_eq!(handler.kind(), KIND_IMPORT);
    let (job, _) = claim(&handler.deps.queue, acct).await;
    let out = handler
        .handle(job.clone(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(out, serde_json::json!({"imported": 1, "skipped": 0}));

    let bad = Job {
        payload: serde_json::json!({"nope": true}),
        ..job
    };
    let res = handler.handle(bad, CancellationToken::new()).await;
    assert!(
        matches!(res, Err(JobError::Unrecoverable { .. })),
        "{res:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// Fix round 1: account-exclusive imports, scoped not-found

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn overlapping_handlers_on_one_account_are_exclusive_with_no_gap_and_no_lost_games(
    pool: PgPool,
) {
    let acct = testkit::insert_account(&pool, "lichess", "overlap").await;
    // Newest first, six days in March.
    let list: Vec<_> = (1..=6u8)
        .rev()
        .map(|d| {
            let at = datetime!(2026-03-01 12:00 UTC).replace_day(d).unwrap();
            make_game("Overlap", "Villain", at)
        })
        .collect();
    // Handler A streams three games, then stalls until the gate opens.
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let src_a = {
        let (gate, list) = (gate.clone(), list.clone());
        FnSource::new(move |_| {
            let (gate, head, rest) = (gate.clone(), list[..3].to_vec(), list[3..].to_vec());
            futures::stream::iter(head.into_iter().map(Ok))
                .chain(
                    futures::stream::once(async move {
                        gate.acquire().await.unwrap().forget();
                        futures::stream::iter(rest.into_iter().map(Ok))
                    })
                    .flatten(),
                )
                .boxed()
        })
    };
    let da = Arc::new(deps(&pool, src_a));
    let (job_a, payload_a) = claim(&da.queue, acct).await;
    let task_a = {
        let (da, job_a) = (da.clone(), job_a.clone());
        tokio::spawn(async move {
            handle_import_account(&da, &job_a, payload_a, CancellationToken::new()).await
        })
    };
    testkit::wait_for(
        "handler A's first three games",
        Duration::from_secs(10),
        Duration::from_millis(20),
        || {
            let pool = pool.clone();
            async move { (game_rows(&pool, acct).await.len() == 3).then_some(()) }
        },
    )
    .await;

    // A's lease is lost and the job is claimed again (B) while A is still running.
    lose_lease(&pool, job_a.id).await;
    let db = deps(&pool, FnSource::listing(list.clone(), false));
    let (job_b, payload_b) = claim(&db.queue, acct).await;
    assert_eq!(job_b.id, job_a.id);
    let res_b = handle_import_account(&db, &job_b, payload_b, CancellationToken::new()).await;
    match &res_b {
        Err(JobError::Release { delay, reason }) => {
            assert_eq!(reason, "account_busy");
            assert_eq!(*delay, ACCOUNT_BUSY_DELAY);
        }
        other => panic!("expected the busy account to release B, got {other:?}"),
    }
    // B wrote nothing: no run of its own, and the cursor is still untouched.
    assert_eq!(list_runs(&pool, acct).await.unwrap().len(), 1);
    assert_eq!(
        get_account(&pool, acct)
            .await
            .unwrap()
            .unwrap()
            .import_cursor,
        None
    );
    assert!(matches!(
        settle(&db.queue, &job_b, &res_b).await,
        FailOutcome::Released { .. }
    ));
    assert_eq!(attempts_of(&pool, job_b.id).await, 0);
    assert_eq!(db.busy_releases.load(Ordering::Relaxed), 1);

    // A finishes its stream (its own complete would be fenced off: its lease is gone).
    gate.add_permits(1);
    let res_a = tokio::time::timeout(Duration::from_secs(10), task_a)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(res_a.imported, 6);
    // A unlocked explicitly on its way out: no wait for a connection close.
    assert_eq!(held_import_locks(&pool).await, 0);

    // B runs again once A is done.
    let res_b = run(&db, acct).await.unwrap();
    assert_eq!(res_b.imported, 0);

    let rows = game_rows(&pool, acct).await;
    assert_eq!(rows.len(), 6, "no lost games");
    assert_eq!(
        get_account(&pool, acct)
            .await
            .unwrap()
            .unwrap()
            .import_cursor,
        Some(ImportCursor {
            oldest_reached: None,
            newest_seen_at: Some(list[0].played_at),
            backfill: false,
        }),
        "the watermark covers a contiguous import"
    );
    let statuses: Vec<_> = list_runs(&pool, acct)
        .await
        .unwrap()
        .iter()
        .map(|r| r.status.as_str())
        .collect();
    assert_eq!(statuses, ["done", "done"]);
    assert_eq!(held_import_locks(&pool).await, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn the_account_lock_is_released_when_a_handler_is_dropped(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "lichess", "dropped").await;
    let one = vec![make_game("Dropped", "Villain", JAN_1)];
    let da = Arc::new(deps(&pool, FnSource::listing(one.clone(), true)));
    let (job_a, payload_a) = claim(&da.queue, acct).await;
    let task_a = {
        let (da, job_a) = (da.clone(), job_a.clone());
        tokio::spawn(async move {
            handle_import_account(&da, &job_a, payload_a, CancellationToken::new()).await
        })
    };
    testkit::wait_for(
        "handler A's game",
        Duration::from_secs(10),
        Duration::from_millis(20),
        || {
            let pool = pool.clone();
            async move { (game_rows(&pool, acct).await.len() == 1).then_some(()) }
        },
    )
    .await;
    assert_eq!(held_import_locks(&pool).await, 1);
    // The runner drops the handler future (abort); the lock connection closes with it.
    task_a.abort();
    assert!(task_a.await.unwrap_err().is_cancelled());
    testkit::wait_for(
        "the account lock to be released",
        Duration::from_secs(10),
        Duration::from_millis(20),
        || {
            let pool = pool.clone();
            async move { (held_import_locks(&pool).await == 0).then_some(()) }
        },
    )
    .await;
    lose_lease(&pool, job_a.id).await;
    let res = run(&deps(&pool, FnSource::listing(one, false)), acct).await;
    assert!(res.is_ok(), "{res:?}");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_404_on_one_chesscom_month_stays_retryable(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "chesscom", "gappy").await;
    let archives = chesscom_archives_url("gappy").await;
    // Month URLs come from the API's archive list, so the fake chooses this one.
    let month = "https://api.chess.com/pub/player/gappy/games/2026/01";
    let client = Arc::new(FakeClient {
        // The month itself is unknown to the fake: a 404 on that one archive.
        json: HashMap::from([(archives.clone(), serde_json::json!({ "archives": [month] }))]),
        ..FakeClient::default()
    });
    let d = deps_with(&pool, Arc::new(ChesscomSource::new()), client.clone());
    let (job, payload) = claim(&d.queue, acct).await;
    let res = handle_import_account(&d, &job, payload, CancellationToken::new()).await;
    match &res {
        Err(JobError::Retryable { code, .. }) => assert_eq!(code.as_deref(), Some("unknown")),
        other => panic!("expected a retryable failure, got {other:?}"),
    }
    let urls: Vec<_> = client.calls().into_iter().map(|(u, _)| u).collect();
    assert_eq!(urls, [archives.as_str(), month]);
    let runs = list_runs(&pool, acct).await.unwrap();
    assert_eq!(runs[0].status, ImportRunStatus::Failed);
    assert_eq!(runs[0].error_code.as_deref(), Some("unknown"));
    assert!(matches!(
        settle(&d.queue, &job, &res).await,
        FailOutcome::Retried { .. }
    ));
    // Not the final attempt, so no job.failed yet.
    assert!(events(&pool, "job.failed").await.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_lichess_user_404_fails_immediately_without_retry(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "lichess", "nobody").await;
    // A first import fetches with no since/until/max; any other URL would stream nothing.
    let url = lichess_games_url("nobody", &FetchOptions::default());
    let client = Arc::new(FakeClient {
        ndjson_by_url: HashMap::from([(
            url.clone(),
            vec![Err(PlatformError::NotFound { url: url.clone() })],
        )]),
        ..FakeClient::default()
    });
    let d = deps_with(
        &pool,
        Arc::new(LichessSource { token: None }),
        client.clone(),
    );
    let (job, payload) = claim(&d.queue, acct).await;
    let res = handle_import_account(&d, &job, payload, CancellationToken::new()).await;
    match &res {
        Err(JobError::Unrecoverable { code, .. }) => {
            assert_eq!(code.as_deref(), Some("not_found"))
        }
        other => panic!("expected Unrecoverable not_found, got {other:?}"),
    }
    let urls: Vec<_> = client.calls().into_iter().map(|(u, _)| u).collect();
    assert_eq!(urls, [url]);
    assert_eq!(settle(&d.queue, &job, &res).await, FailOutcome::Failed);
    assert_eq!(events(&pool, "job.failed").await.len(), 1);
}

// ---------------------------------------------------------------------------------------------
// Fix round 2: lock before the cursor read

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_busy_account_is_released_before_the_account_is_read(pool: PgPool) {
    // The account does not exist, so reading it first would give Unrecoverable. Holding its
    // lock elsewhere must give account_busy instead: the lock comes before the read.
    let missing = Uuid::new_v4();
    let (classid, objid) = account_lock_key(missing);
    let mut holder = pool.acquire().await.unwrap();
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1, $2)")
        .bind(classid)
        .bind(objid)
        .fetch_one(&mut *holder)
        .await
        .unwrap();
    assert!(locked);
    assert_eq!(held_import_locks(&pool).await, 1);

    let d = deps(&pool, FnSource::listing(vec![], false));
    let (job, payload) = claim(&d.queue, missing).await;
    let res = handle_import_account(&d, &job, payload, CancellationToken::new()).await;
    match &res {
        Err(JobError::Release { reason, .. }) => assert_eq!(reason, "account_busy"),
        other => panic!("expected account_busy, got {other:?}"),
    }
    assert_eq!(d.busy_releases.load(Ordering::Relaxed), 1);
    assert!(matches!(
        settle(&d.queue, &job, &res).await,
        FailOutcome::Released { .. }
    ));

    let unlocked: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1, $2)")
        .bind(classid)
        .bind(objid)
        .fetch_one(&mut *holder)
        .await
        .unwrap();
    assert!(unlocked);
    let (job, payload) = claim(&d.queue, missing).await;
    let res = handle_import_account(&d, &job, payload, CancellationToken::new()).await;
    assert!(
        matches!(res, Err(JobError::Unrecoverable { .. })),
        "{res:?}"
    );
    // The missing-account path unlocked too.
    assert_eq!(held_import_locks(&pool).await, 0);
    assert_eq!(d.busy_releases.load(Ordering::Relaxed), 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn back_to_back_imports_never_see_their_own_lock(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "lichess", "again").await;
    let d = deps(
        &pool,
        FnSource::listing(vec![make_game("Again", "Villain", JAN_1)], false),
    );
    for _ in 0..5 {
        run(&d, acct).await.unwrap();
        assert_eq!(held_import_locks(&pool).await, 0);
    }
    assert_eq!(d.busy_releases.load(Ordering::Relaxed), 0);
}

// ---------------------------------------------------------------------------------------------
// Task 22

/// The reconciler closes a run with a stale heartbeat as failed (and enqueues a fresh import).
/// A hung handler that later reaches the end must not flip that run to done.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn finish_does_not_overwrite_a_run_the_reconciler_closed(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "lichess", "hung").await;
    let game = make_game("Hung", "Villain", datetime!(2026-03-04 12:00 UTC));
    let source = {
        let pool = pool.clone();
        FnSource::new(move |_| {
            let pool = pool.clone();
            let close = futures::stream::once(async move {
                sqlx::query(
                    "UPDATE import_runs SET status = 'failed', error_code = 'timeout', \
                     finished_at = now() WHERE account_id = $1",
                )
                .bind(acct)
                .execute(&pool)
                .await
                .unwrap();
            })
            .filter_map(|()| async { None });
            futures::stream::iter(vec![Ok(game.clone())])
                .chain(close)
                .boxed()
        })
    };
    let d = deps(&pool, source);
    let res = run(&d, acct).await;
    assert!(res.is_ok(), "{res:?}");
    let runs = list_runs(&pool, acct).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, ImportRunStatus::Failed);
    assert_eq!(runs[0].error_code.as_deref(), Some("timeout"));
    assert_eq!(game_rows(&pool, acct).await.len(), 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn the_import_handler_is_a_queue_handler(pool: PgPool) {
    use analysis::jobs::import::ImportAccountHandler;
    use queue::runner::Handler;
    let handler: Arc<dyn Handler> = Arc::new(ImportAccountHandler::new(Arc::new(deps(
        &pool,
        FnSource::listing(Vec::new(), false),
    ))));
    assert_eq!(handler.kind(), KIND_IMPORT);
}
