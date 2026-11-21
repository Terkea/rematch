//! The Postgres job queue (ADR 0032): enqueue with live-only dedupe keys, claim with
//! `FOR UPDATE SKIP LOCKED`, lease-token fencing, exponential backoff and attempt-free release.
//!
//! State machine (`job_state`):
//!
//! ```text
//! enqueue ─▶ queued ──dequeue (attempts + 1)──▶ running ──complete──▶ done
//!              ▲                                 │  │
//!              ├──── fail Retryable (backoff) ───┘  ├─ fail Retryable, final attempt ─▶ dead
//!              ├──── fail Release (attempts − 1) ───┤
//!              └──── recurring: complete or fail ───┘─ fail Unrecoverable ─▶ failed
//! ```
//!
//! Every write to a running job is fenced on `(id, lease_token, state = 'running')`, so a worker
//! that lost its lease (the reaper re-queued the job and another worker claimed it) cannot
//! renew, complete or fail it. Every call runs in its own transaction with
//! `SET LOCAL statement_timeout` (default 30 s), plus a client-side backstop, so a call that
//! times out is rolled back and never takes effect.

use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;

use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::types::{EnqueueOptions, Job, JobError, JobState, UnknownJobState};

/// The NOTIFY channel. Its payload is the job's kind; runners wake on it instead of polling.
pub const JOBS_CHANNEL: &str = "jobs";

/// Server-side bound on every queue statement (matches the worker's `statement_timeout`).
pub const DEFAULT_QUERY_TIMEOUT: Duration = Duration::from_secs(30);

/// Extra client-side time on top of the server-side statement timeout, for pool acquire and a
/// connection that stopped answering.
pub const CLIENT_BACKSTOP: Duration = Duration::from_secs(5);

/// `last_error` is stored with at most this many characters.
pub const MAX_ERROR_CHARS: usize = 2000;

pub type Result<T, E = QueueError> = std::result::Result<T, E>;

#[derive(thiserror::Error, Debug)]
pub enum QueueError {
    #[error("queue database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("queue {op} timed out after {after:?}")]
    Timeout { op: &'static str, after: Duration },
    #[error(transparent)]
    UnknownState(#[from] UnknownJobState),
}

/// Exponential retry backoff: `delay(attempts) = base * 2^(attempts - 1)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Backoff {
    pub base: Duration,
}

impl Default for Backoff {
    /// BullMQ's `{ type: "exponential", delay: 5000 }` from the TS `DEFAULT_JOB_OPTIONS`.
    fn default() -> Self {
        Self {
            base: Duration::from_secs(5),
        }
    }
}

impl Backoff {
    /// The delay after failed attempt number `attempts` (1-based; values below 1 give `base`).
    /// The exponent is capped, so huge attempt counts saturate instead of overflowing.
    pub fn delay(&self, attempts: i32) -> Duration {
        let exp = attempts.saturating_sub(1).clamp(0, 30) as u32;
        self.base.saturating_mul(1u32 << exp)
    }
}

/// What `Queue::fail` did with the job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FailOutcome {
    /// Queued again at `run_at`, with the attempt used (retry backoff, or a recurring job's
    /// next run).
    Retried { run_at: OffsetDateTime },
    /// Queued again at `run_at` without using an attempt (`JobError::Release`).
    Released { run_at: OffsetDateTime },
    /// A retryable failure on the final attempt: dead-lettered.
    Dead,
    /// An unrecoverable failure: failed at once, whatever attempts remain.
    Failed,
    /// The caller no longer holds the lease; nothing was written.
    LeaseLost,
}

/// How the next jobs are chosen. The R2 fair-scheduling hook (§8c/§8d round robin per account):
/// a per-`user_key` strategy would walk the `jobs_user_ready_idx (kind, user_key, priority,
/// run_at)` index. R1 has exactly one implementation, [`PriorityFifo`], and uses it for nothing
/// else.
pub trait SelectStrategy: Send + Sync {
    /// The `ORDER BY` of the claim query (served by `jobs_ready_idx`).
    fn order_by(&self) -> &'static str;
}

/// Lower `priority` first, then the earliest `run_at`, then insertion order.
#[derive(Clone, Copy, Debug, Default)]
pub struct PriorityFifo;

