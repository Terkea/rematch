//! Whether this worker's AI runners can write coach text (bug hunt I5, ADR 0078). The web cannot
//! see the worker's `ai_enabled`, so a worker with AI runners records it in `ai_status` at start
//! and refreshes the heartbeat while it runs. The web treats the coach as available only while a
//! row is enabled and fresh ([`STALE_AFTER`]); a stopped worker's row simply goes stale.

use std::time::Duration;

use sqlx::PgPool;
use uuid::Uuid;

/// How often a running worker refreshes its row.
pub const HEARTBEAT_EVERY: Duration = Duration::from_secs(30);

/// After this long without a heartbeat the web treats a row as gone (the web's
/// `AI_STATUS_STALE_MS` mirrors it): four missed beats.
pub const STALE_AFTER: Duration = Duration::from_secs(120);

/// Upserts this worker's row: its AI state and its startup log line (never a key or token).
pub async fn record(
    pool: &PgPool,
    worker_id: Uuid,
    enabled: bool,
    detail: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO ai_status (worker_id, enabled, detail) VALUES ($1, $2, $3) \
         ON CONFLICT (worker_id) DO UPDATE SET enabled = EXCLUDED.enabled, \
         detail = EXCLUDED.detail, heartbeat_at = now()",
    )
    .bind(worker_id)
    .bind(enabled)
    .bind(detail)
    .execute(pool)
    .await?;
    Ok(())
}

/// Refreshes this worker's heartbeat.
pub async fn beat(pool: &PgPool, worker_id: Uuid) -> sqlx::Result<()> {
    sqlx::query("UPDATE ai_status SET heartbeat_at = now() WHERE worker_id = $1")
        .bind(worker_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Beats every [`HEARTBEAT_EVERY`] until aborted. A failed beat is logged and retried on the next
/// tick: the row goes stale only if the database stays unreachable.
pub async fn heartbeat(pool: PgPool, worker_id: Uuid) {
    let mut tick = tokio::time::interval(HEARTBEAT_EVERY);
    tick.tick().await;
    loop {
        tick.tick().await;
        if let Err(e) = beat(&pool, worker_id).await {
            tracing::warn!(error = %e, "ai status heartbeat failed");
        }
    }
}
