//! Monthly reports and weekly reviews regenerated through the Message Batches API (Task 20,
//! ADR 0063): the same requests as the sync path at half the price, with results applied by a
//! poller.
//!
//! **Scheduling.** Both schedule handlers do nothing without an AI client. When the client has
//! no batches (`!ai.batches_enabled()`: no `ANTHROPIC_API_KEY`, but the sidecar has a
//! credential, such as the owner's dev OAuth token; ADR 0062), each due item goes to the normal
//! sync job instead (M5b Task B ruling, ADR 0075): a sync `scheduled` run through
//! [`request_report`], or a sync weekly review through [`request_weekly_review`]. The same items
//! are due either way, and the sync jobs meter at sync rates. The result's `path` is `batch` or
//! `sync`.
//!
//! - [`ReportBatchScheduleHandler`]: for each (account, time class) whose newest report is at
//!   least [`REPORT_BATCH_MIN_AGE_DAYS`] old and has no unfinished run, on the Pro plan and
//!   within budget, with a valid input from the provider that names that account and time
//!   class: a `mode = batch`, `trigger = scheduled` run with the input snapshot and the computed
//!   report, in one transaction. Then one batch item per section.
//! - [`WeeklyBatchScheduleHandler`]: for the last complete week ([`reviewed_week`]: the Monday
//!   before the current week's; M5b Task B, so the Monday 04:00 run reviews the week that just
//!   ended), a `mode = batch` pending weekly review per due item (an existing review for the
//!   week is left alone), then its item.
//!
//! The rows are created `queued` / `pending` in batch mode, which `ai-reconcile` ignores for
//! [`BATCH_SUBMIT_GRACE`]. Items are packed into batches by whole runs (a run never spans two
//! batches) within the API's request and byte limits, each submit bounded by
//! [`BATCH_SUBMIT_TIMEOUT`] (below the grace). After a submit, one transaction records the batch,
//! its items, and sets the runs `awaiting_batch`. A failed submit leaves the rows as they are:
//! past the grace the reconciler generates them synchronously.
//!
//! **Polling** ([`AiBatchPollHandler`]), each open batch under a transaction-scoped advisory
//! lock, re-reading its status under the lock:
//!
//! - not ended: `last_polled_at` only. A batch not ended [`BATCH_GIVE_UP_AFTER`] after its
//!   submit is given up: every unprocessed item falls back to the sync path, and the batch is
//!   `failed`.
//! - ended: the results stream, resolved with [`RegistryResolver`]. Each result is applied only
//!   to the item its custom id names, and only when that item belongs to the batch being
//!   polled: the item id decoded from the custom id must be the item row's id, and the run or
//!   review, its input and its section come from that row, never from the result. A result for
//!   another batch or an unknown item is ignored (and logged). Then the item's run or review must
//!   still be waiting for this batch; otherwise the billed result is metered and nothing else is
//!   written.
//! - every applied result is processed by Task 16's `process_*_result` (validation, at most one
//!   sync repair), then one transaction marks the item processed (the fence against applying a
//!   result twice), meters the batch result keyed by the item (`meter_keyed`, so a billed call is
//!   metered once at batch rates even if the write is retried), stores the section or review and
//!   emits its events last. A sync repair is metered before that, in its own transaction under a
//!   key of its own: it is paid when it returns, even if the item's write then fails (T26). `RetrySync` (or a failed repair) stores nothing and hands the work to
//!   the sync job.
//! - a run whose items are all processed is finished: with all five sections stored it is done
//!   (`report.ready`); otherwise it goes back to `queued` and `generate-report` is enqueued after
//!   the commit, and that job generates only the missing sections. Sections are stored with
//!   Task 18's `report_job::persist_section_outcome`, the same write as the sync path. A weekly review that needs
//!   the sync path gets `weekly-review` enqueued.
//! - a batch the API reports ended is marked `ended` (its `ended_at` starts a bound). A results
//!   line that cannot be decoded (or is too long) is skipped and the stream goes on; a transport
//!   error, another client error or [`BATCH_RESULTS_TIMEOUT`] stops the read, and the next poll
//!   skips the processed items. When a complete stream left an item without a result (none was
//!   sent, or its line was undecodable), only that item falls back to the sync path. A batch whose
//!   results could not be read completely for [`BATCH_RESULTS_GIVE_UP_AFTER`] after it ended is
//!   given up like one that never ends. A batch with every item processed is `processed`.
//!
//! Errored, canceled and expired results are not billed by the API, so they meter nothing
//! (Task 16 ruling).

use std::sync::Arc;
use std::time::Duration;

use futures::future::BoxFuture;
use futures::stream::{BoxStream, StreamExt};
use queue::queue::Queue;
use queue::runner::{Handler, JobContext};
use queue::{Job, JobError};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::budget::{BudgetConfig, budget_guard, meter_keyed};
use super::kinds::{
    self, KIND_AI_BATCH_POLL, KIND_REPORT_BATCH_SCHEDULE, KIND_WEEKLY_BATCH_SCHEDULE,
};
use super::providers::CoachingInputProvider;
use super::reconcile::BATCH_SUBMIT_GRACE;
use super::report_job::{is_narrative, persist_section_outcome, refresh_status, request_report};
use super::weekly_job::request_weekly_review;
use crate::client::cost::PricingTable;
use crate::client::model::{AiModel, Models};
use crate::client::port::{
    AiClient, AiError, BatchItem, BatchOutcome, BatchResultItem, BatchStatus,
};
use crate::db::ItemStatus;
use crate::db::batches::{
    self, BatchItemRow, BatchKind, BatchRow, BatchStatus as DbBatchStatus, NewBatchItem,
};
use crate::db::reports::{self, NewReport, ReportStatus};
use crate::db::runs::{self, NewRun, ReportMode, ReportTrigger, RunStatus};
use crate::db::usage::AiFeature;
use crate::db::weekly::{self, NewWeekly, WeeklyContent};
use crate::entitlements::{Entitlements, Plan};
use crate::events::{
    self, REPORT_READY, REPORT_SECTION_READY, ReportReady, SectionReady, WEEKLY_REVIEW_READY,
    WeeklyReady,
};
use crate::generate::batch::{
    BatchProcessed, MAX_BATCH_BYTES, MAX_BATCH_REQUESTS, RegistryResolver, WeeklyProcessed,
    batch_item_id, batch_request_bytes, process_report_result, process_weekly_result,
    section_batch_item, weekly_batch_item,
};
use crate::generate::computed::build_computed_report;
use crate::generate::context::input_hash;
use crate::generate::payload::REPORT_PAYLOAD_SCHEMA_VERSION;
use crate::generate::section::{CallRecord, SectionError};
use crate::generate::weekly::WeeklyOutcome;
use crate::input::parse::parse_coaching_input;
use crate::input::types::{CoachingInput, TimeClass};
use crate::output::codes::SECTION_KEYS;
use crate::output::disclosure::make_disclosure;
use crate::output::weekly::WeeklyReviewInput;
use crate::prompts::{
    REPORT_PROMPT_SET, REPORT_PROMPT_VERSION, WEEKLY_PROMPT_SET, WEEKLY_PROMPT_VERSION,
    prompt_set_hash,
};

