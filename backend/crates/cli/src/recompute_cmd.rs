//! `cli recompute-metrics` (ADR-FIX-C-1): rewrites every game whose `game_metrics` row an older
//! `PLAYER_GAME_METRICS_VERSION` wrote, from the stored evals, with no engine.
//!
//! It marks the stale games (`profile::requeue::mark_stale_metrics`), then recomputes every mark
//! (`recompute_marked` with `RecomputeDeps::cache_only`): metrics, openings and series rows and
//! `games.white_accuracy` / `black_accuracy`, then one `build-profile` job per account and time
//! class, which a running worker picks up (it rebuilds the profile and the KPI snapshots). A game
//! the cache cannot evaluate keeps its rows and its mark for the worker's difficulty sweep.

use profile::requeue::{RecomputeDeps, mark_stale_metrics, recompute_marked};
use queue::queue::Queue;
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;

pub const RECOMPUTE_USAGE: &str = "usage: cli recompute-metrics";

/// Runs the command; returns the exit code.
pub async fn run_recompute_metrics(
    argv: &[String],
    pool: &PgPool,
    print: &dyn Fn(&str),
    print_error: &dyn Fn(&str),
) -> i32 {
    if argv != ["recompute-metrics"] {
        print_error(RECOMPUTE_USAGE);
        return 1;
    }
    let run = async {
        let marked = mark_stale_metrics(pool).await?;
        print(&format!("marked {marked} games with stale metrics"));
        let deps = RecomputeDeps::cache_only(pool.clone(), Queue::new(pool.clone()));
        let r = recompute_marked(&deps, &CancellationToken::new()).await?;
        print(&format!(
            "recomputed {}, skipped {}, failed {}",
            r.recomputed.len(),
            r.skipped.len(),
            r.failed.len()
        ));
        anyhow::Ok(r.failed.is_empty())
    };
    match run.await {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(e) => {
            print_error(&format!("{e:#}"));
            1
        }
    }
}
