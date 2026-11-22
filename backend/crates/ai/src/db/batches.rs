//! `ai_batches` and `ai_batch_items`: Message Batches submissions for scheduled reports and weekly
//! reviews (ADR 0062 decision 3).
//!
//! Batch states: `submitted` → `ended` (results available) → `processed`, or `failed`. An item is
//! processed once (`processed_at`), so a re-poll never applies a result twice.

use sqlx::PgConnection;
use time::OffsetDateTime;
use uuid::Uuid;

use super::text_enum;
use crate::output::codes::SectionKey;

text_enum!(
    /// `ai_batches.kind` and `ai_batch_items.kind`.
    BatchKind, "ai_batches.kind", {
        Report => "report",
        WeeklyReview => "weekly_review",
    }
);

text_enum!(
    /// `ai_batch_status`.
    BatchStatus, "ai_batch_status", {
        Submitted => "submitted",
        Ended => "ended",
        Processed => "processed",
        Failed => "failed",
    }
);

#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct BatchRow {
    pub id: String,
    #[sqlx(try_from = "String")]
    pub kind: BatchKind,
    #[sqlx(try_from = "String")]
    pub status: BatchStatus,
    pub request_count: i32,
    pub submitted_at: OffsetDateTime,
    pub ended_at: Option<OffsetDateTime>,
    pub processed_at: Option<OffsetDateTime>,
    pub last_polled_at: Option<OffsetDateTime>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NewBatchItem {
    pub id: Uuid,
    pub custom_id: String,
    pub kind: BatchKind,
    pub report_run_id: Option<Uuid>,
    pub section: Option<SectionKey>,
    pub weekly_review_id: Option<Uuid>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BatchItemRow {
    pub id: Uuid,
    pub batch_id: String,
    pub custom_id: String,
    pub kind: BatchKind,
    pub report_run_id: Option<Uuid>,
    pub section: Option<SectionKey>,
    pub weekly_review_id: Option<Uuid>,
    pub processed_at: Option<OffsetDateTime>,
}

impl<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> for BatchItemRow {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        let decode = |col: &str, e: super::ParseEnumError| sqlx::Error::ColumnDecode {
            index: col.to_owned(),
            source: Box::new(e),
        };
        let kind: String = row.try_get("kind")?;
        let section: Option<String> = row.try_get("section")?;
        Ok(Self {
            id: row.try_get("id")?,
            batch_id: row.try_get("batch_id")?,
            custom_id: row.try_get("custom_id")?,
            kind: kind.try_into().map_err(|e| decode("kind", e))?,
            report_run_id: row.try_get("report_run_id")?,
            section: section
                .map(SectionKey::try_from)
                .transpose()
                .map_err(|e| decode("section", e))?,
            weekly_review_id: row.try_get("weekly_review_id")?,
            processed_at: row.try_get("processed_at")?,
        })
    }
}

const BATCH_COLS: &str = "id, kind, status::text AS status, request_count, submitted_at, \
    ended_at, processed_at, last_polled_at";

const ITEM_COLS: &str =
    "id, batch_id, custom_id, kind, report_run_id, section, weekly_review_id, processed_at";

pub async fn insert_batch(
    conn: &mut PgConnection,
    id: &str,
    kind: BatchKind,
    request_count: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO ai_batches (id, kind, request_count) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(kind.as_str())
        .bind(request_count)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn insert_items(
    conn: &mut PgConnection,
    batch_id: &str,
    items: &[NewBatchItem],
) -> Result<(), sqlx::Error> {
    if items.is_empty() {
        return Ok(());
    }
    let ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();
    let custom: Vec<&str> = items.iter().map(|i| i.custom_id.as_str()).collect();
    let kinds: Vec<&str> = items.iter().map(|i| i.kind.as_str()).collect();
    let runs: Vec<Option<Uuid>> = items.iter().map(|i| i.report_run_id).collect();
    let sections: Vec<Option<&str>> = items
        .iter()
        .map(|i| i.section.map(|s| s.as_str()))
        .collect();
    let weekly: Vec<Option<Uuid>> = items.iter().map(|i| i.weekly_review_id).collect();
    sqlx::query(
        "INSERT INTO ai_batch_items (id, batch_id, custom_id, kind, report_run_id, section, \
         weekly_review_id) SELECT u.id, $1, u.custom_id, u.kind, u.run, u.section, u.weekly \
         FROM unnest($2::uuid[], $3::text[], $4::text[], $5::uuid[], $6::text[], $7::uuid[]) \
         AS u(id, custom_id, kind, run, section, weekly)",
    )
    .bind(batch_id)
    .bind(ids)
    .bind(custom)
    .bind(kinds)
    .bind(runs)
    .bind(sections)
    .bind(weekly)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn get_batch(conn: &mut PgConnection, id: &str) -> Result<Option<BatchRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {BATCH_COLS} FROM ai_batches WHERE id = $1"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await
}

/// Batches still to poll or process (`submitted`, `ended`), least recently polled first. Reads
/// `ai_batches_open_idx` in order.
pub async fn open_batches(conn: &mut PgConnection) -> Result<Vec<BatchRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(open_batches_sql()))
        .fetch_all(conn)
        .await
}

/// The SQL of [`open_batches`], for plan guards.
pub fn open_batches_sql() -> String {
    format!(
        "SELECT {BATCH_COLS} FROM ai_batches WHERE status IN ('submitted', 'ended') \
         ORDER BY last_polled_at NULLS FIRST, submitted_at"
    )
}

pub async fn mark_polled(conn: &mut PgConnection, id: &str) -> Result<bool, sqlx::Error> {
    let r = sqlx::query("UPDATE ai_batches SET last_polled_at = now() WHERE id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(r.rows_affected() == 1)
}

/// `submitted` → `ended`.
pub async fn mark_ended(conn: &mut PgConnection, id: &str) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE ai_batches SET status = 'ended', ended_at = now() \
         WHERE id = $1 AND status = 'submitted'",
    )
    .bind(id)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// `submitted` | `ended` → `processed`.
pub async fn mark_processed(conn: &mut PgConnection, id: &str) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE ai_batches SET status = 'processed', processed_at = now(), \
         ended_at = coalesce(ended_at, now()) WHERE id = $1 AND status IN ('submitted', 'ended')",
    )
    .bind(id)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

/// `submitted` | `ended` → `failed`.
pub async fn mark_failed(conn: &mut PgConnection, id: &str) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE ai_batches SET status = 'failed' WHERE id = $1 AND status IN ('submitted', 'ended')",
    )
    .bind(id)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}

pub async fn item_by_custom_id(
    conn: &mut PgConnection,
    custom_id: &str,
) -> Result<Option<BatchItemRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {ITEM_COLS} FROM ai_batch_items WHERE custom_id = $1"
    )))
    .bind(custom_id)
    .fetch_optional(conn)
    .await
}

/// Unprocessed items of a batch.
pub async fn pending_items(
    conn: &mut PgConnection,
    batch_id: &str,
) -> Result<Vec<BatchItemRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {ITEM_COLS} FROM ai_batch_items WHERE batch_id = $1 AND processed_at IS NULL \
         ORDER BY custom_id"
    )))
    .bind(batch_id)
    .fetch_all(conn)
    .await
}

/// Marks an item processed; `false` if it already was (apply each result once).
pub async fn mark_item_processed(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE ai_batch_items SET processed_at = now() WHERE id = $1 AND processed_at IS NULL",
    )
    .bind(id)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() == 1)
}