/// A report this many days old is regenerated by the monthly batch.
pub const REPORT_BATCH_MIN_AGE_DAYS: i64 = 30;
/// The most runs one schedule pass creates (the rest are due on the next day's pass).
pub const REPORT_BATCH_MAX_RUNS: i64 = 2_000;
/// One whole submit, retries included. Below [`BATCH_SUBMIT_GRACE`], so the reconciler never
/// sync-generates a row whose submit may still succeed.
pub const BATCH_SUBMIT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// One status lookup.
pub const BATCH_STATUS_TIMEOUT: Duration = Duration::from_secs(60);
/// Reading one batch's results; on expiry the next poll resumes.
pub const BATCH_RESULTS_TIMEOUT: Duration = Duration::from_secs(20 * 60);
/// A batch not ended this long after its submit is given up to the sync path. The API ends
/// (expires) every batch within 24 hours, so this only fires when the API or the key is gone.
pub const BATCH_GIVE_UP_AFTER: Duration = Duration::from_secs(48 * 3600);

/// An ended batch whose results could not be read completely for this long after it ended is
/// given up to the sync path (results are retrievable for 29 days; this bounds a results
/// endpoint that keeps failing).
pub const BATCH_RESULTS_GIVE_UP_AFTER: Duration = Duration::from_secs(24 * 3600);

/// Advisory-lock class of one batch's poll (two-int key space, like the report version lock).
const POLL_LOCK_CLASS: i32 = 30_020;

const _: () = assert!(BATCH_SUBMIT_TIMEOUT.as_secs() < BATCH_SUBMIT_GRACE.as_secs());

/// The weekly reviews due for the week starting `week_start` (a Monday): (account, time class,
/// `WeeklyReviewInput` JSON). The worker wires R2's builder (M5b Task B); the JSON is validated
/// here, never trusted.
pub trait WeeklyInputProvider: Send + Sync {
    fn due<'a>(
        &'a self,
        week_start: Date,
    ) -> BoxStream<'a, anyhow::Result<(Uuid, TimeClass, Value)>>;
}

/// Nothing is ever due.
#[derive(Clone, Copy, Debug, Default)]
pub struct NullWeeklyProvider;

impl WeeklyInputProvider for NullWeeklyProvider {
    fn due<'a>(
        &'a self,
        _week_start: Date,
    ) -> BoxStream<'a, anyhow::Result<(Uuid, TimeClass, Value)>> {
        futures::stream::empty().boxed()
    }
}

/// What the three batch handlers need.
#[derive(Clone)]
pub struct BatchDeps {
    pub pool: PgPool,
    pub queue: Queue,
    /// `None` (no AI configured): nothing is scheduled.
    pub ai: Option<Arc<dyn AiClient>>,
    pub inputs: Arc<dyn CoachingInputProvider>,
    pub weekly: Arc<dyn WeeklyInputProvider>,
    pub entitlements: Arc<dyn Entitlements>,
    pub budgets: BudgetConfig,
    pub pricing: Arc<PricingTable>,
    pub now: Arc<dyn Fn() -> OffsetDateTime + Send + Sync>,
}

impl BatchDeps {
    async fn within_budget(&self, account_id: Uuid, plan: Plan, now: OffsetDateTime) -> bool {
        let scope = self.entitlements.budget_scope(account_id).await;
        budget_guard(self.pool.clone(), scope, plan, self.budgets, now)()
            .await
            .is_ok()
    }
}

/// The Monday of `now`'s UTC week.
pub fn week_monday(now: OffsetDateTime) -> Date {
    let d = now.to_offset(time::UtcOffset::UTC).date();
    d - time::Duration::days(i64::from(d.weekday().number_days_from_monday()))
}

/// The Monday of the last complete UTC week before `now`: the week a weekly review covers.
pub fn reviewed_week(now: OffsetDateTime) -> Date {
    week_monday(now) - time::Duration::days(7)
}

// -------------------------------------------------------------------------------------------
// Scheduling.

/// One run or review to submit, with its batch items.
struct Planned {
    items: Vec<(NewBatchItem, BatchItem)>,
    /// The run to set `awaiting_batch` after the submit (reports only).
    run_id: Option<Uuid>,
}

/// (account, time class) whose newest report is older than `$1` and that have no unfinished
/// run, oldest first, at most `$2`.
const DUE_REPORTS_SQL: &str = "SELECT r.account_id, r.time_class::text FROM reports r \
    GROUP BY r.account_id, r.time_class HAVING max(r.created_at) <= $1 \
    AND NOT EXISTS (SELECT 1 FROM report_runs x WHERE x.account_id = r.account_id \
      AND x.time_class = r.time_class \
      AND x.status IN ('queued', 'running', 'awaiting_retry', 'awaiting_batch')) \
    ORDER BY max(r.created_at), r.account_id LIMIT $2";

