//! Chat threads (Task 25): what the web calls to start a conversation, post a question, list and
//! read threads, and delete them.
//!
//! **Ownership (review I3).** A thread belongs to the USER who started it. Linked accounts are
//! shared between users (M4 `account_links`), so the account is only the thread's context: every
//! read, post and delete is scoped by `user_id`, and another user of the same account never sees,
//! posts to or deletes it (an id of theirs is simply not found). A thread is started, and posted
//! to, only for an account the user is currently linked to ([`AccountAccess`]).
//!
//! One question is answered at a time per conversation: [`post_user_message`] locks the
//! conversation row and refuses ([`ThreadError::Busy`]) while an assistant answer is pending or
//! streaming, so two turns of a thread are never queued together. (The `coach-chat` job also
//! holds a per-conversation lock, see `chat::job`.)
//!
//! Deletes are hard: messages go by cascade and the Agent SDK session entries by the
//! `coach_conversations_delete_session_entries` trigger, in the same transaction. `ai_usage` rows
//! (no content) and `interactive_evals` rows (their message link is set NULL) stay, so deleting
//! never refunds a budget or a quota.

use futures::future::BoxFuture;
use queue::queue::{Queue, QueueError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use super::anchor::ChatAnchor;
use super::limits::CHAT_LIMITS;
use super::summary::fallback_title;
use crate::db::chat::{self, ChatRole, ChatStatus, MessageRow, NewConversation, NewMessage};
use crate::input::types::TimeClass;
use crate::jobs::kinds::enqueue_chat;

/// The most conversations one list call returns.
pub const MAX_LIST: i64 = 100;

/// Whether a user may act for a platform account. Production uses [`AccountLinksAccess`] (M4's
/// `account_links`, Task 26); [`LegacyAccountAccess`] reads the M1 `linked_accounts.user_id`
/// column, for the CLI-era single owner and tests.
pub trait AccountAccess: Send + Sync {
    fn is_linked<'a>(
        &'a self,
        user_id: Uuid,
        account_id: Uuid,
    ) -> BoxFuture<'a, anyhow::Result<bool>>;
}

/// `linked_accounts.user_id = user` (the M1 CLI's single owner). Not used by the worker, which
/// wires [`AccountLinksAccess`], where any number of users may link one account.
pub struct LegacyAccountAccess(pub PgPool);

impl AccountAccess for LegacyAccountAccess {
    fn is_linked<'a>(
        &'a self,
        user_id: Uuid,
        account_id: Uuid,
    ) -> BoxFuture<'a, anyhow::Result<bool>> {
        Box::pin(async move {
            let ok: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM linked_accounts WHERE id = $1 AND user_id = $2)",
            )
            .bind(account_id)
            .bind(user_id)
            .fetch_one(&self.0)
            .await?;
            Ok(ok)
        })
    }
}

/// M4's `account_links` (ADR-WEB-3): any link of the user to the account, verified or not, as the
/// web's `userAccountIds` reads it. The worker wires this one.
pub struct AccountLinksAccess(pub PgPool);

