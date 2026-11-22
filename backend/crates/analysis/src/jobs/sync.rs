//! The recurring `sync-accounts` job (M5b Task S, ADR 0076): every linked account's newest games
//! are fetched about once per [`SyncOptions::interval`] (`SYNC_INTERVAL_MINUTES`, default 30), and
//! then analysed as usual (the import enqueues `schedule-analysis`, the analyses' profile hook
//! enqueues `build-profile`).
//!
//! The job recurs every [`SyncOptions::tick`] (a sixth of the interval). Each tick enqueues an
//! incremental `import-account` job ([`enqueue_incremental_import`]) for the accounts that are
//! *due*: linked by at least one user (an `account_links` row, or the CLI's `user_id`), with no
//! live import job, and whose newest import run started more than `interval - tick` ago (or never).
//! The longest-waiting go first, and a tick enqueues at most `ceil(linked * tick / interval)`, so
//! the accounts spread across the interval instead of hitting the platforms in a burst. The import
//! jobs keep the `import:<account>` dedupe key, so an account never has two live imports, and they
//! run through the import runner's shared clients (throttle, 429 retry-after, the fixed
//! User-Agent) and breakers.

use std::time::Duration;

use futures::FutureExt;
use futures::future::BoxFuture;
use queue::queue::{Queue, QueueError};
use queue::runner::{Handler, JobContext};
use queue::{EnqueueOptions, Job, JobError, KIND_IMPORT, KIND_SYNC_ACCOUNTS, SYNC_ACCOUNTS_KEY};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::jobs::import::enqueue_incremental_import;

/// The default sync interval (`SYNC_INTERVAL_MINUTES`).
pub const SYNC_INTERVAL: Duration = Duration::from_secs(30 * 60);
/// Ticks per interval.
pub const SYNC_SLICES: u32 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyncOptions {
    /// How often each account is synced.
    pub interval: Duration,
    /// How often the job runs (the recurrence of the `sync-accounts` job).
    pub tick: Duration,
}

impl SyncOptions {
    /// `interval`, ticking every `interval / SYNC_SLICES` (at least 2 s, the queue's floor).
    pub fn every(interval: Duration) -> Self {
        SyncOptions {
            interval,
            tick: (interval / SYNC_SLICES).max(Duration::from_secs(2)),
        }
    }

    pub fn with_tick(self, tick: Duration) -> Self {
        SyncOptions { tick, ..self }
    }

    /// The most accounts one tick enqueues: `ceil(linked * tick / interval)`.
    fn cap(&self, linked: i64) -> i64 {
        let interval = self.interval.as_millis().max(1);
        let tick = self.tick.as_millis().min(interval);
        let linked = u128::try_from(linked.max(0)).unwrap_or(0);
        i64::try_from((linked * tick).div_ceil(interval)).unwrap_or(i64::MAX)
    }
}

