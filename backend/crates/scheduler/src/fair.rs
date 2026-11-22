//! Fair round-robin dequeue (R2 Task 27, R-12, ADR-R2-15).
//!
//! Queued jobs are claimed in the order `(tier, slice, joined_at, priority, run_at, id)`:
//! - `tier = priority / 2` (see [`crate::priority::fair_tier`]): a higher tier always goes first;
//! - `slice = (served + rank - 1) / slice_len`, where `served` is the account's games whose
//!   analysis started within `served_window` (`games.analysis_started_at`, because finished jobs
//!   are pruned) and `rank` is the job's 1-based position among the account's ready jobs in
//!   `(priority, run_at, id)` order. Every account gets `slice_len` jobs per turn;
//! - `joined_at = linked_accounts.created_at`: earlier accounts first within a slice. Jobs
//!   without a `user_key` form one pseudo-user with `joined_at = infinity`. A `user_key` whose
//!   account row is gone keeps its own round robin (it is still its own user), also with
//!   `joined_at = infinity`.
//!
//! Only kinds whose handler sets `games.analysis_started_at` when it starts ([`ServedKind`])
//! may be claimed fairly: for any other kind `served` would stay 0 and the slices would never
//! advance across claims.
//!
//! The statement stays index-backed at any backlog (plan-guarded in `tests/fair.rs`): no
//! `kind = ANY(...)` sort over every ready row. It works in two phases on
//! `jobs_user_ready_idx (kind, user_key, priority, run_at, id)` (migration 020100):
//! 1. **Heads.** A skip scan finds every account with queued jobs of each kind, and one index
//!    probe per account reads its first due job (its head; rank 1, so its key is exact). Only the
//!    best `4 × limit` accounts by head key go on: an account whose head is not among them has
//!    no job among the best `4 × limit` jobs either, because an account's own jobs never
//!    precede its head.
//! 2. **Expand.** For those accounts, the first `4 × limit` due jobs per kind are read (an index
//!    range in order; the global top `4 × limit` holds at most that many of one account), ranked
//!    per account, and ordered.
//!
//! Rows read from `jobs`: about `2 × accounts + kinds × candidates²` (`candidates = 4 × limit`),
//! whatever the backlog. The index reads order by exactly the index columns
//! `(user_key, priority, run_at, id)`: `user_key` first (a constant there) so that the planner
//! reads the `user_key` index in order instead of filtering `jobs_ready_idx` (which for the
//! reference pseudo-user would walk every ready job of the kind). With `id` in the index, ties
//! on `(priority, run_at)` (jobs enqueued in one transaction) are read in claim order without a
//! sort.
//!
//! `FOR UPDATE` cannot share a query level with window functions, so the ranking runs first
//! and the lock is taken after ordering, on the `4 × limit` best candidates with
//! `SKIP LOCKED LIMIT limit`. A row claimed by a concurrent transaction is re-checked by
//! Postgres (`state = 'queued'`) and skipped. When concurrent claimers held or took enough
//! candidates that a claim comes back short although it had more candidates, it is re-run once
//! with the next, wider candidate window ([`retry_candidates`]) and a fresh snapshot, so a
//! claimer does not come back empty while due work remains. The due rule is R1's (`run_at <= now() OR
//! run_at <= created_at`, ADR 0032), so clock steps and zero-delay re-queues behave as in
//! [`queue::queue::Queue::dequeue`].

use std::sync::LazyLock;
use std::time::Duration;

use futures::future::BoxFuture;
use queue::queue::{Queue, QueueError};
use queue::strategy::{Claimed, JOB_COLUMNS, SelectStrategy};
use queue::types::Job;

/// Jobs per account per turn.
pub const DEFAULT_SLICE: i64 = 5;
/// How far back started analyses count as served.
pub const DEFAULT_SERVED_WINDOW: Duration = Duration::from_secs(3600);
/// Candidates per claim (and per account) as a multiple of the limit.
pub const CANDIDATE_FACTOR: i64 = 4;
/// Extra candidates on the retry, for locks held by other claimers whatever their limit.
pub const RETRY_HEADROOM: i64 = 32;

/// The candidate window of the first attempt.
pub fn first_candidates(limit: i64) -> i64 {
    CANDIDATE_FACTOR * limit
}

/// The next, wider candidate window, used once when the first attempt came back short.
pub fn retry_candidates(limit: i64) -> i64 {
    2 * CANDIDATE_FACTOR * limit + RETRY_HEADROOM
}

/// A job kind whose handler sets `games.analysis_started_at` when it starts, which is what
/// `served` counts. This is the allow-list of [`FairRoundRobin`]: add a variant only for a kind
/// whose handler does that.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ServedKind {
    /// `analyse-game` (`analysis::jobs::analyse` sets `analysis_started_at = now()`).
    AnalyseGame,
}