async fn schedule_reports(d: &BatchDeps) -> anyhow::Result<Value> {
    let Some(ai) = d.ai.as_deref() else {
        return Ok(json!({ "skipped": "ai_disabled" }));
    };
    let batches = ai.batches_enabled();
    let mut sync_runs = 0u32;
    let now = (d.now)();
    let due: Vec<(Uuid, String)> = sqlx::query_as(DUE_REPORTS_SQL)
        .bind(now - time::Duration::days(REPORT_BATCH_MIN_AGE_DAYS))
        .bind(REPORT_BATCH_MAX_RUNS)
        .fetch_all(&d.pool)
        .await?;
    let mut planned = Vec::new();
    for (account_id, tc) in due {
        if d.entitlements.plan(account_id).await != Plan::Pro
            || !d.within_budget(account_id, Plan::Pro, now).await
        {
            continue;
        }
        let Ok(time_class) = tc.parse::<TimeClass>() else {
            continue;
        };
        let raw = match d.inputs.coaching_input(account_id, time_class).await {
            Ok(Some(v)) => v,
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!(account = %account_id, error = %e, "coaching input unavailable");
                continue;
            }
        };
        let input = match parse_coaching_input(&raw) {
            Ok(i) if i.account_id == account_id && i.time_class == time_class => i,
            Ok(_) => {
                tracing::error!(account = %account_id, "provider input names another account or time class");
                continue;
            }
            Err(issues) => {
                tracing::warn!(account = %account_id, issues = issues.len(), "invalid coaching input");
                continue;
            }
        };
        if batches {
            planned.push(create_scheduled_run(d, account_id, &tc, &raw, &input).await?);
        } else {
            // The job takes its own snapshot from the provider; this pass only checked it.
            request_report(
                &d.pool,
                &d.queue,
                account_id,
                time_class,
                ReportTrigger::Scheduled,
                Plan::Pro,
            )
            .await?;
            sync_runs += 1;
        }
    }
    if !batches {
        return Ok(json!({ "path": "sync", "runs": sync_runs }));
    }
    let runs = planned.len();
    let submitted = submit_all(d, ai, BatchKind::Report, planned).await?;
    Ok(json!({ "path": "batch", "runs": runs, "batches": submitted }))
}

/// The run, its snapshot and its computed report, in one transaction; then its items.
async fn create_scheduled_run(
    d: &BatchDeps,
    account_id: Uuid,
    time_class: &str,
    raw: &Value,
    input: &CoachingInput,
) -> anyhow::Result<Planned> {
    let mut tx = d.pool.begin().await?;
    let run_id = runs::insert_run(
        &mut tx,
        &NewRun {
            account_id,
            time_class: time_class.to_owned(),
            trigger: ReportTrigger::Scheduled,
            plan: Plan::Pro,
            mode: ReportMode::Batch,
            prompt_version: REPORT_PROMPT_VERSION.to_owned(),
            is_first_report: false,
            ai_disabled_reason: None,
        },
    )
    .await?;
    runs::snapshot_input(
        &mut tx,
        run_id,
        raw,
        &input_hash(input),
        &prompt_set_hash(REPORT_PROMPT_SET),
    )
    .await?;
    let computed = build_computed_report(input, false);
    let report = reports::create_report_with_next_version(
        &mut tx,
        &NewReport {
            account_id,
            time_class: time_class.to_owned(),
            report_run_id: run_id,
            schema_version: REPORT_PAYLOAD_SCHEMA_VERSION.to_owned(),
            computed: serde_json::to_value(&computed)?,
            status: ReportStatus::ComputedOnly,
        },
    )
    .await?;
    runs::link_report(&mut tx, run_id, report.id).await?;
    events::emit(
        &mut tx,
        account_id,
        REPORT_SECTION_READY,
        &SectionReady {
            report_id: report.id,
            report_run_id: run_id,
            section: "computed".to_owned(),
            narrative: false,
        },
    )
    .await?;
    tx.commit().await?;
    let items = SECTION_KEYS
        .into_iter()
        .map(|section| {
            let id = Uuid::new_v4();
            let item = section_batch_item(input, section, false, id);
            (
                NewBatchItem {
                    id,
                    custom_id: item.custom_id.clone(),
                    kind: BatchKind::Report,
                    report_run_id: Some(run_id),
                    section: Some(section),
                    weekly_review_id: None,
                },
                item,
            )
        })
        .collect();
    Ok(Planned {
        items,
        run_id: Some(run_id),
    })
}

