//! `interactive_evals`: engine checks the coach chat runs on the player's request ("what if").
//!
//! NEVER read by KPI, stats, profile or report code (Global Constraints): these rows are
//! `source = 'interactive'` only and live outside `position_evals`. Each row is one quota charge
//! (`charged`); a failure the player did not cause is refunded with `charged = false`. Deleting
//! the conversation detaches a row (`message_id` NULL) but keeps it, so the quota holds.

use std::time::Duration;

use serde_json::Value;
use sqlx::PgConnection;
use time::OffsetDateTime;
use uuid::Uuid;

use super::text_enum;

text_enum!(
    /// `interactive_eval_status`.
    EvalStatus, "interactive_eval_status", {
        Pending => "pending",
        Done => "done",
        Failed => "failed",
    }
);

/// A finished eval of the same (account, FEN, protocol) is reused within this window.
pub const REUSE_WITHIN: Duration = Duration::from_secs(24 * 3600);

#[derive(Clone, Debug, PartialEq)]
pub struct NewEval {
    pub account_id: Uuid,
    pub message_id: Option<Uuid>,
    pub protocol: String,
    pub base_fen: String,
    pub moves: Vec<String>,
    pub fen: String,
}

#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct EvalRow {
    pub id: Uuid,
    pub account_id: Uuid,
    pub message_id: Option<Uuid>,
    pub source: String,
    pub protocol: String,
    pub base_fen: String,
    pub moves: Vec<String>,
    pub fen: String,
    #[sqlx(try_from = "String")]
    pub status: EvalStatus,
    pub lines: Option<Value>,
    pub error: Option<String>,
    pub charged: bool,
    pub created_at: OffsetDateTime,
    pub finished_at: Option<OffsetDateTime>,
}

const COLS: &str = "id, account_id, message_id, source, protocol, base_fen, moves, fen, \
    status::text AS status, lines, error, charged, created_at, finished_at";

/// Inserts a pending, charged eval.
pub async fn insert_eval(conn: &mut PgConnection, e: &NewEval) -> Result<EvalRow, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "INSERT INTO interactive_evals (account_id, message_id, protocol, base_fen, moves, fen) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {COLS}"
    )))
    .bind(e.account_id)
    .bind(e.message_id)
    .bind(&e.protocol)
    .bind(&e.base_fen)
    .bind(&e.moves)
    .bind(&e.fen)
    .fetch_one(conn)
    .await
}

pub async fn get_eval(conn: &mut PgConnection, id: Uuid) -> Result<Option<EvalRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM interactive_evals WHERE id = $1"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await
}

/// `pending` → `done` with the engine lines.
pub async fn complete_eval(
    conn: &mut PgConnection,
    id: Uuid,
    lines: &Value,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE interactive_evals SET status = 'done', lines = $2, finished_at = now() \
         WHERE id = $1 AND status = 'pending'",
    )
    .bind(id)
    .bind(lines)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// `pending` → `failed`; `refund` clears the quota charge.
pub async fn fail_eval(
    conn: &mut PgConnection,
    id: Uuid,
    error: &str,
    refund: bool,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE interactive_evals SET status = 'failed', error = $2, finished_at = now(), \
         charged = charged AND NOT $3 WHERE id = $1 AND status = 'pending'",
    )
    .bind(id)
    .bind(error)
    .bind(refund)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// Links evals to the assistant message that used them; returns the rows linked.
pub async fn link_message(
    conn: &mut PgConnection,
    eval_ids: &[Uuid],
    message_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let r = sqlx::query("UPDATE interactive_evals SET message_id = $2 WHERE id = ANY($1)")
        .bind(eval_ids)
        .bind(message_id)
        .execute(conn)
        .await?;
    Ok(r.rows_affected())
}

/// Charged evals since `since` (the engine-check quota).
pub async fn charged_since(
    conn: &mut PgConnection,
    account_id: Uuid,
    since: OffsetDateTime,
) -> Result<u32, sqlx::Error> {
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM interactive_evals WHERE account_id = $1 AND charged \
         AND created_at >= $2",
    )
    .bind(account_id)
    .bind(since)
    .fetch_one(conn)
    .await?;
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

/// The newest `done` eval of this position and protocol created within `within`.
pub async fn reusable(
    conn: &mut PgConnection,
    account_id: Uuid,
    fen: &str,
    protocol: &str,
    within: Duration,
) -> Result<Option<EvalRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM interactive_evals WHERE account_id = $1 AND fen = $2 \
         AND protocol = $3 AND status = 'done' \
         AND created_at >= now() - $4::float8 * interval '1 second' \
         ORDER BY created_at DESC LIMIT 1"
    )))
    .bind(account_id)
    .bind(fen)
    .bind(protocol)
    .bind(within.as_secs_f64())
    .fetch_optional(conn)
    .await
}

/// Pending evals created before `now() - older_than` (for the reconciler), oldest first.
pub async fn stale_pending(
    conn: &mut PgConnection,
    older_than: Duration,
    limit: i64,
) -> Result<Vec<EvalRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM interactive_evals WHERE status = 'pending' \
         AND created_at < now() - $1::float8 * interval '1 second' ORDER BY created_at LIMIT $2"
    )))
    .bind(older_than.as_secs_f64())
    .bind(limit)
    .fetch_all(conn)
    .await
}
