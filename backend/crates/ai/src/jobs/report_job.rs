//! The `generate-report` job (Task 18, port of TS plan C2; ADR 0061 "Degradation").
//!
//! One run builds one report version. The procedure:
//!
//! 1. **Claim.** [`runs::claim_run_or_stale`]: a `queued` or `awaiting_retry` run, or a
//!    `running` one whose heartbeat is older than [`RECLAIM_STALE_AFTER`] (its worker died). A
//!    run that is finished, waiting for a batch or held by a live worker is left alone (`Ok`).
//!    The claim's `started_at` is a token: every later write locks the run row and checks it
//!    ([`runs::lock_claim`]), so a zombie worker can never overwrite a reclaimed or reset run.
//! 2. **Plan gate.** A free plan with another report in the last [`FREE_REPORT_INTERVAL_DAYS`]
//!    days, and a trigger other than onboarding, fails with `plan_interval` (`Unrecoverable`).
//! 3. **Snapshot.** The first claim takes the input from the [`CoachingInputProvider`], validates
//!    it (`parse_coaching_input`) and stores it with its hashes; retries reuse the snapshot. A
//!    missing or invalid input fails the run (`no_input` / `invalid_input`) with no report row.
//! 4. **Computed report.** If the run has no report yet: the next version, status
//!    `computed_only`, and `report.section.ready {section: "computed"}`, in one transaction.
//! 5. **No AI.** Without a client the run is done with `ai_disabled_reason = no_api_key`; when
//!    the budget is already spent, the same with `budget`. Either way `report.degraded` is
//!    emitted and the computed report ships. A missing key never retries.
//! 6. **Generate.** `generate_report` with the ready sections skipped, the budget guard before
//!    every call, and `on_section` storing each outcome in ONE transaction: the section row, its
//!    `ai_usage` rows, the report status and disclosure, and (if the section changed) its events.
//!    `areas` is announced as one `area:<id>` event per area item.
//! 7. **Finish.** The run is done, the report's status and disclosure are final, and
//!    `report.ready` is emitted, in one transaction.
//! 8. **Errors.** `Unavailable` follows `retry::retry_delay`: `awaiting_retry` plus
//!    `report.degraded {unavailable, retryAt}` and `JobError::Release`, or, once the schedule is
//!    exhausted, `failed` with `ai_unavailable` and `job.failed`. `Budget` mid-run keeps the
//!    sections and finishes as done (`budget`). `Auth` fails with `ai_auth`. Every paid call of a
//!    failed section (`ReportError::unmetered`) is metered in the same transaction as the state
//!    change, so a paid call is metered exactly once, also on failure.
//!
//! The heartbeat runs every `heartbeat_every` while the run is claimed and stops with the
//! handler or `ctx.cancel`. A cancelled handler hands the run back (`queued`) and releases the
//! job, so the next delivery resumes at once.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::future::BoxFuture;
use futures::stream::BoxStream;
use queue::queue::Queue;
use queue::runner::{Handler, JobContext};
use queue::{Job, JobError};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::budget::{BudgetConfig, PricingTable, budget_guard, meter_keyed};
use super::kinds::{self, GenerateReportPayload, KIND_GENERATE_REPORT};
use super::providers::CoachingInputProvider;
use super::retry::retry_delay;
use crate::client::model::Models;
use crate::client::port::{
    AiClient, AiError, BatchItem, BatchResultItem, BatchStatus, ChatTurnRequest, SchemaResolver,
    StructuredRequest, StructuredResult, ToolHandler, TurnEvent,
};
use crate::db::reports::{self, NewReport, ReportStatus, SectionWrite, SectionWriteStatus};
use crate::db::runs::{self, NewRun, ReportMode, ReportTrigger, RunClaim, RunRow};
use crate::db::usage::AiFeature;
use crate::entitlements::{Entitlements, Plan};
use crate::events::{
    self, DegradedReason, JOB_FAILED, JobFailed, REPORT_DEGRADED, REPORT_READY,
    REPORT_SECTION_READY, ReportDegraded, ReportReady, SectionReady,
};
use crate::generate::computed::build_computed_report;
use crate::generate::context::input_hash;
use crate::generate::payload::REPORT_PAYLOAD_SCHEMA_VERSION;
use crate::generate::report::{REPORT_CONCURRENCY, ReportCause, ReportError, generate_report};
use crate::generate::request::build_section_request;
use crate::generate::section::{CallRecord, SectionOutcome, SectionStatus};
use crate::input::parse::parse_coaching_input;
use crate::input::types::{CoachingInput, TimeClass};
use crate::output::codes::{SECTION_KEYS, SectionKey};
use crate::output::disclosure::make_disclosure;
use crate::prompts::{REPORT_PROMPT_SET, REPORT_PROMPT_VERSION, prompt_set_hash};