async fn schedule_weekly(d: &BatchDeps) -> anyhow::Result<Value> {
    let Some(ai) = d.ai.as_deref() else {
        return Ok(json!({ "skipped": "ai_disabled" }));
    };
    let batches = ai.batches_enabled();
    let mut sync_reviews = 0u32;
    let now = (d.now)();
    let week_start = reviewed_week(now);
    let mut planned = Vec::new();
    let mut due = d.weekly.due(week_start);
    while let Some(next) = due.next().await {
        let (account_id, tc, raw) = match next {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "weekly provider error");
                continue;
            }
        };
        let mut input = match serde_json::from_value::<WeeklyReviewInput>(raw.clone()) {
            Ok(i) if i.check_bounds().is_ok() => i,
            _ => {
                tracing::warn!(account = %account_id, "invalid weekly review input");
                continue;
            }
        };
        if input.time_class != tc || input.week_start != week_start {
            tracing::error!(account = %account_id, "weekly input names another time class or week");
            continue;
        }
        let plan = d.entitlements.plan(account_id).await;
        if !d.within_budget(account_id, plan, now).await {
            continue;
        }
        // ADR 0094: the verified owner's plan, the review's one reader (ADR-FIX-N-1).
        let changes = input.changes.clone();
        input.plan = crate::plan::owner_plan(&d.pool, account_id, tc, now, &|k| {
            changes
                .iter()
                .find(|c| c.series_key == k)
                .map(|c| c.label.clone())
        })
        .await;
        let raw = serde_json::to_value(&input).unwrap_or(raw);
        if !batches {
            if request_weekly_review(&d.pool, &d.queue, account_id, tc, week_start, &input)
                .await?
                .is_some()
            {
                sync_reviews += 1;
            }
            continue;
        }
        let mut conn = d.pool.acquire().await?;
        let Some(row) = weekly::insert_weekly_if_absent(
            &mut conn,
            &NewWeekly {
                account_id,
                time_class: tc.to_string(),
                week_start,
                mode: ReportMode::Batch,
                input: raw,
            },
        )
        .await?
        else {
            continue;
        };
        let id = Uuid::new_v4();
        let item = weekly_batch_item(&input, id);
        planned.push(Planned {
            items: vec![(
                NewBatchItem {
                    id,
                    custom_id: item.custom_id.clone(),
                    kind: BatchKind::WeeklyReview,
                    report_run_id: None,
                    section: None,
                    weekly_review_id: Some(row.id),
                },
                item,
            )],
            run_id: None,
        });
    }
    if !batches {
        return Ok(json!({ "path": "sync", "reviews": sync_reviews }));
    }
    let reviews = planned.len();
    let submitted = submit_all(d, ai, BatchKind::WeeklyReview, planned).await?;
    Ok(json!({ "path": "batch", "reviews": reviews, "batches": submitted }))
}

/// Packs whole groups into batches within [`MAX_BATCH_REQUESTS`] and [`MAX_BATCH_BYTES`], in
/// order (a group too large on its own goes alone).
fn pack(groups: Vec<Planned>) -> Vec<Vec<Planned>> {
    let mut out: Vec<Vec<Planned>> = Vec::new();
    let mut current: Vec<Planned> = Vec::new();
    let (mut n, mut bytes) = (0usize, 0usize);
    for g in groups {
        let items: Vec<BatchItem> = g.items.iter().map(|(_, i)| i.clone()).collect();
        // The group's entries plus their separating commas, without the body frame.
        let size = batch_request_bytes(&items).saturating_sub(batch_request_bytes(&[])) + 1;
        if !current.is_empty()
            && (n + items.len() > MAX_BATCH_REQUESTS || bytes + size > MAX_BATCH_BYTES)
        {
            out.push(std::mem::take(&mut current));
            (n, bytes) = (0, 0);
        }
        n += items.len();
        bytes += size;
        current.push(g);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Submits each pack and records it. Returns how many batches were submitted.
async fn submit_all(
    d: &BatchDeps,
    ai: &dyn AiClient,
    kind: BatchKind,
    planned: Vec<Planned>,
) -> anyhow::Result<usize> {
    let mut submitted = 0;
    for chunk in pack(planned) {
        let items: Vec<BatchItem> = chunk
            .iter()
            .flat_map(|g| g.items.iter().map(|(_, i)| i.clone()))
            .collect();
        let id = match tokio::time::timeout(BATCH_SUBMIT_TIMEOUT, ai.batch_submit(&items)).await {
            Ok(Ok(id)) => id,
            Ok(Err(e)) => {
                // The rows stay in batch mode; past the grace the reconciler generates them.
                tracing::warn!(error = %e, requests = items.len(), "batch submit failed");
                continue;
            }
            Err(_) => {
                tracing::warn!(requests = items.len(), "batch submit timed out");
                continue;
            }
        };
        let rows: Vec<NewBatchItem> = chunk
            .iter()
            .flat_map(|g| g.items.iter().map(|(r, _)| r.clone()))
            .collect();
        let mut tx = d.pool.begin().await?;
        batches::insert_batch(
            &mut tx,
            &id,
            kind,
            i32::try_from(items.len()).unwrap_or(i32::MAX),
        )
        .await?;
        batches::insert_items(&mut tx, &id, &rows).await?;
        for run_id in chunk.iter().filter_map(|g| g.run_id) {
            if !runs::set_awaiting_batch(&mut tx, run_id).await? {
                tracing::warn!(run = %run_id, "scheduled run left queued during its submit");
            }
        }
        if let Err(e) = tx.commit().await {
            tracing::error!(batch = %id, error = %e, "submitted batch could not be recorded");
            return Err(e.into());
        }
        submitted += 1;
    }
    Ok(submitted)
}

// -------------------------------------------------------------------------------------------
// Polling.

#[derive(Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PollCounts {
    batches: usize,
    applied: usize,
    retried: usize,
    ignored: usize,
    processed: usize,
    given_up: usize,
}

async fn poll(d: &BatchDeps) -> anyhow::Result<Value> {
    let mut counts = PollCounts::default();
    let Some(ai) = d.ai.as_deref() else {
        return Ok(json!({ "skipped": "no_ai" }));
    };
    let open = {
        let mut conn = d.pool.acquire().await?;
        batches::open_batches(&mut conn).await?
    };
    let mut first_error = None;
    for b in open {
        counts.batches += 1;
        if let Err(e) = poll_one(d, ai, &b, &mut counts).await {
            tracing::warn!(batch = %b.id, error = %e, "batch poll failed");
            first_error.get_or_insert(e);
        }
    }
    if let Some(e) = first_error {
        return Err(e);
    }
    Ok(json!(counts))
}

/// Polls one batch under its advisory lock (a concurrent poller skips it). The lock is
/// transaction-scoped on a transaction held for the poll and never committed, so it is released
/// on every path: the end of the poll, an error, or a cancelled (dropped) poll, whose
/// transaction sqlx rolls back when the connection returns to the pool.
async fn poll_one(
    d: &BatchDeps,
    ai: &dyn AiClient,
    b: &BatchRow,
    counts: &mut PollCounts,
) -> anyhow::Result<()> {
    let mut lock = d.pool.begin().await?;
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1, hashtext($2))")
        .bind(POLL_LOCK_CLASS)
        .bind(&b.id)
        .fetch_one(&mut *lock)
        .await?;
    if !locked {
        return Ok(());
    }
    // Re-read under the lock: another poller may have just finished this batch.
    let current = {
        let mut conn = d.pool.acquire().await?;
        batches::get_batch(&mut conn, &b.id).await?
    };
    let res = match current {
        Some(b) if matches!(b.status, DbBatchStatus::Submitted | DbBatchStatus::Ended) => {
            poll_locked(d, ai, &b, counts).await
        }
        _ => Ok(()),
    };
    lock.rollback().await?;
    res
}

