//! `report_runs`: one attempt to build a report (ADR 0059).
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
    /// `report_run_status`. Also a Postgres type, for callers that bind it directly.
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

/// A worker's hold on a `running` run: the claim's `started_at` is its token. Every write of
/// the generate-report job is fenced on it, so a zombie worker whose run was reclaimed (a new
/// `started_at`) or reset by the reconciler can no longer write (T21 review M1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunClaim {
    pub id: Uuid,
    pub started_at: OffsetDateTime,
}

impl RunClaim {
    /// The claim of a run row returned by a claim (`None` when it has no `started_at`).
    pub fn of(row: &RunRow) -> Option<RunClaim> {
        row.started_at.map(|started_at| RunClaim {
            id: row.id,
            started_at,
        })
    }
}

/// [`claim_run`], plus a `running` run whose heartbeat (or start) is older than `stale_after`:
/// a redelivered job takes over the run of a worker that died (T21 review M2). The new claim has
/// a new `started_at`, which fences the old worker out.
pub async fn claim_run_or_stale(
    conn: &mut PgConnection,
    id: Uuid,
    stale_after: std::time::Duration,
) -> Result<Option<RunRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "UPDATE report_runs SET status = 'running', started_at = clock_timestamp(), \
         heartbeat_at = clock_timestamp() \
         WHERE id = $1 AND (status IN ('queued', 'awaiting_retry') OR (status = 'running' \
         AND coalesce(heartbeat_at, started_at, created_at) < now() - make_interval(secs => $2))) \
         RETURNING {COLS}"
    )))
    .bind(id)
    .bind(stale_after.as_secs_f64())
    .fetch_optional(conn)
    .await
}

/// Locks the run row (`FOR UPDATE`, until the caller's transaction ends) when `claim` still
/// holds it. Run it first in a transaction; the unfenced writers below are then safe to use.
pub async fn lock_claim(conn: &mut PgConnection, claim: &RunClaim) -> Result<bool, sqlx::Error> {
    let r: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM report_runs WHERE id = $1 AND status = 'running' AND started_at = $2 \
         FOR UPDATE",
    )
    .bind(claim.id)
    .bind(claim.started_at)
    .fetch_optional(conn)
    .await?;
    Ok(r.is_some())
}

/// [`heartbeat`], fenced on the claim.
pub async fn heartbeat_claim(
    conn: &mut PgConnection,
    claim: &RunClaim,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE report_runs SET heartbeat_at = clock_timestamp() \
         WHERE id = $1 AND status = 'running' AND started_at = $2",
    )
    .bind(claim.id)
    .bind(claim.started_at)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// Hands a claimed run back (`queued`) so the next delivery claims it at once (a cancelled or
/// failed attempt). Fenced on the claim.
pub async fn release_claim(conn: &mut PgConnection, claim: &RunClaim) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE report_runs SET status = 'queued' \
         WHERE id = $1 AND status = 'running' AND started_at = $2",
    )
    .bind(claim.id)
    .bind(claim.started_at)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// [`set_failed`], except for a run some worker holds alive (`running` with a heartbeat newer
/// than `stale_after`): a dead job never fails a run another delivery is generating (review m5).
pub async fn set_failed_unless_live(
    conn: &mut PgConnection,
    id: Uuid,
    error: &str,
    stale_after: std::time::Duration,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE report_runs SET status = 'failed', finished_at = now(), next_attempt_at = NULL, \
         last_error = $2 WHERE id = $1 AND status NOT IN ('done', 'failed') \
         AND NOT (status = 'running' \
         AND coalesce(heartbeat_at, started_at, created_at) >= now() - make_interval(secs => $3))",
    )
    .bind(id)
    .bind(error)
    .bind(stale_after.as_secs_f64())
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// When the newest report of (account, time class) not made by `run_id` was created.
pub async fn latest_other_report_at(
    conn: &mut PgConnection,
    account_id: Uuid,
    time_class: &str,
    run_id: Uuid,
) -> Result<Option<OffsetDateTime>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT max(created_at) FROM reports WHERE account_id = $1 \
         AND time_class = $2::time_class AND report_run_id <> $3",
    )
    .bind(account_id)
    .bind(time_class)
    .bind(run_id)
    .fetch_one(conn)
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

/// Sections went to the Message Batches API; the batch poller finishes the run. Fenced on
/// `queued`: a run a worker claimed (`running`) is being generated synchronously and must not
/// be paid for twice.
pub async fn set_awaiting_batch(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE report_runs SET status = 'awaiting_batch' WHERE id = $1 AND status = 'queued'",
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

/// Unfinished runs (`queued`, `running`, `awaiting_retry`, `awaiting_batch`), oldest first. Reads
/// `report_runs_live_idx` in order. The statuses are literals in the SQL, so the planner proves
/// the partial-index predicate in the generic plan too (a bound status array would not; with a
/// cached generic plan it fell back to `report_runs_status_idx` plus a sort).
pub async fn live_runs(conn: &mut PgConnection, limit: i64) -> Result<Vec<RunRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(live_runs_sql()))
        .bind(limit)
        .fetch_all(conn)
        .await
}

/// The SQL of [`live_runs`] (`$1` limit), for plan guards.
pub fn live_runs_sql() -> String {
    format!(
        "SELECT {COLS} FROM report_runs \
         WHERE status IN ('queued', 'running', 'awaiting_retry', 'awaiting_batch') \
         ORDER BY created_at LIMIT $1"
    )
}

/// Runs in one status, oldest first. The status is a literal in the SQL (one fixed statement
/// per status), never a bound parameter. The live statuses read `report_runs_live_idx` in order.
/// The reconciler's keyset pages are in `jobs::reconcile`.
pub async fn runs_with_status(
    conn: &mut PgConnection,
    status: RunStatus,
    limit: i64,
) -> Result<Vec<RunRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(runs_with_status_sql(status)))
        .bind(limit)
        .fetch_all(conn)
        .await
}

/// The SQL of [`runs_with_status`] (`$1` limit), for plan guards. `status.as_str()` is one of
/// the enum's fixed labels, so the literal is safe.
pub fn runs_with_status_sql(status: RunStatus) -> String {
    format!(
        "SELECT {COLS} FROM report_runs WHERE status = '{}' ORDER BY created_at LIMIT $1",
        status.as_str()
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
