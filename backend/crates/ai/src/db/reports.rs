//! `reports` and `report_sections` (ADR-R3-3).
//!
//! A report is versioned per (account, time class); its five narrative sections are stored as
//! validated JSON. Status: `computed_only` (no section ready), `partial`, `complete` (all five).

use serde_json::Value;
use sqlx::{Acquire, PgConnection};
use time::OffsetDateTime;
use uuid::Uuid;

use super::text_enum;
use crate::output::codes::{SECTION_KEYS, SectionKey};

text_enum!(
    /// `report_status`.
    ReportStatus, "report_status", {
        ComputedOnly => "computed_only",
        Partial => "partial",
        Complete => "complete",
    }
);

text_enum!(
    /// The two statuses a `report_sections` row may hold (a subset of `ai_item_status`).
    SectionWriteStatus, "report_sections.status", {
        Ready => "ready",
        Dropped => "dropped",
    }
);

/// Advisory-lock class for report versioning (two-int key space; never overlaps the
/// single-bigint user-events lock).
const VERSION_LOCK_CLASS: i32 = 30_001;

#[derive(Clone, Debug, PartialEq)]
pub struct NewReport {
    pub account_id: Uuid,
    pub time_class: String,
    pub report_run_id: Uuid,
    pub schema_version: String,
    pub computed: Value,
    pub status: ReportStatus,
}

#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct ReportRow {
    pub id: Uuid,
    pub account_id: Uuid,
    pub time_class: String,
    pub version: i32,
    pub report_run_id: Uuid,
    pub schema_version: String,
    pub computed: Value,
    #[sqlx(try_from = "String")]
    pub status: ReportStatus,
    pub ai_disclosure: Option<Value>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct SectionRow {
    pub report_id: Uuid,
    #[sqlx(try_from = "String")]
    pub section: SectionKey,
    #[sqlx(try_from = "String")]
    pub status: SectionWriteStatus,
    pub content: Option<Value>,
    pub dropped: Value,
    pub attempts: i16,
    pub model: String,
    pub validated_at: OffsetDateTime,
}

/// A report with its section rows (in `SECTION_KEYS` order): the raw parts Task 14's
/// `assemble_report_payload` turns into a `ReportPayload`.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredReport {
    pub report: ReportRow,
    pub sections: Vec<SectionRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SectionWrite {
    pub section: SectionKey,
    pub status: SectionWriteStatus,
    pub content: Option<Value>,
    /// The dropped items (`Vec<DroppedItem>` as JSON).
    pub dropped: Value,
    pub attempts: i16,
    pub model: String,
}

/// Whether an upsert changed what the player sees (status or content; TS `upsertReportSection`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Changed(pub bool);

const COLS: &str = "id, account_id, time_class::text AS time_class, version, report_run_id, \
    schema_version, computed, status::text AS status, ai_disclosure, created_at, updated_at";

const SECTION_COLS: &str = "report_id, section, status::text AS status, content, dropped, \
    attempts, model, validated_at";

/// Takes the versioning lock of (account, time class) until the caller's transaction ends
/// (re-entrant). Checks that must hold for the next version, such as the free plan's interval,
/// run under it in the same transaction as [`create_report_with_next_version`].
pub async fn lock_versions(
    conn: &mut PgConnection,
    account_id: Uuid,
    time_class: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock($1, hashtext($2::text || ':' || $3))")
        .bind(VERSION_LOCK_CLASS)
        .bind(account_id)
        .bind(time_class)
        .execute(conn)
        .await?;
    Ok(())
}

/// Inserts the next version (1 for the first) for (account, time class). Versions are
/// serialised with a transaction-scoped advisory lock on (account, time class), held until the
/// caller's transaction ends (or this call's own, outside one): a `FOR UPDATE` on the existing
/// rows cannot lock a first report that does not exist yet, and `max()` cannot be combined with
/// `FOR UPDATE`.
///
/// Call it under READ COMMITTED (the default). Under REPEATABLE READ or SERIALIZABLE the
/// caller's snapshot predates the lock wait, so `max()` misses a version committed meanwhile
/// and the insert fails on the unique `(account_id, time_class, version)` (or with a
/// serialization failure). The data stays correct, but the caller must retry.
pub async fn create_report_with_next_version(
    conn: &mut PgConnection,
    report: &NewReport,
) -> Result<ReportRow, sqlx::Error> {
    let mut tx = conn.begin().await?;
    lock_versions(&mut tx, report.account_id, &report.time_class).await?;
    // A new statement after the lock, so its snapshot sees a version committed meanwhile.
    let row = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "INSERT INTO reports (account_id, time_class, version, report_run_id, schema_version, \
         computed, status) \
         SELECT $1, $2::time_class, coalesce(max(version), 0) + 1, $3, $4, $5, $6::report_status \
         FROM reports WHERE account_id = $1 AND time_class = $2::time_class \
         RETURNING {COLS}"
    )))
    .bind(report.account_id)
    .bind(&report.time_class)
    .bind(report.report_run_id)
    .bind(&report.schema_version)
    .bind(&report.computed)
    .bind(report.status.as_str())
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row)
}

