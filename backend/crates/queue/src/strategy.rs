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

impl Queue {
    /// Runs a custom claim statement `sql` in one transaction under the queue's statement
    /// timeout. Binds, in order: `$1` the kinds (`text[]`), `$2` the limit (`int8`), then each of
    /// `params` (`int8`, from `$3`), then the owner (`text`) and the lease in milliseconds
    /// (`int8`). The statement must `RETURNING` [`JOB_COLUMNS`] of the claimed rows and keep the
    /// guarantees listed on [`SelectStrategy`]. Jobs come back in `(priority, run_at, id)`
    /// order, as from [`Queue::dequeue`].
    pub async fn claim_with(
        &self,
        sql: &'static str,
        kinds: &[&str],
        limit: i64,
        params: &[i64],
        owner: &str,
        lease: Duration,
    ) -> Result<Vec<Job>> {
        if kinds.is_empty() || limit <= 0 {
            return Ok(Vec::new());
        }
        let mut query = sqlx::query_as::<_, JobRow>(sql).bind(kinds).bind(limit);
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
        let mut jobs: Vec<Job> = rows.into_iter().map(JobRow::into_job).collect();
        jobs.sort_by_key(|j| (j.priority, j.run_at, j.id));
        Ok(jobs)
    }
}

/// The columns of a claimed job (table alias `j`) that [`Queue::claim_with`] expects back.
pub const JOB_COLUMNS: &str = crate::queue::JOB_COLUMNS;
