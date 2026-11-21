//! `ai-reconcile` (every 300 s, ADR-R3-3 amendment 1): an idempotent repair sweep over the AI
//! tables, in the pattern of R1's `analysis::jobs::reconcile`. `Queue::enqueue` commits on its
//! own, so a row can commit while its job is lost; Postgres holds the state, and the sweep
//! rebuilds what the queue should contain.
//!
//! 1. `queued` runs older than [`QUEUED_GRACE`] with no live `report:<id>` job are enqueued.
//!    A batch-mode run younger than [`BATCH_SUBMIT_GRACE`] may be mid-submit and is skipped.
//! 2. `running` runs whose heartbeat is older than [`STALE_HEARTBEAT`] with no live job are set
//!    back to `queued` (a fenced update) and enqueued.
//! 3. `awaiting_retry` runs whose `next_attempt_at` passed more than [`RETRY_OVERDUE`] ago with no
//!    live job are enqueued (the claim accepts `awaiting_retry`).
//! 4. Due `pending` weekly reviews and change notes older than [`PENDING_GRACE`] with no live job
//!    are enqueued. A batch-mode review is skipped while it is younger than
//!    [`BATCH_SUBMIT_GRACE`] or has an unprocessed item in a `submitted` or `ended` batch (the
//!    poller's).
//! 5. `pending` chat messages older than [`CHAT_LOST_AFTER`], and `streaming` ones that started
//!    streaming more than [`STREAMING_LOST_AFTER`] ago, with no live job, are set `failed` with
//!    [`LOST_REASON`] and get `chat.message.failed` in the same transaction: an answer is stale
//!    by then, so it is never retried.
//! 6. `pending` interactive evals older than [`EVAL_LOST_AFTER`] with no live `ieval:<id>` job are
//!    set `failed` with `charged = false` (a refund).
//! 7. Agent SDK session entries with no conversation, between [`chat::ORPHAN_SESSION_MIN_AGE`]
//!    and [`chat::ORPHAN_SESSION_MAX_AGE`] old, are deleted in pages, on every sweep, within
//!    [`ORPHAN_SWEEP_BUDGET`].
//!
//! `awaiting_batch` runs and finished rows belong to their owners (the batch poller, the user)
//! and are never touched.
//!
//! **Crash cap.** Every reconciler re-enqueue (steps 1–4) first bumps the row's
//! `reconcile_count` in a fenced update. Past [`RECONCILE_CAP`] the row is failed instead
//! (`last_error = 'reconcile_cap'` on runs), with `report.degraded` (a run with a report, so the
//! computed report still shows) or `job.failed` in the same transaction. A poison row that kills
//! its worker every time is therefore given up on after [`RECONCILE_CAP`] repairs. The bump
//! precedes the enqueue, so a sweep cut off in between over-counts by one and never loops.
//!
//! **Paging.** Each sweep reads keyset pages ordered by `(age column, id)` from a literal-status
//! partial index ([`sweep_pages`] lists them; a test guards every plan, custom and generic). The
//! status is a literal, so even a generic plan proves the partial-index predicate. Where the age
//! is the order column the cutoff bounds the scan; elsewhere a `due` flag carries the age test
//! (heartbeat, retry time), so a window is always an ordered range scan that stops at the limit.
//! The cursor advances to the window's last row, due or not.
//!
//! **Interruption and failures.** There is no sweep-wide transaction. Every write is fenced on
//! the state it was read in, and the writes are ordered so a sweep cut off part-way is repaired
//! by the next one: a stale run is reset before its enqueue (an enqueue first could be claimed
//! and dropped by the still-`running` row), and a reset whose enqueue is lost is a `queued` run
//! past the grace, found by step 1. A failed step does not stop the others: every step runs, and
//! the errors are returned together ([`AiReconcileError::Steps`]).
//!
//! **Bounds.** Every await is bounded: the queue calls by the queue's own timeout, the
//! repository reads and writes by [`STEP_TIMEOUT`], the orphan sweep by its budget.

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
use sqlx::{PgConnection, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use super::kinds::{
    self, CHAT_TURN_TIMEOUT, KIND_AI_RECONCILE, KIND_COACH_CHAT, KIND_EXPLAIN_CHANGE,
    KIND_GENERATE_REPORT, KIND_INTERACTIVE_ANALYSE, KIND_WEEKLY_REVIEW,
};
use crate::db::chat;
use crate::events::{
    self, CHAT_MESSAGE_FAILED, ChatFailed, DegradedReason, EmitError, JOB_FAILED, JobFailed,
    REPORT_DEGRADED, ReportDegraded,
};

/// A `queued` run younger than this may still have its enqueue in flight.
pub const QUEUED_GRACE: Duration = Duration::from_secs(5 * 60);
/// A `running` run whose heartbeat is older than this has lost its worker.
pub const STALE_HEARTBEAT: Duration = Duration::from_secs(10 * 60);
/// An `awaiting_retry` run this long past its `next_attempt_at` has lost its released job.
pub const RETRY_OVERDUE: Duration = Duration::from_secs(5 * 60);
/// A `pending` weekly review or change note younger than this may still have its enqueue in
/// flight.
pub const PENDING_GRACE: Duration = Duration::from_secs(5 * 60);
/// A batch-mode run or weekly review younger than this may be mid-submit to the Message
/// Batches API: enqueueing it for sync generation could pay for it twice. Task 20 keeps its
/// submit timeout (the whole submit, retries included) below this.
pub const BATCH_SUBMIT_GRACE: Duration = Duration::from_secs(30 * 60);
/// A `pending` chat answer older than this is stale: it is failed, not retried.
pub const CHAT_LOST_AFTER: Duration = Duration::from_secs(2 * 60);
/// A turn `streaming` for longer than this with no live job is lost: the turn timeout, the
/// sidecar's 30 s slack and a 60 s grace.
pub const STREAMING_LOST_AFTER: Duration = CHAT_TURN_TIMEOUT
    .saturating_add(Duration::from_secs(30))
    .saturating_add(Duration::from_secs(60));
/// A `pending` interactive eval older than this is failed and refunded.
pub const EVAL_LOST_AFTER: Duration = Duration::from_secs(2 * 60);
/// The grace of the orphan session-entry sweep (a first turn's entries may precede its
/// conversation's commit).
pub const ORPHAN_SESSION_GRACE: Duration = chat::ORPHAN_SESSION_MIN_AGE;
/// The orphan sweep stops starting new pages after this long (the next sweep continues).
pub const ORPHAN_SWEEP_BUDGET: Duration = Duration::from_secs(60);
/// Reconciler repairs of one row before it is failed instead (the crash cap).
pub const RECONCILE_CAP: i16 = 3;
/// `report_runs.last_error` (and the `job.failed` reason) of a row failed by the crash cap.
pub const RECONCILE_CAP_ERROR: &str = "reconcile_cap";
/// `coach_messages.failure_reason` and `interactive_evals.error` of a lost item.
pub const LOST_REASON: &str = "lost";
/// Keyset page size of every sweep.
pub const PAGE_SIZE: i64 = 500;
/// The bound on one repository call (or one short transaction) of the sweep.
pub const STEP_TIMEOUT: Duration = DEFAULT_QUERY_TIMEOUT.saturating_add(CLIENT_BACKSTOP);

/// What one sweep did. `reports`, `weekly` and `notes` count new jobs, `capped` the rows failed
/// by the crash cap, `chats` and `evals` the rows failed as lost, `session_entries` the orphan
/// entries deleted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiReconcileCounts {
    pub reports: u32,
    pub weekly: u32,
    pub notes: u32,
    pub chats: u32,
    pub evals: u32,
    pub capped: u32,
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
    /// Some steps failed; the others ran. `counts` is what the sweep did.
    #[error("ai reconcile: {}", steps_message(.errors))]
    Steps {
        counts: AiReconcileCounts,
        errors: Vec<(&'static str, AiReconcileError)>,
    },
}

fn steps_message(errors: &[(&'static str, AiReconcileError)]) -> String {
    errors
        .iter()
        .map(|(step, e)| format!("{step}: {e}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// The keyset window of a sweep page. `$1` the age cutoff, `$2`/`$3` the cursor (`$order`, `id`;
/// NULL for the first page), `$4` the limit. Rows are `(id, account_id, at, created_at, due,
/// batch)`, where `at` is the order column.
macro_rules! page_sql {
    ($from:literal, $a:literal, $account:literal, $status:literal, $order:literal, $age:literal, $due:literal, $batch:literal) => {
        concat!(
            "SELECT ", $a, "id, ", $account, " AS account_id, ", $a, $order, " AS at, ",
            $a, "created_at, ", $due, " AS due, ", $batch, " AS batch FROM ", $from,
            " WHERE ", $a, "status = '", $status, "'", $age,
            " AND ", $a, $order, " >= coalesce($2::timestamptz, '-infinity'::timestamptz)",
            " AND (", $a, $order, ", ", $a, "id) > (coalesce($2::timestamptz, ",
            "'-infinity'::timestamptz), coalesce($3::uuid, '00000000-0000-0000-0000-000000000000'::uuid))",
            " ORDER BY ", $a, $order, ", ", $a, "id LIMIT $4"
        )
    };
}

/// `queued` runs past the grace (`$1` = now − [`QUEUED_GRACE`]).
pub const QUEUED_RUNS_SQL: &str = page_sql!(
    "report_runs",
    "",
    "account_id",
    "queued",
    "created_at",
    " AND created_at < $1",
    "true",
    "mode = 'batch'"
);
/// `running` runs; due when the heartbeat (or the start) is before `$1`.
pub const RUNNING_RUNS_SQL: &str = page_sql!(
    "report_runs",
    "",
    "account_id",
    "running",
    "created_at",
    "",
    "coalesce(heartbeat_at, started_at, created_at) < $1",
    "mode = 'batch'"
);
/// `awaiting_retry` runs; due when `next_attempt_at` is before `$1`.
pub const RETRY_RUNS_SQL: &str = page_sql!(
    "report_runs",
    "",
    "account_id",
    "awaiting_retry",
    "created_at",
    "",
    "coalesce(next_attempt_at, created_at) < $1",
    "mode = 'batch'"
);
/// `pending` weekly reviews older than `$1` (now − [`PENDING_GRACE`]) with no retry pending
/// after it.
pub const PENDING_WEEKLY_SQL: &str = page_sql!(
    "weekly_reviews",
    "",
    "account_id",
    "pending",
    "created_at",
    " AND created_at < $1",
    "(next_attempt_at IS NULL OR next_attempt_at <= $1)",
    "mode = 'batch'"
);
/// `pending` change notes older than `$1` (now − [`PENDING_GRACE`]) with no retry pending
/// after it.
pub const PENDING_NOTES_SQL: &str = page_sql!(
    "weakness_change_notes",
    "",
    "account_id",
    "pending",
    "created_at",
    " AND created_at < $1",
    "(next_attempt_at IS NULL OR next_attempt_at <= $1)",
    "false"
);
/// `pending` chat messages older than `$1` (now − [`CHAT_LOST_AFTER`]), with their account.
pub const PENDING_CHATS_SQL: &str = page_sql!(
    "coach_messages m",
    "m.",
    "(SELECT c.account_id FROM coach_conversations c WHERE c.id = m.conversation_id)",
    "pending",
    "created_at",
    " AND m.created_at < $1",
    "true",
    "false"
);
/// `streaming` chat messages that started streaming before `$1` (now −
/// [`STREAMING_LOST_AFTER`]), with their account.
pub const STREAMING_CHATS_SQL: &str = page_sql!(
    "coach_messages m",
    "m.",
    "(SELECT c.account_id FROM coach_conversations c WHERE c.id = m.conversation_id)",
    "streaming",
    "streaming_at",
    " AND m.streaming_at < $1",
    "true",
    "false"
);
/// `pending` interactive evals older than `$1` (now − [`EVAL_LOST_AFTER`]).
pub const PENDING_EVALS_SQL: &str = page_sql!(
    "interactive_evals",
    "",
    "account_id",
    "pending",
    "created_at",
    " AND created_at < $1",
    "true",
    "false"
);

/// Every sweep page as `(name, sql, index it reads in order)`, for the plan guards. All take
/// `(timestamptz, timestamptz, uuid, int8)`. The orphan sweep's page is
/// [`chat::ORPHAN_PAGE_SQL`].
pub fn sweep_pages() -> [(&'static str, &'static str, &'static str); 8] {
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
            "streaming_chats",
            STREAMING_CHATS_SQL,
            "coach_messages_streaming_idx",
        ),
        (
            "pending_evals",
            PENDING_EVALS_SQL,
            "interactive_evals_pending_idx",
        ),
    ]
}

/// Bumps a queued run's repair count, fenced on `queued` and the grace.
const BUMP_QUEUED_RUN_SQL: &str = "UPDATE report_runs SET reconcile_count = reconcile_count + 1 \
    WHERE id = $1 AND status = 'queued' AND created_at < $2 RETURNING reconcile_count, report_id";
/// Back to `queued` with its repair count bumped, fenced: only if no worker heartbeated or
/// finished it since it was read.
const RESET_STALE_RUN_SQL: &str = "UPDATE report_runs SET status = 'queued', \
    reconcile_count = reconcile_count + 1 WHERE id = $1 AND status = 'running' \
    AND coalesce(heartbeat_at, started_at, created_at) < $2 RETURNING reconcile_count, report_id";
/// Bumps an overdue retry's repair count, fenced on `awaiting_retry` and the overdue time.
const BUMP_RETRY_RUN_SQL: &str = "UPDATE report_runs SET reconcile_count = reconcile_count + 1 \
    WHERE id = $1 AND status = 'awaiting_retry' AND coalesce(next_attempt_at, created_at) < $2 \
    RETURNING reconcile_count, report_id";
/// Fails a run past the cap (the row is locked by the bump in the same transaction).
const CAP_RUN_SQL: &str = "UPDATE report_runs SET status = 'failed', finished_at = now(), \
    next_attempt_at = NULL, last_error = $2 WHERE id = $1";
const BUMP_WEEKLY_SQL: &str = "UPDATE weekly_reviews SET reconcile_count = reconcile_count + 1 \
    WHERE id = $1 AND status = 'pending' RETURNING reconcile_count";
const CAP_WEEKLY_SQL: &str = "UPDATE weekly_reviews SET status = 'failed', \
    next_attempt_at = NULL, updated_at = now() WHERE id = $1";
const BUMP_NOTE_SQL: &str = "UPDATE weakness_change_notes SET \
    reconcile_count = reconcile_count + 1 WHERE id = $1 AND status = 'pending' \
    RETURNING reconcile_count";
const CAP_NOTE_SQL: &str = "UPDATE weakness_change_notes SET status = 'failed', \
    next_attempt_at = NULL, updated_at = now() WHERE id = $1";

/// Fails a lost pending chat answer, fenced on `pending` (a message that started streaming is
/// live) and the cutoff.
const FAIL_PENDING_CHAT_SQL: &str = "UPDATE coach_messages SET status = 'failed', \
    failure_reason = $2 WHERE id = $1 AND status = 'pending' AND created_at < $3";
/// Fails a lost streaming chat answer, fenced on `streaming` and the cutoff.
const FAIL_STREAMING_CHAT_SQL: &str = "UPDATE coach_messages SET status = 'failed', \
    failure_reason = $2 WHERE id = $1 AND status = 'streaming' AND streaming_at < $3";

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
    at: OffsetDateTime,
    created_at: OffsetDateTime,
    due: bool,
    batch: bool,
}

/// One window of a sweep.
struct Page {
    due: Vec<SweepRow>,
    /// The window's last `(at, id)`: the next cursor (`None` when it is the last page).
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
    let next = rows.last().filter(|_| full).map(|r| (r.at, r.id));
    let due = rows.into_iter().filter(|r| r.due).collect();
    Ok(Page { due, next })
}

/// The due rows of the page that have no live job of `kind`.
async fn lost<'a>(
    queue: &Queue,
    kind: &str,
    page: &'a Page,
    key: fn(Uuid) -> String,
) -> Result<Vec<&'a SweepRow>, QueueError> {
    let keys: Vec<String> = page.due.iter().map(|r| key(r.id)).collect();
    let live: HashMap<String, JobState> = queue.live_states(kind, &keys).await?;
    Ok(page
        .due
        .iter()
        .filter(|r| !live.contains_key(&key(r.id)))
        .collect())
}

fn add(n: &mut u32, yes: bool) {
    if yes {
        *n = n.saturating_add(1);
    }
}

/// What the crash-cap bump decided.
enum Repair {
    /// Bumped and under the cap: enqueue it.
    Requeue,
    /// Past the cap: failed, with its event.
    Capped,
    /// The row moved on since it was read: nothing to do.
    Moved,
}

/// The crash-cap target of one repair.
#[derive(Clone, Copy)]
enum Target {
    Run {
        bump_sql: &'static str,
        cutoff: OffsetDateTime,
    },
    Weekly,
    Note,
}

/// Bumps the row's `reconcile_count` (fenced by the target's bump SQL) and, past
/// [`RECONCILE_CAP`], fails the row and emits its event, in one transaction.
async fn bump_or_cap(
    pool: &PgPool,
    target: Target,
    id: Uuid,
    account_id: Uuid,
) -> Result<Repair, AiReconcileError> {
    bounded("reconcile_bump", async {
        let mut tx = pool.begin().await?;
        let repair = bump_or_cap_tx(&mut tx, target, id, account_id).await?;
        tx.commit().await?;
        Ok::<_, AiReconcileError>(repair)
    })
    .await
}

async fn bump_or_cap_tx(
    tx: &mut PgConnection,
    target: Target,
    id: Uuid,
    account_id: Uuid,
) -> Result<Repair, AiReconcileError> {
    let (count, report_id, kind, cap_sql): (i16, Option<Uuid>, &str, &str) = match target {
        Target::Run { bump_sql, cutoff } => {
            let row: Option<(i16, Option<Uuid>)> = sqlx::query_as(bump_sql)
                .bind(id)
                .bind(cutoff)
                .fetch_optional(&mut *tx)
                .await?;
            let Some((count, report_id)) = row else {
                return Ok(Repair::Moved);
            };
            (count, report_id, KIND_GENERATE_REPORT, CAP_RUN_SQL)
        }
        Target::Weekly | Target::Note => {
            let (sql, kind, cap) = match target {
                Target::Weekly => (BUMP_WEEKLY_SQL, KIND_WEEKLY_REVIEW, CAP_WEEKLY_SQL),
                _ => (BUMP_NOTE_SQL, KIND_EXPLAIN_CHANGE, CAP_NOTE_SQL),
            };
            let row: Option<i16> = sqlx::query_scalar(sql)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
            let Some(count) = row else {
                return Ok(Repair::Moved);
            };
            (count, None, kind, cap)
        }
    };
    if count <= RECONCILE_CAP {
        return Ok(Repair::Requeue);
    }
    let mut cap = sqlx::query(cap_sql).bind(id);
    if matches!(target, Target::Run { .. }) {
        cap = cap.bind(RECONCILE_CAP_ERROR);
    }
    cap.execute(&mut *tx).await?;
    match report_id {
        // The computed report stays; its narrative is unavailable.
        Some(report_id) => {
            let payload = ReportDegraded {
                report_id,
                reason: DegradedReason::Unavailable,
                retry_at: None,
            };
            events::emit(tx, account_id, REPORT_DEGRADED, &payload).await?;
        }
        None => {
            let payload = JobFailed {
                kind: kind.to_owned(),
                reason: RECONCILE_CAP_ERROR.to_owned(),
            };
            events::emit(tx, account_id, JOB_FAILED, &payload).await?;
        }
    }
    tracing::warn!(%id, kind, count, "ai reconcile: crash cap reached, failed");
    Ok(Repair::Capped)
}

/// Runs one sweep (see the module docs) with ages measured from `now`.
pub async fn reconcile_ai(
    pool: &PgPool,
    queue: &Queue,
    now: OffsetDateTime,
) -> Result<AiReconcileCounts, AiReconcileError> {
    reconcile_ai_paged(pool, queue, now, PAGE_SIZE).await
}

/// [`reconcile_ai`] with a page size (tests use a small one). Every step runs even if an
/// earlier one failed; the errors come back together in [`AiReconcileError::Steps`].
pub async fn reconcile_ai_paged(
    pool: &PgPool,
    queue: &Queue,
    now: OffsetDateTime,
    page_size: i64,
) -> Result<AiReconcileCounts, AiReconcileError> {
    let s = Sweep {
        pool,
        queue,
        now,
        limit: page_size.max(1),
    };
    let mut c = AiReconcileCounts::default();
    let mut errors = Vec::new();
    macro_rules! step {
        ($name:literal, $call:expr) => {
            if let Err(e) = $call.await {
                tracing::warn!(step = $name, error = %e, "ai reconcile step failed");
                errors.push(($name, e));
            }
        };
    }
    // Queued runs first, so runs reset by the stale step are counted only there.
    step!("queued_runs", s.queued_runs(&mut c));
    step!("running_runs", s.running_runs(&mut c));
    step!("retry_runs", s.retry_runs(&mut c));
    step!("pending_weekly", s.pending_weekly(&mut c));
    step!("pending_notes", s.pending_notes(&mut c));
    step!(
        "pending_chats",
        s.lost_chats(
            &mut c,
            PENDING_CHATS_SQL,
            FAIL_PENDING_CHAT_SQL,
            CHAT_LOST_AFTER
        )
    );
    step!(
        "streaming_chats",
        s.lost_chats(
            &mut c,
            STREAMING_CHATS_SQL,
            FAIL_STREAMING_CHAT_SQL,
            STREAMING_LOST_AFTER
        )
    );
    step!("pending_evals", s.pending_evals(&mut c));
    step!("orphan_session_entries", s.orphan_session_entries(&mut c));

    tracing::info!(
        reports = c.reports,
        weekly = c.weekly,
        notes = c.notes,
        chats = c.chats,
        evals = c.evals,
        capped = c.capped,
        session_entries = c.session_entries,
        failed_steps = errors.len(),
        "ai reconcile sweep"
    );
    if errors.is_empty() {
        Ok(c)
    } else {
        Err(AiReconcileError::Steps { counts: c, errors })
    }
}

struct Sweep<'a> {
    pool: &'a PgPool,
    queue: &'a Queue,
    now: OffsetDateTime,
    limit: i64,
}

impl Sweep<'_> {
    /// Whether a batch-mode row may still be mid-submit.
    fn batch_in_flight(&self, r: &SweepRow) -> bool {
        r.batch && r.created_at > self.now - BATCH_SUBMIT_GRACE
    }

    async fn requeue_run(
        &self,
        c: &mut AiReconcileCounts,
        r: &SweepRow,
        bump_sql: &'static str,
        cutoff: OffsetDateTime,
    ) -> Result<(), AiReconcileError> {
        let target = Target::Run { bump_sql, cutoff };
        match bump_or_cap(self.pool, target, r.id, r.account_id).await? {
            Repair::Requeue => {
                let id = kinds::enqueue_report(self.queue, r.id, r.account_id).await?;
                add(&mut c.reports, id.is_some());
            }
            Repair::Capped => add(&mut c.capped, true),
            Repair::Moved => {}
        }
        Ok(())
    }

    async fn queued_runs(&self, c: &mut AiReconcileCounts) -> Result<(), AiReconcileError> {
        let cutoff = self.now - QUEUED_GRACE;
        let mut after = None;
        loop {
            let page = fetch_page(
                self.pool,
                "queued_runs",
                QUEUED_RUNS_SQL,
                cutoff,
                after,
                self.limit,
            )
            .await?;
            for r in lost(self.queue, KIND_GENERATE_REPORT, &page, kinds::report_key).await? {
                if self.batch_in_flight(r) {
                    continue;
                }
                self.requeue_run(c, r, BUMP_QUEUED_RUN_SQL, cutoff).await?;
            }
            let Some(next) = page.next else { break };
            after = Some(next);
        }
        Ok(())
    }

    async fn running_runs(&self, c: &mut AiReconcileCounts) -> Result<(), AiReconcileError> {
        let cutoff = self.now - STALE_HEARTBEAT;
        let mut after = None;
        loop {
            let page = fetch_page(
                self.pool,
                "running_runs",
                RUNNING_RUNS_SQL,
                cutoff,
                after,
                self.limit,
            )
            .await?;
            for r in lost(self.queue, KIND_GENERATE_REPORT, &page, kinds::report_key).await? {
                self.requeue_run(c, r, RESET_STALE_RUN_SQL, cutoff).await?;
            }
            let Some(next) = page.next else { break };
            after = Some(next);
        }
        Ok(())
    }

    async fn retry_runs(&self, c: &mut AiReconcileCounts) -> Result<(), AiReconcileError> {
        let cutoff = self.now - RETRY_OVERDUE;
        let mut after = None;
        loop {
            let page = fetch_page(
                self.pool,
                "retry_runs",
                RETRY_RUNS_SQL,
                cutoff,
                after,
                self.limit,
            )
            .await?;
            for r in lost(self.queue, KIND_GENERATE_REPORT, &page, kinds::report_key).await? {
                self.requeue_run(c, r, BUMP_RETRY_RUN_SQL, cutoff).await?;
            }
            let Some(next) = page.next else { break };
            after = Some(next);
        }
        Ok(())
    }

    async fn pending_weekly(&self, c: &mut AiReconcileCounts) -> Result<(), AiReconcileError> {
        let cutoff = self.now - PENDING_GRACE;
        let mut after = None;
        loop {
            let page = fetch_page(
                self.pool,
                "pending_weekly",
                PENDING_WEEKLY_SQL,
                cutoff,
                after,
                self.limit,
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
                        .fetch_all(self.pool),
                )
                .await?
            };
            for r in lost(self.queue, KIND_WEEKLY_REVIEW, &page, kinds::weekly_key).await? {
                if self.batch_in_flight(r) || in_batch.contains(&r.id) {
                    continue;
                }
                match bump_or_cap(self.pool, Target::Weekly, r.id, r.account_id).await? {
                    Repair::Requeue => {
                        let id = kinds::enqueue_weekly(self.queue, r.id, r.account_id).await?;
                        add(&mut c.weekly, id.is_some());
                    }
                    Repair::Capped => add(&mut c.capped, true),
                    Repair::Moved => {}
                }
            }
            let Some(next) = page.next else { break };
            after = Some(next);
        }
        Ok(())
    }

    async fn pending_notes(&self, c: &mut AiReconcileCounts) -> Result<(), AiReconcileError> {
        let cutoff = self.now - PENDING_GRACE;
        let mut after = None;
        loop {
            let page = fetch_page(
                self.pool,
                "pending_notes",
                PENDING_NOTES_SQL,
                cutoff,
                after,
                self.limit,
            )
            .await?;
            for r in lost(self.queue, KIND_EXPLAIN_CHANGE, &page, kinds::change_key).await? {
                match bump_or_cap(self.pool, Target::Note, r.id, r.account_id).await? {
                    Repair::Requeue => {
                        let id = kinds::enqueue_change(self.queue, r.id, r.account_id).await?;
                        add(&mut c.notes, id.is_some());
                    }
                    Repair::Capped => add(&mut c.capped, true),
                    Repair::Moved => {}
                }
            }
            let Some(next) = page.next else { break };
            after = Some(next);
        }
        Ok(())
    }

    /// Lost chat answers (`pending` or `streaming`): failed with their event, in one transaction.
    async fn lost_chats(
        &self,
        c: &mut AiReconcileCounts,
        page_sql: &'static str,
        fail_sql: &'static str,
        lost_after: Duration,
    ) -> Result<(), AiReconcileError> {
        let cutoff = self.now - lost_after;
        let mut after = None;
        loop {
            let page = fetch_page(self.pool, "chats", page_sql, cutoff, after, self.limit).await?;
            for r in lost(self.queue, KIND_COACH_CHAT, &page, kinds::chat_key).await? {
                let failed = fail_chat(self.pool, fail_sql, r.id, r.account_id, cutoff).await?;
                add(&mut c.chats, failed);
            }
            let Some(next) = page.next else { break };
            after = Some(next);
        }
        Ok(())
    }

    /// Lost interactive evals: failed and refunded.
    async fn pending_evals(&self, c: &mut AiReconcileCounts) -> Result<(), AiReconcileError> {
        let cutoff = self.now - EVAL_LOST_AFTER;
        let mut after = None;
        loop {
            let page = fetch_page(
                self.pool,
                "pending_evals",
                PENDING_EVALS_SQL,
                cutoff,
                after,
                self.limit,
            )
            .await?;
            for r in lost(
                self.queue,
                KIND_INTERACTIVE_ANALYSE,
                &page,
                kinds::ieval_key,
            )
            .await?
            {
                let failed = bounded(
                    "fail_eval",
                    sqlx::query(FAIL_EVAL_SQL)
                        .bind(r.id)
                        .bind(LOST_REASON)
                        .bind(cutoff)
                        .execute(self.pool),
                )
                .await?;
                add(&mut c.evals, failed.rows_affected() == 1);
            }
            let Some(next) = page.next else { break };
            after = Some(next);
        }
        Ok(())
    }

    /// Orphan Agent SDK session entries, in pages of [`chat::ORPHAN_SWEEP_PAGE`], until a page is
    /// short or [`ORPHAN_SWEEP_BUDGET`] is spent (the next sweep continues).
    async fn orphan_session_entries(
        &self,
        c: &mut AiReconcileCounts,
    ) -> Result<(), AiReconcileError> {
        let deadline = tokio::time::Instant::now() + ORPHAN_SWEEP_BUDGET;
        let mut after = None;
        loop {
            let page = bounded("sweep_orphan_session_entries", async {
                let mut conn = self.pool.acquire().await?;
                chat::sweep_orphan_session_entries_page(
                    &mut conn,
                    self.now,
                    after,
                    chat::ORPHAN_SWEEP_PAGE,
                )
                .await
            })
            .await?;
            c.session_entries += page.deleted;
            match page.next {
                Some(next) if tokio::time::Instant::now() < deadline => after = Some(next),
                _ => return Ok(()),
            }
        }
    }
}