/// A free plan gets one non-onboarding report per this many days.
pub const FREE_REPORT_INTERVAL_DAYS: i64 = 30;
/// How often a claimed run's heartbeat is written.
pub const HEARTBEAT_EVERY: Duration = Duration::from_secs(15);
/// A `running` run whose heartbeat is older than this may be reclaimed by a redelivered job
/// (four missed heartbeats; the reconciler's own threshold is 10 min).
pub const RECLAIM_STALE_AFTER: Duration = Duration::from_secs(60);

/// `report_runs.last_error` and `job.failed` reasons.
const PLAN_INTERVAL: &str = "plan_interval";
const NO_INPUT: &str = "no_input";
const INVALID_INPUT: &str = "invalid_input";
const AI_UNAVAILABLE: &str = "ai_unavailable";
const AI_AUTH: &str = "ai_auth";
const AI_ERROR: &str = "ai_error";
const JOB_DEAD: &str = "job_dead";
/// `report_runs.ai_disabled_reason`.
const NO_API_KEY: &str = "no_api_key";
const BUDGET: &str = "budget";

/// What the generate-report handler needs.
#[derive(Clone)]
pub struct ReportDeps {
    pub pool: PgPool,
    pub queue: Queue,
    /// `None` when no sidecar is configured or it has no credential: computed-only reports.
    pub ai: Option<Arc<dyn AiClient>>,
    pub inputs: Arc<dyn CoachingInputProvider>,
    pub entitlements: Arc<dyn Entitlements>,
    pub budgets: BudgetConfig,
    /// Prices the metered calls (shared with the coach-chat job).
    pub pricing: Arc<PricingTable>,
    pub now: Arc<dyn Fn() -> OffsetDateTime + Send + Sync>,
    pub heartbeat_every: Duration,
}

/// R1's `Handler` for [`KIND_GENERATE_REPORT`].
#[derive(Clone)]
pub struct GenerateReportHandler(pub ReportDeps);

