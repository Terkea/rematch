//! Retention of finished jobs (ADR 0032): the TS worker kept completed BullMQ jobs for 24 h and at
//! most 1000 per queue, and failed ones for 7 days. Rows are deleted in bounded batches, each its
//! own short transaction under the queue's statement timeout, so a large backlog never holds a
//! long lock or runs into the 30 s timeout.

use std::time::Duration;

use crate::queue::{Queue, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Retention {
    /// `done` rows older than this are deleted (24 h).
    pub done_max_age: Duration,
    /// Only the newest this many `done` rows per kind are kept (1000).
    pub done_max_count_per_kind: i64,
    /// `failed` and `dead` rows older than this are deleted (7 days).
    pub failed_max_age: Duration,
    /// At most this many rows per DELETE statement (1000).
    pub batch_size: i64,
}

impl Default for Retention {
    fn default() -> Self {
        Self {
            done_max_age: Duration::from_secs(24 * 3600),
            done_max_count_per_kind: 1000,
            failed_max_age: Duration::from_secs(7 * 24 * 3600),
            batch_size: 1000,
        }
    }
}

/// `$1` is the age in milliseconds and `$2` the batch size; oldest first, on `jobs_finished_idx`.
const DONE_AGE_SQL: &str = "\
DELETE FROM jobs WHERE id IN (
  SELECT id FROM jobs WHERE state = 'done'
    AND finished_at < now() - $1::float8 * interval '1 millisecond'
  ORDER BY finished_at LIMIT $2)";

const FAILED_AGE_SQL: &str = "\
DELETE FROM jobs WHERE id IN (
  SELECT id FROM jobs WHERE state IN ('failed', 'dead')
    AND finished_at < now() - $1::float8 * interval '1 millisecond'
  ORDER BY finished_at LIMIT $2)";

/// `$1` is the number to keep per kind and `$2` the batch size. Runs after the age pass, so it
/// only ranks rows younger than `done_max_age`.
const DONE_COUNT_SQL: &str = "\
DELETE FROM jobs WHERE id IN (
  SELECT id FROM (
    SELECT id, row_number() OVER (PARTITION BY kind ORDER BY finished_at DESC, id DESC) AS rn
    FROM jobs WHERE state = 'done') ranked
  WHERE rn > $1::float8 LIMIT $2)";

impl Queue {
    /// Applies `r` and returns the number of rows deleted. Live jobs (queued, running, paused)
    /// are never touched.
    pub async fn cleanup(&self, r: &Retention) -> Result<u64> {
        let batch = r.batch_size.max(1);
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        let mut total = 0;
        total += self
            .delete_batches("cleanup_done", DONE_AGE_SQL, ms(r.done_max_age), batch)
            .await?;
        total += self
            .delete_batches(
                "cleanup_failed",
                FAILED_AGE_SQL,
                ms(r.failed_max_age),
                batch,
            )
            .await?;
        total += self
            .delete_batches(
                "cleanup_done_count",
                DONE_COUNT_SQL,
                r.done_max_count_per_kind.max(0) as f64,
                batch,
            )
            .await?;
        if total > 0 {
            tracing::info!(deleted = total, "queue retention pruned finished jobs");
        }
        Ok(total)
    }

    /// Runs `sql` (one bounded DELETE) until a batch deletes fewer than `batch` rows.
    async fn delete_batches(
        &self,
        op: &'static str,
        sql: &'static str,
        arg: f64,
        batch: i64,
    ) -> Result<u64> {
        let mut total = 0;
        loop {
            let deleted = self
                .guard(op, async {
                    let mut tx = self.begin().await?;
                    let done = sqlx::query(sql)
                        .bind(arg)
                        .bind(batch)
                        .execute(&mut *tx)
                        .await?;
                    tx.commit().await?;
                    Ok(done.rows_affected())
                })
                .await?;
            total += deleted;
            if deleted < batch as u64 {
                return Ok(total);
            }
        }
    }
}
