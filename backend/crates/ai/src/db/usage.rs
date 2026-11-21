//! `ai_usage`: one row per model call, with our own cost (ADR-R3-6 decision 4). Budgets read it.

use sqlx::PgConnection;
use time::OffsetDateTime;
use uuid::Uuid;

use super::text_enum;

text_enum!(
    /// `ai_feature`.
    AiFeature, "ai_feature", {
        Report => "report",
        WeeklyReview => "weekly_review",
        WeaknessChange => "weakness_change",
        Chat => "chat",
    }
);

#[derive(Clone, Debug, PartialEq)]
pub struct UsageRow {
    pub account_id: Uuid,
    pub feature: AiFeature,
    pub model: String,
    pub batch: bool,
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub cache_creation_tokens: i32,
    pub cache_read_tokens: i32,
    pub cost_micro_usd: i64,
    /// The run, review, note or message id the call served.
    pub ref_id: Option<String>,
}

/// Records one call; returns the row id.
pub async fn record_usage(conn: &mut PgConnection, row: &UsageRow) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO ai_usage (account_id, feature, model, batch, input_tokens, output_tokens, \
         cache_creation_tokens, cache_read_tokens, cost_micro_usd, ref_id) \
         VALUES ($1, $2::ai_feature, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
    )
    .bind(row.account_id)
    .bind(row.feature.as_str())
    .bind(&row.model)
    .bind(row.batch)
    .bind(row.input_tokens)
    .bind(row.output_tokens)
    .bind(row.cache_creation_tokens)
    .bind(row.cache_read_tokens)
    .bind(row.cost_micro_usd)
    .bind(&row.ref_id)
    .fetch_one(conn)
    .await
}

/// Micro-USD spent by `account_ids` since `since` (inclusive). An empty list is 0.
pub async fn spend_since(
    conn: &mut PgConnection,
    account_ids: &[Uuid],
    since: OffsetDateTime,
) -> Result<i64, sqlx::Error> {
    if account_ids.is_empty() {
        return Ok(0);
    }
    sqlx::query_scalar(
        "SELECT coalesce(sum(cost_micro_usd), 0)::bigint FROM ai_usage \
         WHERE account_id = ANY($1) AND created_at >= $2",
    )
    .bind(account_ids)
    .bind(since)
    .fetch_one(conn)
    .await
}

/// Micro-USD spent by everyone since `since` (the global budget).
pub async fn global_spend_since(
    conn: &mut PgConnection,
    since: OffsetDateTime,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT coalesce(sum(cost_micro_usd), 0)::bigint FROM ai_usage WHERE created_at >= $1",
    )
    .bind(since)
    .fetch_one(conn)
    .await
}
