//! Stall recovery (ADR 0032): a running job whose lease expired belonged to a worker that died
//! or hung. The reaper re-queues it and gives back the attempt the claim counted, but counts a
//! stall; a job that stalls more than `max_stalls` times (default 2) is dead-lettered with
//! `error_code = 'stalled'`, so a job that keeps killing its worker cannot loop forever.
//!
//! [`Queue::release_owned`] is the graceful counterpart: a shutting-down runner hands its running
//! jobs back at once, without using an attempt and without a stall.
//!
//! Both re-queues keep the job's `run_at`: the job is due at once and keeps its place in the
//! claim order (it does not fall behind a backlog), and an immediate job stays claimable when
//! the database clock steps back (`run_at <= created_at`, ADR 0032).
//!
//! Expiry is judged on the database clock, which can step forward and make a live lease look
//! expired. [`Queue::recover_expired`] trusts one look; the runner's reaper instead lists
//! [`Queue::expired_leases`], waits until the same lease (same token, same heartbeat) has looked
//! expired for a grace period on the monotonic clock, and only then calls
//! [`Queue::recover_marked`]. A live owner renews in between, which changes the heartbeat.

use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::queue::{JobRow, Queue, Result, job_columns, jobs_channel};
use crate::types::Job;

/// `last_error` of a job dead-lettered by the reaper (and the `on_dead` reason).
pub const STALLED_REASON: &str = "job stalled more than max_stalls times";

/// A job the reaper took back from an expired lease.
#[derive(Clone, Debug, PartialEq)]
pub struct Recovered {
    /// The row after recovery (no lease: its token is nil).
    pub job: Job,
    /// `true` when it stalled more than `max_stalls` times and is now `dead`.
    pub dead: bool,
}

/// A running job whose lease looked expired by the database clock, identified by its lease token
/// and last heartbeat: if either changes, the owner is alive.
#[derive(Clone, Debug, PartialEq, Eq, FromRow)]
pub struct LeaseMark {
    pub id: i64,
    pub lease_token: Uuid,
    pub heartbeat_at: OffsetDateTime,
}

#[derive(FromRow)]
struct RecoveredRow {
    #[sqlx(flatten)]
    job: JobRow,
    dead: bool,
}

/// The recovery `SET` list. It reads the old row, so `j.stalls + 1` is the new stall count;
/// `run_at` is kept (see the module docs).
macro_rules! recover_set {
    () => {
        "stalls = j.stalls + 1,
    state = (CASE WHEN j.stalls + 1 > j.max_stalls THEN 'dead' ELSE 'queued' END)::job_state,
    attempts = CASE WHEN j.stalls + 1 > j.max_stalls THEN j.attempts
      ELSE greatest(j.attempts - 1, 0) END,
    finished_at = CASE WHEN j.stalls + 1 > j.max_stalls THEN now() END,
    last_error = CASE WHEN j.stalls + 1 > j.max_stalls
      THEN 'job stalled more than max_stalls times' ELSE j.last_error END,
    error_code = 'stalled',
    lease_owner = NULL, lease_token = NULL, lease_until = NULL"
    };
}

