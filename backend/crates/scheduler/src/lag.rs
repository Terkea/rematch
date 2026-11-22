//! Queue lag and autoscaling advice (R2 Task 30, spec sections 8d.4 and 8d.5).
//!
//! [`queue_lag`] measures; [`scale_advice`] is a pure function an external autoscaler can call.
//! No cloud API is used here.

use std::time::Duration;

use queue::{KIND_ANALYSE, KIND_PREVIEW};
use sqlx::PgPool;

/// Mean engine CPU per game when no game finished in the last 24 hours.
pub const DEFAULT_CPU_MS_PER_GAME: f64 = 12_000.0;
/// P0 is the fair tiers up to 2, that is priority up to 5 (`priority / 2 <= 2`).
pub const P0_MAX_PRIORITY: i16 = 5;
/// Seconds one server is expected to chew through per scaling decision.
const WINDOW_SECS: f64 = 600.0;

/// `$1` the analyse kind, `$2` the preview kind. Columns: seconds the oldest due P0 job has waited
/// (`NULL` for none), queued analyse jobs, and the mean `engine_cpu_ms` of the last 24 hours
/// (`NULL` for none). Waiting counts from the later of `run_at` and `created_at`; a job that is
/// not yet due does not lag.
pub const LAG_SQL: &str = "\
SELECT \
  (SELECT extract(epoch FROM now() - min(greatest(p.run_at, p.created_at)))::float8 FROM ( \
     SELECT run_at, created_at FROM jobs \
       WHERE state = 'queued' AND kind = $1 AND priority <= 5 \
         AND (run_at <= now() OR run_at <= created_at) \
     UNION ALL \
     SELECT run_at, created_at FROM jobs \
       WHERE state = 'queued' AND kind = $2 AND priority <= 5 \
         AND (run_at <= now() OR run_at <= created_at)) p) AS age_s, \
  (SELECT count(*)::int8 FROM jobs WHERE state = 'queued' AND kind = $1) AS queued, \
  (SELECT avg(engine_cpu_ms)::float8 FROM job_metrics \
     WHERE kind = $1 AND created_at > now() - interval '24 hours' \
       AND engine_cpu_ms IS NOT NULL) AS mean_cpu_ms";

#[derive(Clone, Debug, PartialEq)]
pub struct QueueLag {
    /// How long the oldest due P0 job (preview included) has waited; `None` when none is queued.
    pub oldest_p0_age: Option<Duration>,
    /// Queued analyse jobs.
    pub queued_games: i64,
    /// Queued games times the mean engine CPU per game, in core-seconds.
    pub core_seconds_queued: f64,
}

pub async fn queue_lag(pool: &PgPool) -> Result<QueueLag, sqlx::Error> {
    let (age_s, queued, mean_cpu_ms): (Option<f64>, i64, Option<f64>) = sqlx::query_as(LAG_SQL)
        .bind(KIND_ANALYSE)
        .bind(KIND_PREVIEW)
        .fetch_one(pool)
        .await?;
    let oldest_p0_age = age_s
        .filter(|s| s.is_finite())
        .and_then(|s| Duration::try_from_secs_f64(s.max(0.0)).ok());
    let mean = mean_cpu_ms
        .filter(|m| m.is_finite() && *m >= 0.0)
        .unwrap_or(DEFAULT_CPU_MS_PER_GAME);
    Ok(QueueLag {
        oldest_p0_age,
        queued_games: queued,
        core_seconds_queued: queued.max(0) as f64 * mean / 1000.0,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScaleConfig {
    pub p0_lag_threshold: Duration,
    pub idle_scale_down: Duration,
    pub max_extra_servers: u32,
    pub engines_per_server: u32,
    /// Owner-approved spend cap per month.
    pub monthly_cap_eur: f64,
    pub eur_per_server_hour: f64,
}

impl Default for ScaleConfig {
    /// The briefed thresholds with a zero cap and a zero price: no spending until the owner
    /// sets both (see [`scale_advice`]).
    fn default() -> Self {
        Self {
            p0_lag_threshold: Duration::from_secs(3 * 60),
            idle_scale_down: Duration::from_secs(15 * 60),
            max_extra_servers: 4,
            engines_per_server: 48,
            monthly_cap_eur: 0.0,
            eur_per_server_hour: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleAdvice {
    Add(u32),
    Hold,
    Remove(u32),
}

/// What an external autoscaler should do. Pure: add servers when P0 lag exceeds the threshold
/// and the monthly cap leaves room for at least one server-hour; remove one when idle long enough
/// with extra servers running; else hold. Unusable numbers (a cap or price that is zero,
/// negative or NaN, or a NaN spend) spend nothing, `engines_per_server == 0` holds, and a bad backlog figure asks for one server, never a storm.
pub fn scale_advice(
    lag: &QueueLag,
    idle_for: Duration,
    current_extra: u32,
    spent_this_month_eur: f64,
    cfg: &ScaleConfig,
) -> ScaleAdvice {
    let lagging = lag.oldest_p0_age.is_some_and(|a| a > cfg.p0_lag_threshold);
    if lagging {
        // A zero engine count is a misconfiguration, not "one engine": do nothing.
        if cfg.engines_per_server == 0 {
            return ScaleAdvice::Hold;
        }
        let room = cfg.max_extra_servers.saturating_sub(current_extra);
        let per_server = f64::from(cfg.engines_per_server) * WINDOW_SECS;
        let wanted = if lag.core_seconds_queued.is_finite() && lag.core_seconds_queued > 0.0 {
            (lag.core_seconds_queued / per_server).ceil().max(1.0)
        } else {
            1.0
        };
        let affordable = affordable_server_hours(spent_this_month_eur, cfg);
        let n = wanted.min(f64::from(room)).min(affordable);
        return if n >= 1.0 {
            ScaleAdvice::Add(n as u32)
        } else {
            ScaleAdvice::Hold
        };
    }
    if idle_for >= cfg.idle_scale_down && current_extra > 0 {
        return ScaleAdvice::Remove(1);
    }
    ScaleAdvice::Hold
}

/// Whole server-hours the cap still pays for. Spend needs a positive, finite cap AND price: a
/// zero, negative, NaN or missing cap or price pays for nothing (a free price is no licence).
fn affordable_server_hours(spent: f64, cfg: &ScaleConfig) -> f64 {
    let (cap, price) = (cfg.monthly_cap_eur, cfg.eur_per_server_hour);
    if !(cap.is_finite() && cap > 0.0 && price.is_finite() && price > 0.0 && spent.is_finite()) {
        return 0.0;
    }
    ((cap - spent) / price).floor().max(0.0)
}
