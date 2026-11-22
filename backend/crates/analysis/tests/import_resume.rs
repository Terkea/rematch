//! Port of apps/worker/test/recovery/import-resume.test.ts, plus `breaker_redelay`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use analysis::deadline::JobDeadlineError;
use analysis::jobs::import::{
    ImportAccountDeps, ImportFailure, ImportSummary, breaker_redelay, enqueue_import,
    handle_import_account, import_error_code,
};
use db::accounts::{ImportCursor, get_account};
use db::import_runs::{ImportErrorCode, ImportRunStatus, list_runs};
use futures::StreamExt;
use futures::stream::BoxStream;
use platforms::breaker::{BreakerOptions, CircuitBreaker, CircuitState};
use platforms::lichess::LichessSource;
use platforms::{
    FetchOptions, GameSource, NormalizedGame, Platform, PlatformClient, PlatformError, Player,
};
use queue::queue::{Backoff, Queue};
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
// Helpers

type GameStream = BoxStream<'static, Result<NormalizedGame, PlatformError>>;

fn make_game(white: &str, played_at: OffsetDateTime) -> NormalizedGame {
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
            name: "Villain".to_owned(),
            rating: Some(1500),
        },
        result: "1-0".to_owned(),
        termination: None,
        eco: None,
        opening_name: None,
        lichess_evals: None,
    }
}

