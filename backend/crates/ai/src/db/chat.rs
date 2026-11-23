//! `coach_conversations` and `coach_messages`, the system of record for the coach chat
//! (ADR 0059, ADR 0062 amendment 1). The Agent SDK's transcript lives in
//! `ai_sdk_session_entries` (session id = conversation id). The trigger
//! `coach_conversations_delete_session_entries` deletes a conversation's entries on every delete
//! path; [`sweep_orphan_session_entries_page`] (run by `ai-reconcile`) removes entries the sidecar wrote
//! for a conversation that never committed or was deleted mid-turn.
//!
//! Message states: `pending` (the job is queued) → `streaming` → `done` | `failed`. User
//! messages are inserted `done`.

use std::time::Duration;

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
    /// The user who owns the thread (threads are per user; accounts are shared).
    pub user_id: Uuid,
    pub time_class: String,
    pub title: Option<String>,
    pub anchor: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct ConversationRow {
    pub id: Uuid,
    pub account_id: Uuid,
    pub user_id: Uuid,
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
    /// The SDK's cumulative per-session `modelUsage` after this turn.
    pub session_model_usage: Option<Value>,
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
    /// The SDK's cumulative per-session `modelUsage` after this turn (the next turn meters the
    /// difference; see [`previous_session_model_usage`]).
    pub session_model_usage: Option<Value>,
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
    ai_disclosure, session, session_model_usage, created_at";

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

/// `pending` → `streaming`, stamping `streaming_at` (`ai-reconcile` fails a turn that streams
/// for longer than `jobs::kinds::CHAT_TURN_TIMEOUT` with no live job).
pub async fn set_streaming(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE coach_messages SET status = 'streaming', streaming_at = now() \
         WHERE id = $1 AND status = 'pending'",
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
    let r = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{}UPDATE coach_messages SET status = 'done', text = $2, segments = $3, citations = $4, \
         api_messages = $5, dropped_count = $6, ai_disclosure = $7, session = $8, \
         session_model_usage = $9, algorithm_id = $10, \
         failure_reason = NULL WHERE id = $1 AND status IN ('pending', 'streaming')",
        common::algorithm::register_cte(10, 11)
    )))
    .bind(id)
    .bind(&f.text)
    .bind(&f.segments)
    .bind(&f.citations)
    .bind(&f.api_messages)
    .bind(f.dropped_count)
    .bind(&f.ai_disclosure)
    .bind(f.session.map(SessionPath::as_str))
    .bind(&f.session_model_usage)
    .bind(common::algorithm::id())
    .bind(common::algorithm::manifest())
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

/// The user's conversations on one account, most recently active first.
pub async fn list_user_conversations(
    conn: &mut PgConnection,
    user_id: Uuid,
    account_id: Uuid,
    limit: i64,
) -> Result<Vec<ConversationRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {CONV_COLS} FROM coach_conversations WHERE user_id = $1 AND account_id = $2 \
         ORDER BY last_message_at DESC LIMIT $3"
    )))
    .bind(user_id)
    .bind(account_id)
    .bind(limit)
    .fetch_all(conn)
    .await
}

/// One of the user's conversations, or `None` (another user's id is not found).
pub async fn get_user_conversation(
    conn: &mut PgConnection,
    user_id: Uuid,
    id: Uuid,
) -> Result<Option<ConversationRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {CONV_COLS} FROM coach_conversations WHERE id = $1 AND user_id = $2"
    )))
    .bind(id)
    .bind(user_id)
    .fetch_optional(conn)
    .await
}

/// Deletes one of the user's conversations (messages by cascade, session entries by trigger).
/// `false` when it does not exist or belongs to another user.
pub async fn delete_user_conversation(
    conn: &mut PgConnection,
    user_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query("DELETE FROM coach_conversations WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(conn)
        .await?;
    Ok(r.rows_affected() == 1)
}

/// Deletes all of the user's conversations; returns how many.
pub async fn delete_all_user_conversations(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let r = sqlx::query("DELETE FROM coach_conversations WHERE user_id = $1")
        .bind(user_id)
        .execute(conn)
        .await?;
    Ok(r.rows_affected())
}

/// The previous assistant turn's `session_model_usage` in a conversation (the newest one with
/// `seq < before_seq` that has a value), so a turn meters only its own share of the SDK's
/// cumulative per-session usage. `None` for the first metered turn or after a rebuilt session.
pub async fn previous_session_model_usage(
    conn: &mut PgConnection,
    conversation_id: Uuid,
    before_seq: i32,
) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT session_model_usage FROM coach_messages WHERE conversation_id = $1 \
         AND seq < $2 AND role = 'assistant' AND session_model_usage IS NOT NULL \
         ORDER BY seq DESC LIMIT 1",
    )
    .bind(conversation_id)
    .bind(before_seq)
    .fetch_optional(conn)
    .await
}

/// Session entries older than this with no conversation are orphans.
pub const ORPHAN_SESSION_MIN_AGE: Duration = Duration::from_secs(600);