pub async fn get_report(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<Option<ReportRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM reports WHERE id = $1"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await
}

/// The report a run created, if any.
pub async fn report_for_run(
    conn: &mut PgConnection,
    run_id: Uuid,
) -> Result<Option<ReportRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM reports WHERE report_run_id = $1"
    )))
    .bind(run_id)
    .fetch_optional(conn)
    .await
}

/// The highest version for (account, time class).
pub async fn latest_report(
    conn: &mut PgConnection,
    account_id: Uuid,
    time_class: &str,
) -> Result<Option<ReportRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM reports WHERE account_id = $1 AND time_class = $2::time_class \
         ORDER BY version DESC LIMIT 1"
    )))
    .bind(account_id)
    .bind(time_class)
    .fetch_optional(conn)
    .await
}

/// Sets the status and the disclosure (`None` keeps the stored disclosure).
pub async fn set_status(
    conn: &mut PgConnection,
    id: Uuid,
    status: ReportStatus,
    ai_disclosure: Option<&Value>,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE reports SET status = $2::report_status, \
         ai_disclosure = coalesce($3, ai_disclosure), updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(status.as_str())
    .bind(ai_disclosure)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// Inserts or updates a section. `Changed(true)` only when it is new or its status or content
/// differ (jsonb equality, so key order does not matter); attempts, model and dropped items are
/// stored either way.
pub async fn upsert_section(
    conn: &mut PgConnection,
    report_id: Uuid,
    write: &SectionWrite,
) -> Result<Changed, sqlx::Error> {
    let mut tx = conn.begin().await?;
    let prev: Option<(String, Option<Value>)> = sqlx::query_as(
        "SELECT status::text, content FROM report_sections \
         WHERE report_id = $1 AND section = $2 FOR UPDATE",
    )
    .bind(report_id)
    .bind(write.section.as_str())
    .fetch_optional(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO report_sections (report_id, section, status, content, dropped, attempts, model) \
         VALUES ($1, $2, $3::ai_item_status, $4, $5, $6, $7) \
         ON CONFLICT (report_id, section) DO UPDATE SET status = EXCLUDED.status, \
         content = EXCLUDED.content, dropped = EXCLUDED.dropped, attempts = EXCLUDED.attempts, \
         model = EXCLUDED.model, validated_at = now()",
    )
    .bind(report_id)
    .bind(write.section.as_str())
    .bind(write.status.as_str())
    .bind(&write.content)
    .bind(&write.dropped)
    .bind(write.attempts)
    .bind(&write.model)
    .execute(&mut *tx)
    .await?;
    let changed = match prev {
        None => true,
        Some((status, content)) => {
            // Compare in Postgres: jsonb equality ignores key order and number formatting.
            let same: bool = sqlx::query_scalar(
                "SELECT $1::text = $2 AND $3::jsonb IS NOT DISTINCT FROM $4::jsonb",
            )
            .bind(&status)
            .bind(write.status.as_str())
            .bind(&content)
            .bind(&write.content)
            .fetch_one(&mut *tx)
            .await?;
            !same
        }
    };
    tx.commit().await?;
    Ok(Changed(changed))
}

/// The `ready` sections, in `SECTION_KEYS` order.
pub async fn ready_sections(
    conn: &mut PgConnection,
    report_id: Uuid,
) -> Result<Vec<SectionKey>, sqlx::Error> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT section FROM report_sections WHERE report_id = $1 AND status = 'ready'",
    )
    .bind(report_id)
    .fetch_all(conn)
    .await?;
    Ok(SECTION_KEYS
        .into_iter()
        .filter(|k| rows.iter().any(|r| r == k.as_str()))
        .collect())
}

/// All section rows, in `SECTION_KEYS` order.
pub async fn sections(
    conn: &mut PgConnection,
    report_id: Uuid,
) -> Result<Vec<SectionRow>, sqlx::Error> {
    let mut rows: Vec<SectionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {SECTION_COLS} FROM report_sections WHERE report_id = $1"
    )))
    .bind(report_id)
    .fetch_all(conn)
    .await?;
    rows.sort_by_key(|r| r.section);
    Ok(rows)
}

/// The report and its sections, read in one transaction (a savepoint inside the caller's).
/// Under READ COMMITTED a section written between the two reads may appear; that is harmless,
/// since section events follow their commit. Task 14 assembles the payload from it.
pub async fn load_report_rows(
    conn: &mut PgConnection,
    report_id: Uuid,
) -> Result<Option<StoredReport>, sqlx::Error> {
    let mut tx = conn.begin().await?;
    let Some(report) = get_report(&mut tx, report_id).await? else {
        return Ok(None);
    };
    let sections = sections(&mut tx, report_id).await?;
    tx.commit().await?;
    Ok(Some(StoredReport { report, sections }))
}
