//! `cli queue-lag` (R2 Task 35, ADR-R2-17): the queue's lag and the autoscaler's advice. The
//! advice uses the default [`ScaleConfig`], whose zero cap and price never spend (no server is
//! ever added until the owner sets both), with no extra server running.

use std::time::Duration;

use scheduler::lag::{QueueLag, ScaleAdvice, ScaleConfig, queue_lag, scale_advice};
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::format::to_fixed;

pub const QUEUE_LAG_USAGE: &str = "usage: cli queue-lag [--json]";

fn advice_json(a: ScaleAdvice) -> Value {
    match a {
        ScaleAdvice::Add(n) => json!({ "action": "add", "servers": n }),
        ScaleAdvice::Hold => json!({ "action": "hold", "servers": 0 }),
        ScaleAdvice::Remove(n) => json!({ "action": "remove", "servers": n }),
    }
}

fn advice_text(a: ScaleAdvice) -> String {
    match a {
        ScaleAdvice::Add(n) => format!("add {n}"),
        ScaleAdvice::Hold => "hold".to_owned(),
        ScaleAdvice::Remove(n) => format!("remove {n}"),
    }
}

/// The `--json` document: `oldestP0AgeSeconds` (null without a P0 job), `queuedGames`,
/// `coreSecondsQueued` and `advice`.
pub fn lag_json(lag: &QueueLag, advice: ScaleAdvice) -> Value {
    json!({
        "oldestP0AgeSeconds": lag.oldest_p0_age.map(|d| d.as_secs_f64()),
        "queuedGames": lag.queued_games,
        "coreSecondsQueued": lag.core_seconds_queued,
        "advice": advice_json(advice),
    })
}

pub fn lag_line(lag: &QueueLag, advice: ScaleAdvice) -> String {
    let age = lag
        .oldest_p0_age
        .map_or_else(|| "none".to_owned(), |d| format!("{} s", d.as_secs()));
    format!(
        "oldest P0 age: {age}  queued games: {}  core-seconds queued: {}  advice: {}",
        lag.queued_games,
        to_fixed(lag.core_seconds_queued, 0),
        advice_text(advice),
    )
}

/// `cli queue-lag [--json]`. Returns the exit code.
pub async fn run_queue_lag(
    argv: &[String],
    pool: &PgPool,
    print: &dyn Fn(&str),
    print_error: &dyn Fn(&str),
) -> i32 {
    let json = match argv.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["queue-lag"] => false,
        ["queue-lag", "--json"] => true,
        _ => {
            print_error(QUEUE_LAG_USAGE);
            return 1;
        }
    };
    let lag = match queue_lag(pool).await {
        Ok(l) => l,
        Err(e) => {
            print_error(&format!("{e:#}"));
            return 1;
        }
    };
    let advice = scale_advice(&lag, Duration::ZERO, 0, 0.0, &ScaleConfig::default());
    if json {
        print(&lag_json(&lag, advice).to_string());
    } else {
        print(&lag_line(&lag, advice));
    }
    0
}
