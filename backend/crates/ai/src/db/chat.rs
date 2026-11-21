//! `coach_conversations` and `coach_messages`, the system of record for the coach chat
//! (ADR-R3-3, ADR-R3-6 amendment 1). The Agent SDK's transcript lives in
//! `ai_sdk_session_entries` (session id = conversation id); deleting a conversation deletes it too.
//!
//! Message states: `pending` (the job is queued) → `streaming` → `done` | `failed`. User
//! messages are inserted `done`.

use serde_json::Value;
use sqlx::PgConnection;
use time::OffsetDateTime;
use uuid::Uuid;

use super::{SessionPath, text_enum};

text_enum!(
    /// `chat_role`.
    ChatRole, "chat_role", {
        User => "user",
        Assistant => "assistant",
    }
);

text_enum!(
    /// `chat_status`.
    ChatStatus, "chat_status", {
        Pending => "pending",
        Streaming => "streaming",
        Done => "done",
        Failed => "failed",
    }
);

#[derive(Clone, Debug, PartialEq)]
pub struct NewConversation {
    pub account_id: Uuid,
    pub user_id: Option<Uuid>,
    pub time_class: String,
    pub title: Option<String>,
    pub anchor: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct ConversationRow {
    pub id: Uuid,
    pub account_id: Uuid,
    pub user_id: Option<Uuid>,
    pub time_class: String,
    pub title: Option<String>,
    pub anchor: Option<Value>,
    pub summary: Option<String>,
    pub summary_through_seq: i32,
    pub created_at: OffsetDateTime,
    pub last_message_at: OffsetDateTime,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NewMessage {
    pub conversation_id: Uuid,
    pub seq: i32,
    pub role: ChatRole,
    pub status: ChatStatus,
    pub anchor: Option<Value>,
    pub text: Option<String>,
}

#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct MessageRow {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub seq: i32,
    #[sqlx(try_from = "String")]
    pub role: ChatRole,
    #[sqlx(try_from = "String")]
    pub status: ChatStatus,
    pub anchor: Option<Value>,
    pub text: Option<String>,
    pub segments: Option<Value>,
    pub citations: Value,
    pub api_messages: Option<Value>,
    pub dropped_count: i32,
    pub failure_reason: Option<String>,
    pub ai_disclosure: Option<Value>,
    #[sqlx(try_from = "OptText")]
    pub session: Option<SessionPath>,
    pub created_at: OffsetDateTime,
}

/// The finished assistant turn.
#[derive(Clone, Debug, PartialEq)]
pub struct MessageFinish {
    pub text: String,
    pub segments: Value,
    pub citations: Value,
    pub api_messages: Option<Value>,
    pub dropped_count: i32,
    pub ai_disclosure: Option<Value>,
    pub session: Option<SessionPath>,
}

/// A nullable text column, decoded through `TryFrom` (sqlx's `try_from` needs a named type).
#[derive(sqlx::Type)]
#[sqlx(transparent)]
pub struct OptText(Option<String>);

impl TryFrom<OptText> for Option<SessionPath> {
    type Error = super::ParseEnumError;
    fn try_from(v: OptText) -> Result<Self, Self::Error> {
        v.0.map(SessionPath::try_from).transpose()
    }
}

const CONV_COLS: &str = "id, account_id, user_id, time_class::text AS time_class, title, anchor, \
    summary, summary_through_seq, created_at, last_message_at";

const MSG_COLS: &str = "id, conversation_id, seq, role::text AS role, status::text AS status, \
    anchor, text, segments, citations, api_messages, dropped_count, failure_reason, \
    ai_disclosure, session, created_at";

pub async fn create_conversation(
    conn: &mut PgConnection,
    c: &NewConversation,
) -> Result<ConversationRow, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "INSERT INTO coach_conversations (account_id, user_id, time_class, title, anchor) \
         VALUES ($1, $2, $3::time_class, $4, $5) RETURNING {CONV_COLS}"
    )))
    .bind(c.account_id)
    .bind(c.user_id)
    .bind(&c.time_class)
    .bind(&c.title)
    .bind(&c.anchor)
    .fetch_one(conn)
    .await
}

pub async fn get_conversation(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<Option<ConversationRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {CONV_COLS} FROM coach_conversations WHERE id = $1"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await
}

/// The account's conversations, most recently active first.
pub async fn list_conversations(
    conn: &mut PgConnection,
    account_id: Uuid,
    limit: i64,
) -> Result<Vec<ConversationRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {CONV_COLS} FROM coach_conversations WHERE account_id = $1 \
         ORDER BY last_message_at DESC LIMIT $2"
    )))
    .bind(account_id)
    .bind(limit)
    .fetch_all(conn)
    .await
}

pub async fn set_title(
    conn: &mut PgConnection,
    id: Uuid,
    title: &str,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query("UPDATE coach_conversations SET title = $2 WHERE id = $1")
        .bind(id)
        .bind(title)
        .execute(conn)
        .await?;
    Ok(r.rows_affected() == 1)
}