/// Newest first, as both platforms stream.
fn four_games(name: &str) -> Vec<NormalizedGame> {
    [4u8, 3, 2, 1]
        .iter()
        .map(|d| {
            let at = datetime!(2026-03-01 12:00 UTC).replace_day(*d).unwrap();
            make_game(name, at)
        })
        .collect()
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

    fn calls(&self) -> Vec<FetchOptions> {
        self.calls.lock().unwrap().clone()
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

/// A fake importer that honours since/until (both inclusive). With `fail_after`, the stream
/// fails with a transport error instead of yielding that many-plus-one games.
fn importer(list: Vec<NormalizedGame>, fail_after: Option<usize>) -> Arc<FnSource> {
    FnSource::new(move |opts| {
        let mut items = Vec::new();
        let in_range = |g: &&NormalizedGame| {
            opts.since.is_none_or(|s| g.played_at >= s)
                && opts.until.is_none_or(|u| g.played_at <= u)
        };
        for g in list.iter().filter(in_range) {
            if fail_after == Some(items.len()) {
                items.push(Err(PlatformError::Transport("connection reset".into())));
                break;
            }
            items.push(Ok(g.clone()));
        }
        futures::stream::iter(items).boxed()
    })
}

fn deps_with(
    pool: &PgPool,
    source: Arc<dyn GameSource>,
    client: Arc<dyn PlatformClient>,
) -> ImportAccountDeps {
    let mut deps = ImportAccountDeps::new(pool.clone(), Queue::new(pool.clone()));
    // These tests pin the cursor's fetch ranges; the default budget (R2) is tested in
    // import_account.rs.
    deps.default_max = None;
    deps.sources = HashMap::from([
        (Platform::Lichess, source.clone()),
        (Platform::Chesscom, source),
    ]);
    deps.clients = HashMap::from([
        (Platform::Lichess, client.clone()),
        (Platform::Chesscom, client),
    ]);
    // Exact cursors here; the Lichess look-back has its own test.
    deps.lichess_lookback = Duration::ZERO;
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

/// One import of the account, settled through the queue like the runner would.
async fn run(deps: &ImportAccountDeps, account_id: Uuid) -> Result<ImportSummary, JobError> {
    let (job, payload) = claim(&deps.queue, account_id).await;
    let res = handle_import_account(deps, &job, payload, CancellationToken::new()).await;
    match &res {
        Ok(_) => assert!(deps.queue.complete(&job).await.unwrap()),
        Err(e) => {
            deps.queue.fail(&job, e, Backoff::default()).await.unwrap();
        }
    }
    res
}

fn code_of(res: &Result<ImportSummary, JobError>) -> Option<&str> {
    match res {
        Err(JobError::Retryable { code, .. } | JobError::Unrecoverable { code, .. }) => {
            code.as_deref()
        }
        Err(JobError::Release { reason, .. }) => Some(reason),
        Ok(_) => None,
    }
}

async fn cursor_of(pool: &PgPool, account_id: Uuid) -> Option<ImportCursor> {
    get_account(pool, account_id)
        .await
        .unwrap()
        .unwrap()
        .import_cursor
}

async fn game_ids(pool: &PgPool, account_id: Uuid) -> Vec<(Uuid, String)> {
    sqlx::query_as("SELECT id, platform_game_id FROM games WHERE account_id = $1")
        .bind(account_id)
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn live_keys(pool: &PgPool, kind: &str) -> Vec<String> {
    let mut keys: Vec<String> = sqlx::query_scalar(
        "SELECT dedupe_key FROM jobs WHERE kind = $1 AND state IN ('queued','running','paused')",
    )
    .bind(kind)
    .fetch_all(pool)
    .await
    .unwrap();
    keys.sort();
    keys
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

fn since(at: OffsetDateTime) -> FetchOptions {
    FetchOptions {
        since: Some(at),
        ..FetchOptions::default()
    }
}

// ---------------------------------------------------------------------------------------------
// Ported from import-resume.test.ts

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn import_resumes_from_cursor(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "lichess", "resumer").await;
    let list = four_games("Resumer");

    let res = run(&deps(&pool, importer(list.clone(), Some(2))), acct).await;
    assert!(matches!(res, Err(JobError::Retryable { .. })), "{res:?}");
    assert_eq!(code_of(&res), Some("unknown"));

    assert_eq!(
        cursor_of(&pool, acct).await,
        Some(ImportCursor {
            oldest_reached: Some(list[1].played_at),
            newest_seen_at: None,
            backfill: false,
        })
    );
    assert_eq!(game_ids(&pool, acct).await.len(), 2);
    let runs = list_runs(&pool, acct).await.unwrap();
    assert_eq!(runs.len(), 1);
    let failed = &runs[0];
    assert_eq!(failed.error_code.as_deref(), Some("unknown"));
    assert_eq!(failed.status, ImportRunStatus::Failed);
    assert_eq!(failed.imported, 2);
    assert_eq!(
        failed.last_error.as_deref(),
        Some("transport error: connection reset")
    );
    assert!(failed.finished_at.is_some());

    let src = importer(list.clone(), None);
    let res = run(&deps(&pool, src.clone()), acct).await.unwrap();
    assert_eq!(res.imported, 2);
    // First the unfinished range below the cursor, then new games above everything imported.
    let calls = src.calls();
    assert_eq!(
        calls[0],
        FetchOptions {
            until: Some(list[1].played_at),
            ..FetchOptions::default()
        }
    );
    assert_eq!(calls[1], since(list[0].played_at));
    assert_eq!(calls.len(), 2);

    let rows = game_ids(&pool, acct).await;
    assert_eq!(rows.len(), 4);
    let distinct: std::collections::HashSet<_> = rows.iter().map(|(_, g)| g.clone()).collect();
    assert_eq!(distinct.len(), 4);
    assert_eq!(
        cursor_of(&pool, acct).await,
        Some(ImportCursor {
            oldest_reached: None,
            newest_seen_at: Some(list[0].played_at),
            backfill: false,
        })
    );

    let mut statuses: Vec<_> = list_runs(&pool, acct)
        .await
        .unwrap()
        .iter()
        .map(|r| r.status.as_str())
        .collect();
    statuses.sort();
    assert_eq!(statuses, ["done", "failed"]);

    // A later sync only asks for games since the newest one seen.
    let later = importer(list.clone(), None);
    run(&deps(&pool, later.clone()), acct).await.unwrap();
    assert_eq!(later.calls(), [since(list[0].played_at)]);
}

/// The cursor and the run's imported count observed mid-stream.
type Seen = (Option<ImportCursor>, i32);

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn checkpoints_the_cursor_and_the_run_every_batch(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "lichess", "batcher").await;
    let list = four_games("Batcher");
    let seen: Arc<Mutex<Option<Seen>>> = Arc::new(Mutex::new(None));
    let src = {
        let (pool, seen, list) = (pool.clone(), seen.clone(), list.clone());
        FnSource::new(move |_| {
            let (pool, seen, third) = (pool.clone(), seen.clone(), list[2].clone());
            futures::stream::iter([Ok(list[0].clone()), Ok(list[1].clone())])
                .chain(futures::stream::once(async move {
                    // Before the 3rd game the first batch of 2 must already be durable.
                    let runs = list_runs(&pool, acct).await.unwrap();
                    let cursor = cursor_of(&pool, acct).await;
                    *seen.lock().unwrap() = Some((cursor, runs[0].imported));
                    Ok(third)
                }))
                .boxed()
        })
    };
    let mut d = deps(&pool, src);
    d.checkpoint_every = 2;
    run(&d, acct).await.unwrap();
    assert_eq!(
        *seen.lock().unwrap(),
        Some((
            Some(ImportCursor {
                oldest_reached: Some(list[1].played_at),
                newest_seen_at: None,
                backfill: false,
            }),
            2
        ))
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn re_import_enqueues_the_schedule_job_again(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "lichess", "requeue").await;
    let list: Vec<_> = four_games("Requeue").into_iter().take(2).collect();
    run(&deps(&pool, importer(list.clone(), None)), acct)
        .await
        .unwrap();
    // R2: no per-game analyse jobs, one schedule job for the account.
    assert!(live_analyse_keys(&pool).await.is_empty());
    assert_eq!(
        live_keys(&pool, KIND_SCHEDULE).await,
        vec![schedule_key(acct)]
    );
    // Simulate a crash after the commit, before the schedule job ran: drop it and re-run.
    sqlx::query("DELETE FROM jobs WHERE kind = $1")
        .bind(KIND_SCHEDULE)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE linked_accounts SET import_cursor = NULL WHERE id = $1")
        .bind(acct)
        .execute(&pool)
        .await
        .unwrap();
    let res = run(&deps(&pool, importer(list, None)), acct).await.unwrap();
    assert_eq!(res.imported, 0);
    assert!(live_analyse_keys(&pool).await.is_empty());
    assert_eq!(
        live_keys(&pool, KIND_SCHEDULE).await,
        vec![schedule_key(acct)]
    );
    let statuses: Vec<String> =
        sqlx::query_scalar("SELECT analysis_status::text FROM games WHERE account_id = $1")
            .bind(acct)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(statuses, vec!["imported", "imported"]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_failed_import_enqueues_no_schedule_job(pool: PgPool) {
    // The games it committed stay `imported`; the reconciler schedules them (see reconciler.rs).
    let acct = testkit::insert_account(&pool, "lichess", "failsched").await;
    let list: Vec<_> = four_games("Failsched").into_iter().take(2).collect();
    let source = importer(list, Some(1));
    assert!(run(&deps(&pool, source), acct).await.is_err());
    assert!(live_keys(&pool, KIND_SCHEDULE).await.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn calls_the_platform_through_the_account_s_circuit_breaker(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "lichess", "tripped").await;
    let breaker = Arc::new(CircuitBreaker::new(BreakerOptions {
        failure_threshold: 1,
        cooldown: Duration::from_secs(60),
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
    d.breakers = HashMap::from([
        (Platform::Lichess, breaker.clone()),
        (Platform::Chesscom, breaker),
    ]);
    let res = run(&d, acct).await;
    assert!(matches!(res, Err(JobError::Release { .. })), "{res:?}");
    assert_eq!(code_of(&res), Some("circuit_open"));
    assert!(client.calls().is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn re_fetches_the_last_30_days_on_lichess_so_late_finishing_correspondence_games_land(
    pool: PgPool,
) {
    let acct = testkit::insert_account(&pool, "lichess", "corres").await;
    let newer = make_game("Corres", datetime!(2026-03-20 00:00 UTC));
    // Created earlier, but only finished (and so only listed) after "newer" was stored.
    let mut correspondence = make_game("Corres", datetime!(2026-03-01 00:00 UTC));
    correspondence.time_class = chess_core::TimeClass::Daily;

    let defaults = |src: Arc<FnSource>| {
        let mut d = deps(&pool, src);
        d.lichess_lookback = ImportAccountDeps::new(pool.clone(), d.queue.clone()).lichess_lookback;
        d
    };
    run(&defaults(importer(vec![newer.clone()], None)), acct)
        .await
        .unwrap();

    let src = importer(vec![newer.clone(), correspondence], None);
    let res = run(&defaults(src.clone()), acct).await.unwrap();
    assert_eq!(src.calls(), [since(datetime!(2026-02-18 00:00 UTC))]);
    assert_eq!(res.imported, 1);
    assert_eq!(game_ids(&pool, acct).await.len(), 2);
    // The watermark itself still records the newest game, not the look-back.
    assert_eq!(
        cursor_of(&pool, acct).await.unwrap().newest_seen_at,
        Some(newer.played_at)
    );

    // The look-back is configurable.
    let custom = importer(vec![], None);
    let mut d = deps(&pool, custom.clone());
    d.lichess_lookback = Duration::from_secs(24 * 3600);
    run(&d, acct).await.unwrap();
    assert_eq!(custom.calls(), [since(datetime!(2026-03-19 00:00 UTC))]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn records_an_error_code_from_the_typed_platform_error(pool: PgPool) {
    let p = |e: PlatformError| import_error_code(&ImportFailure::Platform(e));
    let url = || "u".to_owned();
    let not_found = |u: &str| p(PlatformError::NotFound { url: u.to_owned() });
    // Only a user-level 404 (the chess.com archive list, the Lichess user stream) is not_found.
    assert_eq!(
        not_found("https://api.chess.com/pub/player/ghost/games/archives"),
        ImportErrorCode::NotFound
    );
    assert_eq!(
        not_found("https://lichess.org/api/games/user/ghost?sort=dateDesc"),
        ImportErrorCode::NotFound
    );
    assert_eq!(
        not_found("https://api.chess.com/pub/player/ghost/games/2026/01"),
        ImportErrorCode::Unknown
    );
    assert_eq!(
        p(PlatformError::RateLimited {
            url: url(),
            attempts: 3
        }),
        ImportErrorCode::RateLimited
    );
    assert_eq!(
        p(PlatformError::Timeout { url: url(), ms: 10 }),
        ImportErrorCode::Timeout
    );
    assert_eq!(
        import_error_code(&ImportFailure::Deadline(JobDeadlineError {
            kind: "import-account",
            ms: 10
        })),
        ImportErrorCode::Timeout
    );
    assert_eq!(
        p(PlatformError::CircuitOpen {
            retry_in: Duration::from_millis(10)
        }),
        ImportErrorCode::CircuitOpen
    );
    assert_eq!(
        import_error_code(&ImportFailure::Shutdown),
        ImportErrorCode::Shutdown
    );
    assert_eq!(
        p(PlatformError::Transport("?".into())),
        ImportErrorCode::Unknown
    );
    assert_eq!(
        p(PlatformError::Http {
            platform: "lichess".into(),
            status: 503,
            url: url()
        }),
        ImportErrorCode::Unknown
    );

    let acct = testkit::insert_account(&pool, "lichess", "limited").await;
    let src = FnSource::new(|_| {
        futures::stream::iter([Err(PlatformError::RateLimited {
            url: "https://lichess.org/api/games/user/limited".into(),
            attempts: 4,
        })])
        .boxed()
    });
    let res = run(&deps(&pool, src), acct).await;
    assert!(matches!(res, Err(JobError::Retryable { .. })), "{res:?}");
    assert_eq!(code_of(&res), Some("rate_limited"));
    let runs = list_runs(&pool, acct).await.unwrap();
    assert_eq!(runs[0].status, ImportRunStatus::Failed);
    assert_eq!(runs[0].error_code.as_deref(), Some("rate_limited"));
}

// ---------------------------------------------------------------------------------------------
// R1: half-open breaker re-delay (12b review item 2)

#[test]
fn breaker_redelay_floors_and_jitters() {
    let (min, probe) = (Duration::from_secs(5), Duration::from_secs(30));
    assert_eq!(
        breaker_redelay(Duration::from_secs(40), min, probe, 0.0),
        Duration::from_secs(40)
    );
    assert_eq!(
        breaker_redelay(Duration::from_secs(1), min, probe, 0.0),
        Duration::from_secs(5)
    );
    // retry_in 0 = half-open probe in flight: wait at least the probe timeout
    assert_eq!(
        breaker_redelay(Duration::ZERO, min, probe, 0.0),
        Duration::from_secs(30)
    );
    assert_eq!(
        breaker_redelay(Duration::from_secs(10), min, probe, 0.5),
        Duration::from_secs(11)
    ); // +20% * 0.5
}
