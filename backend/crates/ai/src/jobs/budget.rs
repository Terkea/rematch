//! Budgets and metering (Task 18 owns this file).
//!
//! **Task 25 stand-in.** T18 is not on this base, so the coach-chat job uses these minimal
//! versions of T18's planned `BudgetConfig`, `budget_guard` and `meter`. At merge, keep T18's
//! file and adapt the two call sites in `chat/job.rs` (T18's `meter` may take the pricing table
//! from elsewhere and also store `sdk_estimate_micro_usd`).

use futures::future::BoxFuture;
use sqlx::{PgConnection, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::client::cost::{
    AI_MONTHLY_BUDGET_MICRO_USD, Budgets, PricingTable, check_budget, cost_micro_usd,
};
use crate::client::model::AiModel;
use crate::client::port::{AiError, UnavailableReason};
use crate::db::usage::{self, AiFeature, UsageRow};
use crate::entitlements::{Plan, QuotaWindow, window_start};
use crate::generate::section::CallRecord;

/// Monthly budgets per plan and an optional global monthly cap, in micro-USD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudgetConfig {
    pub per_plan: Budgets,
    pub global_monthly: Option<u64>,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        BudgetConfig {
            per_plan: AI_MONTHLY_BUDGET_MICRO_USD,
            global_monthly: None,
        }
    }
}

fn db_unavailable() -> AiError {
    AiError::Unavailable {
        reason: UnavailableReason::Connection,
        retry_after: None,
    }
}

/// The hard stop run before every model call: the spend of `scope` (and of everyone, with a
/// global cap) since the start of `now`'s UTC month. A database error fails closed as
/// `Unavailable`.
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

/// One `ai_usage` row per call record, priced from `pricing` (never the SDK estimate). A
/// response model without a price is metered at the requested model's rates, with a `warn`.
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
        let cost = match cost_micro_usd(pricing, &c.model, &c.usage, c.batch) {
            Ok(v) => v,
            Err(_) => {
                tracing::warn!(
                    requested = requested_model.as_str(),
                    "no pricing for the response model; metered at the requested model's rates"
                );
                cost_micro_usd(pricing, requested_model.as_str(), &c.usage, c.batch).unwrap_or(0)
            }
        };
        usage::record_usage(
            conn,
            &UsageRow {
                account_id,
                feature,
                model: c.model.clone(),
                batch: c.batch,
                input_tokens: tokens(c.usage.input_tokens),
                output_tokens: tokens(c.usage.output_tokens),
                cache_creation_tokens: tokens(c.usage.cache_creation_tokens),
                cache_read_tokens: tokens(c.usage.cache_read_tokens),
                cost_micro_usd: i64::try_from(cost).unwrap_or(i64::MAX),
                ref_id: Some(ref_id.to_owned()),
            },
        )
        .await?;
    }
    Ok(())
}