/// Returns the recovered rows and wakes the runners for the re-queued ones.
macro_rules! recover_returning {
    () => {
        concat!(
            "
  RETURNING ",
            job_columns!(),
            ", j.state = 'dead' AS dead)
SELECT upd.*, (CASE WHEN NOT upd.dead THEN pg_notify('",
            jobs_channel!(),
            "', upd.kind) END) IS NULL AS not_notified
FROM upd"
        )
    };
}

/// One statement. The outer `WHERE` repeats `state` and `lease_until < now()`, so a renew that
/// committed after the inner scan wins (Postgres re-checks the outer quals on the newest row
/// version).
const RECOVER_SQL: &str = concat!(
    "WITH upd AS (
  UPDATE jobs j SET ",
    recover_set!(),
    "
  WHERE j.id IN (SELECT id FROM jobs WHERE state = 'running' AND kind = ANY($1)
                 AND lease_until < now() FOR UPDATE SKIP LOCKED)
    AND j.state = 'running' AND j.lease_until < now()",
    recover_returning!()
);

/// Running jobs of `$1` whose lease looks expired by the database clock (on `jobs_lease_idx`).
const EXPIRED_SQL: &str = "\
SELECT id, lease_token, heartbeat_at FROM jobs
WHERE state = 'running' AND kind = ANY($1) AND lease_until < now()
  AND lease_token IS NOT NULL AND heartbeat_at IS NOT NULL";

/// Recovers exactly the marked leases `($1, $2, $3)` = (ids, tokens, heartbeats) that are still
/// running, still expired and unchanged. Rows locked by a concurrent renew or reaper are skipped.
const RECOVER_MARKED_SQL: &str = concat!(
    "WITH upd AS (
  UPDATE jobs j SET ",
    recover_set!(),
    "
  WHERE j.id IN (SELECT id FROM jobs WHERE id = ANY($1) AND state = 'running'
                 FOR UPDATE SKIP LOCKED)
    AND (j.id, j.lease_token, j.heartbeat_at) IN
        (SELECT * FROM unnest($1::bigint[], $2::uuid[], $3::timestamptz[]))
    AND j.state = 'running' AND j.lease_until < now()",
    recover_returning!()
);

/// Every running job of `owner` back to queued, due at once in its old place (`run_at` kept),
/// with the attempt given back.
const RELEASE_OWNED_SQL: &str = concat!(
    "WITH upd AS (
  UPDATE jobs SET state = 'queued', attempts = greatest(attempts - 1, 0),
    lease_owner = NULL, lease_token = NULL, lease_until = NULL,
    error_code = 'shutdown', last_error = 'released: shutdown'
  WHERE state = 'running' AND lease_owner = $1
  RETURNING kind)
SELECT pg_notify('",
    jobs_channel!(),
    "', kind) IS NULL FROM upd"
);

impl Queue {
    /// Recovers every running job of `kinds` whose lease has expired by one look at the database
    /// clock: re-queued at once (its `run_at` kept) with `stalls + 1` and the attempt given
    /// back, or `dead` (`error_code = 'stalled'`, attempt kept) once `stalls` exceeds
    /// `max_stalls`. Re-queued jobs wake the runners. Rows locked by a concurrent renew or
    /// reaper are skipped. A forward step of the database clock can make a live lease look
    /// expired here; the runner's reaper uses [`Queue::expired_leases`] and
    /// [`Queue::recover_marked`] instead.
    pub async fn recover_expired(&self, kinds: &[&str]) -> Result<Vec<Recovered>> {
        if kinds.is_empty() {
            return Ok(Vec::new());
        }
        let fut = sqlx::query_as::<_, RecoveredRow>(RECOVER_SQL).bind(kinds);
        let rows = self
            .guard("recover_expired", async {
                let mut tx = self.begin().await?;
                let rows = fut.fetch_all(&mut *tx).await?;
                tx.commit().await?;
                Ok(rows)
            })
            .await?;
        Ok(into_recovered(rows))
    }

    /// The running leases of `kinds` that look expired by the database clock (first pass of the
    /// runner's two-pass stall check).
    pub async fn expired_leases(&self, kinds: &[&str]) -> Result<Vec<LeaseMark>> {
        if kinds.is_empty() {
            return Ok(Vec::new());
        }
        let fut = sqlx::query_as::<_, LeaseMark>(EXPIRED_SQL).bind(kinds);
        self.guard("expired_leases", async {
            let mut tx = self.begin().await?;
            let rows = fut.fetch_all(&mut *tx).await?;
            tx.commit().await?;
            Ok(rows)
        })
        .await
    }

    /// Recovers the `marks` whose job is still running under the same lease token and
    /// heartbeat, with the lease still expired (second pass), exactly like
    /// [`recover_expired`](Self::recover_expired) does.
    pub async fn recover_marked(&self, marks: &[LeaseMark]) -> Result<Vec<Recovered>> {
        if marks.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<i64> = marks.iter().map(|m| m.id).collect();
        let tokens: Vec<Uuid> = marks.iter().map(|m| m.lease_token).collect();
        let beats: Vec<OffsetDateTime> = marks.iter().map(|m| m.heartbeat_at).collect();
        let fut = sqlx::query_as::<_, RecoveredRow>(RECOVER_MARKED_SQL)
            .bind(ids)
            .bind(tokens)
            .bind(beats);
        let rows = self
            .guard("recover_marked", async {
                let mut tx = self.begin().await?;
                let rows = fut.fetch_all(&mut *tx).await?;
                tx.commit().await?;
                Ok(rows)
            })
            .await?;
        Ok(into_recovered(rows))
    }

    /// Hands every job `owner` is running back to the queue, due at once in its old place
    /// (`run_at` kept) and without using an attempt (`error_code = 'shutdown'`), and returns how
    /// many. A late write by the old holder is fenced out because the lease token is cleared.
    pub async fn release_owned(&self, owner: &str) -> Result<u64> {
        let fut = sqlx::query_scalar::<_, bool>(RELEASE_OWNED_SQL).bind(owner);
        let rows = self
            .guard("release_owned", async {
                let mut tx = self.begin().await?;
                let rows = fut.fetch_all(&mut *tx).await?;
                tx.commit().await?;
                Ok(rows)
            })
            .await?;
        Ok(rows.len() as u64)
    }
}

fn into_recovered(rows: Vec<RecoveredRow>) -> Vec<Recovered> {
    let mut out: Vec<Recovered> = rows
        .into_iter()
        .map(|r| Recovered {
            job: r.job.into_job(),
            dead: r.dead,
        })
        .collect();
    out.sort_by_key(|r| r.job.id);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_fences_on_an_expired_lease_and_skips_locked_rows() {
        for sql in [RECOVER_SQL, RECOVER_MARKED_SQL] {
            assert!(sql.contains("FOR UPDATE SKIP LOCKED"), "{sql}");
            assert!(
                sql.contains("AND j.state = 'running' AND j.lease_until < now()"),
                "{sql}"
            );
            assert!(sql.contains(STALLED_REASON), "{sql}");
            assert!(sql.contains("pg_notify('jobs'"), "{sql}");
            assert!(!sql.contains("run_at ="), "re-queues keep run_at: {sql}");
        }
        assert!(RECOVER_MARKED_SQL.contains("j.lease_token, j.heartbeat_at"));
        assert!(!RELEASE_OWNED_SQL.contains("run_at ="));
    }
}