impl SelectStrategy for PriorityFifo {
    fn order_by(&self) -> &'static str {
        "priority, run_at, id"
    }
}

/// The columns that make up a [`Job`] (table alias `j`), for `RETURNING` and `SELECT` lists.
macro_rules! job_columns {
    () => {
        "j.id, j.kind, j.payload, j.dedupe_key, j.user_key, j.priority, j.attempts,          j.max_attempts, j.lease_token, j.run_at, j.created_at, j.started_at"
    };
}
/// For the reaper and other crate-internal `RETURNING` lists (Task 14).
#[allow(dead_code)]
pub(crate) const JOB_COLUMNS: &str = job_columns!();

/// The claim order; must equal `PriorityFifo.order_by()` (unit-tested).
macro_rules! claim_order {
    () => {
        "priority, run_at, id"
    };
}

/// Every write to a running job is fenced on its lease token.
macro_rules! fence {
    () => {
        "id = $1 AND lease_token = $2 AND state = 'running'"
    };
}

/// The failure branches that end the job (`failed` or `dead`); see `FAIL_SQL`.
macro_rules! terminal {
    () => {
        "($3 <> 'release' AND recur_every IS NULL           AND ($3 = 'unrecoverable' OR attempts >= max_attempts))"
    };
}

#[derive(sqlx::FromRow)]
pub(crate) struct JobRow {
    id: i64,
    kind: String,
    payload: Value,
    dedupe_key: Option<String>,
    user_key: Option<Uuid>,
    priority: i16,
    attempts: i32,
    max_attempts: i32,
    lease_token: Option<Uuid>,
    run_at: OffsetDateTime,
    created_at: OffsetDateTime,
    started_at: Option<OffsetDateTime>,
}

impl JobRow {
    /// A row that is not running has no lease: its token is nil and `started_at` falls back to
    /// `created_at`.
    pub(crate) fn into_job(self) -> Job {
        Job {
            id: self.id,
            kind: self.kind,
            payload: self.payload,
            dedupe_key: self.dedupe_key,
            user_key: self.user_key,
            priority: self.priority,
            attempts: self.attempts,
            max_attempts: self.max_attempts,
            lease_token: self.lease_token.unwrap_or(Uuid::nil()),
            run_at: self.run_at,
            started_at: self.started_at.unwrap_or(self.created_at),
            created_at: self.created_at,
        }
    }
}

const ENQUEUE_SQL: &str = "\
WITH ins AS (
  INSERT INTO jobs (kind, payload, dedupe_key, user_key, priority, run_at, max_attempts, recur_every)
  VALUES ($1, $2, $3, $4, $5, coalesce($6, now()), $7, $8::float8 * interval '1 millisecond')
  ON CONFLICT (kind, dedupe_key) WHERE dedupe_key IS NOT NULL AND state IN ('queued','running','paused')
  DO NOTHING RETURNING id)
SELECT id, pg_notify('jobs', $1) IS NULL AS not_notified FROM ins";

/// One statement; the attempt is counted at claim. A job enqueued to run at once has
/// `run_at = created_at` (one transaction's `now()`), so `run_at <= created_at` keeps it due even
/// after the wall clock steps back (ADR 0032); a delayed or re-queued job waits for `now()`.
const DEQUEUE_SQL: &str = concat!(
    "WITH next AS (
  SELECT id FROM jobs
  WHERE state = 'queued' AND kind = ANY($1) AND (run_at <= now() OR run_at <= created_at)
  ORDER BY ",
    claim_order!(),
    "
  FOR UPDATE SKIP LOCKED LIMIT $2)
UPDATE jobs j SET state = 'running', attempts = j.attempts + 1, lease_owner = $3,
  lease_token = gen_random_uuid(), lease_until = now() + $4::float8 * interval '1 millisecond',
  heartbeat_at = now(), started_at = now()
FROM next WHERE j.id = next.id
RETURNING ",
    job_columns!()
);

const RENEW_SQL: &str = concat!(
    "UPDATE jobs SET lease_until = now() + $3::float8 * interval '1 millisecond',      heartbeat_at = now() WHERE ",
    fence!()
);

