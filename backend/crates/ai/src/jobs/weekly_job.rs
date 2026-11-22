//! The `weekly-review` job (Task 19): one AI weekly review per (account, time class, week), written
//! by Task 15's [`generate_weekly_review`] on the lighter model.
//!
//! - [`request_weekly_review`] inserts the pending row and enqueues its job. A week that already
//!   has a row gives `None`, with no job and no call (idempotent per week).
//! - [`WeeklyReviewHandler`]:
//!   1. A missing row, or one that is no longer `pending`, is skipped (`Ok {skipped}`), with no
//!      call. A batch-mode row is accepted and run synchronously (T21: the reconciler hands
//!      over a batch review whose batch failed).
//!   2. An input that does not parse fails the row (`invalid_input`); no AI (`ai == None`) fails
//!      it with `ai_disabled`. Both emit `job.failed`.
//!   3. The budget guard runs before every model call (scope and plan from `Entitlements`).
//!   4. **Success** stores the validated content, the web copy of `dropped` (codes and paths,
//!      never model text) and the disclosure, then meters every call and emits
//!      `weekly_review.ready`, in one transaction. A review with nothing usable is stored
//!      `dropped` (no content) with the same event; the row status tells the web which.
//!   5. **Errors** meter the paid calls in the same transaction as the state change.
//!      `Unavailable` records the retry on the row (`retry_count`, `next_attempt_at`) and
//!      returns `Release` with [`retry_delay`]; once the schedule is exhausted the row fails
//!      (`ai_unavailable`). `Budget` fails it (`budget`), `Auth` (`ai_auth`), anything else
//!      (`ai_error`). Every failure emits `job.failed {kind, reason}`.
//!   6. A database error after the calls meters them in their own transaction and returns
//!      `Retryable`. Metering is keyed by row and delivery (`lease_token`), so a re-meter adds
//!      nothing and a new delivery's calls (paid again) are new rows.
//!   7. A cancel (`ctx.cancel`: hard timeout, lost lease, shutdown) stops the generation, meters
//!      the calls already returned (within `CANCEL_GRACE`), leaves the row `pending` with its
//!      retry count unchanged and returns `Release { delay: 0 }`.
//!
//! Events go through account-level `emit`, not `emit_for_user`, on purpose: weekly reviews and
//! change notes belong to the linked account (every user linked to it sees them), and the events
//! carry only ids, keys, statuses and reasons, never review or note text. The T25 user-owned rule
//! is for chat, whose threads and text belong to one user. A failure reason (`ai_disabled`,
//! `budget`, ...) is only in `job.failed`: the tables have no error column.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::future::BoxFuture;
use futures::stream::BoxStream;
use queue::queue::{Queue, QueueError};
use queue::runner::{Handler, JobContext};
use queue::types::{Job, JobError};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::client::cost::PricingTable;
use crate::client::model::{AiModel, Models};
use crate::client::port::{
    AiClient, AiError, BatchItem, BatchResultItem, BatchStatus, ChatTurnRequest, SchemaResolver,
    StructuredRequest, StructuredResult, ToolHandler, TurnEvent,
};
use crate::db::ItemStatus;
use crate::db::runs::ReportMode;
use crate::db::usage::AiFeature;
use crate::db::weekly::{self, NewWeekly, WeeklyContent};
use crate::entitlements::{Entitlements, Plan};
use crate::events::{EmitError, JOB_FAILED, JobFailed, WEEKLY_REVIEW_READY, WeeklyReady, emit};
use crate::generate::section::{CallRecord, SectionError};
use crate::generate::weekly::{WeeklyOutcome, generate_weekly_review};
use crate::input::types::TimeClass;
use crate::jobs::budget::{BudgetConfig, budget_guard, meter_keyed};
use crate::jobs::kinds::{KIND_WEEKLY_REVIEW, WeeklyReviewPayload, enqueue_weekly};
use crate::jobs::report_job::CANCEL_GRACE;
use crate::jobs::retry::retry_delay;
use crate::output::disclosure::make_disclosure;
use crate::output::weekly::{ValidatedWeeklyReview, WeeklyReviewInput};
use crate::prompts::{WEEKLY_PROMPT_SET, WEEKLY_PROMPT_VERSION, prompt_set_hash};