/// Stores the rollover summary covering messages up to `through_seq`. It never moves back.
pub async fn set_summary(
    conn: &mut PgConnection,
    id: Uuid,
    summary: &str,
    through_seq: i32,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE coach_conversations SET summary = $2, summary_through_seq = $3 \
         WHERE id = $1 AND summary_through_seq <= $3",
    )
    .bind(id)
    .bind(summary)
    .bind(through_seq)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// The next free `seq` (1 for an empty conversation). Two writers racing get the same number;
/// the unique `(conversation_id, seq)` rejects the second insert.
pub async fn next_seq(conn: &mut PgConnection, conversation_id: Uuid) -> Result<i32, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT coalesce(max(seq), 0) + 1 FROM coach_messages WHERE conversation_id = $1",
    )
    .bind(conversation_id)
    .fetch_one(conn)
    .await
}

/// Inserts a message and bumps the conversation's `last_message_at`.
pub async fn insert_message(
    conn: &mut PgConnection,
    m: &NewMessage,
) -> Result<MessageRow, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "WITH ins AS (INSERT INTO coach_messages (conversation_id, seq, role, status, anchor, text) \
         VALUES ($1, $2, $3::chat_role, $4::chat_status, $5, $6) RETURNING *), \
         touch AS (UPDATE coach_conversations SET last_message_at = now() \
         WHERE id = (SELECT conversation_id FROM ins)) \
         SELECT {MSG_COLS} FROM ins"
    )))
    .bind(m.conversation_id)
    .bind(m.seq)
    .bind(m.role.as_str())
    .bind(m.status.as_str())
    .bind(&m.anchor)
    .bind(&m.text)
    .fetch_one(conn)
    .await
}

pub async fn get_message(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<Option<MessageRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {MSG_COLS} FROM coach_messages WHERE id = $1"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await
}

/// A conversation's messages in `seq` order.
pub async fn list_messages(
    conn: &mut PgConnection,
    conversation_id: Uuid,
) -> Result<Vec<MessageRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {MSG_COLS} FROM coach_messages WHERE conversation_id = $1 ORDER BY seq"
    )))
    .bind(conversation_id)
    .fetch_all(conn)
    .await
}

/// `pending` → `streaming`.
pub async fn set_streaming(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE coach_messages SET status = 'streaming' WHERE id = $1 AND status = 'pending'",
    )
    .bind(id)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// An unfinished assistant message becomes `done`.
pub async fn finish_message(
    conn: &mut PgConnection,
    id: Uuid,
    f: &MessageFinish,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE coach_messages SET status = 'done', text = $2, segments = $3, citations = $4, \
         api_messages = $5, dropped_count = $6, ai_disclosure = $7, session = $8, \
         failure_reason = NULL WHERE id = $1 AND status IN ('pending', 'streaming')",
    )
    .bind(id)
    .bind(&f.text)
    .bind(&f.segments)
    .bind(&f.citations)
    .bind(&f.api_messages)
    .bind(f.dropped_count)
    .bind(&f.ai_disclosure)
    .bind(f.session.map(SessionPath::as_str))
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// An unfinished assistant message becomes `failed`.
pub async fn fail_message(
    conn: &mut PgConnection,
    id: Uuid,
    reason: &str,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE coach_messages SET status = 'failed', failure_reason = $2 \
         WHERE id = $1 AND status IN ('pending', 'streaming')",
    )
    .bind(id)
    .bind(reason)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// Deletes one of the account's conversations, its messages (cascade) and its Agent SDK session
/// entries, in one statement (so one transaction). `interactive_evals.message_id` is set NULL;
/// the eval rows stay, so deleting never refunds the engine-check quota. `false` when the
/// conversation does not exist or belongs to another account.
pub async fn delete_conversation(
    conn: &mut PgConnection,
    account_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let n: i64 = sqlx::query_scalar(
        "WITH c AS (DELETE FROM coach_conversations WHERE id = $1 AND account_id = $2 RETURNING id), \
         s AS (DELETE FROM ai_sdk_session_entries WHERE session_id IN (SELECT id FROM c)) \
         SELECT count(*) FROM c",
    )
    .bind(id)
    .bind(account_id)
    .fetch_one(conn)
    .await?;
    Ok(n == 1)
}

/// Deletes all of the account's conversations and their session entries in one statement.
/// Returns the number of conversations deleted.
pub async fn delete_all_conversations(
    conn: &mut PgConnection,
    account_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let n: i64 = sqlx::query_scalar(
        "WITH c AS (DELETE FROM coach_conversations WHERE account_id = $1 RETURNING id), \
         s AS (DELETE FROM ai_sdk_session_entries WHERE session_id IN (SELECT id FROM c)) \
         SELECT count(*) FROM c",
    )
    .bind(account_id)
    .fetch_one(conn)
    .await?;
    Ok(u64::try_from(n).unwrap_or(0))
}
