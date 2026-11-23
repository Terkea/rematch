//! Budgets and metering for every AI job (Task 18, ADR 0061 "Budgets" and "Degradation").
//!
//! - [`BudgetConfig`]: the monthly budget per plan and an optional global monthly cap, in
//!   micro-USD, from the environment ([`BudgetConfig::from_env`]).
//! - [`budget_guard`]: the budget check run before every model call (the `BeforeCall` hook of the
//!   generators and the chat).
//! - [`meter`]: one `ai_usage` row per metered model entry, priced from the [`PricingTable`]
//!   (never the SDK's estimate, which is kept beside it for comparison).
//! - [`meter_keyed`]: the same, idempotent per call through `ai_usage.idempotency_key`, so a
//!   retried or re-run write never meters a call twice. Jobs use this one.
//!
//! The report job and the coach-chat job (Task 25) share these, with the names of T25's
//! stand-ins; `ChatDeps.pricing` is the same [`PricingTable`] (re-exported here).

use futures::future::BoxFuture;
use sqlx::{PgConnection, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

pub use crate::client::cost::PricingTable;
use crate::client::cost::{AI_MONTHLY_BUDGET_MICRO_USD, Budgets, check_budget, cost_micro_usd};
use crate::client::model::AiModel;
use crate::client::port::{AiError, UnavailableReason};
use crate::db::usage::{self, AiFeature, UsageRow};
use crate::entitlements::{Plan, QuotaWindow, window_start};
use crate::generate::section::CallRecord;

/// The free plan's monthly budget override.
pub const BUDGET_FREE_ENV: &str = "AI_BUDGET_FREE_MICRO_USD";
/// The pro plan's monthly budget override.
pub const BUDGET_PRO_ENV: &str = "AI_BUDGET_PRO_MICRO_USD";
/// The optional global monthly cap (unset = no global cap).
pub const GLOBAL_BUDGET_ENV: &str = "AI_GLOBAL_MONTHLY_BUDGET_MICRO_USD";

/// Monthly budgets per plan and an optional global monthly cap, in micro-USD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudgetConfig {
    pub per_plan: Budgets,
    pub global_monthly: Option<u64>,
    /// Whether [`budget_guard`] blocks a call over budget. False only on the host's own Claude
    /// Code login (dev only, ADR 0062 Amendment 3), where a call has no real cost: usage is still
    /// metered, at list prices, but the budget is notional.
    pub enforce: bool,
}

impl Default for BudgetConfig {
    /// [`AI_MONTHLY_BUDGET_MICRO_USD`], no global cap.
    fn default() -> Self {
        BudgetConfig {
            per_plan: AI_MONTHLY_BUDGET_MICRO_USD,
            global_monthly: None,
            enforce: true,
        }
    }
}

/// A budget variable that is set but is not a non-negative integer.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{var} must be a non-negative integer number of micro-USD, got {value:?}")]
pub struct BudgetConfigError {
    pub var: String,
    pub value: String,
}

impl BudgetConfig {
    /// Reads [`BUDGET_FREE_ENV`], [`BUDGET_PRO_ENV`] and [`GLOBAL_BUDGET_ENV`]. An unset (or
    /// empty) variable keeps the default; an invalid one is a startup error.
    pub fn from_env() -> Result<Self, BudgetConfigError> {
        Self::from_vars(|k| std::env::var(k).ok())
    }

    /// [`BudgetConfig::from_env`] over any variable lookup.
    pub fn from_vars(get: impl Fn(&str) -> Option<String>) -> Result<Self, BudgetConfigError> {
        let read = |var: &str| -> Result<Option<u64>, BudgetConfigError> {
            match get(var) {
                None => Ok(None),
                Some(v) if v.trim().is_empty() => Ok(None),
                Some(v) => v
                    .trim()
                    .parse::<u64>()
                    .map(Some)
                    .map_err(|_| BudgetConfigError {
                        var: var.to_owned(),
                        value: v,
                    }),
            }
        };
        let d = BudgetConfig::default();
        Ok(BudgetConfig {
            per_plan: Budgets {
                free: read(BUDGET_FREE_ENV)?.unwrap_or(d.per_plan.free),
                pro: read(BUDGET_PRO_ENV)?.unwrap_or(d.per_plan.pro),
            },
            global_monthly: read(GLOBAL_BUDGET_ENV)?,
            enforce: true,
        })
    }

    /// The same budgets, recorded but not enforced (ADR 0062 Amendment 3): the host's own
    /// Claude Code login has no real cost. Production refuses that mode, so it never applies
    /// there.
    pub fn notional(self) -> Self {
        BudgetConfig {
            enforce: false,
            ..self
        }
    }
}

fn db_unavailable() -> AiError {
    AiError::Unavailable {
        reason: UnavailableReason::Connection,
        retry_after: None,
    }
}

