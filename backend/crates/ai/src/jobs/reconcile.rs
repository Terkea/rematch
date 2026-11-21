//! `ai-reconcile` (every 300 s, ADR-R3-3): an idempotent repair sweep over the AI tables, in the
//! pattern of R1's `analysis::jobs::reconcile`. `Queue::enqueue` commits on its own, so a row can
//! commit while its job is lost; Postgres holds the state, and the sweep rebuilds what the queue
//! should contain.
//!
//! 1. `queued` runs older than [`QUEUED_GRACE`] with no live `report:<id>` job are enqueued.
//! 2. `running` runs whose heartbeat is older than [`STALE_HEARTBEAT`] with no live job are set
//!    back to `queued` (a fenced update) and enqueued.
//! 3. `awaiting_retry` runs whose `next_attempt_at` passed more than [`RETRY_OVERDUE`] ago with no
//!    live job are enqueued (the claim accepts `awaiting_retry`).
//! 4. Due `pending` weekly reviews and change notes with no live job are enqueued. A batch-mode
//!    review whose batch item is still open (`submitted` or `ended`, unprocessed) is left to the
//!    batch poller.
//! 5. `pending` chat messages older than [`CHAT_LOST_AFTER`] with no live job are set `failed`
//!    with [`LOST_REASON`] and get `chat.message.failed` in the same transaction: an answer is
//!    stale by then, so it is never retried.
//! 6. `pending` interactive evals older than [`EVAL_LOST_AFTER`] with no live `ieval:<id>` job are
//!    set `failed` with `charged = false` (a refund).
//! 7. Agent SDK session entries with no conversation, older than [`ORPHAN_SESSION_GRACE`], are
//!    deleted (`chat::sweep_orphan_session_entries`), on every sweep.
//!
//! `awaiting_batch` runs and finished rows belong to their owners (the batch poller, the user)
//! and are never touched.
//!
//! **Paging.** Each sweep reads keyset pages ordered by `(created_at, id)` from a literal-status
//! partial index ([`sweep_pages`] lists them; a test guards every plan, custom and generic). The
//! status is a literal, so even a generic plan proves the partial-index predicate. Where the age
//! is the order column (queued runs, chats, evals) the cutoff bounds the scan; elsewhere a `due`
//! flag carries the age test (heartbeat, retry time), so a window is always an ordered range scan
//! that stops at the limit. The cursor advances to the window's last row, due or not.
//!
//! **Interruption.** There is no sweep-wide transaction. Every write is fenced on the state it
//! was read in, and the writes are ordered so a sweep cut off part-way is repaired by the next
//! one: a stale run is reset before its enqueue (an enqueue first could be claimed and dropped by
//! the still-`running` row), and a reset whose enqueue is lost is a `queued` run past the grace,
//! found by step 1. A failed chat and its event commit together.
//!
//! **Bounds.** Every await is bounded: the queue calls by the queue's own timeout, the
//! repository reads and writes by [`STEP_TIMEOUT`].

use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;

use futures::FutureExt;
use futures::future::BoxFuture;
use queue::queue::{CLIENT_BACKSTOP, DEFAULT_QUERY_TIMEOUT, Queue, QueueError};
use queue::runner::{Handler, JobContext};
use queue::{Job, JobError, JobState};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use super::kinds::{
    self, KIND_AI_RECONCILE, KIND_COACH_CHAT, KIND_EXPLAIN_CHANGE, KIND_GENERATE_REPORT,
    KIND_INTERACTIVE_ANALYSE, KIND_WEEKLY_REVIEW,
};
use crate::db::chat;
use crate::events::{self, CHAT_MESSAGE_FAILED, ChatFailed, EmitError};

/// A `queued` run younger than this may still have its enqueue in flight.
pub const QUEUED_GRACE: Duration = Duration::from_secs(5 * 60);
/// A `running` run whose heartbeat is older than this has lost its worker.
pub const STALE_HEARTBEAT: Duration = Duration::from_secs(10 * 60);
/// An `awaiting_retry` run this long past its `next_attempt_at` has lost its released job.
pub const RETRY_OVERDUE: Duration = Duration::from_secs(5 * 60);
/// A `pending` chat answer older than this is stale: it is failed, not retried.
pub const CHAT_LOST_AFTER: Duration = Duration::from_secs(2 * 60);
/// A `pending` interactive eval older than this is failed and refunded.
pub const EVAL_LOST_AFTER: Duration = Duration::from_secs(2 * 60);
/// The grace of the orphan session-entry sweep (a first turn's entries may precede its
/// conversation's commit).
pub const ORPHAN_SESSION_GRACE: Duration = chat::ORPHAN_SESSION_MIN_AGE;
/// `coach_messages.failure_reason` and `interactive_evals.error` of a lost item.
pub const LOST_REASON: &str = "lost";
/// Keyset page size of every sweep.
pub const PAGE_SIZE: i64 = 500;
/// The bound on one repository call (or one short transaction) of the sweep.
pub const STEP_TIMEOUT: Duration = DEFAULT_QUERY_TIMEOUT.saturating_add(CLIENT_BACKSTOP);

