//! Fair round-robin dequeue (R2 Task 27, R-12, ADR-R2-15).
//!
//! Queued jobs are claimed in the order `(tier, slice, joined_at, priority, run_at, id)`:
//! - `tier = priority / 2` (see [`crate::priority::fair_tier`]): a higher tier always goes first;
//! - `slice = (served + rank - 1) / slice_len`, where `served` is the account's games whose
//!   analysis started within `served_window` (`games.analysis_started_at`, because finished jobs
//!   are pruned) and `rank` is the job's 1-based position among the account's ready jobs in
//!   `(priority, run_at, id)` order. Every account gets `slice_len` jobs per turn;
//! - `joined_at = linked_accounts.created_at`: earlier accounts first within a slice. Jobs
//!   without a `user_key` (reference games) form one pseudo-user, and so do jobs whose account
//!   is gone; both have `joined_at = infinity`.
//!
//! The statement stays index-backed at any backlog (plan-guarded in `tests/fair.rs`): no
//! `kind = ANY(...)` sort over every ready row. It works in two phases on
//! `jobs_user_ready_idx (kind, user_key, priority, run_at)`:
//! 1. **Heads.** A skip scan finds every account with queued jobs of each kind, and one index
//!    probe per account reads its first due job (its head; rank 1, so its key is exact). Only the
//!    best `4 × limit` accounts by head key go on: an account whose head is not among them has
//!    no job among the best `4 × limit` jobs either, because an account's own jobs never
//!    precede its head.
//! 2. **Expand.** For those accounts, the first `4 × limit` due jobs per kind are read (an index
//!    range in order; the global top `4 × limit` holds at most that many of one account), ranked
//!    per account, and ordered.
//!
//! Rows read from `jobs`: about `2 × accounts + kinds × (4 × limit)²`, whatever the backlog.
//! The index reads order by exactly the index columns `(user_key, priority, run_at)`: `user_key`
//! first (a constant there) so that the planner reads the `user_key` index in order instead of
//! filtering `jobs_ready_idx` (which for the reference pseudo-user would walk every ready job of
//! the kind), and no `id`, because an incremental sort on `id` would read a whole group of
//! jobs enqueued in one transaction (equal `run_at`, for example 2 000 reference games). Within
//! such a tie the candidates come in index (insertion) order; the ranking and the claim still
//! order by `id`.
//!
//! `FOR UPDATE` cannot share a query level with window functions, so the ranking runs first
//! and the lock is taken after ordering, on the `4 × limit` best candidates with
//! `SKIP LOCKED LIMIT limit` (the headroom lets up to four concurrent claimers fill their
//! batches). A row claimed by a concurrent transaction is re-checked by Postgres
//! (`state = 'queued'`) and skipped. The due rule is R1's (`run_at <= now() OR
//! run_at <= created_at`, ADR 0032), so clock steps and zero-delay re-queues behave as in
//! [`queue::queue::Queue::dequeue`].

use std::sync::LazyLock;
use std::time::Duration;

use futures::future::BoxFuture;
use queue::queue::{Queue, QueueError};
use queue::strategy::{JOB_COLUMNS, SelectStrategy};
use queue::types::Job;

/// Jobs per account per turn.
pub const DEFAULT_SLICE: i64 = 5;
/// How far back started analyses count as served.
pub const DEFAULT_SERVED_WINDOW: Duration = Duration::from_secs(3600);
/// Candidates per claim (and per account) as a multiple of the limit.
pub const CANDIDATE_FACTOR: i64 = 4;

/// The round robin of R-12 as a [`SelectStrategy`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FairRoundRobin {
    /// Jobs per account per turn (5).
    pub slice: i64,
    /// Started analyses within this window count as served (1 h).
    pub served_window: Duration,
}

impl Default for FairRoundRobin {
    fn default() -> Self {
        Self {
            slice: DEFAULT_SLICE,
            served_window: DEFAULT_SERVED_WINDOW,
        }
    }
}

/// The fair ordering without the lock, shared with the ETA (Task 30). A `WITH RECURSIVE` list
/// ending in the CTE `fair (id, user_key, tier, slice, joined_at, priority, run_at)`: the due
/// queued jobs of the kinds `$1::text[]` that can be among the best `$2 * 4`, ranked (the module
/// docs say which). Order by `tier, slice, joined_at, priority, run_at, id`; the first `$2 * 4`
/// rows of that order are exact. `$3` is the served window in milliseconds (`int8`), `$4` the
/// slice length (`int8`). Callers append `, more AS (...)` CTEs and a statement on `fair`.
pub fn fair_order_sql() -> &'static str {
    FAIR_ORDER_SQL
}

const FAIR_ORDER_SQL: &str = "\
WITH RECURSIVE
kinds AS (SELECT DISTINCT unnest($1::text[]) AS kind),
users AS (
  SELECT k.kind,
    (SELECT j.user_key FROM jobs j
     WHERE j.state = 'queued' AND j.kind = k.kind AND j.user_key IS NOT NULL
     ORDER BY j.user_key LIMIT 1) AS user_key
  FROM kinds k
  UNION ALL
  SELECT u.kind,
    (SELECT j.user_key FROM jobs j
     WHERE j.state = 'queued' AND j.kind = u.kind AND j.user_key > u.user_key
     ORDER BY j.user_key LIMIT 1)
  FROM users u WHERE u.user_key IS NOT NULL),
