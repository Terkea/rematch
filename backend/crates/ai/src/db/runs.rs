//! `report_runs`: one attempt to build a report (ADR-R3-3).
//!
//! States: `queued` → `running` (claim) → `done` | `failed`, with `awaiting_retry` (a transient
//! AI failure; claimable again after `next_attempt_at`) and `awaiting_batch` (sections submitted to
//! the Message Batches API) in between.

use serde_json::Value;
use sqlx::PgConnection;
use time::OffsetDateTime;
use uuid::Uuid;

use super::text_enum;
use crate::entitlements::Plan;

text_enum!(
    /// `report_trigger`.
    ReportTrigger, "report_trigger", {
        Onboarding => "onboarding",
        Manual => "manual",
        Scheduled => "scheduled",
        ProfileChange => "profile_change",
    }
);

text_enum!(
    /// `report_mode` (also `weekly_reviews.mode`).
    ReportMode, "report_mode", {
        Sync => "sync",
        Batch => "batch",
    }
);

text_enum!(
    /// `report_run_status`. Also a Postgres type (and array) for binding `report_run_status[]`.
    #[derive(sqlx::Type)]
    #[sqlx(type_name = "report_run_status", rename_all = "snake_case")]
    RunStatus, "report_run_status", {
        Queued => "queued",
        Running => "running",
        AwaitingRetry => "awaiting_retry",
        AwaitingBatch => "awaiting_batch",
        Done => "done",
        Failed => "failed",
    }
);

#[derive(Clone, Debug, PartialEq)]
pub struct NewRun {
    pub account_id: Uuid,
    pub time_class: String,
    pub trigger: ReportTrigger,
    pub plan: Plan,
    pub mode: ReportMode,
    pub prompt_version: String,
    pub is_first_report: bool,
    pub ai_disabled_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct RunRow {
    pub id: Uuid,
    pub account_id: Uuid,
    pub time_class: String,
    #[sqlx(try_from = "String")]
    pub trigger: ReportTrigger,
    #[sqlx(try_from = "String")]
    pub plan: Plan,
    #[sqlx(try_from = "String")]
    pub mode: ReportMode,
    #[sqlx(try_from = "String")]
    pub status: RunStatus,
    pub retry_count: i32,
    pub last_error: Option<String>,
    pub next_attempt_at: Option<OffsetDateTime>,
    pub started_at: Option<OffsetDateTime>,
    pub heartbeat_at: Option<OffsetDateTime>,
    pub finished_at: Option<OffsetDateTime>,
    pub prompt_version: String,
    pub prompt_hash: Option<String>,
    pub input_hash: Option<String>,
    pub input: Option<Value>,
    pub is_first_report: bool,
    pub report_id: Option<Uuid>,
    pub ai_disabled_reason: Option<String>,
    pub created_at: OffsetDateTime,
}

const COLS: &str = "id, account_id, time_class::text AS time_class, trigger::text AS trigger, \
    plan::text AS plan, mode::text AS mode, status::text AS status, retry_count, last_error, \
    next_attempt_at, started_at, heartbeat_at, finished_at, prompt_version, prompt_hash, \
    input_hash, input, is_first_report, report_id, ai_disabled_reason, created_at";

pub async fn insert_run(conn: &mut PgConnection, run: &NewRun) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO report_runs (account_id, time_class, trigger, plan, mode, prompt_version, \
         is_first_report, ai_disabled_reason) VALUES ($1, $2::time_class, $3::report_trigger, \
         $4::ai_plan, $5::report_mode, $6, $7, $8) RETURNING id",
    )
    .bind(run.account_id)
    .bind(&run.time_class)
    .bind(run.trigger.as_str())
    .bind(super::plan_str(run.plan))
    .bind(run.mode.as_str())
    .bind(&run.prompt_version)
    .bind(run.is_first_report)
    .bind(&run.ai_disabled_reason)
    .fetch_one(conn)
    .await
}

/// Claims a `queued` or `awaiting_retry` run in one statement; concurrent callers get at most
/// one `Some`.
pub async fn claim_run(conn: &mut PgConnection, id: Uuid) -> Result<Option<RunRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "UPDATE report_runs SET status = 'running', started_at = now(), heartbeat_at = now() \
         WHERE id = $1 AND status IN ('queued', 'awaiting_retry') RETURNING {COLS}"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await
}

pub async fn get_run(conn: &mut PgConnection, id: Uuid) -> Result<Option<RunRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM report_runs WHERE id = $1"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await
}