/// Orphans are only ever young: the delete trigger removes a deleted conversation's entries, so
/// an orphan is a first turn whose conversation never committed, or a turn that wrote after its
/// conversation was deleted. The sweep looks no further back than this.
pub const ORPHAN_SESSION_MAX_AGE: Duration = Duration::from_secs(24 * 3600);

/// Rows deleted per orphan-sweep page.
pub const ORPHAN_SWEEP_PAGE: i64 = 1000;

/// One page of session entries in the sweep window, oldest first, orphan or not: `$1` the window
/// start (now − [`ORPHAN_SESSION_MAX_AGE`]), `$2` the cursor (NULL first), `$3` the window end
/// (now − [`ORPHAN_SESSION_MIN_AGE`]), `$4` the limit. It reads
/// `ai_sdk_session_entries_created_idx` in order (plan-guarded, custom and generic plan). The
/// orphans among its rows are deleted; live ones are passed, so a page reads at most `$4` rows
/// however many live entries the window holds (T21 re-review).
pub const ORPHAN_PAGE_SQL: &str = "SELECT e.ctid, e.created_at, e.session_id \
    FROM ai_sdk_session_entries e \
    WHERE e.created_at >= greatest($1::timestamptz, \
    coalesce($2::timestamptz, '-infinity'::timestamptz)) AND e.created_at < $3::timestamptz \
    ORDER BY e.created_at LIMIT $4";

/// The `ai_sweep_cursors` row of the orphan sweep.
pub const ORPHAN_CURSOR: &str = "orphan_session_entries";

/// One page of the orphan sweep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrphanPage {
    pub deleted: u64,
    /// The next cursor; `None` when the page was short (the window is done).
    pub next: Option<OffsetDateTime>,
    /// The newest `created_at` the page read (`None` for an empty page).
    pub newest: Option<OffsetDateTime>,
}

/// Reads one page of entries created between `now - ORPHAN_SESSION_MAX_AGE` and
/// `now - ORPHAN_SESSION_MIN_AGE` (the sidecar may write a new conversation's first entries
/// before Rust commits it), oldest first from `after`, and deletes those whose session has no
/// conversation.
///
/// The cursor is inclusive (`>=` the page's newest time): entries tied with it that did not fit
/// are read again. A full page whose rows all share one time cannot move it, so that instant's
/// orphans are deleted at once (one instant is one transaction's writes) and the cursor steps
/// past it. Every full page therefore advances.
pub async fn sweep_orphan_session_entries_page(
    conn: &mut PgConnection,
    now: OffsetDateTime,
    after: Option<OffsetDateTime>,
    limit: i64,
) -> Result<OrphanPage, sqlx::Error> {
    let limit = limit.max(1);
    let (scanned, oldest, newest, deleted): (
        i64,
        Option<OffsetDateTime>,
        Option<OffsetDateTime>,
        i64,
    ) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "WITH page AS ({ORPHAN_PAGE_SQL}), \
         gone AS (DELETE FROM ai_sdk_session_entries d USING page p WHERE d.ctid = p.ctid \
           AND NOT EXISTS (SELECT 1 FROM coach_conversations c WHERE c.id = p.session_id) \
           RETURNING 1) \
         SELECT (SELECT count(*) FROM page), (SELECT min(created_at) FROM page), \
           (SELECT max(created_at) FROM page), (SELECT count(*) FROM gone)"
    )))
    .bind(now - ORPHAN_SESSION_MAX_AGE)
    .bind(after)
    .bind(now - ORPHAN_SESSION_MIN_AGE)
    .bind(limit)
    .fetch_one(&mut *conn)
    .await?;
    let mut deleted = u64::try_from(deleted).unwrap_or(0);
    let next = match (oldest, newest) {
        (Some(oldest), Some(newest)) if scanned >= limit => {
            if oldest == newest {
                let tied = sqlx::query(
                    "DELETE FROM ai_sdk_session_entries e WHERE e.created_at = $1 \
                     AND NOT EXISTS (SELECT 1 FROM coach_conversations c WHERE c.id = e.session_id)",
                )
                .bind(newest)
                .execute(&mut *conn)
                .await?;
                deleted += tied.rows_affected();
                Some(newest + Duration::from_micros(1))
            } else {
                Some(newest)
            }
        }
        _ => None,
    };
    Ok(OrphanPage {
        deleted,
        next,
        newest,
    })
}

/// Where the last orphan sweep stopped (`None` before the first one).
pub async fn orphan_cursor(conn: &mut PgConnection) -> Result<Option<OffsetDateTime>, sqlx::Error> {
    sqlx::query_scalar("SELECT position FROM ai_sweep_cursors WHERE name = $1")
        .bind(ORPHAN_CURSOR)
        .fetch_optional(conn)
        .await
}

/// Stores where the orphan sweep stopped.
pub async fn set_orphan_cursor(
    conn: &mut PgConnection,
    position: OffsetDateTime,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO ai_sweep_cursors (name, position) VALUES ($1, $2) \
         ON CONFLICT (name) DO UPDATE SET position = EXCLUDED.position, updated_at = now()",
    )
    .bind(ORPHAN_CURSOR)
    .bind(position)
    .execute(conn)
    .await?;
    Ok(())
}