/// Every unprocessed item to the sync path, and the batch `failed`.
async fn give_up(d: &BatchDeps, b: &BatchRow, counts: &mut PollCounts) -> anyhow::Result<()> {
    for item in pending(d, &b.id).await? {
        fall_back(d, &item).await?;
        counts.retried += 1;
    }
    let mut conn = d.pool.acquire().await?;
    batches::mark_failed(&mut conn, &b.id).await?;
    counts.given_up += 1;
    Ok(())
}

async fn poll_locked(
    d: &BatchDeps,
    ai: &dyn AiClient,
    b: &BatchRow,
    counts: &mut PollCounts,
) -> anyhow::Result<()> {
    let status = tokio::time::timeout(BATCH_STATUS_TIMEOUT, ai.batch_status(&b.id)).await;
    let now = (d.now)();
    {
        let mut conn = d.pool.acquire().await?;
        batches::mark_polled(&mut conn, &b.id).await?;
        if matches!(status, Ok(Ok(BatchStatus::Ended))) && b.status == DbBatchStatus::Submitted {
            // `ended_at` starts the bound on reading the results.
            batches::mark_ended(&mut conn, &b.id).await?;
        }
    }
    match status {
        Ok(Ok(BatchStatus::Ended)) => {}
        other => {
            if let Ok(Err(e)) = &other {
                tracing::warn!(batch = %b.id, error = %e, "batch status failed");
            }
            if now - b.submitted_at > BATCH_GIVE_UP_AFTER {
                tracing::warn!(batch = %b.id, "batch never ended; giving it up to the sync path");
                give_up(d, b, counts).await?;
            }
            return Ok(());
        }
    }

    let resolver = RegistryResolver;
    let mut results = ai.batch_results(&b.id, &resolver);
    let deadline = tokio::time::Instant::now() + BATCH_RESULTS_TIMEOUT;
    let complete = loop {
        match tokio::time::timeout_at(deadline, results.next()).await {
            Err(_) => {
                tracing::warn!(batch = %b.id, "batch results timed out; resuming next poll");
                break false;
            }
            Ok(None) => break true,
            // One line that cannot be decoded (or is too long): the client skips it and goes
            // on. Its item gets no result, so it falls back to sync once the stream completes.
            Ok(Some(Err(AiError::Response(e)))) => {
                tracing::warn!(batch = %b.id, error = %e, "undecodable batch result line skipped");
                counts.ignored += 1;
            }
            Ok(Some(Err(e))) => {
                tracing::warn!(batch = %b.id, error = %e, "batch results stream failed; resuming next poll");
                break false;
            }
            Ok(Some(Ok(r))) => match apply_result(d, ai, b, r).await? {
                Applied::Stored => counts.applied += 1,
                Applied::Retry => counts.retried += 1,
                Applied::Ignored => counts.ignored += 1,
            },
        }
    };
    drop(results);
    if !complete {
        // A results endpoint that keeps failing is bounded too.
        let ended = b.ended_at.unwrap_or(now);
        if now - ended > BATCH_RESULTS_GIVE_UP_AFTER {
            tracing::warn!(batch = %b.id, "batch results unreadable; giving it up to the sync path");
            give_up(d, b, counts).await?;
        }
        return Ok(());
    }
    // A complete stream that left an item without a result: that item goes to the sync path.
    for item in pending(d, &b.id).await? {
        tracing::warn!(batch = %b.id, item = %item.id, "no result for the item");
        fall_back(d, &item).await?;
        counts.retried += 1;
    }
    let mut conn = d.pool.acquire().await?;
    if batches::pending_items(&mut conn, &b.id).await?.is_empty()
        && batches::mark_processed(&mut conn, &b.id).await?
    {
        counts.processed += 1;
    }
    Ok(())
}

async fn pending(d: &BatchDeps, batch_id: &str) -> anyhow::Result<Vec<BatchItemRow>> {
    let mut conn = d.pool.acquire().await?;
    Ok(batches::pending_items(&mut conn, batch_id).await?)
}

enum Applied {
    Stored,
    Retry,
    Ignored,
}

/// The billed records of a result (a succeeded one carries usage; nothing else is billed).
fn billed(r: &BatchResultItem) -> Vec<CallRecord> {
    match (&r.result, r.outcome) {
        (Some(res), BatchOutcome::Succeeded) => CallRecord::from_result(res, true),
        _ => Vec::new(),
    }
}

/// The metering key prefix of one batch item: every call of its result (the batch call, a sync
/// repair) is metered once under it.
fn usage_key(item: &BatchItemRow) -> String {
    format!("batch:{}", item.id)
}

/// Meters the sync repairs among `calls` (every record after the first `n_billed`, which are the
/// batch result's) right away, in their own transaction, under a key unique to this attempt, and
/// returns the batch records for the item's transaction. A repair is paid when it returns: if
/// the item's write then fails, the next poll repairs again, and both repairs are really paid
/// (T20 parked item). The batch records stay keyed by the item, so they are metered once.
#[allow(clippy::too_many_arguments)]
async fn meter_repairs_now(
    d: &BatchDeps,
    item: &BatchItemRow,
    account_id: Uuid,
    feature: AiFeature,
    model: AiModel,
    ref_id: Uuid,
    mut calls: Vec<CallRecord>,
    n_billed: usize,
) -> anyhow::Result<Vec<CallRecord>> {
    let repairs = calls.split_off(n_billed.min(calls.len()));
    if !repairs.is_empty() {
        let mut tx = d.pool.begin().await?;
        meter_keyed(
            &mut tx,
            &d.pricing,
            account_id,
            feature,
            &repairs,
            &ref_id.to_string(),
            model,
            &format!("{}:repair:{}", usage_key(item), Uuid::new_v4()),
        )
        .await?;
        tx.commit().await?;
    }
    Ok(calls)
}