impl AccountAccess for AccountLinksAccess {
    fn is_linked<'a>(
        &'a self,
        user_id: Uuid,
        account_id: Uuid,
    ) -> BoxFuture<'a, anyhow::Result<bool>> {
        Box::pin(async move {
            let ok: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM account_links \
                 WHERE user_id = $1 AND linked_account_id = $2)",
            )
            .bind(user_id)
            .bind(account_id)
            .fetch_one(&self.0)
            .await?;
            Ok(ok)
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ThreadError {
    /// No such conversation for this user.
    #[error("conversation not found")]
    NotFound,
    /// The user is not linked to the conversation's account.
    #[error("not linked to this account")]
    NotLinked,
    /// The link lookup failed.
    #[error("account access check failed")]
    Access(#[source] anyhow::Error),
    /// An answer of this conversation is still pending or streaming.
    #[error("an answer is still in progress")]
    Busy,
    /// A blank question.
    #[error("empty question")]
    Empty,
    /// Longer than `CHAT_LIMITS.max_question_chars`.
    #[error("question too long")]
    TooLong,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    /// The rows committed but the job was not enqueued; `ai-reconcile` fails the lost answer.
    #[error(transparent)]
    Queue(#[from] QueueError),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    pub id: Uuid,
    /// The stored title, or the anchor's fallback title.
    pub title: String,
    pub anchor: Option<ChatAnchor>,
    #[serde(with = "common::js_time::iso")]
    pub last_message_at: OffsetDateTime,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadMessage {
    pub id: Uuid,
    pub seq: i32,
    pub role: ChatRole,
    pub status: ChatStatus,
    pub anchor: Option<ChatAnchor>,
    pub text: Option<String>,
    pub segments: Option<Value>,
    pub citations: Value,
    /// Why a turn failed (`weekly_cap`, `unavailable`, …); the web maps it to fixed copy.
    pub failure_reason: Option<String>,
    pub ai_disclosure: Option<Value>,
    #[serde(with = "common::js_time::iso")]
    pub created_at: OffsetDateTime,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub conversation: ConversationSummary,
    pub time_class: String,
    pub messages: Vec<ThreadMessage>,
}

fn anchor_of(v: Option<&Value>) -> Option<ChatAnchor> {
    v.and_then(|v| serde_json::from_value(v.clone()).ok())
}

fn summary(c: &chat::ConversationRow) -> ConversationSummary {
    let anchor = anchor_of(c.anchor.as_ref());
    ConversationSummary {
        id: c.id,
        title: c
            .title
            .clone()
            .unwrap_or_else(|| fallback_title(anchor.as_ref()).to_owned()),
        anchor,
        last_message_at: c.last_message_at,
    }
}

fn message(m: MessageRow) -> ThreadMessage {
    ThreadMessage {
        id: m.id,
        seq: m.seq,
        role: m.role,
        status: m.status,
        anchor: anchor_of(m.anchor.as_ref()),
        text: m.text,
        segments: m.segments,
        citations: m.citations,
        failure_reason: m.failure_reason,
        ai_disclosure: m.ai_disclosure,
        created_at: m.created_at,
    }
}

async fn require_link(
    access: &dyn AccountAccess,
    user_id: Uuid,
    account_id: Uuid,
) -> Result<(), ThreadError> {
    match access.is_linked(user_id, account_id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(ThreadError::NotLinked),
        Err(e) => Err(ThreadError::Access(e)),
    }
}

/// Starts a conversation of `user_id` on `account_id` (untitled; the first answer sets the
/// title). The user must be linked to the account.
pub async fn create_conversation(
    pool: &PgPool,
    access: &dyn AccountAccess,
    user_id: Uuid,
    account_id: Uuid,
    tc: TimeClass,
    anchor: Option<ChatAnchor>,
) -> Result<Uuid, ThreadError> {
    require_link(access, user_id, account_id).await?;
    let mut conn = pool.acquire().await?;
    let row = chat::create_conversation(
        &mut conn,
        &NewConversation {
            account_id,
            user_id,
            time_class: tc.as_str().to_owned(),
            title: None,
            anchor: anchor.map(|a| serde_json::to_value(a).expect("anchors serialise")),
        },
    )
    .await?;
    Ok(row.id)
}

/// Posts a question to one of the user's conversations: inserts the user message and a pending
/// assistant message (bumping `last_message_at`) in one transaction, then enqueues `coach-chat`
/// for the assistant message. The user must still be linked to the conversation's account.
/// Returns `(user message, assistant message)`.
pub async fn post_user_message(
    pool: &PgPool,
    queue: &Queue,
    access: &dyn AccountAccess,
    user_id: Uuid,
    conversation_id: Uuid,
    question: &str,
    anchor: Option<ChatAnchor>,
) -> Result<(Uuid, Uuid), ThreadError> {
    if question.trim().is_empty() {
        return Err(ThreadError::Empty);
    }
    if question.chars().count() > CHAT_LIMITS.max_question_chars {
        return Err(ThreadError::TooLong);
    }
    let account_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT account_id FROM coach_conversations WHERE id = $1 AND user_id = $2",
    )
    .bind(conversation_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    let Some(account_id) = account_id else {
        return Err(ThreadError::NotFound);
    };
    require_link(access, user_id, account_id).await?;
    let mut tx = pool.begin().await?;
    // The row lock serialises posts to one conversation; the checks below are new statements,
    // so under READ COMMITTED they see a post that committed while this one waited.
    let found: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM coach_conversations WHERE id = $1 AND user_id = $2 FOR UPDATE",
    )
    .bind(conversation_id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    if found.is_none() {
        return Err(ThreadError::NotFound);
    }
    let busy: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM coach_messages WHERE conversation_id = $1 \
         AND role = 'assistant' AND status IN ('pending', 'streaming'))",
    )
    .bind(conversation_id)
    .fetch_one(&mut *tx)
    .await?;
    if busy {
        return Err(ThreadError::Busy);
    }
    let seq = chat::next_seq(&mut tx, conversation_id).await?;
    let user = chat::insert_message(
        &mut tx,
        &NewMessage {
            conversation_id,
            seq,
            role: ChatRole::User,
            status: ChatStatus::Done,
            anchor: anchor.map(|a| serde_json::to_value(a).expect("anchors serialise")),
            text: Some(question.to_owned()),
        },
    )
    .await?;
    let assistant = chat::insert_message(
        &mut tx,
        &NewMessage {
            conversation_id,
            seq: seq + 1,
            role: ChatRole::Assistant,
            status: ChatStatus::Pending,
            anchor: None,
            text: None,
        },
    )
    .await?;
    tx.commit().await?;
    enqueue_chat(queue, assistant.id, account_id).await?;
    Ok((user.id, assistant.id))
}

/// The user's conversations on `account_id`, most recently active first (`limit` is clamped to
/// 1..=100).
pub async fn list_conversations(
    pool: &PgPool,
    user_id: Uuid,
    account_id: Uuid,
    limit: i64,
) -> Result<Vec<ConversationSummary>, ThreadError> {
    let mut conn = pool.acquire().await?;
    let rows =
        chat::list_user_conversations(&mut conn, user_id, account_id, limit.clamp(1, MAX_LIST))
            .await?;
    Ok(rows.iter().map(summary).collect())
}

/// One of the user's conversations with its messages in order, or `None`.
pub async fn get_thread(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<Option<Thread>, ThreadError> {
    let mut conn = pool.acquire().await?;
    let Some(c) = chat::get_user_conversation(&mut conn, user_id, id).await? else {
        return Ok(None);
    };
    let messages = chat::list_messages(&mut conn, id).await?;
    Ok(Some(Thread {
        conversation: summary(&c),
        time_class: c.time_class.clone(),
        messages: messages.into_iter().map(message).collect(),
    }))
}

/// Hard-deletes one of the user's conversations. `false` when it does not exist or belongs to
/// another user. A user may delete their threads after unlinking the account.
pub async fn delete_conversation(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<bool, ThreadError> {
    let mut tx = pool.begin().await?;
    let deleted = chat::delete_user_conversation(&mut tx, user_id, id).await?;
    tx.commit().await?;
    Ok(deleted)
}

/// Hard-deletes all of the user's conversations (on every account); returns how many.
pub async fn delete_all_conversations(pool: &PgPool, user_id: Uuid) -> Result<u64, ThreadError> {
    let mut tx = pool.begin().await?;
    let n = chat::delete_all_user_conversations(&mut tx, user_id).await?;
    tx.commit().await?;
    Ok(n)
}