/// Fails one lost chat answer with `fail_sql` (fenced on its status and `cutoff`) and emits
/// `chat.message.failed` in the same transaction. `false` when the message moved on.
async fn fail_chat(
    pool: &PgPool,
    fail_sql: &'static str,
    message_id: Uuid,
    account_id: Uuid,
    cutoff: OffsetDateTime,
) -> Result<bool, AiReconcileError> {
    bounded("fail_chat", async {
        let mut tx = pool.begin().await?;
        let failed = sqlx::query(fail_sql)
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
    fn page_queries_filter_a_literal_status_and_page_by_their_age_column_and_id() {
        for (name, sql, _) in sweep_pages() {
            assert!(sql.contains("status = '"), "{name}: {sql}");
            assert!(
                sql.ends_with("created_at, id LIMIT $4")
                    || sql.ends_with("m.created_at, m.id LIMIT $4")
                    || sql.ends_with("m.streaming_at, m.id LIMIT $4"),
                "{name}: {sql}"
            );
        }
    }

    #[test]
    fn the_repairs_are_fenced_on_the_page_age() {
        assert!(
            RESET_STALE_RUN_SQL.contains("coalesce(heartbeat_at, started_at, created_at) < $2")
        );
        assert!(RUNNING_RUNS_SQL.contains("coalesce(heartbeat_at, started_at, created_at) < $1"));
        assert!(BUMP_RETRY_RUN_SQL.contains("coalesce(next_attempt_at, created_at) < $2"));
        assert!(FAIL_STREAMING_CHAT_SQL.contains("status = 'streaming' AND streaming_at < $3"));
    }

    #[test]
    fn grace_constants() {
        assert_eq!(STREAMING_LOST_AFTER, Duration::from_secs(600 + 30 + 60));
        assert!(BATCH_SUBMIT_GRACE > Duration::from_secs(10 * 60));
    }
}