/// Resolves `r` to its item of batch `b` and applies it there.
async fn apply_result(
    d: &BatchDeps,
    ai: &dyn AiClient,
    b: &BatchRow,
    r: BatchResultItem,
) -> anyhow::Result<Applied> {
    let Some(item_id) = batch_item_id(&r.custom_id) else {
        tracing::error!(batch = %b.id, "result with a foreign custom id ignored");
        return Ok(Applied::Ignored);
    };
    let row = {
        let mut conn = d.pool.acquire().await?;
        batches::item_by_custom_id(&mut conn, &r.custom_id).await?
    };
    let Some(item) = row.filter(|i| i.id == item_id && i.batch_id == b.id) else {
        tracing::error!(batch = %b.id, item = %item_id, "result for an item outside this batch ignored");
        return Ok(Applied::Ignored);
    };
    if item.processed_at.is_some() {
        return Ok(Applied::Ignored);
    }
    match item.kind {
        BatchKind::Report => apply_report(d, ai, &item, r).await,
        BatchKind::WeeklyReview => apply_weekly(d, ai, &item, r).await,
    }
}

/// Meters `calls` and marks `item` processed, writing nothing else (the item's owner is gone or
/// no longer waiting for this batch).
async fn meter_only(
    d: &BatchDeps,
    item: &BatchItemRow,
    account_id: Uuid,
    feature: AiFeature,
    calls: &[CallRecord],
    ref_id: Uuid,
) -> anyhow::Result<Applied> {
    let mut tx = d.pool.begin().await?;
    if batches::mark_item_processed(&mut tx, item.id).await? {
        let model = if feature == AiFeature::Report {
            Models::REPORT
        } else {
            Models::WEEKLY
        };
        meter_keyed(
            &mut tx,
            &d.pricing,
            account_id,
            feature,
            calls,
            &ref_id.to_string(),
            model,
            &usage_key(item),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Applied::Ignored)
}

/// The run of a report item, when it is still waiting for this batch, with its validated input.
struct WaitingRun {
    run_id: Uuid,
    account_id: Uuid,
    report_id: Uuid,
    is_first_report: bool,
    input: Option<CoachingInput>,
}

async fn waiting_run(
    d: &BatchDeps,
    run_id: Uuid,
) -> anyhow::Result<Option<(Uuid, Option<WaitingRun>)>> {
    let mut conn = d.pool.acquire().await?;
    let Some(run) = runs::get_run(&mut conn, run_id).await? else {
        return Ok(None);
    };
    let waiting = run.status == RunStatus::AwaitingBatch && run.mode == ReportMode::Batch;
    let (Some(report_id), true) = (run.report_id, waiting) else {
        return Ok(Some((run.account_id, None)));
    };
    let input = run
        .input
        .as_ref()
        .and_then(|raw| parse_coaching_input(raw).ok())
        .filter(|i| i.account_id == run.account_id);
    Ok(Some((
        run.account_id,
        Some(WaitingRun {
            run_id,
            account_id: run.account_id,
            report_id,
            is_first_report: run.is_first_report,
            input,
        }),
    )))
}

async fn apply_report(
    d: &BatchDeps,
    ai: &dyn AiClient,
    item: &BatchItemRow,
    r: BatchResultItem,
) -> anyhow::Result<Applied> {
    let (Some(run_id), Some(section)) = (item.report_run_id, item.section) else {
        tracing::error!(item = %item.id, "report item without a run or section");
        let mut conn = d.pool.acquire().await?;
        batches::mark_item_processed(&mut conn, item.id).await?;
        return Ok(Applied::Ignored);
    };
    let Some((account_id, run)) = waiting_run(d, run_id).await? else {
        return Ok(Applied::Ignored);
    };
    let Some(run) = run else {
        return meter_only(d, item, account_id, AiFeature::Report, &billed(&r), run_id).await;
    };
    let n_billed = billed(&r).len();
    let (outcome, calls) = match &run.input {
        None => {
            tracing::error!(run = %run_id, "the run's input snapshot is unusable");
            (None, billed(&r))
        }
        Some(input) => {
            let now = (d.now)();
            let plan = d.entitlements.plan(account_id).await;
            let scope = d.entitlements.budget_scope(account_id).await;
            let guard = budget_guard(d.pool.clone(), scope, plan, d.budgets, now);
            match process_report_result(ai, &guard, r, input, section, run.is_first_report).await {
                Ok(BatchProcessed::Outcome(o)) => {
                    let calls = o.calls.clone();
                    (Some(*o), calls)
                }
                Ok(BatchProcessed::RetrySync { calls }) => (None, calls),
                Err(SectionError { error, calls }) => {
                    tracing::warn!(run = %run_id, error = %error, "batch repair failed; sync path");
                    (None, calls)
                }
            }
        }
    };
    let calls = meter_repairs_now(
        d,
        item,
        account_id,
        AiFeature::Report,
        Models::REPORT,
        run_id,
        calls,
        n_billed,
    )
    .await?;
    let applied = outcome.is_some();
    let now = (d.now)();
    let mut tx = d.pool.begin().await?;
    lock_run(&mut tx, run_id).await?;
    if !batches::mark_item_processed(&mut tx, item.id).await? {
        return Ok(Applied::Ignored);
    }
    meter_keyed(
        &mut tx,
        &d.pricing,
        account_id,
        AiFeature::Report,
        &calls,
        &run_id.to_string(),
        Models::REPORT,
        &usage_key(item),
    )
    .await?;
    let labels = match &outcome {
        Some(o) => persist_section_outcome(&mut tx, run.report_id, o, now).await?,
        None => Vec::new(),
    };
    let fin = finish_run_if_complete(&mut tx, &run, now).await?;
    for section in labels {
        events::emit(
            &mut tx,
            account_id,
            REPORT_SECTION_READY,
            &SectionReady {
                report_id: run.report_id,
                report_run_id: run_id,
                section,
                narrative: outcome.as_ref().is_some_and(is_narrative),
            },
        )
        .await?;
    }
    emit_finish(&mut tx, &run, fin).await?;
    tx.commit().await?;
    after_finish(d, &run, fin).await;
    Ok(if applied {
        Applied::Stored
    } else {
        Applied::Retry
    })
}

/// Serialises writers of one run (the item fence and the finish check run under it).
async fn lock_run(conn: &mut PgConnection, run_id: Uuid) -> sqlx::Result<()> {
    sqlx::query("SELECT 1 FROM report_runs WHERE id = $1 FOR UPDATE")
        .bind(run_id)
        .execute(conn)
        .await?;
    Ok(())
}

#[derive(Clone, Copy)]
enum RunFinish {
    /// Items of the run are still open.
    Waiting,
    Done(ReportStatus),
    /// Some section has no stored outcome: the sync job generates it.
    Requeued,
}

/// Finishes the run once its last item is processed (on the caller's transaction, under the
/// run lock).
async fn finish_run_if_complete(
    conn: &mut PgConnection,
    run: &WaitingRun,
    now: OffsetDateTime,
) -> anyhow::Result<RunFinish> {
    let open: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ai_batch_items WHERE report_run_id = $1 AND processed_at IS NULL",
    )
    .bind(run.run_id)
    .fetch_one(&mut *conn)
    .await?;
    if open > 0 {
        return Ok(RunFinish::Waiting);
    }
    let stored = reports::sections(conn, run.report_id).await?;
    let complete = SECTION_KEYS
        .iter()
        .all(|k| stored.iter().any(|s| s.section == *k));
    if complete {
        if !runs::set_done(conn, run.run_id, Some(run.report_id)).await? {
            return Ok(RunFinish::Waiting);
        }
        let status = refresh_status(conn, run.report_id, now).await?;
        return Ok(RunFinish::Done(status));
    }
    let r = sqlx::query(
        "UPDATE report_runs SET status = 'queued' WHERE id = $1 AND status = 'awaiting_batch'",
    )
    .bind(run.run_id)
    .execute(&mut *conn)
    .await?;
    Ok(if r.rows_affected() == 1 {
        RunFinish::Requeued
    } else {
        RunFinish::Waiting
    })
}

