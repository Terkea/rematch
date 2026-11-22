//! The `explain-weakness-change` job (Task 19): one short AI note per weakness change the R2
//! timeline detected, written by Task 15's [`explain_weakness_change`] on the lighter model.
//!
//! - [`enqueue_missing_change_notes`] runs on a coaching input (Task 18's report handler calls it
//!   right after the input snapshot). Every change in `input.changes`, and only those, gets a
//!   pending note keyed by [`change_key`], with its `WeaknessChangeInput`, and a job. A change
//!   that already has a note is left alone, so a second call returns 0 (idempotent per change).
//! - [`ExplainChangeHandler`] follows the weekly pattern ([`super::weekly_job`]): skip a row that
//!   is not `pending`; fail it with `invalid_input` (also when its input is not the change its key
//!   names) or `ai_disabled`; budget guard before every call; the retry schedule on the row with
//!   `Release` for `Unavailable`; every paid call metered in the state transaction. A ready note
//!   stores the text, segments, citations, model and disclosure; a note that failed validation
//!   twice is stored `dropped` with no text (nothing unvalidated is stored). Both emit
//!   `weakness.note.ready {changeKey, status}`. Terminal failures emit `job.failed`.

use futures::future::BoxFuture;
use queue::queue::Queue;
use queue::runner::{Handler, JobContext};
use queue::types::{Job, JobError};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use super::weekly_job::{
    AiItemDeps, Metering, REASON_AI_DISABLED, REASON_AI_UNAVAILABLE, REASON_INVALID_INPUT,
    REASON_JOB_DEAD, TxError, failure_reason, model_of, plan_and_scope, retryable, skipped,
};
use crate::client::model::Models;
use crate::client::port::AiError;
use crate::db::ItemStatus;
use crate::db::notes::{self, NewNote, NoteContent};
use crate::db::usage::AiFeature;
use crate::events::{EmitError, JOB_FAILED, JobFailed, NoteReady, WEAKNESS_NOTE_READY, emit};
use crate::generate::change::{ChangeOutcome, explain_weakness_change};
use crate::generate::section::{SectionError, SectionStatus};
use crate::input::types::CoachingInput;
use crate::jobs::budget::budget_guard;
use crate::jobs::kinds::{ExplainChangePayload, KIND_EXPLAIN_CHANGE, enqueue_change};
use crate::jobs::retry::retry_delay;
use crate::output::change::{WeaknessChangeInput, change_key};
use crate::output::disclosure::make_disclosure;
use crate::prompts::CHANGE_PROMPT_VERSION;

/// Inserts a pending note, and enqueues its job, for every change of `input` without a note;
/// returns how many were new. `input` must be `account_id`'s.
pub async fn enqueue_missing_change_notes(
    pool: &PgPool,
    queue: &Queue,
    account_id: Uuid,
    input: &CoachingInput,
) -> anyhow::Result<u32> {
    anyhow::ensure!(
        input.account_id == account_id,
        "the coaching input belongs to another account"
    );
    let mut created = 0u32;
    for change in &input.changes {
        let note_input = WeaknessChangeInput {
            change: change.clone(),
            band: input.band,
            time_class: input.time_class,
            basis: input.basis.clone(),
        };
        let row = {
            let mut conn = pool.acquire().await?;
            notes::insert_note_if_absent(
                &mut conn,
                &NewNote {
                    account_id,
                    change_key: change_key(change),
                    series_key: change.series_key.clone(),
                    onset_date: Some(change.onset_date),
                    input: serde_json::to_value(&note_input)?,
                },
            )
            .await?
        };
        if let Some(row) = row {
            enqueue_change(queue, row.id, account_id)
                .await
                .map_err(|e| anyhow::anyhow!("enqueue change note: {e}"))?;
            created += 1;
        }
    }
    Ok(created)
}

/// R1 `Handler` for `explain-weakness-change`.
pub struct ExplainChangeHandler(pub AiItemDeps);

