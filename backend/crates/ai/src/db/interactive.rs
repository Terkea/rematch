//! `interactive_evals`: engine checks the coach chat runs on the player's request ("what if").
//!
//! NEVER read by KPI, stats, profile or report code (Global Constraints): these rows are
//! `source = 'interactive'` only and live outside `position_evals`. Each row is one quota charge
//! (`charged`); a failure the player did not cause is refunded with `charged = false`. Deleting
//! the conversation detaches a row (`message_id` NULL) but keeps it, so the quota holds.

use std::collections::HashMap;
use std::time::Duration;

use serde_json::Value;
use sqlx::{Connection, PgConnection};
use time::OffsetDateTime;
use uuid::Uuid;

use super::text_enum;
use crate::entitlements::{Quota, exhausted, window_start};

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

/// The newest charged `done` eval of this position and protocol created within `within`.
/// Uncharged `done` rows are free copies of an earlier result, so the window runs from the
/// charged original and a copy never extends it.
pub async fn reusable(
    conn: &mut PgConnection,
    account_id: Uuid,
    fen: &str,
    protocol: &str,
    within: Duration,
) -> Result<Option<EvalRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM interactive_evals WHERE account_id = $1 AND fen = $2 \
         AND protocol = $3 AND status = 'done' AND charged \
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

/// Pending evals created before `now() - older_than` (for the reconciler), oldest first. Reads
/// `interactive_evals_pending_idx` in order.
pub async fn stale_pending(
    conn: &mut PgConnection,
    older_than: Duration,
    limit: i64,
) -> Result<Vec<EvalRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(stale_pending_sql()))
        .bind(older_than.as_secs_f64())
        .bind(limit)
        .fetch_all(conn)
        .await
}

/// The SQL of [`stale_pending`] (`$1` seconds, `$2` limit), for plan guards.
pub fn stale_pending_sql() -> String {
    format!(
        "SELECT {COLS} FROM interactive_evals WHERE status = 'pending' \
         AND created_at < now() - $1::float8 * interval '1 second' ORDER BY created_at LIMIT $2"
    )
}

/// The advisory-lock class of [`reserve_eval`] (`pg_advisory_xact_lock(class, hashtext(account))`).
pub const QUOTA_LOCK_CLASS: i32 = 30_002;

/// One account's eval, or `None` for an unknown id or another account's.
pub async fn get_account_eval(
    conn: &mut PgConnection,
    account_id: Uuid,
    id: Uuid,
) -> Result<Option<EvalRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM interactive_evals WHERE id = $1 AND account_id = $2"
    )))
    .bind(id)
    .bind(account_id)
    .fetch_optional(conn)
    .await
}

/// Charges one engine check atomically: inserts a pending, charged eval unless a quota is used
/// up, in which case it returns that quota and its reset time and inserts nothing.
///
/// Reservations of one account are serialised by a transaction-scoped advisory lock, and the
/// counts are new statements after the lock, so under READ COMMITTED (the default) they see
/// every reservation committed before it. Parallel requests therefore never overspend.
pub async fn reserve_eval(
    conn: &mut PgConnection,
    e: &NewEval,
    quotas: &[Quota],
    now: OffsetDateTime,
) -> Result<Result<EvalRow, (Quota, OffsetDateTime)>, sqlx::Error> {
    let mut tx = conn.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1, hashtext($2::text))")
        .bind(QUOTA_LOCK_CLASS)
        .bind(e.account_id)
        .execute(&mut *tx)
        .await?;
    let mut used = HashMap::new();
    for q in quotas {
        let n = charged_since(&mut tx, e.account_id, window_start(q.window, now)).await?;
        used.insert(q.window, n);
    }
    if let Some(hit) = exhausted(quotas, |w| used.get(&w).copied().unwrap_or(0), now) {
        tx.rollback().await?;
        return Ok(Err(hit));
    }
    let row = insert_eval(&mut tx, e).await?;
    tx.commit().await?;
    Ok(Ok(row))
}

/// A free copy of a finished eval for a new request: `done`, the same lines, `charged = false`.
pub async fn insert_reused(
    conn: &mut PgConnection,
    src: &EvalRow,
    message_id: Option<Uuid>,
    base_fen: &str,
    moves: &[String],
) -> Result<EvalRow, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "INSERT INTO interactive_evals (account_id, message_id, protocol, base_fen, moves, fen, \
         status, lines, charged, finished_at) \
         VALUES ($1, $2, $3, $4, $5, $6, 'done', $7, false, now()) RETURNING {COLS}"
    )))
    .bind(src.account_id)
    .bind(message_id)
    .bind(&src.protocol)
    .bind(base_fen)
    .bind(moves)
    .bind(&src.fen)
    .bind(&src.lines)
    .fetch_one(conn)
    .await
}

/// `interactive_evals.error` of a check abandoned or cancelled before its result.
pub const ABANDONED: &str = "abandoned";

/// Abandons one account's pending eval: `failed` with [`ABANDONED`], and refunded. The job's
/// late lines are then dropped (`complete_eval` needs `pending`), so a refunded check never
/// becomes a free reusable result. A row that is already `done` keeps its charge, because the
/// result exists. `false` when nothing was pending.
pub async fn abandon_eval(
    conn: &mut PgConnection,
    account_id: Uuid,
    id: Uuid,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE interactive_evals SET status = 'failed', error = $3, charged = false, \
         finished_at = now() WHERE id = $1 AND account_id = $2 AND status = 'pending'",
    )
    .bind(id)
    .bind(account_id)
    .bind(ABANDONED)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}