async fn emit_finish(
    conn: &mut PgConnection,
    run: &WaitingRun,
    fin: RunFinish,
) -> Result<(), events::EmitError> {
    if let RunFinish::Done(status) = fin {
        events::emit(
            conn,
            run.account_id,
            REPORT_READY,
            &ReportReady {
                report_id: run.report_id,
                status,
            },
        )
        .await?;
    }
    Ok(())
}

/// After the commit: a requeued run gets its sync job (a lost enqueue is repaired by
/// `ai-reconcile`, which finds the `queued` run).
async fn after_finish(d: &BatchDeps, run: &WaitingRun, fin: RunFinish) {
    if let RunFinish::Requeued = fin
        && let Err(e) = kinds::enqueue_report(&d.queue, run.run_id, run.account_id).await
    {
        tracing::warn!(run = %run.run_id, error = %e, "generate-report enqueue failed");
    }
}

fn weekly_is_empty(o: &WeeklyOutcome) -> bool {
    let c = &o.content;
    c.headline.is_none()
        && c.process.is_none()
        && c.outcomes.is_empty()
        && c.exemplars.is_empty()
        && c.next_week.is_none()
}

async fn apply_weekly(
    d: &BatchDeps,
    ai: &dyn AiClient,
    item: &BatchItemRow,
    r: BatchResultItem,
) -> anyhow::Result<Applied> {
    let Some(weekly_id) = item.weekly_review_id else {
        tracing::error!(item = %item.id, "weekly item without a review");
        let mut conn = d.pool.acquire().await?;
        batches::mark_item_processed(&mut conn, item.id).await?;
        return Ok(Applied::Ignored);
    };
    let row = {
        let mut conn = d.pool.acquire().await?;
        weekly::get_weekly(&mut conn, weekly_id).await?
    };
    let Some(row) = row else {
        return Ok(Applied::Ignored);
    };
    let account_id = row.account_id;
    if row.status != ItemStatus::Pending || row.mode != ReportMode::Batch {
        return meter_only(
            d,
            item,
            account_id,
            AiFeature::WeeklyReview,
            &billed(&r),
            weekly_id,
        )
        .await;
    }
    let input = serde_json::from_value::<WeeklyReviewInput>(row.input.clone())
        .ok()
        .filter(|i| i.check_bounds().is_ok() && i.week_start == row.week_start);
    let n_billed = billed(&r).len();
    let (outcome, calls) = match &input {
        None => {
            tracing::error!(weekly = %weekly_id, "the review's input is unusable");
            (None, billed(&r))
        }
        Some(input) => {
            let now = (d.now)();
            let plan = d.entitlements.plan(account_id).await;
            let scope = d.entitlements.budget_scope(account_id).await;
            let guard = budget_guard(d.pool.clone(), scope, plan, d.budgets, now);
            match process_weekly_result(ai, &guard, r, input).await {
                Ok(WeeklyProcessed::Outcome(o)) => {
                    let calls = o.calls.clone();
                    (Some(*o), calls)
                }
                Ok(WeeklyProcessed::RetrySync { calls }) => (None, calls),
                Err(SectionError { error, calls }) => {
                    tracing::warn!(weekly = %weekly_id, error = %error, "batch repair failed; sync path");
                    (None, calls)
                }
            }
        }
    };
    let calls = meter_repairs_now(
        d,
        item,
        account_id,
        AiFeature::WeeklyReview,
        Models::WEEKLY,
        weekly_id,
        calls,
        n_billed,
    )
    .await?;
    let now = (d.now)();
    let mut tx = d.pool.begin().await?;
    sqlx::query("SELECT 1 FROM weekly_reviews WHERE id = $1 FOR UPDATE")
        .bind(weekly_id)
        .execute(&mut *tx)
        .await?;
    if !batches::mark_item_processed(&mut tx, item.id).await? {
        return Ok(Applied::Ignored);
    }
    meter_keyed(
        &mut tx,
        &d.pricing,
        account_id,
        AiFeature::WeeklyReview,
        &calls,
        &weekly_id.to_string(),
        Models::WEEKLY,
        &usage_key(item),
    )
    .await?;
    let Some(o) = outcome else {
        tx.commit().await?;
        enqueue_weekly(d, weekly_id, account_id).await;
        return Ok(Applied::Retry);
    };
    if weekly_is_empty(&o) {
        weekly::set_weekly_dropped(
            &mut tx,
            weekly_id,
            &serde_json::to_value(&o.dropped)?,
            Models::WEEKLY.as_str(),
        )
        .await?;
    } else {
        let ready = weekly::set_weekly_ready(
            &mut tx,
            weekly_id,
            &WeeklyContent {
                content: serde_json::to_value(&o.content)?,
                dropped: serde_json::to_value(&o.dropped)?,
                model: Models::WEEKLY.as_str().to_owned(),
                prompt_hash: Some(prompt_set_hash(WEEKLY_PROMPT_SET)),
                ai_disclosure: serde_json::to_value(make_disclosure(
                    Models::WEEKLY.as_str(),
                    WEEKLY_PROMPT_VERSION,
                    now,
                ))?,
            },
        )
        .await?;
        if ready {
            events::emit(
                &mut tx,
                account_id,
                WEEKLY_REVIEW_READY,
                &WeeklyReady { id: weekly_id },
            )
            .await?;
        }
    }
    tx.commit().await?;
    Ok(Applied::Stored)
}