/// The budget check run before every model call (not a strict cap: calls in flight can pass
/// the budget by a few calls; ADR 0061 "Degradation"): the spend of `scope` (the accounts
/// sharing one budget) and, with a global cap, of everyone, since the start of `now`'s UTC
/// month. Spend at or above the budget is `Err(AiError::Budget)`. A database error fails closed as
/// `Unavailable` (the job retries later; no call is made). A notional budget
/// ([`BudgetConfig::notional`]) always passes: the calls are still metered.
pub fn budget_guard(
    pool: PgPool,
    scope: Vec<Uuid>,
    plan: Plan,
    cfg: BudgetConfig,
    now: OffsetDateTime,
) -> impl Fn() -> BoxFuture<'static, Result<(), AiError>> + Send + Sync {
    move || {
        let pool = pool.clone();
        let scope = scope.clone();
        Box::pin(async move {
            if !cfg.enforce {
                return Ok(());
            }
            let since = window_start(QuotaWindow::Month, now);
            let mut conn = pool.acquire().await.map_err(|_| db_unavailable())?;
            let spent = usage::spend_since(&mut conn, &scope, since)
                .await
                .map_err(|_| db_unavailable())?;
            check_budget(u64::try_from(spent).unwrap_or(0), plan, &cfg.per_plan)?;
            if let Some(cap) = cfg.global_monthly {
                let all = usage::global_spend_since(&mut conn, since)
                    .await
                    .map_err(|_| db_unavailable())?;
                if u64::try_from(all).unwrap_or(0) >= cap {
                    return Err(AiError::Budget);
                }
            }
            Ok(())
        })
    }
}

fn tokens(n: u64) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// USD to micro-USD, rounded half up; `None` for a missing, negative or non-finite estimate.
fn sdk_micro(usd: Option<f64>) -> Option<i64> {
    let micro = (usd? * 1_000_000.0).round();
    (0.0..9.0e18).contains(&micro).then_some(micro as i64)
}

/// The cost of one record: at its own model's price; an unpriced response model at the
/// requested model's price (with a `warn`); and if that is unpriced too, at the most expensive
/// priced model (fails closed: a paid call is never free).
fn cost_of(pricing: &PricingTable, c: &CallRecord, requested_model: AiModel) -> u64 {
    if let Ok(v) = cost_micro_usd(pricing, &c.model, &c.usage, c.batch) {
        return v;
    }
    tracing::warn!(
        requested = requested_model.as_str(),
        "no pricing for the response model; metered at the requested model's rates"
    );
    if let Ok(v) = cost_micro_usd(pricing, requested_model.as_str(), &c.usage, c.batch) {
        return v;
    }
    tracing::error!("no pricing for the requested model; metered at the highest known rates");
    pricing
        .model_ids()
        .filter_map(|m| cost_micro_usd(pricing, m, &c.usage, c.batch).ok())
        .max()
        // Unreachable: `PricingTable` validation requires a price for every `AiModel`, so the
        // requested model is always priced (review m6). Kept fail-closed regardless.
        .unwrap_or(u64::MAX)
}

fn row(
    pricing: &PricingTable,
    account_id: Uuid,
    feature: AiFeature,
    c: &CallRecord,
    ref_id: &str,
    requested_model: AiModel,
) -> UsageRow {
    UsageRow {
        account_id,
        feature,
        model: c.model.clone(),
        batch: c.batch,
        input_tokens: tokens(c.usage.input_tokens),
        output_tokens: tokens(c.usage.output_tokens),
        cache_creation_tokens: tokens(c.usage.cache_creation_tokens),
        cache_read_tokens: tokens(c.usage.cache_read_tokens),
        cost_micro_usd: i64::try_from(cost_of(pricing, c, requested_model)).unwrap_or(i64::MAX),
        ref_id: Some(ref_id.to_owned()),
        sdk_estimate_micro_usd: sdk_micro(c.sdk_cost_usd),
    }
}

/// Writes one `ai_usage` row per call record (a call that touched two models has two records),
/// priced from `pricing`, never from the SDK estimate, which is stored as
/// `sdk_estimate_micro_usd`. Run it in the transaction that stores what the calls produced.
/// Prefer [`meter_keyed`] wherever the same write can run twice.
pub async fn meter(
    conn: &mut PgConnection,
    pricing: &PricingTable,
    account_id: Uuid,
    feature: AiFeature,
    calls: &[CallRecord],
    ref_id: &str,
    requested_model: AiModel,
) -> Result<(), sqlx::Error> {
    for c in calls {
        usage::record_usage(
            conn,
            &row(pricing, account_id, feature, c, ref_id, requested_model),
        )
        .await?;
    }
    Ok(())
}

/// [`meter`], once per call: record `i` is keyed `"{key_prefix}:{i}"`
/// (`ai_usage.idempotency_key`), so metering the same calls again (a retried or re-run write)
/// adds nothing. The prefix must name the paid attempt, not just the item: a regenerated item
/// is a new paid call and needs a new prefix.
#[allow(clippy::too_many_arguments)]
pub async fn meter_keyed(
    conn: &mut PgConnection,
    pricing: &PricingTable,
    account_id: Uuid,
    feature: AiFeature,
    calls: &[CallRecord],
    ref_id: &str,
    requested_model: AiModel,
    key_prefix: &str,
) -> Result<(), sqlx::Error> {
    for (i, c) in calls.iter().enumerate() {
        usage::record_usage_once(
            conn,
            &row(pricing, account_id, feature, c, ref_id, requested_model),
            &format!("{key_prefix}:{i}"),
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::sdk_micro;

    #[test]
    fn sdk_estimates_round_to_whole_micro_usd_and_reject_junk() {
        assert_eq!(sdk_micro(Some(0.0125)), Some(12_500));
        assert_eq!(sdk_micro(Some(1.4e-6)), Some(1));
        assert_eq!(sdk_micro(Some(1.6e-6)), Some(2));
        assert_eq!(sdk_micro(Some(0.0)), Some(0));
        assert_eq!(sdk_micro(None), None);
        assert_eq!(sdk_micro(Some(-0.5)), None);
        assert_eq!(sdk_micro(Some(f64::NAN)), None);
    }
}
