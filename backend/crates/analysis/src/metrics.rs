//! Per-stage timing metrics of one job attempt (ADR 0036): one `job_metrics` row plus one
//! `tracing` event with target `metrics`.

use std::collections::BTreeMap;

use sqlx::PgPool;
use sqlx::types::Json;
use uuid::Uuid;

use queue::KIND_ANALYSE;

/// The timings and counts of one analyse-game attempt. Every duration is in milliseconds; a
/// `None` stage did not run (for example `persist_ms` of a failed attempt).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameMetrics {
    pub job_id: i64,
    pub game_id: Uuid,
    pub account_id: Uuid,
    /// `job.started_at - max(job.run_at, job.created_at)`: how long the ready job waited.
    pub queue_wait_ms: Option<f64>,
    /// `parse_pgn`.
    pub parse_ms: Option<f64>,
    /// Summed time inside eval-cache calls (`EvalStats::cache_ms`).
    pub cache_ms: Option<f64>,
    /// SUM of engine-reported wall time over completed searches (`EvalStats::engine_wall_ms`).
    pub engine_wall_ms: Option<f64>,
    /// SUM of Stockfish CPU time (utime + stime from `/proc/<pid>/stat`) over completed searches.
    pub engine_cpu_ms: Option<f64>,
    /// The persist transaction (claim `done`, replace the moves, commit).
    pub persist_ms: Option<f64>,
    /// The whole handler, from entry to just before the metrics are written.
    pub total_ms: f64,
    /// Positions in the game (`plies + 1`).
    pub positions: Option<i32>,
    /// Distinct positions whose engine search completed.
    pub engine_positions: Option<i32>,
    pub cache_hits: Option<i32>,
    pub lichess_used: Option<i32>,
    /// Real elapsed span of the engine stage (`EvalStats::engine_stage_elapsed_ms`), persisted
    /// in `job_metrics.stages` (R2 Task 3).
    pub engine_stage_ms: Option<f64>,
    /// Real elapsed span of `evaluate_game` (`EvalStats::total_elapsed_ms`), in `stages`.
    pub evaluate_ms: Option<f64>,
    /// `done`, `skipped`, `superseded`, `failed`, `retry` or `released`. Logged only.
    pub outcome: &'static str,
}

/// The spans that have no column, as the `job_metrics.stages` map (a `None` span is left out).
pub fn stages(m: &GameMetrics) -> BTreeMap<&'static str, f64> {
    [
        ("engine_stage_ms", m.engine_stage_ms),
        ("evaluate_ms", m.evaluate_ms),
    ]
    .into_iter()
    .filter_map(|(k, v)| v.map(|v| (k, v)))
    .collect()
}

/// Writes one `job_metrics` row of kind `analyse-game`; `stages` carries [`stages`].
pub async fn record_metrics(pool: &PgPool, m: &GameMetrics) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO job_metrics (job_id, kind, game_id, account_id, queue_wait_ms, parse_ms, \
         cache_ms, engine_wall_ms, engine_cpu_ms, persist_ms, total_ms, positions, \
         engine_positions, cache_hits, lichess_used, stages) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
    )
    .bind(m.job_id)
    .bind(KIND_ANALYSE)
    .bind(m.game_id)
    .bind(m.account_id)
    .bind(m.queue_wait_ms)
    .bind(m.parse_ms)
    .bind(m.cache_ms)
    .bind(m.engine_wall_ms)
    .bind(m.engine_cpu_ms)
    .bind(m.persist_ms)
    .bind(m.total_ms)
    .bind(m.positions)
    .bind(m.engine_positions)
    .bind(m.cache_hits)
    .bind(m.lichess_used)
    .bind(Json(stages(m)))
    .execute(pool)
    .await?;
    Ok(())
}

/// Emits the metrics as one `tracing::info!` event with target `metrics` (all fields, including
/// the ones `job_metrics` has no column for).
pub fn log_metrics(m: &GameMetrics) {
    tracing::info!(
        target: "metrics",
        kind = KIND_ANALYSE,
        job_id = m.job_id,
        game_id = %m.game_id,
        account_id = %m.account_id,
        outcome = m.outcome,
        queue_wait_ms = m.queue_wait_ms,
        parse_ms = m.parse_ms,
        cache_ms = m.cache_ms,
        engine_wall_ms = m.engine_wall_ms,
        engine_cpu_ms = m.engine_cpu_ms,
        engine_stage_ms = m.engine_stage_ms,
        evaluate_ms = m.evaluate_ms,
        persist_ms = m.persist_ms,
        total_ms = m.total_ms,
        positions = m.positions,
        engine_positions = m.engine_positions,
        cache_hits = m.cache_hits,
        lichess_used = m.lichess_used,
        "job metrics"
    );
}
