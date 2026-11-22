//! AI user events (ADR 0059). They are emitted inside the transaction that writes the state they
//! describe, through R1's `insert_user_event`, so a client never sees an event for state that
//! did not commit, and a rolled-back write leaves no event.

use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::db::ItemStatus;
use crate::db::reports::ReportStatus;
use crate::output::codes::Citation;
use crate::segments::Segment;

pub const REPORT_SECTION_READY: &str = "report.section.ready";
pub const REPORT_READY: &str = "report.ready";
pub const REPORT_DEGRADED: &str = "report.degraded";
pub const WEEKLY_REVIEW_READY: &str = "weekly_review.ready";
pub const WEAKNESS_NOTE_READY: &str = "weakness.note.ready";
pub const CHAT_CHUNK: &str = "chat.chunk";
pub const CHAT_MESSAGE_READY: &str = "chat.message.ready";
pub const CHAT_MESSAGE_FAILED: &str = "chat.message.failed";
pub const JOB_FAILED: &str = "job.failed";

/// Who an event is for (`user_events.user_id`, ADR 0064, section 16).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Audience {
    /// Every user linked to the account (`user_id` NULL). The payload carries ids and codes
    /// only; the text lives in the account's own rows.
    Account,
    /// One user only: the event is about their chat thread, and `chat.chunk` carries model
    /// text. Emitted only through [`emit_for_user`]; [`emit`] refuses it.
    User,
}

/// Every event kind and its audience.
pub const EVENT_KINDS: [(&str, Audience); 9] = [
    (REPORT_SECTION_READY, Audience::Account),
    (REPORT_READY, Audience::Account),
    (REPORT_DEGRADED, Audience::Account),
    (WEEKLY_REVIEW_READY, Audience::Account),
    (WEAKNESS_NOTE_READY, Audience::Account),
    (CHAT_CHUNK, Audience::User),
    (CHAT_MESSAGE_READY, Audience::User),
    (CHAT_MESSAGE_FAILED, Audience::User),
    (JOB_FAILED, Audience::Account),
];

/// The audience of `ty`, `None` for a kind this crate does not emit.
pub fn audience(ty: &str) -> Option<Audience> {
    EVENT_KINDS.iter().find(|(k, _)| *k == ty).map(|(_, a)| *a)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionReady {
    pub report_id: Uuid,
    pub report_run_id: Uuid,
    /// `"computed"`, a `SectionKey`, or `"area:<id>"`.
    pub section: String,
    pub narrative: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportReady {
    pub report_id: Uuid,
    pub status: ReportStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DegradedReason {
    Unavailable,
    Budget,
    AiDisabled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportDegraded {
    pub report_id: Uuid,
    pub reason: DegradedReason,
    /// ISO time of the next attempt, or `null`.
    pub retry_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyReady {
    pub id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteReady {
    pub change_key: String,
    pub status: ItemStatus,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatChunk {
    pub message_id: Uuid,
    /// The turn's AI attempt (`coach_messages.turn_attempts`, 1 = the first). A retried turn
    /// streams again from `seq` 0, so a client keys chunks by `(attempt, seq)` and drops the
    /// chunks of any lower attempt.
    pub attempt: i16,
    pub seq: u32,
    pub text: String,
    pub segments: Vec<Segment>,
    pub citations: Vec<Citation>,
    /// An interim chunk may be replaced by the validated message.
    pub interim: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatReady {
    pub message_id: Uuid,
    /// The attempt that finished (0 when no attempt was reserved).
    pub attempt: i16,
    /// The validated text differs from what was streamed: the client replaces it.
    pub replace: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatFailed {
    pub message_id: Uuid,
    /// The message's last attempt (0 when none was reserved).
    pub attempt: i16,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobFailed {
    pub kind: String,
    pub reason: String,
}

#[derive(Debug, thiserror::Error)]
pub enum EmitError {
    #[error(transparent)]
    Events(#[from] ::events::store::Error),
    #[error("event payload does not serialise: {0}")]
    Json(#[from] serde_json::Error),
    /// A user-owned kind ([`Audience::User`]) emitted without its owner.
    #[error("event {0} belongs to one user; emit it with emit_for_user")]
    NeedsOwner(String),
    /// A kind missing from [`EVENT_KINDS`]: refused until it is classified (deny by default).
    #[error("event {0} is not in EVENT_KINDS")]
    UnknownKind(String),
}

/// Inserts one account event on `tx` (make it the last statement before the commit; see
/// `insert_user_event`). Returns the event id. A user-owned kind is refused: every user linked
/// to the account would receive it, and so is a kind missing from [`EVENT_KINDS`].
pub async fn emit(
    tx: &mut PgConnection,
    account_id: Uuid,
    ty: &str,
    payload: &impl Serialize,
) -> Result<i64, EmitError> {
    match audience(ty) {
        Some(Audience::Account) => {}
        Some(Audience::User) => return Err(EmitError::NeedsOwner(ty.to_owned())),
        None => return Err(EmitError::UnknownKind(ty.to_owned())),
    }
    let payload = serde_json::to_value(payload)?;
    let stored = ::events::store::insert_user_event(tx, account_id, ty, &payload).await?;
    Ok(stored.id)
}

/// [`emit`] for an event owned by one user: it is for that user only (`user_events.user_id`).
/// Every chat event is emitted this way, with the thread owner's user (review N1): a linked
/// account is shared between users, and readers deliver an event with a user only to that user.
pub async fn emit_for_user(
    tx: &mut PgConnection,
    account_id: Uuid,
    user_id: Uuid,
    ty: &str,
    payload: &impl Serialize,
) -> Result<i64, EmitError> {
    let payload = serde_json::to_value(payload)?;
    let stored =
        ::events::store::insert_owned_event(tx, account_id, Some(user_id), ty, &payload).await?;
    Ok(stored.id)
}