/// `done`, or for a recurring job back to `queued` at `now() + recur_every` with a fresh budget.
const COMPLETE_SQL: &str = concat!(
    "UPDATE jobs SET
  state = (CASE WHEN recur_every IS NULL THEN 'done' ELSE 'queued' END)::job_state,
  finished_at = CASE WHEN recur_every IS NULL THEN now() END,
  run_at = coalesce(now() + recur_every, run_at),
  attempts = CASE WHEN recur_every IS NULL THEN attempts ELSE 0 END,
  stalls = CASE WHEN recur_every IS NULL THEN stalls ELSE 0 END,
  lease_owner = NULL, lease_token = NULL, lease_until = NULL
WHERE ",
    fence!()
);

/// `$3` is the mode (`retry`, `unrecoverable` or `release`) and `$4` the delay in milliseconds
/// (the backoff for `retry`, the release delay for `release`). A release never uses an attempt,
/// even on the final one; a recurring job is rescheduled whatever the error. A re-queue that is
/// ready at once (a zero-delay release) wakes the runners.
const FAIL_SQL: &str = concat!(
    "WITH upd AS (
  UPDATE jobs SET
    state = (CASE
      WHEN $3 = 'release' OR recur_every IS NOT NULL THEN 'queued'
      WHEN $3 = 'unrecoverable' THEN 'failed'
      WHEN attempts >= max_attempts THEN 'dead'
      ELSE 'queued' END)::job_state,
    attempts = CASE
      WHEN $3 = 'release' THEN greatest(attempts - 1, 0)
      WHEN recur_every IS NOT NULL THEN 0
      ELSE attempts END,
    run_at = CASE
      WHEN $3 = 'release' THEN now() + $4::float8 * interval '1 millisecond'
      WHEN recur_every IS NOT NULL THEN now() + recur_every
      WHEN ",
    terminal!(),
    " THEN run_at
      ELSE now() + $4::float8 * interval '1 millisecond' END,
    finished_at = CASE WHEN ",
    terminal!(),
    " THEN now() END,
    last_error = $5, error_code = $6,
    lease_owner = NULL, lease_token = NULL, lease_until = NULL
  WHERE ",
    fence!(),
    "
  RETURNING kind, state, run_at)
SELECT state::text AS state, run_at,
  (CASE WHEN state = 'queued' AND run_at <= now() THEN pg_notify('jobs', kind) END) IS NULL
    AS not_notified
FROM upd"
);

const LIVE_STATES_SQL: &str = "\
SELECT dedupe_key, state::text FROM jobs
WHERE kind = $1 AND dedupe_key = ANY($2) AND state IN ('queued', 'running', 'paused')";

const LIVE_COUNT_SQL: &str = "\
SELECT count(*) FROM jobs
WHERE kind = ANY($1) AND state IN ('queued', 'running', 'paused')
  AND ($2::uuid IS NULL OR user_key = $2)";

/// SQLSTATE 57014 `query_canceled`, which `statement_timeout` raises.
fn is_statement_timeout(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|code| code == "57014")
}

fn millis(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn truncate_chars(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((at, _)) => s[..at].to_owned(),
        None => s.to_owned(),
    }
}

/// A cheap, cloneable handle on the `jobs` table.
#[derive(Clone, Debug)]
pub struct Queue {
    pub(crate) pool: PgPool,
    pub(crate) query_timeout: Duration,
}

impl Queue {
    pub fn new(pool: PgPool) -> Queue {
        Queue {
            pool,
            query_timeout: DEFAULT_QUERY_TIMEOUT,
        }
    }

    /// Overrides the per-call timeout (default [`DEFAULT_QUERY_TIMEOUT`]).
    pub fn with_query_timeout(mut self, timeout: Duration) -> Queue {
        self.query_timeout = timeout;
        self
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Opens a transaction whose statements the server cancels after the query timeout
    /// (`SET LOCAL statement_timeout`), independent of the pool's own setting. A queue write is
    /// committed only by `tx.commit()`, so a call that times out never takes effect.
    pub(crate) async fn begin(&self) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT set_config('statement_timeout', $1, true)")
            .bind(format!("{}ms", self.query_timeout.as_millis().max(1)))
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }

