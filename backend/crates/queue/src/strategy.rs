//! How a claim chooses its jobs (R2 Task 27). [`PriorityFifo`] is R1's claim
//! ([`Queue::dequeue`]) and the runner's default; the fair round robin per account lives in the
//! scheduler crate (`scheduler::fair::FairRoundRobin`) and claims through
//! [`Queue::claim_with`].

use std::time::Duration;

use futures::future::BoxFuture;

use crate::queue::{JobRow, Queue, QueueError, Result, millis};
use crate::types::Job;

/// Chooses and claims the next jobs. Every implementation must keep R1's claim guarantees:
/// one statement, `FOR UPDATE SKIP LOCKED` (concurrent callers never get the same job), the
/// attempt counted at claim, a fresh lease token, and the due rule
/// `run_at <= now() OR run_at <= created_at` (ADR 0032).
pub trait SelectStrategy: Send + Sync {
    /// Claims up to `limit` ready jobs of `kinds` for `owner`, each leased for `lease`. An empty
    /// result is not "no work" (ADR 0032).
    fn dequeue<'a>(
        &'a self,
        queue: &'a Queue,
        kinds: &'a [&'a str],
        owner: &'a str,
        limit: i64,
        lease: Duration,
    ) -> BoxFuture<'a, Result<Vec<Job>, QueueError>>;

    /// The claim order, for logs and `Debug` (`priority, run_at, id` for [`PriorityFifo`]).
    fn order_by(&self) -> &'static str;
}

/// Lower `priority` first, then the earliest `run_at`, then insertion order: R1's claim
/// ([`Queue::dequeue`]), index-ordered on `jobs_ready_idx` for one kind.
#[derive(Clone, Copy, Debug, Default)]
pub struct PriorityFifo;

impl SelectStrategy for PriorityFifo {
    fn dequeue<'a>(
        &'a self,
        queue: &'a Queue,
        kinds: &'a [&'a str],
        owner: &'a str,
        limit: i64,
        lease: Duration,
    ) -> BoxFuture<'a, Result<Vec<Job>, QueueError>> {
        Box::pin(queue.dequeue(kinds, owner, limit, lease))
    }

    fn order_by(&self) -> &'static str {
        "priority, run_at, id"
    }
}

/// What a custom claim ([`Queue::claim_with`]) did.
#[derive(Clone, Debug, PartialEq)]
pub struct Claimed {
    /// The claimed jobs, in `(priority, run_at, id)` order.
    pub jobs: Vec<Job>,
    /// How many candidates the statement considered before locking. More candidates than
    /// claimed jobs (with fewer jobs than asked for) means concurrent claimers held or took the
    /// rest: work may remain.
    pub candidates: i64,
}

/// One result row of a custom claim: the candidate count, plus a claimed job or all NULLs.
#[derive(sqlx::FromRow)]
struct ClaimRow {
    candidates: i64,
    id: Option<i64>,
    kind: Option<String>,
    payload: Option<serde_json::Value>,
    dedupe_key: Option<String>,
    user_key: Option<uuid::Uuid>,
    priority: Option<i16>,
    attempts: Option<i32>,
    max_attempts: Option<i32>,
    lease_token: Option<uuid::Uuid>,
    run_at: Option<time::OffsetDateTime>,
    created_at: Option<time::OffsetDateTime>,
    started_at: Option<time::OffsetDateTime>,
}

impl ClaimRow {
    fn into_job_row(self) -> Option<JobRow> {
        Some(JobRow {
            id: self.id?,
            kind: self.kind?,
            payload: self.payload?,
            dedupe_key: self.dedupe_key,
            user_key: self.user_key,
            priority: self.priority?,
            attempts: self.attempts?,
            max_attempts: self.max_attempts?,
            lease_token: self.lease_token,
            run_at: self.run_at?,
            created_at: self.created_at?,
            started_at: self.started_at,
        })
    }
}

impl Queue {
    /// Runs a custom claim statement `sql` in one transaction under the queue's statement
    /// timeout and client backstop. Binds, in order: `$1` the kinds (`text[]`), then each of
    /// `params` (`int8`, from `$2`), then the owner (`text`) and the lease in milliseconds
    /// (`int8`).
    ///
    /// The statement must keep the guarantees listed on [`SelectStrategy`] and return at least
    /// one row: each row has an `int8` column `candidates` and the [`JOB_COLUMNS`] of one
    /// claimed job, or NULL job columns when it claimed nothing (for example
    /// `SELECT (SELECT count(*) FROM ordered) AS candidates, j.* FROM (SELECT 1) one
    /// LEFT JOIN upd j ON true`).
    pub async fn claim_with(
        &self,
        sql: &'static str,
        kinds: &[&str],
        params: &[i64],
        owner: &str,
        lease: Duration,
    ) -> Result<Claimed> {
        let mut query = sqlx::query_as::<_, ClaimRow>(sql).bind(kinds);
        for p in params {
            query = query.bind(*p);
        }
        let fut = query.bind(owner).bind(millis(lease).round() as i64);
        let rows = self
            .guard("dequeue", async {
                let mut tx = self.begin().await?;
                let rows = fut.fetch_all(&mut *tx).await?;
                tx.commit().await?;
                Ok(rows)
            })
            .await?;
        let candidates = rows.first().map_or(0, |r| r.candidates);
        let mut jobs: Vec<Job> = rows
            .into_iter()
            .filter_map(ClaimRow::into_job_row)
            .map(JobRow::into_job)
            .collect();
        jobs.sort_by_key(|j| (j.priority, j.run_at, j.id));
        Ok(Claimed { jobs, candidates })
    }
}

/// The columns of a claimed job (table alias `j`) that [`Queue::claim_with`] expects back.
pub const JOB_COLUMNS: &str = crate::queue::JOB_COLUMNS;
