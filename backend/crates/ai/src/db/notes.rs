//! `weakness_change_notes`: one short AI note per detected weakness change (ADR-R3-3).
//!
//! States (`ai_item_status`): `pending` → `ready` | `dropped` (validation failed twice) |
//! `failed` (retries exhausted). A transient failure stays `pending` with `next_attempt_at`.

use serde_json::Value;
use sqlx::PgConnection;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::ItemStatus;

#[derive(Clone, Debug, PartialEq)]
pub struct NewNote {
    pub account_id: Uuid,
    pub change_key: String,
    pub series_key: String,
    pub onset_date: Option<Date>,
    pub input: Value,
}

#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct NoteRow {
    pub id: Uuid,
    pub account_id: Uuid,
    pub change_key: String,
    pub series_key: String,
    pub onset_date: Option<Date>,
    #[sqlx(try_from = "String")]
    pub status: ItemStatus,
    pub input: Value,
    pub text: Option<String>,
    pub segments: Option<Value>,
    pub citations: Value,
    pub model: Option<String>,
    pub retry_count: i32,
    pub next_attempt_at: Option<OffsetDateTime>,
    pub ai_disclosure: Option<Value>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// A validated note.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteContent {
    pub text: String,
    pub segments: Value,
    pub citations: Value,
    pub model: String,
    pub ai_disclosure: Value,
}

const COLS: &str = "id, account_id, change_key, series_key, onset_date, status::text AS status, \
    input, text, segments, citations, model, retry_count, next_attempt_at, ai_disclosure, \
    created_at, updated_at";

/// Inserts a pending note; `None` when (account, change key) already has one.
pub async fn insert_note_if_absent(
    conn: &mut PgConnection,
    note: &NewNote,
) -> Result<Option<NoteRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "INSERT INTO weakness_change_notes (account_id, change_key, series_key, onset_date, input) \
         VALUES ($1, $2, $3, $4, $5) ON CONFLICT (account_id, change_key) DO NOTHING \
         RETURNING {COLS}"
    )))
    .bind(note.account_id)
    .bind(&note.change_key)
    .bind(&note.series_key)
    .bind(note.onset_date)
    .bind(&note.input)
    .fetch_optional(conn)
    .await
}

pub async fn get_note(conn: &mut PgConnection, id: Uuid) -> Result<Option<NoteRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM weakness_change_notes WHERE id = $1"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await
}

pub async fn get_note_by_key(
    conn: &mut PgConnection,
    account_id: Uuid,
    change_key: &str,
) -> Result<Option<NoteRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM weakness_change_notes WHERE account_id = $1 AND change_key = $2"
    )))
    .bind(account_id)
    .bind(change_key)
    .fetch_optional(conn)
    .await
}

/// The account's notes, newest first.
pub async fn list_notes(
    conn: &mut PgConnection,
    account_id: Uuid,
    limit: i64,
) -> Result<Vec<NoteRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM weakness_change_notes WHERE account_id = $1 \
         ORDER BY created_at DESC LIMIT $2"
    )))
    .bind(account_id)
    .bind(limit)
    .fetch_all(conn)
    .await
}

pub async fn set_note_ready(
    conn: &mut PgConnection,
    id: Uuid,
    c: &NoteContent,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE weakness_change_notes SET status = 'ready', text = $2, segments = $3, \
         citations = $4, model = $5, ai_disclosure = $6, next_attempt_at = NULL, \
         updated_at = now() WHERE id = $1 AND status = 'pending'",
    )
    .bind(id)
    .bind(&c.text)
    .bind(&c.segments)
    .bind(&c.citations)
    .bind(&c.model)
    .bind(&c.ai_disclosure)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// The note failed validation twice: `dropped` (the computed change is still shown).
pub async fn set_note_dropped(
    conn: &mut PgConnection,
    id: Uuid,
    model: &str,
) -> Result<bool, sqlx::Error> {
    finish(conn, id, "dropped", Some(model)).await
}

/// Retries are exhausted.
pub async fn set_note_failed(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
    finish(conn, id, "failed", None).await
}

async fn finish(
    conn: &mut PgConnection,
    id: Uuid,
    status: &str,
    model: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE weakness_change_notes SET status = $2::ai_item_status, \
         model = coalesce($3, model), next_attempt_at = NULL, updated_at = now() \
         WHERE id = $1 AND status = 'pending'",
    )
    .bind(id)
    .bind(status)
    .bind(model)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// A transient failure: stays `pending` until `next_attempt_at`.
pub async fn set_note_retry(
    conn: &mut PgConnection,
    id: Uuid,
    retry_count: i32,
    next_attempt_at: OffsetDateTime,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE weakness_change_notes SET retry_count = $2, next_attempt_at = $3, \
         updated_at = now() WHERE id = $1 AND status = 'pending'",
    )
    .bind(id)
    .bind(retry_count)
    .bind(next_attempt_at)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// Pending notes that are due (for the reconciler), oldest first.
pub async fn pending_notes_due(
    conn: &mut PgConnection,
    limit: i64,
) -> Result<Vec<NoteRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM weakness_change_notes WHERE status = 'pending' \
         AND (next_attempt_at IS NULL OR next_attempt_at <= now()) ORDER BY created_at LIMIT $1"
    )))
    .bind(limit)
    .fetch_all(conn)
    .await
}