    /// Bounds `fut` (a whole `begin` … `commit` unit) on the client as well: pool acquire and a
    /// dead connection are covered by a backstop of the query timeout plus
    /// [`CLIENT_BACKSTOP`]. A server-side statement timeout is reported as a timeout too.
    pub(crate) async fn guard<T, F>(&self, op: &'static str, fut: F) -> Result<T>
    where
        F: Future<Output = Result<T, sqlx::Error>>,
    {
        let timeout = QueueError::Timeout {
            op,
            after: self.query_timeout,
        };
        match tokio::time::timeout(self.query_timeout + CLIENT_BACKSTOP, fut).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(e)) if is_statement_timeout(&e) => Err(timeout),
            Ok(Err(e)) => Err(QueueError::Db(e)),
            Err(_) => Err(timeout),
        }
    }

    /// Runs a fenced single-row write; `true` when it hit the row.
    async fn write(
        &self,
        op: &'static str,
        query: sqlx::query::Query<'_, Postgres, sqlx::postgres::PgArguments>,
    ) -> Result<bool> {
        self.guard(op, async {
            let mut tx = self.begin().await?;
            let done = query.execute(&mut *tx).await?;
            tx.commit().await?;
            Ok(done.rows_affected() == 1)
        })
        .await
    }

    /// Inserts a job and notifies [`JOBS_CHANNEL`] with its kind. Returns `None`, and notifies
    /// nothing, when a live job (queued, running or paused) of the same kind already holds the
    /// dedupe key. Finished jobs never block a new one.
    pub async fn enqueue(
        &self,
        kind: &str,
        payload: &Value,
        opts: EnqueueOptions,
    ) -> Result<Option<i64>, QueueError> {
        let fut = sqlx::query_scalar::<_, i64>(ENQUEUE_SQL)
            .bind(kind)
            .bind(payload)
            .bind(opts.dedupe_key)
            .bind(opts.user_key)
            .bind(opts.priority)
            .bind(opts.run_at)
            .bind(opts.max_attempts)
            .bind(opts.recur_every.map(millis));
        self.guard("enqueue", async {
            let mut tx = self.begin().await?;
            let id = fut.fetch_optional(&mut *tx).await?;
            tx.commit().await?;
            Ok(id)
        })
        .await
    }

    /// Claims up to `limit` ready jobs of `kinds` for `owner`, in [`PriorityFifo`] order, each
    /// with a fresh lease token valid for `lease`. Concurrent callers never get the same job.
    pub async fn dequeue(
        &self,
        kinds: &[&str],
        owner: &str,
        limit: i64,
        lease: Duration,
    ) -> Result<Vec<Job>> {
        if kinds.is_empty() || limit <= 0 {
            return Ok(Vec::new());
        }
        let fut = sqlx::query_as::<_, JobRow>(DEQUEUE_SQL)
            .bind(kinds)
            .bind(limit)
            .bind(owner)
            .bind(millis(lease));
        let rows = self
            .guard("dequeue", async {
                let mut tx = self.begin().await?;
                let rows = fut.fetch_all(&mut *tx).await?;
                tx.commit().await?;
                Ok(rows)
            })
            .await?;
        let mut jobs: Vec<Job> = rows.into_iter().map(JobRow::into_job).collect();
        // UPDATE … RETURNING has no defined order.
        jobs.sort_by_key(|j| (j.priority, j.run_at, j.id));
        Ok(jobs)
    }

    /// Extends the lease to `now() + lease`. `false` means the lease is lost: stop working.
    pub async fn renew(&self, job: &Job, lease: Duration) -> Result<bool> {
        let fut = sqlx::query(RENEW_SQL)
            .bind(job.id)
            .bind(job.lease_token)
            .bind(millis(lease));
        self.write("renew", fut).await
    }

    /// Marks the job done (a recurring job is rescheduled). `false` means the lease was lost and
    /// nothing was written.
    pub async fn complete(&self, job: &Job) -> Result<bool> {
        let fut = sqlx::query(COMPLETE_SQL).bind(job.id).bind(job.lease_token);
        self.write("complete", fut).await
    }

    /// Records a failed attempt:
    /// - `Retryable`: `dead` once `attempts >= max_attempts`, otherwise queued at
    ///   `now() + backoff.delay(attempts)`;
    /// - `Unrecoverable` (for example `not_found`): `failed` at once;
    /// - `Release`: queued at `now() + delay` with the attempt given back, even on the final
    ///   attempt (shutdown, a closed engine pool, an open circuit breaker).
    ///
    /// A recurring job goes back to queued at `now() + recur_every` whatever the error (a
    /// release still uses its own delay). Every branch stores `last_error` (at most 2000
    /// characters) and `error_code` (a release stores its reason) and clears the lease.
    pub async fn fail(&self, job: &Job, err: &JobError, backoff: Backoff) -> Result<FailOutcome> {
        let (mode, delay, code) = match err {
            JobError::Retryable { code, .. } => {
                ("retry", backoff.delay(job.attempts), code.clone())
            }
            JobError::Unrecoverable { code, .. } => ("unrecoverable", Duration::ZERO, code.clone()),
            JobError::Release { delay, reason } => ("release", *delay, Some(reason.clone())),
        };
        let message = truncate_chars(&err.to_string(), MAX_ERROR_CHARS);
        let fut = sqlx::query_as::<_, (String, OffsetDateTime, bool)>(FAIL_SQL)
            .bind(job.id)
            .bind(job.lease_token)
            .bind(mode)
            .bind(millis(delay))
            .bind(message)
            .bind(code);
        let row = self
            .guard("fail", async {
                let mut tx = self.begin().await?;
                let row = fut.fetch_optional(&mut *tx).await?;
                tx.commit().await?;
                Ok(row)
            })
            .await?;
        let Some((state, run_at, _)) = row else {
            return Ok(FailOutcome::LeaseLost);
        };
        Ok(match (state.parse::<JobState>()?, err) {
            (JobState::Dead, _) => FailOutcome::Dead,
            (JobState::Failed, _) => FailOutcome::Failed,
            (_, JobError::Release { .. }) => FailOutcome::Released { run_at },
            _ => FailOutcome::Retried { run_at },
        })
    }

    /// The state of every *live* job (queued, running, paused) of `kind` among `dedupe_keys`, in
    /// one query on `jobs_dedupe_live_uq`. Keys with no live job are absent.
    pub async fn live_states(
        &self,
        kind: &str,
        dedupe_keys: &[String],
    ) -> Result<HashMap<String, JobState>> {
        if dedupe_keys.is_empty() {
            return Ok(HashMap::new());
        }
        let fut = sqlx::query_as::<_, (String, String)>(LIVE_STATES_SQL)
            .bind(kind)
            .bind(dedupe_keys);
        let rows = self
            .guard("live_states", async {
                let mut tx = self.begin().await?;
                let rows = fut.fetch_all(&mut *tx).await?;
                tx.commit().await?;
                Ok(rows)
            })
            .await?;
        rows.into_iter()
            .map(|(key, state)| Ok((key, state.parse::<JobState>()?)))
            .collect()
    }

    /// The number of live jobs of `kinds`, optionally only those of one account (`user_key`).
    pub async fn live_count(&self, kinds: &[&str], user_key: Option<Uuid>) -> Result<i64> {
        if kinds.is_empty() {
            return Ok(0);
        }
        let fut = sqlx::query_scalar::<_, i64>(LIVE_COUNT_SQL)
            .bind(kinds)
            .bind(user_key);
        self.guard("live_count", async {
            let mut tx = self.begin().await?;
            let n = fut.fetch_one(&mut *tx).await?;
            tx.commit().await?;
            Ok(n)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_query_uses_the_priority_fifo_order() {
        let order = format!(
            "ORDER BY {}
",
            PriorityFifo.order_by()
        );
        assert!(DEQUEUE_SQL.contains(&order), "{DEQUEUE_SQL}");
        assert!(DEQUEUE_SQL.contains("FOR UPDATE SKIP LOCKED"));
    }

    #[test]
    fn truncates_on_char_boundaries() {
        assert_eq!(truncate_chars("héllo", 2), "hé");
        assert_eq!(truncate_chars("hi", 2000), "hi");
    }
}