/// Inserts a queued run for (account, time class) and enqueues its job after the commit. A lost
/// enqueue is repaired by `ai-reconcile` (queued runs without a job), so it is only logged.
pub async fn request_report(
    pool: &PgPool,
    queue: &Queue,
    account_id: Uuid,
    tc: TimeClass,
    trigger: ReportTrigger,
    plan: Plan,
) -> anyhow::Result<Uuid> {
    let mut tx = pool.begin().await?;
    let is_first_report = reports::latest_report(&mut tx, account_id, tc.as_str())
        .await?
        .is_none();
    let id = runs::insert_run(
        &mut tx,
        &NewRun {
            account_id,
            time_class: tc.as_str().to_owned(),
            trigger,
            plan,
            mode: ReportMode::Sync,
            prompt_version: REPORT_PROMPT_VERSION.to_owned(),
            is_first_report,
            ai_disabled_reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    if let Err(e) = kinds::enqueue_report(queue, id, account_id).await {
        tracing::warn!(run = %id, error = %e, "report job not enqueued; ai-reconcile will");
    }
    Ok(id)
}

/// Why processing stopped early.
enum Stop {
    /// The run's state is recorded; this is the job's result.
    Job(JobError),
    /// Another worker took the run over (or the reconciler reset it): do nothing more.
    Lost,
    /// A database or provider error: hand the run back and retry the job.
    Internal(anyhow::Error),
}

impl<E: Into<anyhow::Error>> From<E> for Stop {
    fn from(e: E) -> Self {
        Stop::Internal(e.into())
    }
}

fn unrecoverable(reason: &str) -> Stop {
    Stop::Job(JobError::Unrecoverable {
        message: format!("generate-report: {reason}"),
        code: Some(reason.to_owned()),
    })
}

/// The `report.section.ready` labels of an outcome: one per area item for `areas`.
fn section_labels(o: &SectionOutcome) -> Vec<String> {
    match (o.section, o.content.as_ref().and_then(|c| c.areas())) {
        (SectionKey::Areas, Some(a)) if !a.items.is_empty() => {
            let mut out: Vec<String> = Vec::new();
            for i in &a.items {
                let l = format!("area:{}", i.area.as_str());
                if !out.contains(&l) {
                    out.push(l);
                }
            }
            out
        }
        (k, _) => vec![k.as_str().to_owned()],
    }
}

/// Sets the report's status from its ready sections, with the disclosure whenever AI text is
/// shown. Returns the status.
pub(crate) async fn refresh_status(
    conn: &mut PgConnection,
    report_id: Uuid,
    now: OffsetDateTime,
) -> Result<ReportStatus, sqlx::Error> {
    let ready = reports::ready_sections(conn, report_id).await?.len();
    let status = if ready == SECTION_KEYS.len() {
        ReportStatus::Complete
    } else if ready == 0 {
        ReportStatus::ComputedOnly
    } else {
        ReportStatus::Partial
    };
    let disclosure = (status != ReportStatus::ComputedOnly).then(|| {
        serde_json::to_value(make_disclosure(
            Models::REPORT.as_str(),
            REPORT_PROMPT_VERSION,
            now,
        ))
        .expect("the disclosure serialises")
    });
    reports::set_status(conn, report_id, status, disclosure.as_ref()).await?;
    Ok(status)
}

/// Stores one section outcome and refreshes the report's status and disclosure, on the
/// caller's transaction. Returns the `report.section.ready` labels to emit, none when the
/// section did not change; the caller emits them last. Metering is the caller's: its key
/// depends on the path (a claim here, a batch item in `batch_jobs`).
pub(crate) async fn persist_section_outcome(
    conn: &mut PgConnection,
    report_id: Uuid,
    o: &SectionOutcome,
    now: OffsetDateTime,
) -> anyhow::Result<Vec<String>> {
    let write = SectionWrite {
        section: o.section,
        status: match o.status {
            SectionStatus::Ready => SectionWriteStatus::Ready,
            SectionStatus::Dropped => SectionWriteStatus::Dropped,
        },
        content: o.content.as_ref().map(serde_json::to_value).transpose()?,
        // The full items (with details) are the server-side audit copy; the web payload
        // projects them without model text.
        dropped: serde_json::to_value(&o.dropped)?,
        attempts: i16::from(o.attempts),
        model: Models::REPORT.as_str().to_owned(),
    };
    let changed = reports::upsert_section(conn, report_id, &write).await?;
    refresh_status(conn, report_id, now).await?;
    Ok(if changed.0 {
        section_labels(o)
    } else {
        Vec::new()
    })
}

/// Whether a stored outcome shows narrative text (`SectionReady.narrative`).
pub(crate) fn is_narrative(o: &SectionOutcome) -> bool {
    o.status == SectionStatus::Ready && o.content.is_some()
}

/// The metering key prefix of `what` (a section key, or `unmetered`) within one claim: the
/// claim token is part of it, so a section regenerated by a later claim (a new paid call) gets
/// new keys, while re-running the same write within a claim meters nothing twice.
///
/// Keys are positional (`<prefix>:<i>`, the i-th record of the list metered under the prefix).
/// That is sound because each prefix names one fixed, append-only list per claim: a section's
/// calls are made once per claim (attempt 1, then at most one repair), and the [`Ledger`] keeps
/// them in call order, which is the order of `SectionOutcome::calls`. Never meter two different
/// lists under one prefix (review m2): the second would silently be dropped as duplicates.
fn usage_key(claim: &RunClaim, what: &str) -> String {
    format!(
        "report:{}:{}:{what}",
        claim.id,
        claim.started_at.unix_timestamp_nanos() / 1000
    )
}

/// Every paid call of one claim, by section, in call order, recorded the moment the client
/// returns it (review I2). The job meters it on every terminal path (done, degraded, retry,
/// failed, lost, cancelled) under the same keys `on_section` uses, so a call already metered
/// with its section is a no-op (`ON CONFLICT`) and a call no `on_section` committed is metered
/// exactly once: attempt 1 of a section whose repair failed, an outcome whose store rolled
/// back, a store whose commit was ambiguous (review m1), and calls finished before a cancel.
///
/// A request still in flight when the handler is cancelled reports no usage, so it cannot be
/// metered (as in the coach chat, ADR 0064 §12).
struct Ledger {
    /// The task block of each section request (the last block of the first message); a repair
    /// repeats its section's first message, so it maps to the same section.
    tasks: Vec<(SectionKey, Value)>,
    calls: Mutex<BTreeMap<Option<SectionKey>, Vec<CallRecord>>>,
}

impl Ledger {
    fn new(input: &CoachingInput, is_first_report: bool) -> Ledger {
        let tasks = SECTION_KEYS
            .into_iter()
            .filter_map(|k| {
                let req = build_section_request(input, k, is_first_report);
                let task = req.messages.first()?.content.last()?.clone();
                Some((k, task))
            })
            .collect();
        Ledger {
            tasks,
            calls: Mutex::default(),
        }
    }

    fn section_of(&self, req: &StructuredRequest) -> Option<SectionKey> {
        let task = req.messages.first()?.content.last()?;
        self.tasks.iter().find(|(_, t)| t == task).map(|(k, _)| *k)
    }

    fn record(&self, req: &StructuredRequest, r: &StructuredResult) {
        let section = self.section_of(req);
        let mut calls = self.calls.lock().unwrap_or_else(|e| e.into_inner());
        calls
            .entry(section)
            .or_default()
            .extend(CallRecord::from_result(r, false));
    }

    /// The recorded calls, grouped by key prefix.
    fn snapshot(&self) -> Vec<(Option<SectionKey>, Vec<CallRecord>)> {
        let calls = self.calls.lock().unwrap_or_else(|e| e.into_inner());
        calls.iter().map(|(k, v)| (*k, v.clone())).collect()
    }

    /// The records of `extra` that the ledger does not hold (a multiset difference). Normally
    /// empty: `ReportError::unmetered` is a subset of the ledger.
    fn uncovered(&self, extra: &[CallRecord]) -> Vec<CallRecord> {
        let mut pool: Vec<CallRecord> = self.snapshot().into_iter().flat_map(|(_, v)| v).collect();
        let mut out = Vec::new();
        for c in extra {
            match pool.iter().position(|p| p == c) {
                Some(i) => {
                    pool.swap_remove(i);
                }
                None => out.push(c.clone()),
            }
        }
        out
    }
}

/// The run's client: the claim's [`AiClient`], with every structured result recorded in the
/// [`Ledger`] before it is handed back.
struct LedgerClient {
    inner: Arc<dyn AiClient>,
    ledger: Arc<Ledger>,
}

impl AiClient for LedgerClient {
    fn structured<'a>(
        &'a self,
        req: &'a StructuredRequest,
    ) -> BoxFuture<'a, Result<StructuredResult, AiError>> {
        Box::pin(async move {
            let r = self.inner.structured(req).await;
            if let Ok(res) = &r {
                self.ledger.record(req, res);
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

/// Meters the ledger, plus any `extra` record it does not hold, on `conn`.
#[allow(clippy::too_many_arguments)]
async fn meter_ledger(
    conn: &mut PgConnection,
    pricing: &PricingTable,
    account_id: Uuid,
    claim: &RunClaim,
    ledger: &Ledger,
    extra: &[CallRecord],
) -> sqlx::Result<()> {
    let ref_id = claim.id.to_string();
    for (section, calls) in ledger.snapshot() {
        let what = section.map_or("unknown", SectionKey::as_str);
        meter_keyed(
            conn,
            pricing,
            account_id,
            AiFeature::Report,
            &calls,
            &ref_id,
            Models::REPORT,
            &usage_key(claim, what),
        )
        .await?;
    }
    let uncovered = ledger.uncovered(extra);
    if !uncovered.is_empty() {
        tracing::warn!(run = %claim.id, n = uncovered.len(), "unmetered calls outside the ledger");
        meter_keyed(
            conn,
            pricing,
            account_id,
            AiFeature::Report,
            &uncovered,
            &ref_id,
            Models::REPORT,
            &usage_key(claim, "unmetered"),
        )
        .await?;
    }
    Ok(())
}

/// How long a cancelled handler may spend metering its ledger before it hands the run back.
pub const CANCEL_GRACE: Duration = Duration::from_secs(5);

/// `on_section` found the run taken over by another delivery (or reset by the reconciler).
#[derive(Debug, thiserror::Error)]
#[error("the run was taken over")]
struct ClaimLost;

/// Stores, meters and announces one section in one transaction (the `on_section` of the run).
struct SectionSink {
    pool: PgPool,
    pricing: Arc<PricingTable>,
    claim: RunClaim,
    account_id: Uuid,
    report_id: Uuid,
    now: OffsetDateTime,
}

impl SectionSink {
    async fn store(&self, o: SectionOutcome) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        if !runs::lock_claim(&mut tx, &self.claim).await? {
            // The calls come back as `unmetered` and are metered by the caller.
            return Err(ClaimLost.into());
        }
        let labels = persist_section_outcome(&mut tx, self.report_id, &o, self.now).await?;
        meter_keyed(
            &mut tx,
            &self.pricing,
            self.account_id,
            AiFeature::Report,
            &o.calls,
            &self.claim.id.to_string(),
            Models::REPORT,
            &usage_key(&self.claim, o.section.as_str()),
        )
        .await?;
        let narrative = is_narrative(&o);
        for section in labels {
            events::emit(
                &mut tx,
                self.account_id,
                REPORT_SECTION_READY,
                &SectionReady {
                    report_id: self.report_id,
                    report_run_id: self.claim.id,
                    section,
                    narrative,
                },
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

/// One claimed run being processed.
struct Run<'a> {
    d: &'a ReportDeps,
    row: RunRow,
    claim: RunClaim,
    now: OffsetDateTime,
    /// Set once the input is known (step 3); every paid call of the claim is in it.
    ledger: Mutex<Option<Arc<Ledger>>>,
}

impl Run<'_> {
    fn ledger(&self) -> Option<Arc<Ledger>> {
        self.ledger
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Meters every paid call of the claim on `conn` (inside the caller's transaction): the
    /// [`Ledger`], plus `calls` (`ReportError::unmetered`) where the ledger lacks them. Calls
    /// already metered by `on_section` are no-ops, so every terminal path can run it.
    async fn meter_on(&self, conn: &mut PgConnection, calls: &[CallRecord]) -> sqlx::Result<()> {
        let ledger = self.ledger().unwrap_or_else(|| {
            Arc::new(Ledger {
                tasks: Vec::new(),
                calls: Mutex::default(),
            })
        });
        meter_ledger(
            conn,
            &self.d.pricing,
            self.row.account_id,
            &self.claim,
            &ledger,
            calls,
        )
        .await
    }

    /// Meters the ledger in its own transaction, bounded by `grace` (a cancelled or failed
    /// attempt, before it hands the run back). A failure is logged: the calls are then lost.
    async fn meter_unsettled(&self, grace: Duration) {
        let Some(ledger) = self.ledger() else { return };
        let work = async {
            let mut tx = self.d.pool.begin().await?;
            meter_ledger(
                &mut tx,
                &self.d.pricing,
                self.row.account_id,
                &self.claim,
                &ledger,
                &[],
            )
            .await?;
            tx.commit().await
        };
        match tokio::time::timeout(grace, work).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                tracing::error!(run = %self.claim.id, error = %e, "paid report calls not metered");
            }
            Err(_) => {
                tracing::error!(run = %self.claim.id, "paid report calls not metered (timeout)")
            }
        }
    }

    /// Begins a transaction holding the run row, or meters the claim's paid calls alone and
    /// stops with [`Stop::Lost`] when the claim is gone (paid calls are metered whoever owns the
    /// run).
    async fn begin_owned(
        &self,
        calls: &[CallRecord],
    ) -> Result<sqlx::Transaction<'static, sqlx::Postgres>, Stop> {
        let mut tx = self.d.pool.begin().await?;
        if runs::lock_claim(&mut tx, &self.claim).await? {
            return Ok(tx);
        }
        drop(tx);
        let mut tx = self.d.pool.begin().await?;
        self.meter_on(&mut tx, calls).await?;
        tx.commit().await?;
        Err(Stop::Lost)
    }

    /// Fails the run with `reason`, meters `calls` and emits `job.failed`.
    async fn fail(&self, reason: &str, calls: &[CallRecord]) -> Result<(), Stop> {
        let mut tx = self.begin_owned(calls).await?;
        self.meter_on(&mut tx, calls).await?;
        runs::set_failed(&mut tx, self.claim.id, reason).await?;
        events::emit(
            &mut tx,
            self.row.account_id,
            JOB_FAILED,
            &JobFailed {
                kind: KIND_GENERATE_REPORT.to_owned(),
                reason: reason.to_owned(),
            },
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Finishes the run without (more) AI: done, `ai_disabled_reason`, `report.degraded`.
    async fn degrade(
        &self,
        report_id: Uuid,
        disabled: &str,
        reason: DegradedReason,
        calls: &[CallRecord],
    ) -> Result<Value, Stop> {
        let mut tx = self.begin_owned(calls).await?;
        self.meter_on(&mut tx, calls).await?;
        runs::set_ai_disabled_reason(&mut tx, self.claim.id, disabled).await?;
        runs::set_done(&mut tx, self.claim.id, Some(report_id)).await?;
        let status = refresh_status(&mut tx, report_id, self.now).await?;
        events::emit(
            &mut tx,
            self.row.account_id,
            REPORT_DEGRADED,
            &ReportDegraded {
                report_id,
                reason,
                retry_at: None,
            },
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"runId": self.claim.id, "status": status, "aiDisabled": disabled}))
    }

    /// The AI is unavailable: schedule the retry, or fail once the schedule is exhausted.
    async fn unavailable(
        &self,
        report_id: Uuid,
        e: &AiError,
        calls: &[CallRecord],
    ) -> Result<Value, Stop> {
        let count = self.row.retry_count.saturating_add(1);
        let Some(delay) = retry_delay(count, e.retry_after()) else {
            self.fail(AI_UNAVAILABLE, calls).await?;
            return Ok(json!({"runId": self.claim.id, "failed": AI_UNAVAILABLE}));
        };
        let at = self.now + delay;
        let mut tx = self.begin_owned(calls).await?;
        self.meter_on(&mut tx, calls).await?;
        runs::set_awaiting_retry(&mut tx, self.claim.id, count, at, AI_UNAVAILABLE).await?;
        refresh_status(&mut tx, report_id, self.now).await?;
        events::emit(
            &mut tx,
            self.row.account_id,
            REPORT_DEGRADED,
            &ReportDegraded {
                report_id,
                reason: DegradedReason::Unavailable,
                retry_at: Some(common::js_time::to_iso_string(at)),
            },
        )
        .await?;
        tx.commit().await?;
        Err(Stop::Job(JobError::Release {
            delay,
            reason: format!("ai unavailable ({e}); retry {count}"),
        }))
    }

    /// Any other client error.
    async fn ai_failed(
        &self,
        report_id: Uuid,
        e: &AiError,
        calls: &[CallRecord],
    ) -> Result<Value, Stop> {
        match e {
            AiError::Unavailable { .. } => self.unavailable(report_id, e, calls).await,
            AiError::Budget => {
                self.degrade(report_id, BUDGET, DegradedReason::Budget, calls)
                    .await
            }
            AiError::Auth { status } => {
                // The error carries only the HTTP status, never the key.
                tracing::error!(run = %self.claim.id, status, "AI credentials rejected");
                self.fail(AI_AUTH, calls).await?;
                Err(unrecoverable(AI_AUTH))
            }
            other => {
                tracing::error!(run = %self.claim.id, error = %other, "report generation failed");
                self.fail(AI_ERROR, calls).await?;
                Err(unrecoverable(AI_ERROR))
            }
        }
    }

    /// Step 3: the validated input, snapshotted on the first claim.
    async fn input(&self, tc: TimeClass) -> Result<CoachingInput, Stop> {
        let raw = match &self.row.input {
            Some(v) => v.clone(),
            None => match self
                .d
                .inputs
                .coaching_input(self.row.account_id, tc)
                .await?
            {
                Some(v) => v,
                None => {
                    self.fail(NO_INPUT, &[]).await?;
                    return Err(unrecoverable(NO_INPUT));
                }
            },
        };
        let input = match parse_coaching_input(&raw) {
            Ok(i) => i,
            Err(issues) => {
                tracing::warn!(run = %self.claim.id, issues = issues.len(), "invalid coaching input");
                self.fail(INVALID_INPUT, &[]).await?;
                return Err(unrecoverable(INVALID_INPUT));
            }
        };
        if self.row.input.is_none() {
            let mut tx = self.begin_owned(&[]).await?;
            runs::snapshot_input(
                &mut tx,
                self.claim.id,
                &raw,
                &input_hash(&input),
                &prompt_set_hash(REPORT_PROMPT_SET),
            )
            .await?;
            tx.commit().await?;
        }
        Ok(input)
    }

    /// Whether the free plan's interval applies to this run (step 2).
    fn gated(&self, plan: Plan) -> bool {
        plan == Plan::Free && self.row.trigger != ReportTrigger::Onboarding
    }

    /// Whether another report of (account, time class) is inside the free interval.
    async fn inside_interval(&self, conn: &mut PgConnection) -> Result<bool, sqlx::Error> {
        let last = runs::latest_other_report_at(
            conn,
            self.row.account_id,
            &self.row.time_class,
            self.claim.id,
        )
        .await?;
        Ok(last.is_some_and(|at| self.now - at < time::Duration::days(FREE_REPORT_INTERVAL_DAYS)))
    }

    /// Step 4: the run's report, created with its computed part if it does not exist yet. A
    /// report already linked to the run is reused without a write (the writes that follow are
    /// fenced). The free plan's interval is checked again under the versioning lock, in the
    /// transaction that creates the report, so two concurrent free runs cannot both pass it
    /// (review m3).
    async fn report(&self, input: &CoachingInput, plan: Plan) -> Result<Uuid, Stop> {
        if let Some(id) = self.row.report_id {
            return Ok(id);
        }
        let mut tx = self.begin_owned(&[]).await?;
        if let Some(r) = reports::report_for_run(&mut tx, self.claim.id).await? {
            return Ok(r.id);
        }
        if self.gated(plan) {
            reports::lock_versions(&mut tx, self.row.account_id, &self.row.time_class).await?;
            if self.inside_interval(&mut tx).await? {
                drop(tx);
                self.fail(PLAN_INTERVAL, &[]).await?;
                return Err(unrecoverable(PLAN_INTERVAL));
            }
        }
        let computed = build_computed_report(input, self.row.is_first_report);
        let r = reports::create_report_with_next_version(
            &mut tx,
            &NewReport {
                account_id: self.row.account_id,
                time_class: self.row.time_class.clone(),
                report_run_id: self.claim.id,
                schema_version: REPORT_PAYLOAD_SCHEMA_VERSION.to_owned(),
                computed: serde_json::to_value(&computed)?,
                status: ReportStatus::ComputedOnly,
            },
        )
        .await?;
        runs::link_report(&mut tx, self.claim.id, r.id).await?;
        events::emit(
            &mut tx,
            self.row.account_id,
            REPORT_SECTION_READY,
            &SectionReady {
                report_id: r.id,
                report_run_id: self.claim.id,
                section: "computed".to_owned(),
                narrative: false,
            },
        )
        .await?;
        tx.commit().await?;
        Ok(r.id)
    }

    /// Step 7.
    async fn finish(&self, report_id: Uuid) -> Result<Value, Stop> {
        let mut tx = self.begin_owned(&[]).await?;
        // Normally all metered by `on_section` already (no-ops); see [`Ledger`].
        self.meter_on(&mut tx, &[]).await?;
        runs::set_done(&mut tx, self.claim.id, Some(report_id)).await?;
        let status = refresh_status(&mut tx, report_id, self.now).await?;
        events::emit(
            &mut tx,
            self.row.account_id,
            REPORT_READY,
            &ReportReady { report_id, status },
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"runId": self.claim.id, "reportId": report_id, "status": status}))
    }

    async fn process(&self) -> Result<Value, Stop> {
        let d = self.d;
        let account_id = self.row.account_id;
        let tc: TimeClass = self
            .row
            .time_class
            .parse()
            .map_err(|_| anyhow::anyhow!("unknown time class {:?}", self.row.time_class))?;
        let plan = d.entitlements.plan(account_id).await;

        // 2. The free plan's interval (only before this run has a report).
        // (An early exit before the provider call; step 4 re-checks under the lock.)
        if self.row.report_id.is_none() && self.gated(plan) {
            let mut conn = d.pool.acquire().await?;
            let inside = self.inside_interval(&mut conn).await?;
            drop(conn);
            if inside {
                self.fail(PLAN_INTERVAL, &[]).await?;
                return Err(unrecoverable(PLAN_INTERVAL));
            }
        }

        // 3 and 4.
        let input = self.input(tc).await?;
        // Task 19: a pending note and a job for every detected change without a note. It is
        // idempotent, so a retry repairs a lost enqueue; a failure never blocks the report.
        if let Err(e) = crate::jobs::change_job::enqueue_missing_change_notes(
            &d.pool, &d.queue, account_id, &input,
        )
        .await
        {
            tracing::warn!(run = %self.claim.id, error = %format!("{e:#}"), "change notes not enqueued");
        }
        let ledger = Arc::new(Ledger::new(&input, self.row.is_first_report));
        *self.ledger.lock().unwrap_or_else(|e| e.into_inner()) = Some(ledger.clone());
        let report_id = self.report(&input, plan).await?;

        let skip = {
            let mut conn = d.pool.acquire().await?;
            reports::ready_sections(&mut conn, report_id).await?
        };
        if skip.len() == SECTION_KEYS.len() {
            return self.finish(report_id).await;
        }

        // 5. Degrade without AI.
        let Some(inner) = d.ai.clone() else {
            return self
                .degrade(report_id, NO_API_KEY, DegradedReason::AiDisabled, &[])
                .await;
        };
        let scope = d.entitlements.budget_scope(account_id).await;
        let guard = budget_guard(d.pool.clone(), scope, plan, d.budgets, self.now);
        if let Err(e) = guard().await {
            return self.ai_failed(report_id, &e, &[]).await;
        }

        // 6. Generate.
        let sink = Arc::new(SectionSink {
            pool: d.pool.clone(),
            pricing: d.pricing.clone(),
            claim: self.claim,
            account_id,
            report_id,
            now: self.now,
        });
        let on_section = move |o: SectionOutcome| -> BoxFuture<'static, anyhow::Result<()>> {
            let sink = sink.clone();
            Box::pin(async move { sink.store(o).await })
        };
        let ai = LedgerClient { inner, ledger };
        let res = generate_report(
            &ai,
            &guard,
            &input,
            self.row.is_first_report,
            &skip,
            REPORT_CONCURRENCY,
            &on_section,
        )
        .await;
        match res {
            Ok(_) => self.finish(report_id).await,
            Err(ReportError {
                cause: ReportCause::Ai(e),
                unmetered,
            }) => self.ai_failed(report_id, &e, &unmetered).await,
            Err(ReportError {
                cause: ReportCause::OnSection(e),
                unmetered,
            }) => {
                if !unmetered.is_empty() {
                    let mut tx = d.pool.begin().await?;
                    self.meter_on(&mut tx, &unmetered).await?;
                    tx.commit().await?;
                }
                if e.downcast_ref::<ClaimLost>().is_some() {
                    return Err(Stop::Lost);
                }
                Err(Stop::Internal(e.context("storing a section")))
            }
        }
    }
}

/// Writes the heartbeat every `every` until `stop` fires or the claim is gone.
fn spawn_heartbeat(pool: PgPool, claim: RunClaim, every: Duration, stop: CancellationToken) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(every);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        tick.tick().await;
        loop {
            tokio::select! {
                biased;
                () = stop.cancelled() => break,
                _ = tick.tick() => {}
            }
            let beat = async {
                let mut conn = pool.acquire().await?;
                runs::heartbeat_claim(&mut conn, &claim).await
            };
            let beat = tokio::select! {
                biased;
                () = stop.cancelled() => break,
                r = beat => r,
            };
            match beat {
                Ok(true) => {}
                Ok(false) => break,
                Err(e) => tracing::warn!(run = %claim.id, error = %e, "report heartbeat failed"),
            }
        }
    });
}

