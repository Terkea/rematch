//! Stall recovery (ADR 0032): a running job whose lease expired belonged to a worker that died
//! or hung. The reaper re-queues it and gives back the attempt the claim counted, but counts a
//! stall; a job that stalls more than `max_stalls` times (default 2) is dead-lettered with
//! `error_code = 'stalled'`, so a job that keeps killing its worker cannot loop forever.
//!
//! [`Queue::release_owned`] is the graceful counterpart: a shutting-down runner hands its running
//! jobs back at once, without using an attempt and without a stall.

use sqlx::FromRow;

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

#[derive(FromRow)]
struct RecoveredRow {
    #[sqlx(flatten)]
    job: JobRow,
    dead: bool,
}

/// One statement. The outer `WHERE` repeats `state` and `lease_until < now()`, so a renew that
/// committed after the inner scan wins (Postgres re-checks the outer quals on the newest row
/// version). The `SET` expressions read the old row, so `j.stalls + 1` is the new stall count.
const RECOVER_SQL: &str = concat!(
    "WITH upd AS (
  UPDATE jobs j SET
    stalls = j.stalls + 1,
    state = (CASE WHEN j.stalls + 1 > j.max_stalls THEN 'dead' ELSE 'queued' END)::job_state,
    attempts = CASE WHEN j.stalls + 1 > j.max_stalls THEN j.attempts
      ELSE greatest(j.attempts - 1, 0) END,
    finished_at = CASE WHEN j.stalls + 1 > j.max_stalls THEN now() END,
    last_error = CASE WHEN j.stalls + 1 > j.max_stalls THEN '",
    "job stalled more than max_stalls times",
    "' ELSE j.last_error END,
    error_code = 'stalled', run_at = now(),
    lease_owner = NULL, lease_token = NULL, lease_until = NULL
  WHERE j.id IN (SELECT id FROM jobs WHERE state = 'running' AND kind = ANY($1)
                 AND lease_until < now() FOR UPDATE SKIP LOCKED)
    AND j.state = 'running' AND j.lease_until < now()
  RETURNING ",
    job_columns!(),
    ", j.state = 'dead' AS dead)
SELECT upd.*, (CASE WHEN NOT upd.dead THEN pg_notify('",
    jobs_channel!(),
    "', upd.kind) END) IS NULL AS not_notified
FROM upd"
);

/// Every running job of `owner` back to queued, ready at once, with the attempt given back.
const RELEASE_OWNED_SQL: &str = concat!(
    "WITH upd AS (
  UPDATE jobs SET state = 'queued', attempts = greatest(attempts - 1, 0), run_at = now(),
    lease_owner = NULL, lease_token = NULL, lease_until = NULL,
    error_code = 'shutdown', last_error = 'released: shutdown'
  WHERE state = 'running' AND lease_owner = $1
  RETURNING kind)
SELECT pg_notify('",
    jobs_channel!(),
    "', kind) IS NULL FROM upd"
);

impl Queue {
    /// Recovers every running job of `kinds` whose lease has expired: re-queued at once with
    /// `stalls + 1` and the attempt given back, or `dead` (`error_code = 'stalled'`, attempt
    /// kept) once `stalls` exceeds `max_stalls`. Re-queued jobs wake the runners. Rows locked by
    /// a concurrent renew or reaper are skipped.
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
        let mut out: Vec<Recovered> = rows
            .into_iter()
            .map(|r| Recovered {
                job: r.job.into_job(),
                dead: r.dead,
            })
            .collect();
        out.sort_by_key(|r| r.job.id);
        Ok(out)
    }

    /// Hands every job `owner` is running back to the queue, ready at once and without using an
    /// attempt (`error_code = 'shutdown'`), and returns how many. A late write by the old holder
    /// is fenced out because the lease token is cleared.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_fences_on_an_expired_lease_and_skips_locked_rows() {
        assert!(RECOVER_SQL.contains("FOR UPDATE SKIP LOCKED"));
        assert!(RECOVER_SQL.contains("AND j.state = 'running' AND j.lease_until < now()"));
        assert!(RECOVER_SQL.contains(STALLED_REASON));
        assert!(RECOVER_SQL.contains("pg_notify('jobs'"));
    }
}
