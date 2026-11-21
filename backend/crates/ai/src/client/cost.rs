//! Pricing, cost metering and the monthly budgets (ADR-R3-5).

use super::errors::redact;
use super::port::{AiError, AiUsage};
use crate::entitlements::Plan;

/// USD per MTok, equal to micro-USD per token.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rates {
    pub input: f64,
    pub output: f64,
    /// The 5-minute TTL write rate.
    pub cache_write: f64,
    pub cache_read: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelPricing {
    pub input: f64,
    pub output: f64,
    pub cache_write: f64,
    pub cache_read: f64,
    /// `(above, rates)`: used when the prompt (input + cache write + cache read) exceeds
    /// `above` tokens.
    pub long_prompt: Option<(u64, Rates)>,
}

impl ModelPricing {
    fn base(&self) -> Rates {
        Rates {
            input: self.input,
            output: self.output,
            cache_write: self.cache_write,
            cache_read: self.cache_read,
        }
    }
}

/// Source: https://platform.claude.com/docs/en/about-claude/pricing, read 2026-11-21 (ruling
/// Q5), cross-checked with the `claude-api` skill table (cached 2026-10-06). The live page has
/// Sonnet 5.5 cache reads at 0.10 (0.05x input); the skill and the TS table said 0.20. Cache
/// write is the 5-minute TTL rate (1.25x input). The Batch API is 50% of these.
pub const MODEL_PRICING: [(&str, ModelPricing); 2] = [
    (
        "claude-sonnet-5-5",
        ModelPricing {
            input: 2.00,
            output: 10.00,
            cache_write: 2.50,
            cache_read: 0.10,
            long_prompt: None,
        },
    ),
    (
        "claude-haiku-5-5",
        ModelPricing {
            input: 0.10,
            output: 0.50,
            cache_write: 0.125,
            cache_read: 0.01,
            long_prompt: Some((
                100_000,
                Rates {
                    input: 0.50,
                    output: 2.50,
                    cache_write: 0.625,
                    cache_read: 0.05,
                },
            )),
        },
    ),
];

/// The pricing of an exact model id (no prefix or date-suffix matching).
pub fn pricing(model: &str) -> Option<&'static ModelPricing> {
    MODEL_PRICING
        .iter()
        .find(|(id, _)| *id == model)
        .map(|(_, p)| p)
}

/// Rates in thousandths of a micro-USD per token, so the sum is exact integer arithmetic. Every
/// rate in the table has at most three decimals.
fn milli(rate: f64) -> u128 {
    (rate * 1000.0).round() as u128
}

/// The cost of one response in whole micro-USD: halved for the Batch API, rounded half up.
/// Callers metering a response whose model is not priced use the requested model's id (and log
/// a `warn` at the metering site, ADR-R3-5).
pub fn cost_micro_usd(model: &str, u: &AiUsage, batch: bool) -> Result<u64, AiError> {
    let p = pricing(model).ok_or_else(|| AiError::UnknownModel(redact(model)))?;
    let prompt = u.input_tokens + u.cache_creation_tokens + u.cache_read_tokens;
    let r = match p.long_prompt {
        Some((above, rates)) if prompt > above => rates,
        _ => p.base(),
    };
    let raw = u128::from(u.input_tokens) * milli(r.input)
        + u128::from(u.output_tokens) * milli(r.output)
        + u128::from(u.cache_creation_tokens) * milli(r.cache_write)
        + u128::from(u.cache_read_tokens) * milli(r.cache_read);
    let divisor: u128 = if batch { 2000 } else { 1000 };
    let rounded = (raw + divisor / 2) / divisor;
    Ok(u64::try_from(rounded).unwrap_or(u64::MAX))
}

/// Monthly per-account budgets in micro-USD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budgets {
    pub free: u64,
    pub pro: u64,
}

impl Budgets {
    pub fn for_plan(&self, plan: Plan) -> u64 {
        match plan {
            Plan::Free => self.free,
            Plan::Pro => self.pro,
        }
    }
}

/// USD 0.20 (free) and USD 2.00 (pro) per calendar month (UTC).
pub const AI_MONTHLY_BUDGET_MICRO_USD: Budgets = Budgets {
    free: 200_000,
    pro: 2_000_000,
};

/// The hard stop, checked before every model call: spend at or above the budget is refused.
pub fn check_budget(spent: u64, plan: Plan, b: &Budgets) -> Result<(), AiError> {
    if spent >= b.for_plan(plan) {
        Err(AiError::Budget)
    } else {
        Ok(())
    }
}