impl ServedKind {
    pub const ALL: &'static [ServedKind] = &[ServedKind::AnalyseGame];

    pub fn as_str(self) -> &'static str {
        match self {
            ServedKind::AnalyseGame => queue::KIND_ANALYSE,
        }
    }

    /// The allow-listed kind named `kind`, if any.
    pub fn from_kind(kind: &str) -> Option<ServedKind> {
        Self::ALL.iter().copied().find(|k| k.as_str() == kind)
    }
}

/// A kind that does not record `analysis_started_at` and so cannot be claimed fairly.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
#[error("fair dequeue: kind {0:?} does not set games.analysis_started_at")]
pub struct NotAServedKind(pub String);

/// The round robin of R-12 as a [`SelectStrategy`], for [`ServedKind`]s only.
///
/// `served` counts `games.analysis_started_at`, so the strategy is only fair for kinds whose
/// handler sets that column when it starts. [`FairRoundRobin::for_kinds`] refuses any other
/// kind, and `dequeue` returns `QueueError::InvalidOptions` (claiming nothing) when asked for a
/// kind outside [`FairRoundRobin::kinds`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FairRoundRobin {
    /// Jobs per account per turn (5).
    pub slice: i64,
    /// Started analyses within this window count as served (1 h).
    pub served_window: Duration,
    kinds: Vec<ServedKind>,
}

impl Default for FairRoundRobin {
    /// Slices of 5 over one hour, for every [`ServedKind`] (`analyse-game`).
    fn default() -> Self {
        Self::new(ServedKind::ALL)
    }
}

impl FairRoundRobin {
    /// Slices of 5 over one hour, for exactly `kinds`.
    pub fn new(kinds: &[ServedKind]) -> Self {
        Self {
            slice: DEFAULT_SLICE,
            served_window: DEFAULT_SERVED_WINDOW,
            kinds: kinds.to_vec(),
        }
    }

    /// [`FairRoundRobin::new`] from kind names. Fails on the first name that is not a
    /// [`ServedKind`].
    pub fn for_kinds(kinds: &[&str]) -> Result<Self, NotAServedKind> {
        let kinds = kinds
            .iter()
            .map(|k| ServedKind::from_kind(k).ok_or_else(|| NotAServedKind((*k).to_owned())))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self::new(&kinds))
    }

    /// The kinds this strategy may claim.
    pub fn kinds(&self) -> &[ServedKind] {
        &self.kinds
    }

    /// `Err` with the first of `kinds` this strategy may not claim.
    pub fn check(&self, kinds: &[&str]) -> Result<(), NotAServedKind> {
        match kinds
            .iter()
            .find(|k| !self.kinds.iter().any(|s| s.as_str() == **k))
        {
            Some(k) => Err(NotAServedKind((*k).to_owned())),
            None => Ok(()),
        }
    }

    /// One claim attempt over the best `candidates` candidates.
    async fn attempt(
        &self,
        queue: &Queue,
        kinds: &[&str],
        owner: &str,
        limit: i64,
        candidates: i64,
        lease: Duration,
    ) -> Result<Claimed, QueueError> {
        let window_ms = i64::try_from(self.served_window.as_millis()).unwrap_or(i64::MAX);
        let slice = self.slice.max(1);
        queue
            .claim_with(
                fair_claim_sql(),
                kinds,
                &[candidates, window_ms, slice, limit],
                owner,
                lease,
            )
            .await
    }
}

/// The fair ordering without the lock, shared with the ETA (Task 30). A `WITH RECURSIVE` list
/// ending in the CTE `fair (id, user_key, tier, slice, joined_at, priority, run_at)`: the due
/// queued jobs of the kinds `$1::text[]` that can be among the best `$2` (the candidate count),
/// ranked (the module docs say which). Order by `tier, slice, joined_at, priority, run_at, id`;
/// the first `$2` rows of that order are exact. `$3` is the served window in milliseconds
/// (`int8`), `$4` the slice length (`int8`). Callers append `, more AS (...)` CTEs and a
/// statement on `fair`.
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
    ORDER BY j.user_key, j.priority, j.run_at, j.id LIMIT 1) c
  WHERE u.user_key IS NOT NULL
  UNION ALL
  SELECT NULL::uuid, c.id, c.priority, c.run_at
  FROM kinds k CROSS JOIN LATERAL (
    SELECT j.id, j.priority, j.run_at FROM jobs j
    WHERE j.state = 'queued' AND j.kind = k.kind AND j.user_key IS NULL
      AND (j.run_at <= now() OR j.run_at <= j.created_at)
    ORDER BY j.user_key, j.priority, j.run_at, j.id LIMIT 1) c),
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
  LIMIT $2),
