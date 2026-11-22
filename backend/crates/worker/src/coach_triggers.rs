//! What starts coach work without the player asking (M5b Task B, ADR 0075).
//!
//! - **The first report (ruling G3).** [`FirstReportOnBuild`] wraps the `build-profile` handler.
//!   When a build completes, [`request_first_report`] creates the account × time class's first
//!   report run (`trigger = onboarding`, the account's plan) and enqueues `generate-report` with
//!   T18's payload `{reportRunId}` under its dedupe key `report:<run id>`. It does nothing when
//!   no user is linked to the account, when a report exists, when a run is unfinished
//!   (`queued`, `running`, `awaiting_retry`, `awaiting_batch`), or when a run failed less than
//!   [`FIRST_REPORT_RETRY_AFTER`] ago (a run that fails, say for lack of input, is not
//!   re-requested on every build). The check and the insert run under one transaction-scoped
//!   advisory lock per account × time class, so concurrent builds create one run. Reports are
//!   account data (ADR 0064 §18): one run serves every linked user.
//! - **Weekly reviews (ruling G2).** [`ProfileWeeklyProvider`] is the batch schedule's
//!   `WeeklyInputProvider`: R2's `profile::coaching::weekly` builder for every linked account ×
//!   time class with games in the week.

use std::sync::Arc;

use ai::db::runs::{self, NewRun, ReportMode, ReportTrigger};
use ai::entitlements::{Entitlements, Plan};
use ai::input::types::TimeClass;
use ai::jobs::batch_jobs::WeeklyInputProvider;
use ai::jobs::kinds::enqueue_report;
use ai::prompts::REPORT_PROMPT_VERSION;
use anyhow::Context;
use futures::StreamExt;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use queue::queue::Queue;
use queue::runner::{Handler, JobContext};
use queue::{BuildProfilePayload, Job, JobError};
use serde_json::Value;
use sqlx::PgPool;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

/// A failed first run is requested again only this long after it was created.
pub const FIRST_REPORT_RETRY_AFTER: time::Duration = time::Duration::days(1);

/// Advisory-lock class of the first-report check (two-int key space, as the AI locks).
const FIRST_REPORT_LOCK_CLASS: i32 = 30_040;

/// Creates and enqueues the first report run of (account, time class) when one is due (module
/// docs). Returns the new run's id, `None` when nothing was due.
pub async fn request_first_report(
    pool: &PgPool,
    queue: &Queue,
    account_id: Uuid,
    tc: TimeClass,
    plan: Plan,
) -> anyhow::Result<Option<Uuid>> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1, hashtext($2))")
        .bind(FIRST_REPORT_LOCK_CLASS)
        .bind(format!("first-report:{account_id}:{}", tc.as_str()))
        .execute(&mut *tx)
        .await?;
    let due: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM linked_accounts la WHERE la.id = $1 \
             AND (la.user_id IS NOT NULL OR EXISTS (SELECT 1 FROM account_links al \
               WHERE al.linked_account_id = la.id))) \
         AND NOT EXISTS (SELECT 1 FROM reports WHERE account_id = $1 \
             AND time_class = $2::time_class) \
         AND NOT EXISTS (SELECT 1 FROM report_runs WHERE account_id = $1 \
             AND time_class = $2::time_class \
             AND (status IN ('queued', 'running', 'awaiting_retry', 'awaiting_batch') \
               OR (status = 'failed' AND created_at > now() - $3)))",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .bind(FIRST_REPORT_RETRY_AFTER)
    .fetch_one(&mut *tx)
    .await?;
    if !due {
        return Ok(None);
    }
    let run_id = runs::insert_run(
        &mut tx,
        &NewRun {
            account_id,
            time_class: tc.as_str().to_owned(),
            trigger: ReportTrigger::Onboarding,
            plan,
            mode: ReportMode::Sync,
            prompt_version: REPORT_PROMPT_VERSION.to_owned(),
            is_first_report: true,
            ai_disabled_reason: None,
        },
    )
    .await?;
    tx.commit().await?;
    // A lost enqueue is repaired by `ai-reconcile` (queued runs without a job).
    if let Err(e) = enqueue_report(queue, run_id, account_id).await {
        tracing::warn!(run = %run_id, error = %e, "first report job not enqueued; ai-reconcile will");
    }
    Ok(Some(run_id))
}

