//! M5b Task S: the recurring `sync-accounts` job enqueues incremental imports of linked accounts,
//! deduped per account and spread across the interval.

use std::time::Duration;

use analysis::jobs::import::enqueue_import;
use analysis::jobs::sync::{
    SyncAccountsHandler, SyncOptions, ensure_sync_scheduled, sync_accounts,
};
use queue::queue::Queue;
use queue::runner::Handler;
use queue::{ImportAccountPayload, KIND_IMPORT, KIND_SYNC_ACCOUNTS, SYNC_ACCOUNTS_KEY};
use sqlx::PgPool;
use uuid::Uuid;

const HALF_HOUR: Duration = Duration::from_secs(30 * 60);

/// An account that a web user linked (an unverified `account_links` row is enough).
async fn linked_account(pool: &PgPool, username: &str) -> Uuid {
    let account = testkit::seed::insert_account(pool, "lichess", username).await;
    let user = testkit::seed::insert_user(pool, username).await;
    sqlx::query(
        "INSERT INTO account_links (user_id, linked_account_id, method) VALUES ($1, $2, 'username')",
    )
    .bind(user)
    .bind(account)
    .execute(pool)
    .await
    .unwrap();
    account
}

/// The live import jobs, as `(account, payload)`.
async fn live_imports(pool: &PgPool) -> Vec<(Uuid, ImportAccountPayload)> {
    let rows: Vec<(serde_json::Value,)> = sqlx::query_as(
        "SELECT payload FROM jobs WHERE kind = $1 AND state IN ('queued','running','paused') \
         ORDER BY id",
    )
    .bind(KIND_IMPORT)
    .fetch_all(pool)
    .await
    .unwrap();
    rows.into_iter()
        .map(|(p,)| {
            let p: ImportAccountPayload = serde_json::from_value(p).unwrap();
            (p.account_id, p)
        })
        .collect()
}