/// What the weekly-review and change-note handlers need.
#[derive(Clone)]
pub struct AiItemDeps {
    pub pool: PgPool,
    pub queue: Queue,
    /// `None` when no AI is configured: items fail with `ai_disabled`.
    pub ai: Option<Arc<dyn AiClient>>,
    pub entitlements: Arc<dyn Entitlements>,
    pub budgets: BudgetConfig,
    pub pricing: Arc<PricingTable>,
    pub now: Arc<dyn Fn() -> OffsetDateTime + Send + Sync>,
}

/// The `job.failed` reasons of weekly reviews and change notes.
pub const REASON_AI_DISABLED: &str = "ai_disabled";
pub const REASON_INVALID_INPUT: &str = "invalid_input";
pub const REASON_AI_UNAVAILABLE: &str = "ai_unavailable";
pub const REASON_BUDGET: &str = "budget";
pub const REASON_AI_AUTH: &str = "ai_auth";
pub const REASON_AI_ERROR: &str = "ai_error";
pub const REASON_JOB_DEAD: &str = "job_dead";

#[derive(Debug, thiserror::Error)]
pub(crate) enum TxError {
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Emit(#[from] EmitError),
}

pub(crate) fn retryable(what: &str, e: impl std::fmt::Display) -> JobError {
    JobError::Retryable {
        message: format!("{what}: {e}"),
        code: None,
    }
}

pub(crate) fn skipped(why: &str) -> Value {
    json!({ "skipped": why })
}

/// The terminal reason of a client error other than `Unavailable`.
pub(crate) fn failure_reason(e: &AiError) -> &'static str {
    match e {
        AiError::Unavailable { .. } => REASON_AI_UNAVAILABLE,
        AiError::Budget => REASON_BUDGET,
        AiError::Auth { .. } => REASON_AI_AUTH,
        _ => REASON_AI_ERROR,
    }
}

/// The plan and the budget scope of `account_id` (always including the account itself).
pub(crate) async fn plan_and_scope(d: &AiItemDeps, account_id: Uuid) -> (Plan, Vec<Uuid>) {
    let plan = d.entitlements.plan(account_id).await;
    let mut scope = d.entitlements.budget_scope(account_id).await;
    if !scope.contains(&account_id) {
        scope.push(account_id);
    }
    scope.sort();
    scope.dedup();
    (plan, scope)
}

/// The model the calls ran on (the last response's), else the requested one.
pub(crate) fn model_of(calls: &[CallRecord], requested: AiModel) -> String {
    calls
        .last()
        .map_or_else(|| requested.as_str().to_owned(), |c| c.model.clone())
}

/// What one metering needs: the calls of one delivery of one row.
pub(crate) struct Metering<'a> {
    pub account_id: Uuid,
    pub feature: AiFeature,
    pub ref_id: String,
    pub model: AiModel,
    pub key: String,
    pub calls: &'a [CallRecord],
}

impl Metering<'_> {
    pub(crate) async fn on(
        &self,
        conn: &mut PgConnection,
        pricing: &PricingTable,
    ) -> sqlx::Result<()> {
        meter_keyed(
            conn,
            pricing,
            self.account_id,
            self.feature,
            self.calls,
            &self.ref_id,
            self.model,
            &self.key,
        )
        .await
    }

    /// Meters in its own transaction (after the state transaction failed). Keyed, so calls
    /// the failed transaction may have metered are not counted twice.
    pub(crate) async fn alone(&self, pool: &PgPool, pricing: &PricingTable) {
        if self.calls.is_empty() {
            return;
        }
        let r = async {
            let mut tx = pool.begin().await?;
            self.on(&mut tx, pricing).await?;
            tx.commit().await
        }
        .await;
        if let Err(e) = r {
            tracing::error!(error = %e, feature = self.feature.as_str(), "metering AI calls failed");
        }
    }
}

