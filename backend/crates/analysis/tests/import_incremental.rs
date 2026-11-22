//! M5b Task S: the incremental mode of `import-account`, used by the recurring sync. It fetches
//! only games newer than the last import: no backfill below an open frontier, and on Lichess the
//! short look-back except on the first sync of each UTC day.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use analysis::jobs::import::{
    ImportAccountDeps, ImportSummary, enqueue_incremental_import, handle_import_account,
};
use db::accounts::ImportCursor;
use futures::StreamExt;
use futures::stream::BoxStream;
use platforms::{
    FetchOptions, GameSource, NormalizedGame, Platform, PlatformClient, PlatformError, Player,
};
use queue::queue::{Backoff, Queue};
use queue::{ImportAccountPayload, KIND_IMPORT, KIND_SCHEDULE};
use sqlx::PgPool;
use testkit::fake_platform::FakeClient;
use time::OffsetDateTime;
use time::macros::datetime;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const DAY: Duration = Duration::from_secs(24 * 3600);

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

fn four_games(name: &str) -> Vec<NormalizedGame> {
    [4u8, 3, 2, 1]
        .iter()
        .map(|d| {
            make_game(
                name,
                datetime!(2026-03-01 12:00 UTC).replace_day(*d).unwrap(),
            )
        })
        .collect()
}

/// Honours since/until (inclusive) and records every fetch.
struct ListSource {
    calls: Mutex<Vec<FetchOptions>>,
    list: Vec<NormalizedGame>,
}

impl ListSource {
    fn new(list: Vec<NormalizedGame>) -> Arc<Self> {
        Arc::new(ListSource {
            calls: Mutex::new(Vec::new()),
            list,
        })
    }

    fn calls(&self) -> Vec<FetchOptions> {
        self.calls.lock().unwrap().clone()
    }
}

impl GameSource for ListSource {
    fn fetch<'a>(
        &'a self,
        _client: Arc<dyn PlatformClient>,
        _username: &'a str,
        opts: FetchOptions,
    ) -> BoxStream<'a, Result<NormalizedGame, PlatformError>> {
        self.calls.lock().unwrap().push(opts.clone());
        let items: Vec<_> = self
            .list
            .iter()
            .filter(|g| {
                opts.since.is_none_or(|s| g.played_at >= s)
                    && opts.until.is_none_or(|u| g.played_at <= u)
            })
            .cloned()
            .map(Ok)
            .collect();
        futures::stream::iter(items).boxed()
    }
}

fn deps(pool: &PgPool, source: Arc<ListSource>) -> ImportAccountDeps {
    let mut deps = ImportAccountDeps::new(pool.clone(), Queue::new(pool.clone()));
    let source: Arc<dyn GameSource> = source;
    let client: Arc<dyn PlatformClient> = Arc::new(FakeClient::default());
    deps.sources = HashMap::from([(Platform::Lichess, source)]);
    deps.clients = HashMap::from([(Platform::Lichess, client)]);
    deps.lichess_lookback = 30 * DAY;
    deps.sync_lichess_lookback = DAY;
    deps
}

/// Enqueues the account's incremental import, claims it and settles it like the runner.
async fn run_incremental(deps: &ImportAccountDeps, account: Uuid) -> ImportSummary {
    assert!(
        enqueue_incremental_import(&deps.queue, account)
            .await
            .unwrap()
    );
    let job = deps
        .queue
        .dequeue(&[KIND_IMPORT], "test", 1, Duration::from_secs(60))
        .await
        .unwrap()
        .pop()
        .expect("an import job");
    let payload: ImportAccountPayload = serde_json::from_value(job.payload.clone()).unwrap();
    assert!(payload.incremental);
    let res = handle_import_account(deps, &job, payload, CancellationToken::new()).await;
    match &res {
        Ok(_) => assert!(deps.queue.complete(&job).await.unwrap()),
        Err(e) => {
            deps.queue.fail(&job, e, Backoff::default()).await.unwrap();
        }
    }
    res.unwrap()
}