/// What one sweep did. `reports`, `weekly` and `notes` count new jobs, `chats` and `evals` the
/// rows failed, `session_entries` the orphan entries deleted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiReconcileCounts {
    pub reports: u32,
    pub weekly: u32,
    pub notes: u32,
    pub chats: u32,
    pub evals: u32,
    pub session_entries: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum AiReconcileError {
    #[error("ai reconcile database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Queue(#[from] QueueError),
    #[error("ai reconcile event error: {0}")]
    Emit(#[from] EmitError),
    #[error("ai reconcile step {op} timed out after {after:?}")]
    Timeout { op: &'static str, after: Duration },
}

/// The keyset window of a sweep page. `$1` the age cutoff, `$2`/`$3` the cursor
/// (`created_at`, `id`; NULL for the first page), `$4` the limit. Rows are
/// `(id, account_id, created_at, due, batch)`.
macro_rules! page_sql {
    ($from:literal, $alias:literal, $account:literal, $status:literal, $age:literal, $due:literal, $batch:literal) => {
        concat!(
            "SELECT ", $alias, "id, ", $account, " AS account_id, ", $alias, "created_at, ",
            $due, " AS due, ", $batch, " AS batch FROM ", $from,
            " WHERE ", $alias, "status = '", $status, "'", $age,
            " AND ", $alias, "created_at >= coalesce($2::timestamptz, '-infinity'::timestamptz)",
            " AND (", $alias, "created_at, ", $alias, "id) > (coalesce($2::timestamptz, ",
            "'-infinity'::timestamptz), coalesce($3::uuid, '00000000-0000-0000-0000-000000000000'::uuid))",
            " ORDER BY ", $alias, "created_at, ", $alias, "id LIMIT $4"
        )
    };
}

/// `queued` runs past the grace (`$1` = now − [`QUEUED_GRACE`]).
pub const QUEUED_RUNS_SQL: &str = page_sql!(
    "report_runs",
    "",
    "account_id",
    "queued",
    " AND created_at < $1",
    "true",
    "false"
);
/// `running` runs; due when the heartbeat (or the start) is before `$1`.
pub const RUNNING_RUNS_SQL: &str = page_sql!(
    "report_runs",
    "",
    "account_id",
    "running",
    "",
    "coalesce(heartbeat_at, started_at, created_at) < $1",
    "false"
);
/// `awaiting_retry` runs; due when `next_attempt_at` is before `$1`.
pub const RETRY_RUNS_SQL: &str = page_sql!(
    "report_runs",
    "",
    "account_id",
    "awaiting_retry",
    "",
    "coalesce(next_attempt_at, created_at) < $1",
    "false"
);
/// `pending` weekly reviews; due when they have no retry pending after `$1` (now).
pub const PENDING_WEEKLY_SQL: &str = page_sql!(
    "weekly_reviews",
    "",
    "account_id",
    "pending",
    "",
    "(next_attempt_at IS NULL OR next_attempt_at <= $1)",
    "mode = 'batch'"
);
/// `pending` change notes; due when they have no retry pending after `$1` (now).
pub const PENDING_NOTES_SQL: &str = page_sql!(
    "weakness_change_notes",
    "",
    "account_id",
    "pending",
    "",
    "(next_attempt_at IS NULL OR next_attempt_at <= $1)",
    "false"
);
/// `pending` chat messages older than `$1` (now − [`CHAT_LOST_AFTER`]), with their account.
pub const PENDING_CHATS_SQL: &str = page_sql!(
    "coach_messages m",
    "m.",
    "(SELECT c.account_id FROM coach_conversations c WHERE c.id = m.conversation_id)",
    "pending",
    " AND m.created_at < $1",
    "true",
    "false"
);
/// `pending` interactive evals older than `$1` (now − [`EVAL_LOST_AFTER`]).
pub const PENDING_EVALS_SQL: &str = page_sql!(
    "interactive_evals",
    "",
    "account_id",
    "pending",
    " AND created_at < $1",
    "true",
    "false"
);

/// Every sweep page as `(name, sql, index it reads in order)`, for the plan guards.
pub fn sweep_pages() -> [(&'static str, &'static str, &'static str); 7] {
    [
        ("queued_runs", QUEUED_RUNS_SQL, "report_runs_live_idx"),
        ("running_runs", RUNNING_RUNS_SQL, "report_runs_live_idx"),
        ("retry_runs", RETRY_RUNS_SQL, "report_runs_live_idx"),
        (
            "pending_weekly",
            PENDING_WEEKLY_SQL,
            "weekly_reviews_pending_idx",
        ),
        (
            "pending_notes",
            PENDING_NOTES_SQL,
            "weakness_change_notes_pending_idx",
        ),
        (
            "pending_chats",
            PENDING_CHATS_SQL,
            "coach_messages_pending_idx",
        ),
        (
            "pending_evals",
            PENDING_EVALS_SQL,
            "interactive_evals_pending_idx",
        ),
    ]
}

/// Back to `queued`, fenced: only if no worker heartbeated or finished it since it was read.
const RESET_STALE_SQL: &str = "UPDATE report_runs SET status = 'queued' \
    WHERE id = $1 AND status = 'running' AND coalesce(heartbeat_at, started_at, created_at) < $2";

/// Fails a lost chat answer, fenced on `pending` (a message that started streaming is live).
const FAIL_CHAT_SQL: &str = "UPDATE coach_messages SET status = 'failed', failure_reason = $2 \
    WHERE id = $1 AND status = 'pending' AND created_at < $3";

/// Fails and refunds a lost eval, fenced on `pending`.
const FAIL_EVAL_SQL: &str = "UPDATE interactive_evals SET status = 'failed', error = $2, \
    finished_at = now(), charged = false WHERE id = $1 AND status = 'pending' AND created_at < $3";

/// Which of `$1` (batch-mode weekly reviews) still have an open batch item.
const IN_OPEN_BATCH_SQL: &str = "SELECT DISTINCT i.weekly_review_id FROM ai_batch_items i \
    JOIN ai_batches b ON b.id = i.batch_id WHERE i.weekly_review_id = ANY($1) \
    AND i.processed_at IS NULL AND b.status IN ('submitted', 'ended')";

#[derive(sqlx::FromRow)]
struct SweepRow {
    id: Uuid,
    account_id: Uuid,
    created_at: OffsetDateTime,
    due: bool,
    batch: bool,
}

/// One window of a sweep.
struct Page {
    due: Vec<SweepRow>,
    /// The window's last `(created_at, id)`: the next cursor (`None` when it is the last page).
    next: Option<(OffsetDateTime, Uuid)>,
}

/// Bounds one repository call (or one short transaction) by [`STEP_TIMEOUT`].
async fn bounded<T, E>(
    op: &'static str,
    fut: impl Future<Output = Result<T, E>>,
) -> Result<T, AiReconcileError>
where
    AiReconcileError: From<E>,
{
    match tokio::time::timeout(STEP_TIMEOUT, fut).await {
        Ok(res) => res.map_err(AiReconcileError::from),
        Err(_) => Err(AiReconcileError::Timeout {
            op,
            after: STEP_TIMEOUT,
        }),
    }
}

async fn fetch_page(
    pool: &PgPool,
    op: &'static str,
    sql: &'static str,
    cutoff: OffsetDateTime,
    after: Option<(OffsetDateTime, Uuid)>,
    limit: i64,
) -> Result<Page, AiReconcileError> {
    let rows: Vec<SweepRow> = bounded(
        op,
        sqlx::query_as(sql)
            .bind(cutoff)
            .bind(after.map(|a| a.0))
            .bind(after.map(|a| a.1))
            .bind(limit)
            .fetch_all(pool),
    )
    .await?;
    let full = rows.len() as i64 >= limit;
    let next = rows.last().filter(|_| full).map(|r| (r.created_at, r.id));
    let due = rows.into_iter().filter(|r| r.due).collect();
    Ok(Page { due, next })
}

async fn live_states(
    queue: &Queue,
    kind: &str,
    rows: &[SweepRow],
    key: fn(Uuid) -> String,
) -> Result<HashMap<String, JobState>, QueueError> {
    let keys: Vec<String> = rows.iter().map(|r| key(r.id)).collect();
    queue.live_states(kind, &keys).await
}

/// The due rows of the page that have no live job.
async fn lost<'a>(
    queue: &Queue,
    kind: &str,
    page: &'a Page,
    key: fn(Uuid) -> String,
) -> Result<Vec<&'a SweepRow>, QueueError> {
    let live = live_states(queue, kind, &page.due, key).await?;
    Ok(page
        .due
        .iter()
        .filter(|r| !live.contains_key(&key(r.id)))
        .collect())
}

fn bump(n: &mut u32, enqueued: Option<i64>) {
    if enqueued.is_some() {
        *n = n.saturating_add(1);
    }
}

/// Runs one sweep (see the module docs) with ages measured from `now`.
pub async fn reconcile_ai(
    pool: &PgPool,
    queue: &Queue,
    now: OffsetDateTime,
) -> Result<AiReconcileCounts, AiReconcileError> {
    reconcile_ai_paged(pool, queue, now, PAGE_SIZE).await
}

/// [`reconcile_ai`] with a page size (tests use a small one).
pub async fn reconcile_ai_paged(
    pool: &PgPool,
    queue: &Queue,
    now: OffsetDateTime,
    page_size: i64,
) -> Result<AiReconcileCounts, AiReconcileError> {
    let limit = page_size.max(1);
    let mut c = AiReconcileCounts::default();

    // 1. Lost queued runs. First, so runs reset in step 2 are counted only there.
    let cutoff = now - QUEUED_GRACE;
    let mut after = None;
    loop {
        let page = fetch_page(pool, "queued_runs", QUEUED_RUNS_SQL, cutoff, after, limit).await?;
        for r in lost(queue, KIND_GENERATE_REPORT, &page, kinds::report_key).await? {
            bump(
                &mut c.reports,
                kinds::enqueue_report(queue, r.id, r.account_id).await?,
            );
        }
        let Some(next) = page.next else { break };
        after = Some(next);
    }

    // 2. Stale running runs: reset (fenced), then enqueue.
    let cutoff = now - STALE_HEARTBEAT;
    let mut after = None;
    loop {
        let page = fetch_page(pool, "running_runs", RUNNING_RUNS_SQL, cutoff, after, limit).await?;
        for r in lost(queue, KIND_GENERATE_REPORT, &page, kinds::report_key).await? {
            let reset = bounded(
                "reset_stale_run",
                sqlx::query(RESET_STALE_SQL)
                    .bind(r.id)
                    .bind(cutoff)
                    .execute(pool),
            )
            .await?;
            if reset.rows_affected() == 0 {
                continue; // it moved on meanwhile
            }
            bump(
                &mut c.reports,
                kinds::enqueue_report(queue, r.id, r.account_id).await?,
            );
        }
        let Some(next) = page.next else { break };
        after = Some(next);
    }

    // 3. Overdue retries.
    let cutoff = now - RETRY_OVERDUE;
    let mut after = None;
    loop {
        let page = fetch_page(pool, "retry_runs", RETRY_RUNS_SQL, cutoff, after, limit).await?;
        for r in lost(queue, KIND_GENERATE_REPORT, &page, kinds::report_key).await? {
            bump(
                &mut c.reports,
                kinds::enqueue_report(queue, r.id, r.account_id).await?,
            );
        }
        let Some(next) = page.next else { break };
        after = Some(next);
    }

    // 4a. Weekly reviews (batch-mode ones in an open batch are the poller's).
    let mut after = None;
    loop {
        let page = fetch_page(
            pool,
            "pending_weekly",
            PENDING_WEEKLY_SQL,
            now,
            after,
            limit,
        )
        .await?;
        let batch_ids: Vec<Uuid> = page.due.iter().filter(|r| r.batch).map(|r| r.id).collect();
        let in_batch: Vec<Uuid> = if batch_ids.is_empty() {
            Vec::new()
        } else {
            bounded(
                "weekly_in_open_batch",
                sqlx::query_scalar(IN_OPEN_BATCH_SQL)
                    .bind(&batch_ids)
                    .fetch_all(pool),
            )
            .await?
        };
        for r in lost(queue, KIND_WEEKLY_REVIEW, &page, kinds::weekly_key).await? {
            if in_batch.contains(&r.id) {
                continue;
            }
            bump(
                &mut c.weekly,
                kinds::enqueue_weekly(queue, r.id, r.account_id).await?,
            );
        }
        let Some(next) = page.next else { break };
        after = Some(next);
    }

    // 4b. Change notes.
    let mut after = None;
    loop {
        let page = fetch_page(pool, "pending_notes", PENDING_NOTES_SQL, now, after, limit).await?;
        for r in lost(queue, KIND_EXPLAIN_CHANGE, &page, kinds::change_key).await? {
            bump(
                &mut c.notes,
                kinds::enqueue_change(queue, r.id, r.account_id).await?,
            );
        }
        let Some(next) = page.next else { break };
        after = Some(next);
    }

    // 5. Lost chat answers: failed with their event, in one transaction.
    let cutoff = now - CHAT_LOST_AFTER;
    let mut after = None;
    loop {
        let page = fetch_page(
            pool,
            "pending_chats",
            PENDING_CHATS_SQL,
            cutoff,
            after,
            limit,
        )
        .await?;
        for r in lost(queue, KIND_COACH_CHAT, &page, kinds::chat_key).await? {
            if fail_chat(pool, r.id, r.account_id, cutoff).await? {
                c.chats = c.chats.saturating_add(1);
            }
        }
        let Some(next) = page.next else { break };
        after = Some(next);
    }

    // 6. Lost interactive evals: failed and refunded.
    let cutoff = now - EVAL_LOST_AFTER;
    let mut after = None;
    loop {
        let page = fetch_page(
            pool,
            "pending_evals",
            PENDING_EVALS_SQL,
            cutoff,
            after,
            limit,
        )
        .await?;
        for r in lost(queue, KIND_INTERACTIVE_ANALYSE, &page, kinds::ieval_key).await? {
            let failed = bounded(
                "fail_eval",
                sqlx::query(FAIL_EVAL_SQL)
                    .bind(r.id)
                    .bind(LOST_REASON)
                    .bind(cutoff)
                    .execute(pool),
            )
            .await?;
            if failed.rows_affected() == 1 {
                c.evals = c.evals.saturating_add(1);
            }
        }
        let Some(next) = page.next else { break };
        after = Some(next);
    }

    // 7. Orphan Agent SDK session entries, on every sweep.
    c.session_entries = bounded("sweep_orphan_session_entries", async {
        let mut conn = pool.acquire().await?;
        chat::sweep_orphan_session_entries(&mut conn, ORPHAN_SESSION_GRACE).await
    })
    .await?;

    tracing::info!(
        reports = c.reports,
        weekly = c.weekly,
        notes = c.notes,
        chats = c.chats,
        evals = c.evals,
        session_entries = c.session_entries,
        "ai reconcile sweep"
    );
    Ok(c)
}

/// Fails one lost chat answer and emits `chat.message.failed` in the same transaction. `false`
/// when the message moved on since it was read.
async fn fail_chat(
    pool: &PgPool,
    message_id: Uuid,
    account_id: Uuid,
    cutoff: OffsetDateTime,
) -> Result<bool, AiReconcileError> {
    bounded("fail_chat", async {
        let mut tx = pool.begin().await?;
        let failed = sqlx::query(FAIL_CHAT_SQL)
            .bind(message_id)
            .bind(LOST_REASON)
            .bind(cutoff)
            .execute(&mut *tx)
            .await?;
        if failed.rows_affected() == 0 {
            return Ok::<_, AiReconcileError>(false);
        }
        let payload = ChatFailed {
            message_id,
            reason: LOST_REASON.to_owned(),
        };
        events::emit(&mut tx, account_id, CHAT_MESSAGE_FAILED, &payload).await?;
        tx.commit().await?;
        Ok(true)
    })
    .await
}

/// The queue handler for [`KIND_AI_RECONCILE`] (scheduled by `kinds::ensure_ai_recurring`).
pub struct AiReconcileHandler {
    pub pool: PgPool,
    pub queue: Queue,
}

impl Handler for AiReconcileHandler {
    fn kind(&self) -> &'static str {
        KIND_AI_RECONCILE
    }

    fn handle(&self, _job: Job, ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        async move {
            let sweep = reconcile_ai(&self.pool, &self.queue, OffsetDateTime::now_utc());
            tokio::select! {
                biased;
                // Every step is idempotent, so a sweep cut off part-way is safe to drop.
                () = ctx.cancel.cancelled() => Err(JobError::Release {
                    delay: Duration::ZERO,
                    reason: "shutdown".to_owned(),
                }),
                res = sweep => match res {
                    Ok(counts) => Ok(json!(counts)),
                    Err(e) => Err(JobError::Retryable {
                        message: e.to_string(),
                        code: None,
                    }),
                },
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_queries_filter_a_literal_status_and_page_by_created_at_and_id() {
        for (name, sql, _) in sweep_pages() {
            assert!(sql.contains("status = '"), "{name}: {sql}");
            assert!(sql.contains("ORDER BY "), "{name}");
            assert!(
                sql.ends_with("created_at, id LIMIT $4") || sql.ends_with("m.id LIMIT $4"),
                "{name}: {sql}"
            );
        }
    }

    #[test]
    fn the_stale_reset_is_fenced_on_the_page_age() {
        assert!(RESET_STALE_SQL.contains("coalesce(heartbeat_at, started_at, created_at) < $2"));
        assert!(RUNNING_RUNS_SQL.contains("coalesce(heartbeat_at, started_at, created_at) < $1"));
    }
}