/// The item's client: every structured result is recorded the moment the client returns it, so
/// a handler cancelled mid-generation still meters the calls already paid (a request in flight
/// at the cancel reports no usage, as in the report job).
pub(crate) struct RecordingClient {
    inner: Arc<dyn AiClient>,
    calls: Mutex<Vec<CallRecord>>,
}

impl RecordingClient {
    pub(crate) fn new(inner: Arc<dyn AiClient>) -> RecordingClient {
        RecordingClient {
            inner,
            calls: Mutex::default(),
        }
    }

    pub(crate) fn calls(&self) -> Vec<CallRecord> {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl AiClient for RecordingClient {
    fn structured<'a>(
        &'a self,
        req: &'a StructuredRequest,
    ) -> BoxFuture<'a, Result<StructuredResult, AiError>> {
        Box::pin(async move {
            let r = self.inner.structured(req).await;
            if let Ok(res) = &r {
                self.calls
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .extend(CallRecord::from_result(res, false));
            }
            r
        })
    }

    fn stream_turn<'a>(
        &'a self,
        req: &'a ChatTurnRequest,
        tools: Option<Arc<dyn ToolHandler>>,
    ) -> BoxStream<'a, Result<TurnEvent, AiError>> {
        self.inner.stream_turn(req, tools)
    }

    fn batches_enabled(&self) -> bool {
        self.inner.batches_enabled()
    }

    fn batch_submit<'a>(
        &'a self,
        items: &'a [BatchItem],
    ) -> BoxFuture<'a, Result<String, AiError>> {
        self.inner.batch_submit(items)
    }

    fn batch_status<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<BatchStatus, AiError>> {
        self.inner.batch_status(id)
    }

    fn batch_results<'a>(
        &'a self,
        id: &'a str,
        resolver: &'a dyn SchemaResolver,
    ) -> BoxStream<'a, Result<BatchResultItem, AiError>> {
        self.inner.batch_results(id, resolver)
    }
}

/// A cancelled handler: meters what was paid (bounded by [`CANCEL_GRACE`]), leaves the row
/// `pending` (a cancel is not an AI retry) and releases the job at once for the next delivery.
pub(crate) async fn cancelled(d: &AiItemDeps, m: &Metering<'_>) -> JobError {
    if tokio::time::timeout(CANCEL_GRACE, m.alone(&d.pool, &d.pricing))
        .await
        .is_err()
    {
        tracing::error!(
            feature = m.feature.as_str(),
            "paid AI calls not metered (cancel timeout)"
        );
    }
    JobError::Release {
        delay: Duration::ZERO,
        reason: "cancelled".to_owned(),
    }
}

/// Inserts the pending review of (account, time class, week) and enqueues its job; `None` when
/// that week already has a review (no job, no call). `input` must be for that week and time
/// class.
pub async fn request_weekly_review(
    pool: &PgPool,
    queue: &Queue,
    account_id: Uuid,
    tc: TimeClass,
    week_start: Date,
    input: &WeeklyReviewInput,
) -> anyhow::Result<Option<Uuid>> {
    anyhow::ensure!(
        input.week_start == week_start,
        "weekly input is for week {}, not {week_start}",
        input.week_start
    );
    anyhow::ensure!(
        input.time_class == tc,
        "weekly input is for {}, not {}",
        input.time_class.as_str(),
        tc.as_str()
    );
    let mut conn = pool.acquire().await?;
    let row = weekly::insert_weekly_if_absent(
        &mut conn,
        &NewWeekly {
            account_id,
            time_class: tc.as_str().to_owned(),
            week_start,
            mode: ReportMode::Sync,
            input: serde_json::to_value(input)?,
        },
    )
    .await?;
    drop(conn);
    let Some(row) = row else {
        return Ok(None);
    };
    enqueue_weekly(queue, row.id, account_id)
        .await
        .map_err(|e: QueueError| anyhow::anyhow!("enqueue weekly review: {e}"))?;
    Ok(Some(row.id))
}

