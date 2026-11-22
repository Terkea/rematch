//! `weekly_reviews`: one AI review per (account, time class, week starting Monday) (ADR-R3-3).
//!
//! States as for change notes: `pending` → `ready` | `dropped` | `failed`; a transient failure
//! stays `pending` with `next_attempt_at`. `mode = batch` reviews are finished by the batch poller.

use serde_json::Value;
use sqlx::PgConnection;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::ItemStatus;
use super::runs::ReportMode;

#[derive(Clone, Debug, PartialEq)]
pub struct NewWeekly {
    pub account_id: Uuid,
    pub time_class: String,
    pub week_start: Date,
    pub mode: ReportMode,
    pub input: Value,
}

#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct WeeklyRow {
    pub id: Uuid,
    pub account_id: Uuid,
    pub time_class: String,
    pub week_start: Date,
    #[sqlx(try_from = "String")]
    pub status: ItemStatus,
    #[sqlx(try_from = "String")]
    pub mode: ReportMode,
    pub input: Value,
    pub content: Option<Value>,
    pub dropped: Value,
    pub model: Option<String>,
    pub prompt_hash: Option<String>,
    pub retry_count: i32,
    pub next_attempt_at: Option<OffsetDateTime>,
    pub ai_disclosure: Option<Value>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// A validated review.
#[derive(Clone, Debug, PartialEq)]
pub struct WeeklyContent {
    pub content: Value,
    pub dropped: Value,
    pub model: String,
    pub prompt_hash: Option<String>,
    pub ai_disclosure: Value,
}

const COLS: &str = "id, account_id, time_class::text AS time_class, week_start, \
    status::text AS status, mode::text AS mode, input, content, dropped, model, prompt_hash, \
    retry_count, next_attempt_at, ai_disclosure, created_at, updated_at";

/// Inserts a pending review; `None` when that week already has one.
pub async fn insert_weekly_if_absent(
    conn: &mut PgConnection,
    w: &NewWeekly,
) -> Result<Option<WeeklyRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "INSERT INTO weekly_reviews (account_id, time_class, week_start, mode, input) \
         VALUES ($1, $2::time_class, $3, $4::report_mode, $5) \
         ON CONFLICT (account_id, time_class, week_start) DO NOTHING RETURNING {COLS}"
    )))
    .bind(w.account_id)
    .bind(&w.time_class)
    .bind(w.week_start)
    .bind(w.mode.as_str())
    .bind(&w.input)
    .fetch_optional(conn)
    .await
}

pub async fn get_weekly(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<Option<WeeklyRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM weekly_reviews WHERE id = $1"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await
}

/// The newest review for (account, time class).
pub async fn latest_weekly(
    conn: &mut PgConnection,
    account_id: Uuid,
    time_class: &str,
) -> Result<Option<WeeklyRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM weekly_reviews WHERE account_id = $1 AND time_class = $2::time_class \
         ORDER BY week_start DESC LIMIT 1"
    )))
    .bind(account_id)
    .bind(time_class)
    .fetch_optional(conn)
    .await
}

pub async fn set_weekly_ready(
    conn: &mut PgConnection,
    id: Uuid,
    c: &WeeklyContent,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE weekly_reviews SET status = 'ready', content = $2, dropped = $3, model = $4, \
         prompt_hash = $5, ai_disclosure = $6, next_attempt_at = NULL, updated_at = now() \
         WHERE id = $1 AND status = 'pending'",
    )
    .bind(id)
    .bind(&c.content)
    .bind(&c.dropped)
    .bind(&c.model)
    .bind(&c.prompt_hash)
    .bind(&c.ai_disclosure)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// Validation failed twice: `dropped`, with the dropped items.
pub async fn set_weekly_dropped(
    conn: &mut PgConnection,
    id: Uuid,
    dropped: &Value,
    model: &str,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE weekly_reviews SET status = 'dropped', dropped = $2, model = $3, \
         next_attempt_at = NULL, updated_at = now() WHERE id = $1 AND status = 'pending'",
    )
    .bind(id)
    .bind(dropped)
    .bind(model)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// Retries are exhausted.
pub async fn set_weekly_failed(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE weekly_reviews SET status = 'failed', next_attempt_at = NULL, updated_at = now() \
         WHERE id = $1 AND status = 'pending'",
    )
    .bind(id)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// A transient failure: stays `pending` until `next_attempt_at`.
pub async fn set_weekly_retry(
    conn: &mut PgConnection,
    id: Uuid,
    retry_count: i32,
    next_attempt_at: OffsetDateTime,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE weekly_reviews SET retry_count = $2, next_attempt_at = $3, updated_at = now() \
         WHERE id = $1 AND status = 'pending'",
    )
    .bind(id)
    .bind(retry_count)
    .bind(next_attempt_at)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// Pending reviews that are due (for the reconciler), oldest first, in either mode: the caller
/// skips batch reviews whose batch item is still open.
pub async fn pending_weekly_due(
    conn: &mut PgConnection,
    limit: i64,
) -> Result<Vec<WeeklyRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM weekly_reviews WHERE status = 'pending' \
         AND (next_attempt_at IS NULL OR next_attempt_at <= now()) ORDER BY created_at LIMIT $1"
    )))
    .bind(limit)
    .fetch_all(conn)
    .await
}