impl Default for SyncOptions {
    fn default() -> Self {
        SyncOptions::every(SYNC_INTERVAL)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct SyncSummary {
    /// Accounts with at least one linked user.
    pub linked: u32,
    /// Of those, the accounts due for a sync.
    pub due: u32,
    /// Import jobs this tick enqueued.
    pub enqueued: u32,
}

#[derive(thiserror::Error, Debug)]
pub enum SyncError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Queue(#[from] QueueError),
}

/// The linked accounts and the due ones, the longest-waiting first. `$1` is `interval - tick` in
/// milliseconds.
const DUE_SQL: &str = "
WITH linked AS (
  SELECT la.id,
    (SELECT max(r.started_at) FROM import_runs r WHERE r.account_id = la.id) AS last_run
  FROM linked_accounts la
  WHERE la.user_id IS NOT NULL
     OR EXISTS (SELECT 1 FROM account_links l WHERE l.linked_account_id = la.id)
)
SELECT l.id,
  (l.last_run IS NULL OR l.last_run <= now() - $1::float8 * interval '1 millisecond')
  AND NOT EXISTS (
    SELECT 1 FROM jobs j WHERE j.kind = $2 AND j.dedupe_key = 'import:' || l.id::text
      AND j.state IN ('queued', 'running', 'paused')
  ) AS due
FROM linked l
ORDER BY l.last_run ASC NULLS FIRST, l.id";

/// One tick: enqueues the due accounts' incremental imports (see the module docs).
pub async fn sync_accounts(
    pool: &PgPool,
    queue: &Queue,
    opts: &SyncOptions,
) -> Result<SyncSummary, SyncError> {
    let threshold = opts.interval.saturating_sub(opts.tick);
    let rows: Vec<(Uuid, bool)> = sqlx::query_as(DUE_SQL)
        .bind(threshold.as_secs_f64() * 1000.0)
        .bind(KIND_IMPORT)
        .fetch_all(pool)
        .await?;
    let linked = i64::try_from(rows.len()).unwrap_or(i64::MAX);
    let due: Vec<Uuid> = rows
        .into_iter()
        .filter(|(_, d)| *d)
        .map(|(id, _)| id)
        .collect();
    let cap = usize::try_from(opts.cap(linked)).unwrap_or(usize::MAX);
    let mut enqueued = 0u32;
    for account in due.iter().take(cap) {
        if enqueue_incremental_import(queue, *account).await? {
            enqueued += 1;
        }
    }
    let summary = SyncSummary {
        linked: u32::try_from(linked).unwrap_or(u32::MAX),
        due: u32::try_from(due.len()).unwrap_or(u32::MAX),
        enqueued,
    };
    tracing::info!(
        linked = summary.linked,
        due = summary.due,
        enqueued = summary.enqueued,
        "sync-accounts tick"
    );
    Ok(summary)
}

/// Schedules the recurring sync job (every [`SyncOptions::tick`]). Idempotent: the live dedupe
/// key keeps one job, and a live job whose recurrence differs (a new `SYNC_INTERVAL_MINUTES`)
/// is moved to the new one.
pub async fn ensure_sync_scheduled(queue: &Queue, opts: &SyncOptions) -> Result<(), QueueError> {
    queue
        .enqueue(
            KIND_SYNC_ACCOUNTS,
            &json!({}),
            EnqueueOptions {
                dedupe_key: Some(SYNC_ACCOUNTS_KEY.to_owned()),
                priority: 0,
                max_attempts: 1,
                recur_every: Some(opts.tick),
                ..EnqueueOptions::default()
            },
        )
        .await?;
    sqlx::query(
        "UPDATE jobs SET recur_every = $3::float8 * interval '1 millisecond' \
         WHERE kind = $1 AND dedupe_key = $2 AND state IN ('queued', 'running', 'paused') \
         AND recur_every IS DISTINCT FROM $3::float8 * interval '1 millisecond'",
    )
    .bind(KIND_SYNC_ACCOUNTS)
    .bind(SYNC_ACCOUNTS_KEY)
    .bind(opts.tick.as_secs_f64() * 1000.0)
    .execute(queue.pool())
    .await
    .map_err(QueueError::from)?;
    Ok(())
}

/// The queue handler for [`KIND_SYNC_ACCOUNTS`]: one [`sync_accounts`] tick.
pub struct SyncAccountsHandler {
    pub pool: PgPool,
    pub queue: Queue,
    pub opts: SyncOptions,
}

impl SyncAccountsHandler {
    /// One tick; a failure is retryable (the job recurs either way).
    pub async fn run(&self, _job: Job) -> Result<Value, JobError> {
        match sync_accounts(&self.pool, &self.queue, &self.opts).await {
            Ok(summary) => Ok(json!(summary)),
            Err(e) => Err(JobError::Retryable {
                message: e.to_string(),
                code: None,
            }),
        }
    }
}

impl Handler for SyncAccountsHandler {
    fn kind(&self) -> &'static str {
        KIND_SYNC_ACCOUNTS
    }

    fn handle(&self, job: Job, _ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        self.run(job).boxed()
    }
}