impl ExplainChangeHandler {
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
        if notes::set_note_failed(&mut tx, id).await? {
            emit(
                &mut tx,
                account_id,
                JOB_FAILED,
                &JobFailed {
                    kind: KIND_EXPLAIN_CHANGE.to_owned(),
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
        key: &str,
        o: &ChangeOutcome,
        m: &Metering<'_>,
        now: OffsetDateTime,
    ) -> Result<Option<ItemStatus>, TxError> {
        let d = &self.0;
        let model = model_of(&o.calls, Models::EXPLAIN);
        let ready = match (o.status, &o.text, &o.segments) {
            (SectionStatus::Ready, Some(text), Some(segments)) if !o.citations.is_empty() => {
                Some(NoteContent {
                    text: text.clone(),
                    segments: serde_json::to_value(segments).map_err(EmitError::from)?,
                    citations: serde_json::to_value(&o.citations).map_err(EmitError::from)?,
                    model: model.clone(),
                    ai_disclosure: serde_json::to_value(make_disclosure(
                        &model,
                        CHANGE_PROMPT_VERSION,
                        now,
                    ))
                    .map_err(EmitError::from)?,
                })
            }
            _ => None,
        };
        let mut tx = d.pool.begin().await?;
        m.on(&mut tx, &d.pricing).await?;
        let (changed, status) = match &ready {
            Some(c) => (
                notes::set_note_ready(&mut tx, id, c).await?,
                ItemStatus::Ready,
            ),
            None => (
                notes::set_note_dropped(&mut tx, id, &model).await?,
                ItemStatus::Dropped,
            ),
        };
        if changed {
            emit(
                &mut tx,
                account_id,
                WEAKNESS_NOTE_READY,
                &NoteReady {
                    change_key: key.to_owned(),
                    status,
                },
            )
            .await?;
        }
        tx.commit().await?;
        Ok(changed.then_some(status))
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
        notes::set_note_retry(&mut tx, id, count, at).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn run(&self, job: Job) -> Result<Value, JobError> {
        let d = &self.0;
        let payload: ExplainChangePayload =
            serde_json::from_value(job.payload.clone()).map_err(|e| JobError::Unrecoverable {
                message: format!("explain-weakness-change payload: {e}"),
                code: None,
            })?;
        let id = payload.note_id;
        let row = {
            let mut conn = d.pool.acquire().await.map_err(|e| retryable("db", e))?;
            notes::get_note(&mut conn, id)
                .await
                .map_err(|e| retryable("load change note", e))?
        };
        let Some(row) = row else {
            return Ok(skipped("missing"));
        };
        if row.status != ItemStatus::Pending {
            return Ok(skipped(row.status.as_str()));
        }
        let account_id = row.account_id;
        let tx_err = |e: TxError| retryable("change note write", e);

        // Only a change the timeline detected gets a note: the stored input must be the very
        // change the row's key names.
        let input = serde_json::from_value::<WeaknessChangeInput>(row.input.clone())
            .ok()
            .filter(|i| change_key(&i.change) == row.change_key);
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
        let result = explain_weakness_change(ai.as_ref(), &guard, &input).await;
        let calls = match &result {
            Ok(o) => o.calls.clone(),
            Err(SectionError { calls, .. }) => calls.clone(),
        };
        let m = Metering {
            account_id,
            feature: AiFeature::WeaknessChange,
            ref_id: id.to_string(),
            model: Models::EXPLAIN,
            key: format!("wchange:{id}:{}", job.lease_token),
            calls: &calls,
        };
        let written = match result {
            Ok(o) => self
                .store(account_id, id, &row.change_key, &o, &m, now)
                .await
                .map(|s| match s {
                    Some(s) => json!({ "status": s.as_str() }),
                    None => skipped("not_pending"),
                }),
            Err(SectionError {
                error: AiError::Unavailable { retry_after, .. },
                ..
            }) => {
                let count = row.retry_count + 1;
                match retry_delay(count, retry_after) {
                    Some(delay) => match self.retry_later(id, count, now + delay, &m).await {
                        Ok(()) => {
                            return Err(JobError::Release {
                                delay,
                                reason: "AI unavailable".to_owned(),
                            });
                        }
                        Err(e) => Err(e),
                    },
                    None => self
                        .fail(account_id, id, REASON_AI_UNAVAILABLE, Some(&m))
                        .await
                        .map(|()| json!({ "failed": REASON_AI_UNAVAILABLE })),
                }
            }
            Err(SectionError { error, .. }) => {
                let reason = failure_reason(&error);
                if let AiError::Auth { status } = error {
                    tracing::error!(status, "AI credentials rejected (change note)");
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

impl Handler for ExplainChangeHandler {
    fn kind(&self) -> &'static str {
        KIND_EXPLAIN_CHANGE
    }

    fn handle(&self, job: Job, _ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        Box::pin(self.run(job))
    }

    fn on_dead<'a>(&'a self, job: &'a Job, _reason: &'a str) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let Ok(p) = serde_json::from_value::<ExplainChangePayload>(job.payload.clone()) else {
                return;
            };
            let row = match self.0.pool.acquire().await {
                Ok(mut c) => notes::get_note(&mut c, p.note_id).await.ok().flatten(),
                Err(_) => None,
            };
            if let Some(row) = row
                && let Err(e) = self
                    .fail(row.account_id, p.note_id, REASON_JOB_DEAD, None)
                    .await
            {
                tracing::error!(error = %e, "failing a dead change note failed");
            }
        })
    }
}
