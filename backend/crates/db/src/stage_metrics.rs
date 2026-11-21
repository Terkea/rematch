//! Per-stage timings for the R2 job kinds: one `job_metrics` row per job run with `total_ms` and a
//! `stages` JSON map (R1 ADR 0036; the `stages` column is added by the R2 profile migration).

use std::collections::BTreeMap;

use sqlx::PgPool;
use sqlx::types::Json;
use uuid::Uuid;

/// One job run's timings. `stages` maps a stage name (by convention ending in `_ms`) to its
/// wall-clock milliseconds; a `BTreeMap` keeps the JSON key order stable.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StageMetrics {
    pub job_id: Option<i64>,
    pub kind: String,
    pub game_id: Option<Uuid>,
    pub account_id: Option<Uuid>,
    pub total_ms: f64,
    pub stages: BTreeMap<String, f64>,
}

/// Inserts one `job_metrics` row. Callers treat a failure as best-effort (log and carry on).
pub async fn record_stage_metrics(pool: &PgPool, m: &StageMetrics) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO job_metrics (job_id, kind, game_id, account_id, total_ms, stages) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(m.job_id)
    .bind(&m.kind)
    .bind(m.game_id)
    .bind(m.account_id)
    .bind(m.total_ms)
    .bind(Json(&m.stages))
    .execute(pool)
    .await?;
    Ok(())
}