kind_heads AS (
  SELECT u.user_key, c.id, c.priority, c.run_at
  FROM users u CROSS JOIN LATERAL (
    SELECT j.id, j.priority, j.run_at FROM jobs j
    WHERE j.state = 'queued' AND j.kind = u.kind AND j.user_key = u.user_key
      AND (j.run_at <= now() OR j.run_at <= j.created_at)
    ORDER BY j.user_key, j.priority, j.run_at LIMIT 1) c
  WHERE u.user_key IS NOT NULL
  UNION ALL
  SELECT NULL::uuid, c.id, c.priority, c.run_at
  FROM kinds k CROSS JOIN LATERAL (
    SELECT j.id, j.priority, j.run_at FROM jobs j
    WHERE j.state = 'queued' AND j.kind = k.kind AND j.user_key IS NULL
      AND (j.run_at <= now() OR j.run_at <= j.created_at)
    ORDER BY j.user_key, j.priority, j.run_at LIMIT 1) c),
heads AS (
  SELECT DISTINCT ON (user_key) user_key, id, priority, run_at FROM kind_heads
  ORDER BY user_key, priority, run_at, id),
served AS (
  SELECT g.account_id AS user_key, count(*) AS n FROM games g
  WHERE g.analysis_started_at > now() - $3::float8 * interval '1 millisecond'
  GROUP BY g.account_id),
head_keys AS (
  SELECT h.user_key, coalesce(s.n, 0) AS served,
    coalesce((SELECT a.created_at FROM linked_accounts a WHERE a.id = h.user_key),
             'infinity'::timestamptz) AS joined_at,
    h.priority / 2 AS tier, coalesce(s.n, 0) / $4::int8 AS slice, h.priority, h.run_at, h.id
  FROM heads h LEFT JOIN served s ON s.user_key = h.user_key),
chosen AS (
  SELECT user_key, served, joined_at FROM head_keys
  ORDER BY tier, slice, joined_at, priority, run_at, id
  LIMIT $2 * 4),
ready_rows AS (
  SELECT c.id, ch.user_key, ch.served, ch.joined_at, c.priority, c.run_at
  FROM chosen ch CROSS JOIN kinds k CROSS JOIN LATERAL (
    SELECT j.id, j.priority, j.run_at FROM jobs j
    WHERE j.state = 'queued' AND j.kind = k.kind AND j.user_key = ch.user_key
      AND (j.run_at <= now() OR j.run_at <= j.created_at)
    ORDER BY j.user_key, j.priority, j.run_at LIMIT $2 * 4) c
  WHERE ch.user_key IS NOT NULL
  UNION ALL
  SELECT c.id, NULL::uuid, ch.served, ch.joined_at, c.priority, c.run_at
  FROM chosen ch CROSS JOIN kinds k CROSS JOIN LATERAL (
    SELECT j.id, j.priority, j.run_at FROM jobs j
    WHERE j.state = 'queued' AND j.kind = k.kind AND j.user_key IS NULL
      AND (j.run_at <= now() OR j.run_at <= j.created_at)
    ORDER BY j.user_key, j.priority, j.run_at LIMIT $2 * 4) c
  WHERE ch.user_key IS NULL),
fair AS (
  SELECT r.id, r.user_key, r.priority / 2 AS tier,
    (r.served + row_number() OVER (PARTITION BY r.user_key
                                   ORDER BY r.priority, r.run_at, r.id) - 1) / $4::int8 AS slice,
    r.joined_at, r.priority, r.run_at
  FROM ready_rows r)";

/// The claim: [`fair_order_sql`], the best `4 × $2` candidates, then the lock (`SKIP LOCKED`,
/// at most `$2` rows) and the R1 claim update. `$5` is the owner, `$6` the lease in ms (`int8`).
pub fn fair_claim_sql() -> &'static str {
    &FAIR_CLAIM_SQL
}

static FAIR_CLAIM_SQL: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{FAIR_ORDER_SQL},
ordered AS (
  SELECT * FROM fair
  ORDER BY tier, slice, joined_at, priority, run_at, id
  LIMIT $2 * 4),
pick AS (
  SELECT j.id FROM ordered o JOIN jobs j ON j.id = o.id
  WHERE j.state = 'queued'
  ORDER BY o.tier, o.slice, o.joined_at, o.priority, o.run_at, o.id
  LIMIT $2
  FOR UPDATE OF j SKIP LOCKED)
UPDATE jobs j SET state = 'running', attempts = j.attempts + 1, lease_owner = $5,
  lease_token = gen_random_uuid(), lease_until = now() + $6::float8 * interval '1 millisecond',
  heartbeat_at = now(), started_at = now()
FROM pick WHERE j.id = pick.id
RETURNING {JOB_COLUMNS}"
    )
});

impl SelectStrategy for FairRoundRobin {
    fn dequeue<'a>(
        &'a self,
        queue: &'a Queue,
        kinds: &'a [&'a str],
        owner: &'a str,
        limit: i64,
        lease: Duration,
    ) -> BoxFuture<'a, Result<Vec<Job>, QueueError>> {
        let window_ms = i64::try_from(self.served_window.as_millis()).unwrap_or(i64::MAX);
        let slice = self.slice.max(1);
        Box::pin(async move {
            queue
                .claim_with(
                    fair_claim_sql(),
                    kinds,
                    limit,
                    &[window_ms, slice],
                    owner,
                    lease,
                )
                .await
        })
    }

    fn order_by(&self) -> &'static str {
        "tier, slice, joined_at, priority, run_at, id"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_extends_the_shared_order_and_locks_after_ordering() {
        let sql = fair_claim_sql();
        assert!(sql.starts_with(fair_order_sql()));
        assert!(sql.contains("FOR UPDATE OF j SKIP LOCKED"));
        assert!(!fair_order_sql().contains("FOR UPDATE"));
        assert!(sql.contains("(j.run_at <= now() OR j.run_at <= j.created_at)"));
        assert!(sql.contains(&format!(
            "ORDER BY {}",
            FairRoundRobin::default().order_by()
        )));
        assert!(!sql.contains("ANY("), "no kind = ANY sort");
    }
}
