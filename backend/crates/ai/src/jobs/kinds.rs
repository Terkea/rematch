//! AI job kinds on R1's Postgres queue (ADR 0032, ADR-R3-3). They replace the TS BullMQ queues.
//!
//! `user_key` is the account id (fair scheduling). Payloads are camelCase JSON; the web (M4)
//! enqueues `coach-chat` jobs through the same `jobs` table with [`CoachChatPayload`].
//!
//! `Queue::enqueue` commits in its own transaction, so a row can commit while its enqueue is
//! lost (an error or a crash in between). `ai-reconcile` is the recovery: it re-enqueues unfinished
//! rows that have no live job, and the dedupe keys make a double enqueue harmless.

use std::time::Duration;

use queue::EnqueueOptions;
use queue::queue::{Queue, QueueError};
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, Time, Weekday};
use uuid::Uuid;

pub const KIND_GENERATE_REPORT: &str = "generate-report";
pub const KIND_WEEKLY_REVIEW: &str = "weekly-review";
pub const KIND_EXPLAIN_CHANGE: &str = "explain-weakness-change";
pub const KIND_COACH_CHAT: &str = "coach-chat";
pub const KIND_INTERACTIVE_ANALYSE: &str = "interactive-analyse";
pub const KIND_REPORT_BATCH_SCHEDULE: &str = "report-batch-schedule";
pub const KIND_WEEKLY_BATCH_SCHEDULE: &str = "weekly-batch-schedule";
pub const KIND_AI_BATCH_POLL: &str = "ai-batch-poll";
pub const KIND_AI_RECONCILE: &str = "ai-reconcile";

pub const REPORT_PRIORITY: i16 = 50;
pub const REPORT_MAX_ATTEMPTS: i32 = 3;
pub const WEEKLY_PRIORITY: i16 = 100;
pub const CHANGE_PRIORITY: i16 = 100;
pub const CHAT_PRIORITY: i16 = 10;
pub const CHAT_MAX_ATTEMPTS: i32 = 2;
pub const INTERACTIVE_PRIORITY: i16 = 1;
pub const INTERACTIVE_MAX_ATTEMPTS: i32 = 1;

pub const REPORT_BATCH_EVERY: Duration = Duration::from_secs(24 * 3600);
/// The report batch schedule first runs at the next 03:00 UTC.
pub const REPORT_BATCH_HOUR: u8 = 3;
pub const WEEKLY_BATCH_EVERY: Duration = Duration::from_secs(7 * 24 * 3600);
/// The weekly batch schedule first runs at the next Monday 04:00 UTC.
pub const WEEKLY_BATCH_HOUR: u8 = 4;
pub const AI_BATCH_POLL_EVERY: Duration = Duration::from_secs(600);
pub const AI_RECONCILE_EVERY: Duration = Duration::from_secs(300);