async fn import_run_started(pool: &PgPool, account: Uuid, minutes_ago: i32) {
    sqlx::query(
        "INSERT INTO import_runs (account_id, status, started_at, finished_at) \
         VALUES ($1, 'done', now() - make_interval(mins => $2), now() - make_interval(mins => $2))",
    )
    .bind(account)
    .bind(minutes_ago)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn enqueues_an_incremental_import_for_each_account_with_a_linked_user(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let a = linked_account(&pool, "alice").await;
    let b = linked_account(&pool, "bob").await;
    let _orphan = testkit::seed::insert_account(&pool, "lichess", "nobody").await;

    let summary = sync_accounts(
        &pool,
        &queue,
        &SyncOptions::every(HALF_HOUR).with_tick(HALF_HOUR),
    )
    .await
    .unwrap();

    assert_eq!(summary.linked, 2);
    assert_eq!(summary.enqueued, 2);
    let mut jobs = live_imports(&pool).await;
    jobs.sort_by_key(|(id, _)| *id);
    let mut want = vec![a, b];
    want.sort();
    assert_eq!(jobs.iter().map(|(id, _)| *id).collect::<Vec<_>>(), want);
    for (_, p) in &jobs {
        assert!(p.incremental, "a sync import is incremental: {p:?}");
        assert_eq!(p.max, None);
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn keeps_at_most_one_live_import_per_account(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let a = linked_account(&pool, "alice").await;
    let b = linked_account(&pool, "bob").await;
    // A manual import of `a` is already queued.
    assert!(enqueue_import(&queue, a, None).await.unwrap());
    let opts = SyncOptions::every(HALF_HOUR).with_tick(HALF_HOUR);

    let first = sync_accounts(&pool, &queue, &opts).await.unwrap();
    let second = sync_accounts(&pool, &queue, &opts).await.unwrap();

    assert_eq!(first.enqueued, 1, "only b: a already has a live import");
    assert_eq!(second.enqueued, 0);
    let jobs = live_imports(&pool).await;
    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs.iter().filter(|(id, _)| *id == a).count(), 1);
    assert_eq!(jobs.iter().filter(|(id, _)| *id == b).count(), 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn skips_accounts_imported_within_the_interval(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let fresh = linked_account(&pool, "fresh").await;
    let stale = linked_account(&pool, "stale").await;
    import_run_started(&pool, fresh, 5).await;
    import_run_started(&pool, stale, 40).await;

    // Due after `interval - tick` (25 minutes), so a tick's lateness does not add up.
    let summary = sync_accounts(&pool, &queue, &SyncOptions::every(HALF_HOUR))
        .await
        .unwrap();

    assert_eq!(summary.due, 1);
    let jobs = live_imports(&pool).await;
    assert_eq!(jobs.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [stale]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn spreads_the_accounts_across_the_interval(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let mut accounts = Vec::new();
    for i in 0..12 {
        accounts.push(linked_account(&pool, &format!("player{i}")).await);
    }
    // The longest-unsynced accounts go first: two never imported, the rest by age.
    for (i, a) in accounts.iter().enumerate().skip(2) {
        import_run_started(&pool, *a, 60 + i32::try_from(i).unwrap()).await;
    }
    // A tick of 5 minutes in a 30-minute interval: 12 accounts / 6 ticks = 2 per tick.
    let opts = SyncOptions::every(HALF_HOUR).with_tick(Duration::from_secs(5 * 60));

    let summary = sync_accounts(&pool, &queue, &opts).await.unwrap();

    assert_eq!(summary.linked, 12);
    assert_eq!(summary.due, 12);
    assert_eq!(summary.enqueued, 2);
    let mut got: Vec<Uuid> = live_imports(&pool)
        .await
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    got.sort();
    let mut want = vec![accounts[0], accounts[1]];
    want.sort();
    assert_eq!(got, want);

    // The next tick takes the next two oldest (the queued ones are skipped).
    let next = sync_accounts(&pool, &queue, &opts).await.unwrap();
    assert_eq!(next.enqueued, 2);
    let got: Vec<Uuid> = live_imports(&pool)
        .await
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert!(
        got.contains(&accounts[11]) && got.contains(&accounts[10]),
        "{got:?}"
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn the_tick_is_a_sixth_of_the_interval(_pool: PgPool) {
    let opts = SyncOptions::every(HALF_HOUR);
    assert_eq!(opts.interval, HALF_HOUR);
    assert_eq!(opts.tick, Duration::from_secs(5 * 60));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn schedules_one_recurring_sync_job_and_follows_the_interval(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    ensure_sync_scheduled(&queue, &SyncOptions::every(HALF_HOUR))
        .await
        .unwrap();
    ensure_sync_scheduled(&queue, &SyncOptions::every(HALF_HOUR))
        .await
        .unwrap();
    let every = |pool: PgPool| async move {
        sqlx::query_scalar::<_, f64>(
            "SELECT extract(epoch FROM recur_every)::float8 FROM jobs \
             WHERE kind = $1 AND dedupe_key = $2 AND state IN ('queued','running','paused')",
        )
        .bind(KIND_SYNC_ACCOUNTS)
        .bind(SYNC_ACCOUNTS_KEY)
        .fetch_all(&pool)
        .await
        .unwrap()
    };
    assert_eq!(every(pool.clone()).await, [300.0]);

    // A new SYNC_INTERVAL_MINUTES takes effect on the live job at the next start.
    ensure_sync_scheduled(&queue, &SyncOptions::every(Duration::from_secs(60 * 60)))
        .await
        .unwrap();
    assert_eq!(every(pool).await, [600.0]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn the_handler_runs_one_sync(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let a = linked_account(&pool, "alice").await;
    let handler = SyncAccountsHandler {
        pool: pool.clone(),
        queue: queue.clone(),
        opts: SyncOptions::every(HALF_HOUR),
    };
    assert_eq!(Handler::kind(&handler), KIND_SYNC_ACCOUNTS);
    ensure_sync_scheduled(&queue, &handler.opts).await.unwrap();
    let job = queue
        .dequeue(&[KIND_SYNC_ACCOUNTS], "test", 1, Duration::from_secs(60))
        .await
        .unwrap()
        .pop()
        .expect("the sync job is due at once");
    let out = handler.run(job).await.unwrap();
    assert_eq!(out["enqueued"], 1);
    assert_eq!(live_imports(&pool).await[0].0, a);
}