async fn set_account(
    pool: &PgPool,
    account: Uuid,
    cursor: &ImportCursor,
    synced: Option<OffsetDateTime>,
) {
    sqlx::query("UPDATE linked_accounts SET import_cursor = $2, last_synced_at = $3 WHERE id = $1")
        .bind(account)
        .bind(sqlx::types::Json(cursor))
        .bind(synced)
        .execute(pool)
        .await
        .unwrap();
}

fn days(n: i64) -> time::Duration {
    time::Duration::days(n)
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn an_incremental_import_does_not_backfill_below_the_frontier(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "lichess", "frontier").await;
    let list = four_games("Frontier");
    // A run stopped at `max` left an open backfill below games[1].
    let cursor = ImportCursor {
        oldest_reached: Some(list[1].played_at),
        newest_seen_at: Some(list[0].played_at),
        backfill: true,
    };
    set_account(&pool, acct, &cursor, Some(OffsetDateTime::now_utc())).await;
    let src = ListSource::new(list.clone());

    run_incremental(&deps(&pool, src.clone()), acct).await;

    let calls = src.calls();
    assert_eq!(calls.len(), 1, "only the new-games pass: {calls:?}");
    assert_eq!(calls[0].until, None);
    assert_eq!(calls[0].since, Some(list[0].played_at - days(1)));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_sync_later_the_same_day_uses_the_short_lichess_lookback(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "lichess", "shorty").await;
    let newest = datetime!(2026-03-04 12:00 UTC);
    let cursor = ImportCursor {
        newest_seen_at: Some(newest),
        ..ImportCursor::default()
    };
    set_account(&pool, acct, &cursor, Some(OffsetDateTime::now_utc())).await;
    let src = ListSource::new(Vec::new());

    run_incremental(&deps(&pool, src.clone()), acct).await;

    assert_eq!(src.calls()[0].since, Some(newest - days(1)));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn the_first_sync_of_a_day_uses_the_full_lichess_lookback(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "lichess", "daily").await;
    let newest = datetime!(2026-03-04 12:00 UTC);
    let cursor = ImportCursor {
        newest_seen_at: Some(newest),
        ..ImportCursor::default()
    };
    // Last synced yesterday (UTC): late correspondence games get their monthly re-check today.
    set_account(
        &pool,
        acct,
        &cursor,
        Some(OffsetDateTime::now_utc() - days(1)),
    )
    .await;
    let src = ListSource::new(Vec::new());
    let d = deps(&pool, src.clone());

    run_incremental(&d, acct).await;
    assert_eq!(src.calls()[0].since, Some(newest - days(30)));

    // The run set last_synced_at to now, so the next sync today is short again.
    run_incremental(&d, acct).await;
    assert_eq!(src.calls()[1].since, Some(newest - days(1)));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_sync_over_stored_games_imports_none_and_keeps_their_analysis(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "lichess", "again").await;
    let list = four_games("Again");
    let src = ListSource::new(list.clone());
    let d = deps(&pool, src.clone());
    assert_eq!(run_incremental(&d, acct).await.imported, 4);
    sqlx::query("UPDATE games SET analysis_status = 'done' WHERE account_id = $1")
        .bind(acct)
        .execute(&pool)
        .await
        .unwrap();
    // The first run's schedule-analysis job is done.
    sqlx::query("UPDATE jobs SET state = 'done' WHERE kind = $1")
        .bind(KIND_SCHEDULE)
        .execute(&pool)
        .await
        .unwrap();

    let again = run_incremental(&d, acct).await;

    assert_eq!(again.imported, 0);
    let statuses: Vec<String> =
        sqlx::query_scalar("SELECT analysis_status::text FROM games WHERE account_id = $1")
            .bind(acct)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(statuses, ["done"; 4]);
    // The finished run still publishes its account-scoped `import.progress` (done), so the web
    // refreshes, and schedules analysis as usual (the scheduler finds nothing new).
    let done: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM user_events WHERE account_id = $1 AND type = 'import.progress' \
         AND (payload->>'done')::boolean",
    )
    .bind(acct)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(done, 2);
    let schedules: i64 =
        sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = $1 AND state = 'queued'")
            .bind(KIND_SCHEDULE)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(schedules, 1);
}