ready_rows AS (
  SELECT c.id, ch.user_key, ch.served, ch.joined_at, c.priority, c.run_at
  FROM chosen ch CROSS JOIN kinds k CROSS JOIN LATERAL (
    SELECT j.id, j.priority, j.run_at FROM jobs j
    WHERE j.state = 'queued' AND j.kind = k.kind AND j.user_key = ch.user_key
      AND (j.run_at <= now() OR j.run_at <= j.created_at)
    ORDER BY j.user_key, j.priority, j.run_at, j.id LIMIT $2) c
  WHERE ch.user_key IS NOT NULL
  UNION ALL
  SELECT c.id, NULL::uuid, ch.served, ch.joined_at, c.priority, c.run_at
  FROM chosen ch CROSS JOIN kinds k CROSS JOIN LATERAL (
    SELECT j.id, j.priority, j.run_at FROM jobs j
    WHERE j.state = 'queued' AND j.kind = k.kind AND j.user_key IS NULL
      AND (j.run_at <= now() OR j.run_at <= j.created_at)
    ORDER BY j.user_key, j.priority, j.run_at, j.id LIMIT $2) c
  WHERE ch.user_key IS NULL),
fair AS (
  SELECT r.id, r.user_key, r.priority / 2 AS tier,
    (r.served + row_number() OVER (PARTITION BY r.user_key
                                   ORDER BY r.priority, r.run_at, r.id) - 1) / $4::int8 AS slice,
    r.joined_at, r.priority, r.run_at
  FROM ready_rows r)";

/// The claim: [`fair_order_sql`], the best `$2` candidates, then the lock (`SKIP LOCKED`, at
/// most `$5` rows, the limit) and the R1 claim update. `$6` is the owner, `$7` the lease in ms
/// (`int8`). It returns the shape [`Queue::claim_with`] expects: the candidate count, and one
/// row per claimed job or one row of NULL job columns.
pub fn fair_claim_sql() -> &'static str {
    &FAIR_CLAIM_SQL
}

static FAIR_CLAIM_SQL: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{FAIR_ORDER_SQL},
ordered AS (
  SELECT * FROM fair
  ORDER BY tier, slice, joined_at, priority, run_at, id
  LIMIT $2),
pick AS (
  SELECT j.id FROM ordered o JOIN jobs j ON j.id = o.id
  WHERE j.state = 'queued'
  ORDER BY o.tier, o.slice, o.joined_at, o.priority, o.run_at, o.id
  LIMIT $5
  FOR UPDATE OF j SKIP LOCKED),
upd AS (
  UPDATE jobs j SET state = 'running', attempts = j.attempts + 1, lease_owner = $6,
    lease_token = gen_random_uuid(), lease_until = now() + $7::float8 * interval '1 millisecond',
    heartbeat_at = now(), started_at = now()
  FROM pick WHERE j.id = pick.id
  RETURNING j.*)
SELECT (SELECT count(*) FROM ordered) AS candidates, {JOB_COLUMNS}
FROM (SELECT 1) one LEFT JOIN upd j ON true"
    )
});

impl SelectStrategy for FairRoundRobin {
    /// Refuses (claiming nothing) any kind outside [`FairRoundRobin::kinds`]. A claim that comes
    /// back short although it had more candidates than it got is re-run once, with
    /// [`retry_candidates`].
    fn dequeue<'a>(
        &'a self,
        queue: &'a Queue,
        kinds: &'a [&'a str],
        owner: &'a str,
        limit: i64,
        lease: Duration,
    ) -> BoxFuture<'a, Result<Vec<Job>, QueueError>> {
        Box::pin(async move {
            if self.check(kinds).is_err() {
                return Err(QueueError::InvalidOptions(
                    "fair dequeue: kind does not set games.analysis_started_at",
                ));
            }
            if kinds.is_empty() || limit <= 0 {
                return Ok(Vec::new());
            }
            let first = self
                .attempt(queue, kinds, owner, limit, first_candidates(limit), lease)
                .await?;
            let mut jobs = first.jobs;
            let got = jobs.len() as i64;
            if got < limit && first.candidates > got {
                let more = self
                    .attempt(
                        queue,
                        kinds,
                        owner,
                        limit - got,
                        retry_candidates(limit),
                        lease,
                    )
                    .await?;
                jobs.extend(more.jobs);
                jobs.sort_by_key(|j| (j.priority, j.run_at, j.id));
            }
            Ok(jobs)
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