async fn enqueue_weekly(d: &BatchDeps, weekly_id: Uuid, account_id: Uuid) {
    if let Err(e) = kinds::enqueue_weekly(&d.queue, weekly_id, account_id).await {
        tracing::warn!(weekly = %weekly_id, error = %e, "weekly-review enqueue failed");
    }
}

/// An item that will get no usable result (none in a complete stream, or its batch was given
/// up): processed with nothing billed, and its run or review handed to the sync path.
async fn fall_back(d: &BatchDeps, item: &BatchItemRow) -> anyhow::Result<()> {
    match item.kind {
        BatchKind::Report => {
            let Some(run_id) = item.report_run_id else {
                let mut conn = d.pool.acquire().await?;
                batches::mark_item_processed(&mut conn, item.id).await?;
                return Ok(());
            };
            let run = waiting_run(d, run_id).await?.and_then(|(_, r)| r);
            let mut tx = d.pool.begin().await?;
            lock_run(&mut tx, run_id).await?;
            if !batches::mark_item_processed(&mut tx, item.id).await? {
                return Ok(());
            }
            let Some(run) = run else {
                tx.commit().await?;
                return Ok(());
            };
            let fin = finish_run_if_complete(&mut tx, &run, (d.now)()).await?;
            emit_finish(&mut tx, &run, fin).await?;
            tx.commit().await?;
            after_finish(d, &run, fin).await;
        }
        BatchKind::WeeklyReview => {
            let mut conn = d.pool.acquire().await?;
            if !batches::mark_item_processed(&mut conn, item.id).await? {
                return Ok(());
            }
            let Some(id) = item.weekly_review_id else {
                return Ok(());
            };
            if let Some(w) = weekly::get_weekly(&mut conn, id).await?
                && w.status == ItemStatus::Pending
            {
                enqueue_weekly(d, id, w.account_id).await;
            }
        }
    }
    Ok(())
}

// -------------------------------------------------------------------------------------------
// Handlers.

async fn run_handler(
    ctx: JobContext,
    work: impl std::future::Future<Output = anyhow::Result<Value>>,
) -> Result<Value, JobError> {
    tokio::select! {
        biased;
        // Every step is fenced and idempotent, so a cut-off pass is safe to drop.
        () = ctx.cancel.cancelled() => Err(JobError::Release {
            delay: Duration::ZERO,
            reason: "shutdown".to_owned(),
        }),
        res = work => res.map_err(|e| JobError::Retryable {
            message: e.to_string(),
            code: None,
        }),
    }
}

/// [`KIND_REPORT_BATCH_SCHEDULE`]: the monthly report regeneration.
#[derive(Clone)]
pub struct ReportBatchScheduleHandler(pub BatchDeps);

impl Handler for ReportBatchScheduleHandler {
    fn kind(&self) -> &'static str {
        KIND_REPORT_BATCH_SCHEDULE
    }

    fn handle(&self, _job: Job, ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        Box::pin(run_handler(ctx, schedule_reports(&self.0)))
    }
}

/// [`KIND_WEEKLY_BATCH_SCHEDULE`]: the weekly reviews of the current week.
#[derive(Clone)]
pub struct WeeklyBatchScheduleHandler(pub BatchDeps);

impl Handler for WeeklyBatchScheduleHandler {
    fn kind(&self) -> &'static str {
        KIND_WEEKLY_BATCH_SCHEDULE
    }

    fn handle(&self, _job: Job, ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        Box::pin(run_handler(ctx, schedule_weekly(&self.0)))
    }
}

/// [`KIND_AI_BATCH_POLL`]: polls the open batches and applies their results.
#[derive(Clone)]
pub struct AiBatchPollHandler(pub BatchDeps);

impl Handler for AiBatchPollHandler {
    fn kind(&self) -> &'static str {
        KIND_AI_BATCH_POLL
    }

    fn handle(&self, _job: Job, ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        Box::pin(run_handler(ctx, poll(&self.0)))
    }
}