/// Freezes the coaching input the run generates from (a resume regenerates from the same input).
pub async fn snapshot_input(
    conn: &mut PgConnection,
    id: Uuid,
    input: &Value,
    input_hash: &str,
    prompt_hash: &str,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE report_runs SET input = $2, input_hash = $3, prompt_hash = $4 WHERE id = $1",
    )
    .bind(id)
    .bind(input)
    .bind(input_hash)
    .bind(prompt_hash)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// A running run waits for `next_attempt_at` (the job is released with `retry::retry_delay`).
pub async fn set_awaiting_retry(
    conn: &mut PgConnection,
    id: Uuid,
    retry_count: i32,
    next_attempt_at: OffsetDateTime,
    last_error: &str,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE report_runs SET status = 'awaiting_retry', retry_count = $2, \
         next_attempt_at = $3, last_error = $4 WHERE id = $1 AND status = 'running'",
    )
    .bind(id)
    .bind(retry_count)
    .bind(next_attempt_at)
    .bind(last_error)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// Sections went to the Message Batches API; the batch poller finishes the run.
pub async fn set_awaiting_batch(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE report_runs SET status = 'awaiting_batch' \
         WHERE id = $1 AND status IN ('queued', 'running')",
    )
    .bind(id)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// Links the report as soon as it exists (before the run finishes), so a resume reuses it.
pub async fn link_report(
    conn: &mut PgConnection,
    id: Uuid,
    report_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query("UPDATE report_runs SET report_id = $2 WHERE id = $1")
        .bind(id)
        .bind(report_id)
        .execute(conn)
        .await?;
    Ok(r.rows_affected() == 1)
}

/// Sets the AI-disabled reason (`no_api_key`, `budget`) on a run that degrades to computed-only.
pub async fn set_ai_disabled_reason(
    conn: &mut PgConnection,
    id: Uuid,
    reason: &str,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query("UPDATE report_runs SET ai_disabled_reason = $2 WHERE id = $1")
        .bind(id)
        .bind(reason)
        .execute(conn)
        .await?;
    Ok(r.rows_affected() == 1)
}

/// Finishes an unfinished run. `report_id` `None` keeps an already linked report.
pub async fn set_done(
    conn: &mut PgConnection,
    id: Uuid,
    report_id: Option<Uuid>,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE report_runs SET status = 'done', finished_at = now(), next_attempt_at = NULL, \
         report_id = coalesce($2, report_id) WHERE id = $1 AND status NOT IN ('done', 'failed')",
    )
    .bind(id)
    .bind(report_id)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

pub async fn set_failed(
    conn: &mut PgConnection,
    id: Uuid,
    error: &str,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE report_runs SET status = 'failed', finished_at = now(), next_attempt_at = NULL, \
         last_error = $2 WHERE id = $1 AND status NOT IN ('done', 'failed')",
    )
    .bind(id)
    .bind(error)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// Marks a running run alive (the reconciler re-enqueues runs whose heartbeat is stale).
pub async fn heartbeat(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE report_runs SET heartbeat_at = now() WHERE id = $1 AND status = 'running'",
    )
    .bind(id)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// Runs in `statuses`, oldest first (for the reconciler).
///
/// For the live statuses (`queued`, `running`, `awaiting_retry`, `awaiting_batch`) this reads
/// `report_runs_live_idx` in order; `$1` is cast to the enum array so the planner can prove the
/// partial-index predicate. The statuses are bound as a real `report_run_status[]`: a text array
/// cast in SQL is not constant-folded (`enum_in` is stable), and the planner could not prove it.
pub async fn runs_in_status(
    conn: &mut PgConnection,
    statuses: &[RunStatus],
    limit: i64,
) -> Result<Vec<RunRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(runs_in_status_sql()))
        .bind(statuses)
        .bind(limit)
        .fetch_all(conn)
        .await
}

/// The SQL of [`runs_in_status`] (`$1` a `&[RunStatus]`, `$2` limit), for plan guards.
pub fn runs_in_status_sql() -> String {
    format!(
        "SELECT {COLS} FROM report_runs WHERE status = ANY($1::report_run_status[]) \
         ORDER BY created_at LIMIT $2"
    )
}

/// When the newest report for (account, time class) was created; `None` if there is none.
pub async fn latest_report_at(
    conn: &mut PgConnection,
    account_id: Uuid,
    time_class: &str,
) -> Result<Option<OffsetDateTime>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT max(created_at) FROM reports WHERE account_id = $1 AND time_class = $2::time_class",
    )
    .bind(account_id)
    .bind(time_class)
    .fetch_one(conn)
    .await
}