/// The `build-profile` handler, followed on success by [`request_first_report`]. A failure of
/// that request is logged and never fails the build.
pub struct FirstReportOnBuild {
    pub inner: Arc<dyn Handler>,
    pub pool: PgPool,
    pub queue: Queue,
    pub entitlements: Arc<dyn Entitlements>,
}

impl FirstReportOnBuild {
    async fn after_build(&self, job: &Job) -> anyhow::Result<Option<Uuid>> {
        let p: BuildProfilePayload =
            serde_json::from_value(job.payload.clone()).context("build-profile payload")?;
        let tc: TimeClass = p
            .time_class
            .parse()
            .map_err(|_| anyhow::anyhow!("unknown time class {:?}", p.time_class))?;
        let plan = self.entitlements.plan(p.account_id).await;
        request_first_report(&self.pool, &self.queue, p.account_id, tc, plan).await
    }
}

impl Handler for FirstReportOnBuild {
    fn kind(&self) -> &'static str {
        self.inner.kind()
    }

    fn handle(&self, job: Job, ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        Box::pin(async move {
            let out = self.inner.handle(job.clone(), ctx).await?;
            match self.after_build(&job).await {
                Ok(Some(run)) => tracing::info!(%run, "first report requested"),
                Ok(None) => {}
                Err(e) => tracing::warn!(error = %format!("{e:#}"), "first report not requested"),
            }
            Ok(out)
        })
    }

    fn on_dead<'a>(&'a self, job: &'a Job, reason: &'a str) -> BoxFuture<'a, ()> {
        self.inner.on_dead(job, reason)
    }
}

/// R2's weekly-input builder as the weekly schedule's provider (module docs).
pub struct ProfileWeeklyProvider {
    pub pool: PgPool,
}

impl ProfileWeeklyProvider {
    async fn one(
        &self,
        account_id: Uuid,
        tc: chess_core::TimeClass,
        week_start: Date,
    ) -> anyhow::Result<Option<Value>> {
        let pool = self.pool.clone();
        // The builder's future is not `Send` (R2's coaching-input builder holds a `RefCell`
        // across awaits), so it runs on a blocking thread, as `ProfileInputProvider` does.
        let rt = tokio::runtime::Handle::current();
        let input = tokio::task::spawn_blocking(move || {
            rt.block_on(profile::coaching::weekly::build_weekly_input(
                &pool,
                account_id,
                tc,
                week_start,
                OffsetDateTime::now_utc(),
            ))
        })
        .await
        .context("the weekly-input builder task")??;
        input
            .map(|i| serde_json::to_value(&i).context("weekly input JSON"))
            .transpose()
    }
}

impl WeeklyInputProvider for ProfileWeeklyProvider {
    fn due<'a>(
        &'a self,
        week_start: Date,
    ) -> BoxStream<'a, anyhow::Result<(Uuid, TimeClass, Value)>> {
        let items = async move {
            match profile::coaching::weekly::weekly_due(&self.pool, week_start).await {
                Ok(due) => futures::stream::iter(due.into_iter().map(Ok)).boxed(),
                Err(e) => futures::stream::once(async move { Err(e) }).boxed(),
            }
        };
        futures::stream::once(items)
            .flatten()
            .filter_map(move |next| async move {
                let (account_id, tc) = match next {
                    Ok(v) => v,
                    Err(e) => return Some(Err(anyhow::Error::from(e))),
                };
                match self.one(account_id, tc, week_start).await {
                    Ok(Some(v)) => Some(Ok((account_id, tc.into(), v))),
                    Ok(None) => None,
                    Err(e) => Some(Err(e.context(format!("weekly input of {account_id}")))),
                }
            })
            .boxed()
    }
}