impl GenerateReportHandler {
    async fn release(&self, claim: &RunClaim) {
        let r = async {
            let mut conn = self.0.pool.acquire().await?;
            runs::release_claim(&mut conn, claim).await
        };
        match tokio::time::timeout(Duration::from_secs(10), r).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => tracing::warn!(run = %claim.id, error = %e, "run not handed back"),
            Err(_) => tracing::warn!(run = %claim.id, "run not handed back (timeout)"),
        }
    }

    async fn run(&self, job: Job, ctx: JobContext) -> Result<Value, JobError> {
        let d = &self.0;
        let payload: GenerateReportPayload =
            serde_json::from_value(job.payload).map_err(|e| JobError::Unrecoverable {
                message: format!("generate-report payload: {e}"),
                code: Some("bad_payload".to_owned()),
            })?;
        let run_id = payload.report_run_id;
        let retryable = |e: sqlx::Error| JobError::Retryable {
            message: format!("generate-report claim: {e}"),
            code: None,
        };

        // 1. Claim.
        let claimed = {
            let mut conn = d.pool.acquire().await.map_err(retryable)?;
            runs::claim_run_or_stale(&mut conn, run_id, RECLAIM_STALE_AFTER)
                .await
                .map_err(retryable)?
        };
        let Some(row) = claimed.as_ref().and_then(|r| Some((r, RunClaim::of(r)?))) else {
            let mut conn = d.pool.acquire().await.map_err(retryable)?;
            let status = runs::get_run(&mut conn, run_id)
                .await
                .map_err(retryable)?
                .map(|r| r.status.as_str());
            return Ok(json!({"runId": run_id, "skipped": status}));
        };
        let (row, claim) = (row.0.clone(), row.1);

        let stop = ctx.cancel.child_token();
        let _stop_heartbeat = stop.clone().drop_guard();
        spawn_heartbeat(d.pool.clone(), claim, d.heartbeat_every, stop);

        let run = Run {
            d,
            row,
            claim,
            now: (d.now)(),
            ledger: Mutex::new(None),
        };
        let res = tokio::select! {
            biased;
            () = ctx.cancel.cancelled() => None,
            r = run.process() => Some(r),
        };
        // `process` is dropped by now (a cancel drops it mid-call); its paid calls are in the
        // ledger.
        let Some(res) = res else {
            run.meter_unsettled(CANCEL_GRACE).await;
            self.release(&claim).await;
            return Err(JobError::Release {
                delay: Duration::ZERO,
                reason: "cancelled".to_owned(),
            });
        };
        match res {
            Ok(v) => Ok(v),
            Err(Stop::Job(e)) => Err(e),
            Err(Stop::Lost) => Ok(json!({"runId": run_id, "lost": true})),
            Err(Stop::Internal(e)) => {
                tracing::warn!(run = %run_id, error = %format!("{e:#}"), "report run failed; retrying");
                run.meter_unsettled(CANCEL_GRACE).await;
                self.release(&claim).await;
                Err(JobError::Retryable {
                    message: format!("generate-report: {e:#}"),
                    code: None,
                })
            }
        }
    }

    async fn dead(&self, job: &Job) -> anyhow::Result<()> {
        let payload: GenerateReportPayload = serde_json::from_value(job.payload.clone())?;
        let mut tx = self.0.pool.begin().await?;
        let Some(run) = runs::get_run(&mut tx, payload.report_run_id).await? else {
            return Ok(());
        };
        if runs::set_failed_unless_live(&mut tx, run.id, JOB_DEAD, RECLAIM_STALE_AFTER).await? {
            events::emit(
                &mut tx,
                run.account_id,
                JOB_FAILED,
                &JobFailed {
                    kind: KIND_GENERATE_REPORT.to_owned(),
                    reason: JOB_DEAD.to_owned(),
                },
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

impl Handler for GenerateReportHandler {
    fn kind(&self) -> &'static str {
        KIND_GENERATE_REPORT
    }

    fn handle(&self, job: Job, ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        Box::pin(self.run(job, ctx))
    }

    /// The job's retryable attempts ran out: the run fails (`job_dead`) with `job.failed`.
    fn on_dead<'a>(&'a self, job: &'a Job, reason: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if let Err(e) = self.dead(job).await {
                tracing::error!(job = job.id, reason, error = %format!("{e:#}"), "dead report job not recorded");
            }
        })
    }
}
