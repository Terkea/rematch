//! Back-pressure ETA (R2 Task 30, spec section 8d.4).
//!
//! The ETA is honest. It is `None` unless there is enough throughput data (at least
//! [`MIN_RATE_ROWS`] finished analyses in the last 10 minutes) and the account's last queued job
//! has an exact position in the fair order. It is never a guess, never a zero-width range (it is
//! a single figure), and never NaN or infinite.

use std::time::Duration;

use queue::KIND_ANALYSE;
use sqlx::PgPool;
use uuid::Uuid;

use crate::fair::{DEFAULT_SERVED_WINDOW, DEFAULT_SLICE, fair_order_sql};

/// Finished analyses needed in the rate window before a rate is believed.
pub const MIN_RATE_ROWS: i64 = 5;
/// The throughput window, in seconds.
pub const RATE_WINDOW_SECS: f64 = 600.0;
/// How many places of the fair order the position is read from. Only the first `$2` rows of
/// [`fair_order_sql`] are exact, so a position past this window is unknown.
pub const ETA_WINDOW: i64 = 400;

/// Selected games of the account by status: `total` over pending, analysing and done, and `done`.
/// `$1` is the account. Index: `games_account_status_tc_idx`.
pub const ETA_COUNTS_SQL: &str = "\
SELECT count(*)::int8 AS total, (count(*) FILTER (WHERE analysis_status = 'done'))::int8 AS done \
FROM games \
WHERE account_id = $1 AND selected_at IS NOT NULL \
  AND analysis_status IN ('pending', 'analysing', 'done')";

/// Finished analyses in the last 10 minutes. `$1` is the job kind.
pub const ETA_RATE_SQL: &str = "\
SELECT count(*)::int8 FROM job_metrics \
WHERE kind = $1 AND created_at > now() - interval '10 minutes'";

const QUEUED_SQL: &str = "\
SELECT count(*)::int8 FROM jobs WHERE state = 'queued' AND kind = $1 AND user_key = $2";

/// Where the account's jobs sit in the fair order: the greatest position and how many of its jobs
/// are inside the window. `$1` kinds, `$2` window, `$3` served window ms, `$4` slice, `$5` account.
fn position_sql() -> String {
    format!(
        "{fair}, \
ordered AS (SELECT user_key, \
    row_number() OVER (ORDER BY tier, slice, joined_at, priority, run_at, id) AS pos \
  FROM (SELECT * FROM fair ORDER BY tier, slice, joined_at, priority, run_at, id LIMIT $2) f) \
SELECT coalesce(max(pos), 0)::int8, count(*)::int8 FROM ordered WHERE user_key = $5",
        fair = fair_order_sql()
    )
}

/// How far an account's analysis has got and how long the rest should take.
#[derive(Clone, Debug, PartialEq)]
pub struct AccountEta {
    /// Games done, of the selected ones.
    pub done: i64,
    /// The account's selected games that are pending, analysing or done.
    pub total: i64,
    /// The position, 1-based and in the fair order, of the account's last queued analyse job (the
    /// jobs ahead of it, itself included). A lower bound when `eta` is `None` because of the
    /// window or a delayed job. 0 when nothing is queued.
    pub queued_ahead: i64,
    /// Finished analyses per second over the last 10 minutes; `None` under [`MIN_RATE_ROWS`].
    pub games_per_sec: Option<f64>,
    /// `queued_ahead / games_per_sec`; `None` when unknown. `Some(0)` when nothing is queued.
    pub eta: Option<Duration>,
}

/// The ETA of `account_id`, reading the fair order through [`ETA_WINDOW`] places.
pub async fn account_eta(pool: &PgPool, account_id: Uuid) -> Result<AccountEta, sqlx::Error> {
    account_eta_in(pool, account_id, ETA_WINDOW).await
}

/// [`account_eta`] with an explicit window (the number of places of the fair order that are read).
pub async fn account_eta_in(
    pool: &PgPool,
    account_id: Uuid,
    window: i64,
) -> Result<AccountEta, sqlx::Error> {
    let (total, done): (i64, i64) = sqlx::query_as(ETA_COUNTS_SQL)
        .bind(account_id)
        .fetch_one(pool)
        .await?;
    let rows: i64 = sqlx::query_scalar(ETA_RATE_SQL)
        .bind(KIND_ANALYSE)
        .fetch_one(pool)
        .await?;
    let games_per_sec = (rows >= MIN_RATE_ROWS).then(|| rows as f64 / RATE_WINDOW_SECS);

    let queued: i64 = sqlx::query_scalar(QUEUED_SQL)
        .bind(KIND_ANALYSE)
        .bind(account_id)
        .fetch_one(pool)
        .await?;
    if queued == 0 {
        return Ok(AccountEta {
            done,
            total,
            queued_ahead: 0,
            games_per_sec,
            eta: Some(Duration::ZERO),
        });
    }
    let served_ms = i64::try_from(DEFAULT_SERVED_WINDOW.as_millis()).unwrap_or(i64::MAX);
    let (last, found): (i64, i64) = sqlx::query_as(sqlx::AssertSqlSafe(position_sql()))
        .bind(vec![KIND_ANALYSE])
        .bind(window.max(1))
        .bind(served_ms)
        .bind(DEFAULT_SLICE)
        .bind(account_id)
        .fetch_one(pool)
        .await?;
    // Every queued job must be placed: one past the window, or delayed (not due yet), has no
    // exact position, so the last job's position is unknown.
    let exact = found == queued;
    let eta = match (exact, games_per_sec) {
        (true, Some(rate)) if rate.is_finite() && rate > 0.0 => {
            Duration::try_from_secs_f64(last as f64 / rate).ok()
        }
        _ => None,
    };
    Ok(AccountEta {
        done,
        total,
        queued_ahead: last,
        games_per_sec,
        eta,
    })
}

/// The sentence for the UI: never a spinner, never a made-up number.
pub fn format_eta(e: &AccountEta) -> String {
    match e.eta {
        Some(d) if d < Duration::from_secs(60) => {
            format!("under 1 min, analysing {} / {}", e.done, e.total)
        }
        Some(d) => {
            let whole = d.as_secs().div_ceil(60);
            let minutes = whole + u64::from(d.as_secs() % 60 == 0 && d.subsec_nanos() > 0);
            format!("about {minutes} min, analysing {} / {}", e.done, e.total)
        }
        None => format!("analysing {} / {}", e.done, e.total),
    }
}
