//! Port of apps/worker/test/recovery/import-max-backfill.test.ts (TS fix 004307b): a run that
//! stops at `max` keeps its frontier, so a later, larger `max` backfills below it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use analysis::jobs::import::{
    ImportAccountDeps, ImportSummary, enqueue_import, handle_import_account,
};
use db::accounts::{ImportCursor, get_account};
use futures::StreamExt;
use futures::stream::BoxStream;
use platforms::{
    FetchOptions, GameSource, NormalizedGame, Platform, PlatformClient, PlatformError, Player,
};
use queue::queue::{Backoff, Queue};
use queue::{ImportAccountPayload, KIND_IMPORT};
use sqlx::PgPool;
use testkit::fake_platform::FakeClient;
use time::OffsetDateTime;
use time::macros::datetime;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

fn make_game(white: &str, played_at: OffsetDateTime) -> NormalizedGame {
    NormalizedGame {
        platform: Platform::Chesscom,
        platform_game_id: Uuid::new_v4().simple().to_string(),
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

/// `count` games an hour apart, newest first.
fn history(name: &str, count: i64) -> Vec<NormalizedGame> {
    (0..count)
        .map(|i| {
            make_game(
                name,
                datetime!(2026-03-01 00:00 UTC) - time::Duration::hours(i),
            )
        })
        .collect()
}

/// Honours since/until (inclusive) and max, like the real importers; records every fetch.
struct FakeSource {
    list: Vec<NormalizedGame>,
    calls: Mutex<Vec<FetchOptions>>,
}

impl GameSource for FakeSource {
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
            .filter(|g| opts.since.is_none_or(|s| g.played_at >= s))
            .filter(|g| opts.until.is_none_or(|u| g.played_at <= u))
            .take(opts.max.map_or(usize::MAX, |m| m as usize))
            .cloned()
            .map(Ok)
            .collect();
        futures::stream::iter(items).boxed()
    }
}

fn deps(pool: &PgPool, source: Arc<FakeSource>) -> ImportAccountDeps {
    let mut deps = ImportAccountDeps::new(pool.clone(), Queue::new(pool.clone()));
    let source: Arc<dyn GameSource> = source;
    let client: Arc<dyn PlatformClient> = Arc::new(FakeClient::default());
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

/// One `import --max <max>` of the account, settled through the queue like the runner would.
async fn run(deps: &ImportAccountDeps, account_id: Uuid, max: u32) -> ImportSummary {
    enqueue_import(&deps.queue, account_id, Some(max))
        .await
        .unwrap();
    sqlx::query(
        "UPDATE jobs SET run_at = now() - interval '1 minute' WHERE kind = $1 AND state = 'queued'",
    )
    .bind(KIND_IMPORT)
    .execute(deps.queue.pool())
    .await
    .unwrap();
    let job = deps
        .queue
        .dequeue(&[KIND_IMPORT], "test", 1, Duration::from_secs(60))
        .await
        .unwrap()
        .pop()
        .expect("an import job to claim");
    let payload: ImportAccountPayload = serde_json::from_value(job.payload.clone()).unwrap();
    assert_eq!(payload.max, Some(max));
    let res = handle_import_account(deps, &job, payload, CancellationToken::new()).await;
    match &res {
        Ok(_) => assert!(deps.queue.complete(&job).await.unwrap()),
        Err(e) => {
            deps.queue.fail(&job, e, Backoff::default()).await.unwrap();
        }
    }
    res.unwrap()
}

async fn cursor_of(pool: &PgPool, account_id: Uuid) -> Option<ImportCursor> {
    get_account(pool, account_id)
        .await
        .unwrap()
        .unwrap()
        .import_cursor
}

async fn stored(pool: &PgPool, account_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM games WHERE account_id = $1")
        .bind(account_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_later_larger_max_continues_below_the_games_a_smaller_run_imported(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "chesscom", "maxbackfill").await;
    let list = history("maxbackfill", 10);
    let source = Arc::new(FakeSource {
        list: list.clone(),
        calls: Mutex::new(Vec::new()),
    });
    let deps = deps(&pool, source.clone());

    assert_eq!(run(&deps, acct, 3).await.imported, 3);
    // Stopped at max: the frontier stays open as a backfill, above it everything is imported.
    assert_eq!(
        cursor_of(&pool, acct).await,
        Some(ImportCursor {
            oldest_reached: Some(list[2].played_at),
            newest_seen_at: Some(list[0].played_at),
            backfill: true,
        })
    );

    assert_eq!(run(&deps, acct, 8).await.imported, 5); // 8 in total
    assert_eq!(stored(&pool, acct).await, 8);
    assert_eq!(run(&deps, acct, 50).await.imported, 2); // history ends
    assert_eq!(stored(&pool, acct).await, 10);

    // History ended: the frontier is released and the watermark is the newest game.
    assert_eq!(
        cursor_of(&pool, acct).await,
        Some(ImportCursor {
            oldest_reached: None,
            newest_seen_at: Some(list[0].played_at),
            backfill: false,
        })
    );

    // The second run first looked for new games above the watermark, then fetched only the
    // shortfall (plus the inclusive boundary game) below the frontier, with no lower bound.
    let calls = source.calls.lock().unwrap().clone();
    assert_eq!(calls[1], fetch(Some(list[0].played_at), None, 8));
    assert_eq!(calls[2], fetch(None, Some(list[2].played_at), 6));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn the_backfill_flag_round_trips_and_is_omitted_when_false(pool: PgPool) {
    let at = datetime!(2026-03-01 00:00 UTC);
    let open = ImportCursor {
        oldest_reached: Some(at),
        newest_seen_at: Some(at),
        backfill: true,
    };
    let json = serde_json::to_value(&open).unwrap();
    assert_eq!(json["backfill"], serde_json::json!(true));
    assert_eq!(serde_json::from_value::<ImportCursor>(json).unwrap(), open);

    let closed = ImportCursor {
        oldest_reached: None,
        newest_seen_at: Some(at),
        backfill: false,
    };
    let json = serde_json::to_value(&closed).unwrap();
    assert!(json.get("backfill").is_none(), "{json}");
    // A TS cursor without the key reads as no backfill.
    let ts = serde_json::json!({"oldestReached": null, "newestSeenAt": "2026-03-01T00:00:00.000Z"});
    assert_eq!(serde_json::from_value::<ImportCursor>(ts).unwrap(), closed);
    drop(pool);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_max_stop_above_an_old_watermark_keeps_the_gap_for_the_next_run(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "chesscom", "maxgap").await;
    let list = history("maxgap", 10);
    let old = Arc::new(FakeSource {
        list: list[6..].to_vec(),
        calls: Mutex::new(Vec::new()),
    });
    assert_eq!(run(&deps(&pool, old), acct, 50).await.imported, 4);

    // Six new games; a run capped at 2 stops above the old watermark.
    let source = Arc::new(FakeSource {
        list: list.clone(),
        calls: Mutex::new(Vec::new()),
    });
    let deps = deps(&pool, source.clone());
    assert_eq!(run(&deps, acct, 2).await.imported, 2);
    assert_eq!(
        cursor_of(&pool, acct).await,
        Some(ImportCursor {
            oldest_reached: Some(list[1].played_at),
            newest_seen_at: Some(list[6].played_at),
            backfill: false,
        })
    );

    // The next run closes the gap between the frontier and the old watermark, then looks for
    // newer games.
    assert_eq!(run(&deps, acct, 50).await.imported, 4);
    assert_eq!(stored(&pool, acct).await, 10);
    let calls = source.calls.lock().unwrap().clone();
    assert_eq!(
        calls[1],
        FetchOptions {
            since: Some(list[6].played_at),
            until: Some(list[1].played_at),
            max: Some(50),
        }
    );
    assert_eq!(
        cursor_of(&pool, acct).await,
        Some(ImportCursor {
            oldest_reached: None,
            newest_seen_at: Some(list[0].played_at),
            backfill: false,
        })
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_backfill_already_at_max_only_looks_for_new_games(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "chesscom", "maxsame").await;
    let list = history("maxsame", 10);
    let source = source(&list);
    let deps = deps(&pool, source.clone());
    assert_eq!(run(&deps, acct, 3).await.imported, 3);
    let before = cursor_of(&pool, acct).await;
    assert_eq!(run(&deps, acct, 3).await.imported, 0);
    let calls = source.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1], fetch(Some(list[0].played_at), None, 3));
    assert_eq!(cursor_of(&pool, acct).await, before);
}

/// `count` games newer than `history`'s, an hour apart, newest first.
fn newer(name: &str, count: i64) -> Vec<NormalizedGame> {
    (1..=count)
        .rev()
        .map(|i| {
            make_game(
                name,
                datetime!(2026-03-01 00:00 UTC) + time::Duration::hours(i),
            )
        })
        .collect()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_pending_backfill_never_starves_newly_played_games(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "chesscom", "maxnew").await;
    let old = history("maxnew", 10);
    assert_eq!(run(&deps(&pool, source(&old)), acct, 3).await.imported, 3);

    // Two new games: they come in first, and the budget (5 in total) is then used up.
    let new = newer("maxnew", 2);
    let all: Vec<_> = new.iter().chain(&old).cloned().collect();
    assert_eq!(run(&deps(&pool, source(&all)), acct, 5).await.imported, 2);
    assert_eq!(
        cursor_of(&pool, acct).await,
        Some(ImportCursor {
            oldest_reached: Some(old[2].played_at),
            newest_seen_at: Some(new[0].played_at),
            backfill: true,
        })
    );

    // A larger max backfills below the frontier.
    assert_eq!(run(&deps(&pool, source(&all)), acct, 8).await.imported, 3);
    assert_eq!(stored(&pool, acct).await, 8);
    assert_eq!(
        cursor_of(&pool, acct).await,
        Some(ImportCursor {
            oldest_reached: Some(old[5].played_at),
            newest_seen_at: Some(new[0].played_at),
            backfill: true,
        })
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn new_games_that_use_up_the_budget_leave_the_backfill_frontier_intact(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "chesscom", "maxburst").await;
    let old = history("maxburst", 10);
    assert_eq!(run(&deps(&pool, source(&old)), acct, 3).await.imported, 3);
    let opened = cursor_of(&pool, acct).await;

    // Four new games and a budget of 3: the three newest come in, nothing below the frontier.
    let new = newer("maxburst", 4);
    let all: Vec<_> = new.iter().chain(&old).cloned().collect();
    let src = source(&all);
    assert_eq!(run(&deps(&pool, src.clone()), acct, 3).await.imported, 3);
    assert_eq!(
        src.calls.lock().unwrap().clone(),
        [fetch(Some(old[0].played_at), None, 3)]
    );
    // The frontier and the watermark are as they were: the next run fetches from the watermark.
    assert_eq!(cursor_of(&pool, acct).await, opened);

    // The next run fills the rest of the new games, then resumes the backfill to the end.
    assert_eq!(run(&deps(&pool, source(&all)), acct, 50).await.imported, 8);
    assert_eq!(stored(&pool, acct).await, 14);
    assert_eq!(
        cursor_of(&pool, acct).await,
        Some(ImportCursor {
            oldest_reached: None,
            newest_seen_at: Some(new[0].played_at),
            backfill: false,
        })
    );
}

fn source(list: &[NormalizedGame]) -> Arc<FakeSource> {
    Arc::new(FakeSource {
        list: list.to_vec(),
        calls: Mutex::new(Vec::new()),
    })
}

fn fetch(since: Option<OffsetDateTime>, until: Option<OffsetDateTime>, max: u32) -> FetchOptions {
    FetchOptions {
        since,
        until,
        max: Some(max),
    }
}