/// R1 `Handler` for `weekly-review`.
pub struct WeeklyReviewHandler(pub AiItemDeps);

fn is_empty(r: &ValidatedWeeklyReview) -> bool {
    r.headline.is_none()
        && r.process.is_none()
        && r.outcomes.is_empty()
        && r.exemplars.is_empty()
        && r.next_week.is_none()
}

impl WeeklyReviewHandler {
    async fn fail(
        &self,
        account_id: Uuid,
        id: Uuid,
        reason: &str,
        m: Option<&Metering<'_>>,
    ) -> Result<(), TxError> {
        let d = &self.0;
        let mut tx = d.pool.begin().await?;
        if let Some(m) = m {
            m.on(&mut tx, &d.pricing).await?;
        }
        if weekly::set_weekly_failed(&mut tx, id).await? {
            emit(
                &mut tx,
                account_id,
                JOB_FAILED,
                &JobFailed {
                    kind: KIND_WEEKLY_REVIEW.to_owned(),
                    reason: reason.to_owned(),
                },
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn store(
        &self,
        account_id: Uuid,
        id: Uuid,
        o: &WeeklyOutcome,
        m: &Metering<'_>,
        now: OffsetDateTime,
    ) -> Result<bool, TxError> {
        let d = &self.0;
        let model = model_of(&o.calls, Models::WEEKLY);
        let dropped = serde_json::to_value(o.web_dropped()).map_err(EmitError::from)?;
        let mut tx = d.pool.begin().await?;
        m.on(&mut tx, &d.pricing).await?;
        let changed = if is_empty(&o.content) {
            weekly::set_weekly_dropped(&mut tx, id, &dropped, &model).await?
        } else {
            let content = WeeklyContent {
                content: serde_json::to_value(&o.content).map_err(EmitError::from)?,
                dropped,
                model: model.clone(),
                prompt_hash: Some(prompt_set_hash(WEEKLY_PROMPT_SET)),
                ai_disclosure: serde_json::to_value(make_disclosure(
                    &model,
                    WEEKLY_PROMPT_VERSION,
                    now,
                ))
                .map_err(EmitError::from)?,
            };
            weekly::set_weekly_ready(&mut tx, id, &content).await?
        };
        if changed {
            emit(
                &mut tx,
                account_id,
                WEEKLY_REVIEW_READY,
                &WeeklyReady { id },
            )
            .await?;
        }
        tx.commit().await?;
        Ok(changed)
    }

    async fn retry_later(
        &self,
        id: Uuid,
        count: i32,
        at: OffsetDateTime,
        m: &Metering<'_>,
    ) -> Result<(), TxError> {
        let d = &self.0;
        let mut tx = d.pool.begin().await?;
        m.on(&mut tx, &d.pricing).await?;
        weekly::set_weekly_retry(&mut tx, id, count, at).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn run(&self, job: Job, ctx: JobContext) -> Result<Value, JobError> {
        let d = &self.0;
        let payload: WeeklyReviewPayload =
            serde_json::from_value(job.payload.clone()).map_err(|e| JobError::Unrecoverable {
                message: format!("weekly-review payload: {e}"),
                code: None,
            })?;
        let id = payload.weekly_review_id;
        let row = {
            let mut conn = d.pool.acquire().await.map_err(|e| retryable("db", e))?;
            weekly::get_weekly(&mut conn, id)
                .await
                .map_err(|e| retryable("load weekly review", e))?
        };
        let Some(row) = row else {
            return Ok(skipped("missing"));
        };
        if row.status != ItemStatus::Pending {
            return Ok(skipped(row.status.as_str()));
        }
        let account_id = row.account_id;
        let tx_err = |e: TxError| retryable("weekly review write", e);

        let input = serde_json::from_value::<WeeklyReviewInput>(row.input.clone())
            .ok()
            .filter(|i| i.check_bounds().is_ok());
        let Some(input) = input else {
            self.fail(account_id, id, REASON_INVALID_INPUT, None)
                .await
                .map_err(tx_err)?;
            return Ok(json!({ "failed": REASON_INVALID_INPUT }));
        };
        let Some(ai) = d.ai.clone() else {
            self.fail(account_id, id, REASON_AI_DISABLED, None)
                .await
                .map_err(tx_err)?;
            return Ok(json!({ "failed": REASON_AI_DISABLED }));
        };

        let now = (d.now)();
        let (plan, scope) = plan_and_scope(d, account_id).await;
        let guard = budget_guard(d.pool.clone(), scope, plan, d.budgets, now);
        // The one safe point that matters: the generation (the only paid part) stops at a
        // cancel; the short state transactions that follow always run to the end.
        let rec = RecordingClient::new(ai);
        let result = tokio::select! {
            biased;
            () = ctx.cancel.cancelled() => None,
            r = generate_weekly_review(&rec, &guard, &input) => Some(r),
        };
        let calls = rec.calls();
        let m = Metering {
            account_id,
            feature: AiFeature::WeeklyReview,
            ref_id: id.to_string(),
            model: Models::WEEKLY,
            key: format!("weekly:{id}:{}", job.lease_token),
            calls: &calls,
        };
        let Some(result) = result else {
            return Err(cancelled(d, &m).await);
        };
        let written = match result {
            Ok(o) => self
                .store(account_id, id, &o, &m, now)
                .await
                .map(|changed| {
                    if changed {
                        json!({ "status": if is_empty(&o.content) { "dropped" } else { "ready" } })
                    } else {
                        skipped("not_pending")
                    }
                }),
            Err(SectionError {
                error: AiError::Unavailable { retry_after, .. },
                ..
            }) => {
                let count = row.retry_count + 1;
                match retry_delay(count, retry_after) {
                    Some(delay) => {
                        let at = now + delay;
                        match self.retry_later(id, count, at, &m).await {
                            Ok(()) => {
                                return Err(JobError::Release {
                                    delay,
                                    reason: "AI unavailable".to_owned(),
                                });
                            }
                            Err(e) => Err(e),
                        }
                    }
                    None => self
                        .fail(account_id, id, REASON_AI_UNAVAILABLE, Some(&m))
                        .await
                        .map(|()| json!({ "failed": REASON_AI_UNAVAILABLE })),
                }
            }
            Err(SectionError { error, .. }) => {
                let reason = failure_reason(&error);
                if let AiError::Auth { status } = error {
                    tracing::error!(status, "AI credentials rejected (weekly review)");
                }
                self.fail(account_id, id, reason, Some(&m))
                    .await
                    .map(|()| json!({ "failed": reason }))
            }
        };
        match written {
            Ok(v) => Ok(v),
            Err(e) => {
                m.alone(&d.pool, &d.pricing).await;
                Err(tx_err(e))
            }
        }
    }
}

impl Handler for WeeklyReviewHandler {
    fn kind(&self) -> &'static str {
        KIND_WEEKLY_REVIEW
    }

    fn handle(&self, job: Job, ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        Box::pin(self.run(job, ctx))
    }

    fn on_dead<'a>(&'a self, job: &'a Job, _reason: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let Ok(p) = serde_json::from_value::<WeeklyReviewPayload>(job.payload.clone()) else {
                return;
            };
            let id = p.weekly_review_id;
            let row = match self.0.pool.acquire().await {
                Ok(mut c) => weekly::get_weekly(&mut c, id).await.ok().flatten(),
                Err(_) => None,
            };
            if let Some(row) = row
                && let Err(e) = self.fail(row.account_id, id, REASON_JOB_DEAD, None).await
            {
                tracing::error!(error = %e, "failing a dead weekly review failed");
            }
        })
    }
}