/// The longest a coach-chat turn may run. T25 passes it to the sidecar as `timeoutMs`;
/// `ai-reconcile` fails a turn that has streamed for longer than this plus the sidecar's 30 s
/// slack and a 60 s grace, with no live job.
pub const CHAT_TURN_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateReportPayload {
    pub report_run_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyReviewPayload {
    pub weekly_review_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainChangePayload {
    pub note_id: Uuid,
}

/// The plan is not in the payload: the handler asks `Entitlements`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoachChatPayload {
    pub message_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InteractiveAnalysePayload {
    pub eval_id: Uuid,
}

pub fn report_key(run_id: Uuid) -> String {
    format!("report:{run_id}")
}

pub fn weekly_key(weekly_review_id: Uuid) -> String {
    format!("weekly:{weekly_review_id}")
}

pub fn change_key(note_id: Uuid) -> String {
    format!("wchange:{note_id}")
}

pub fn chat_key(message_id: Uuid) -> String {
    format!("chat:{message_id}")
}

pub fn ieval_key(eval_id: Uuid) -> String {
    format!("ieval:{eval_id}")
}

async fn enqueue_one(
    queue: &Queue,
    kind: &str,
    payload: &impl Serialize,
    dedupe_key: String,
    account_id: Uuid,
    priority: i16,
    max_attempts: Option<i32>,
) -> Result<Option<i64>, QueueError> {
    let payload = serde_json::to_value(payload).expect("payload structs always serialise");
    let defaults = EnqueueOptions::default();
    let opts = EnqueueOptions {
        dedupe_key: Some(dedupe_key),
        user_key: Some(account_id),
        priority,
        max_attempts: max_attempts.unwrap_or(defaults.max_attempts),
        ..defaults
    };
    queue.enqueue(kind, &payload, opts).await
}

/// `None` when a live job already holds `report:<run_id>`.
pub async fn enqueue_report(
    queue: &Queue,
    run_id: Uuid,
    account_id: Uuid,
) -> Result<Option<i64>, QueueError> {
    let payload = GenerateReportPayload {
        report_run_id: run_id,
    };
    let key = report_key(run_id);
    enqueue_one(
        queue,
        KIND_GENERATE_REPORT,
        &payload,
        key,
        account_id,
        REPORT_PRIORITY,
        Some(REPORT_MAX_ATTEMPTS),
    )
    .await
}

pub async fn enqueue_weekly(
    queue: &Queue,
    weekly_review_id: Uuid,
    account_id: Uuid,
) -> Result<Option<i64>, QueueError> {
    let payload = WeeklyReviewPayload { weekly_review_id };
    let key = weekly_key(weekly_review_id);
    enqueue_one(
        queue,
        KIND_WEEKLY_REVIEW,
        &payload,
        key,
        account_id,
        WEEKLY_PRIORITY,
        None,
    )
    .await
}

pub async fn enqueue_change(
    queue: &Queue,
    note_id: Uuid,
    account_id: Uuid,
) -> Result<Option<i64>, QueueError> {
    let payload = ExplainChangePayload { note_id };
    let key = change_key(note_id);
    enqueue_one(
        queue,
        KIND_EXPLAIN_CHANGE,
        &payload,
        key,
        account_id,
        CHANGE_PRIORITY,
        None,
    )
    .await
}

pub async fn enqueue_chat(
    queue: &Queue,
    message_id: Uuid,
    account_id: Uuid,
) -> Result<Option<i64>, QueueError> {
    let payload = CoachChatPayload { message_id };
    let key = chat_key(message_id);
    enqueue_one(
        queue,
        KIND_COACH_CHAT,
        &payload,
        key,
        account_id,
        CHAT_PRIORITY,
        Some(CHAT_MAX_ATTEMPTS),
    )
    .await
}

pub async fn enqueue_interactive(
    queue: &Queue,
    eval_id: Uuid,
    account_id: Uuid,
) -> Result<Option<i64>, QueueError> {
    let payload = InteractiveAnalysePayload { eval_id };
    let key = ieval_key(eval_id);
    enqueue_one(
        queue,
        KIND_INTERACTIVE_ANALYSE,
        &payload,
        key,
        account_id,
        INTERACTIVE_PRIORITY,
        Some(INTERACTIVE_MAX_ATTEMPTS),
    )
    .await
}

/// The first instant strictly after `now` at `hour`:00 UTC.
pub fn next_daily_at(now: OffsetDateTime, hour: u8) -> OffsetDateTime {
    let now = now.to_offset(time::UtcOffset::UTC);
    let at = Time::from_hms(hour, 0, 0).expect("hour < 24");
    let today = now.replace_time(at);
    if today > now {
        today
    } else {
        today + time::Duration::days(1)
    }
}

/// The first Monday `hour`:00 UTC strictly after `now`.
pub fn next_monday_at(now: OffsetDateTime, hour: u8) -> OffsetDateTime {
    let mut at = next_daily_at(now, hour);
    while at.weekday() != Weekday::Monday {
        at += time::Duration::days(1);
    }
    at
}

/// Enqueues the four recurring AI jobs, each deduplicated on its kind, so calling it at every
/// worker start is idempotent (a recurring job stays live: it is re-queued, never finished).
pub async fn ensure_ai_recurring(queue: &Queue) -> Result<(), QueueError> {
    let now = OffsetDateTime::now_utc();
    let recurring = [
        (
            KIND_REPORT_BATCH_SCHEDULE,
            REPORT_BATCH_EVERY,
            Some(next_daily_at(now, REPORT_BATCH_HOUR)),
        ),
        (
            KIND_WEEKLY_BATCH_SCHEDULE,
            WEEKLY_BATCH_EVERY,
            Some(next_monday_at(now, WEEKLY_BATCH_HOUR)),
        ),
        (KIND_AI_BATCH_POLL, AI_BATCH_POLL_EVERY, None),
        (KIND_AI_RECONCILE, AI_RECONCILE_EVERY, None),
    ];
    for (kind, every, run_at) in recurring {
        let opts = EnqueueOptions {
            dedupe_key: Some(kind.to_owned()),
            run_at,
            recur_every: Some(every),
            ..EnqueueOptions::default()
        };
        queue.enqueue(kind, &serde_json::json!({}), opts).await?;
    }
    Ok(())
}
