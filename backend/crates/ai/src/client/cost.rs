//! Pricing, cost metering and the monthly budgets (ADR 0061).
//!
//! Prices come from a validated [`PricingTable`]. At startup the binary calls
//! [`PricingTable::from_env`]: with `AI_PRICING_FILE` set it loads that JSON file (format below),
//! otherwise it uses the compiled-in dated seed [`MODEL_PRICING`]. Either way the table is
//! validated before use, and an invalid table is a startup error.
//!
//! File format (`backend/crates/ai/config/pricing.json` is the seed, kept equal to
//! [`MODEL_PRICING`] by a test):
//!
//! ```json
//! {
//!   "source": "https://platform.claude.com/docs/en/about-claude/pricing",
//!   "as_of": "2026-11-21",
//!   "unit": "USD per MTok (= micro-USD per token); cache_write is the 5-minute TTL rate",
//!   "models": {
//!     "claude-haiku-5-5": {
//!       "input": 0.1, "output": 0.5, "cache_write": 0.125, "cache_read": 0.01,
//!       "long_prompt": { "above": 100000, "input": 0.5, "output": 2.5,
//!                        "cache_write": 0.625, "cache_read": 0.05 }
//!     }
//!   }
//! }
//! ```
//!
//! Every [`AiModel::ALL`] id must be priced; rates are finite, non-negative and have at most three
//! decimals (exact integer metering); `long_prompt.above` is positive. Unknown keys are errors.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use super::errors::redact;
use super::model::AiModel;
use super::port::{AiError, AiUsage};
use crate::entitlements::Plan;

/// USD per MTok, equal to micro-USD per token.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
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

/// Where the seed prices were read, and when.
pub const PRICING_SOURCE: &str = "https://platform.claude.com/docs/en/about-claude/pricing";
pub const PRICING_AS_OF: &str = "2026-11-21";
/// The env var naming a pricing JSON file that overrides the seed.
pub const PRICING_FILE_ENV: &str = "AI_PRICING_FILE";

/// The dated default seed. Source: [`PRICING_SOURCE`], read [`PRICING_AS_OF`] (ruling Q5),
/// cross-checked with the `claude-api` skill table (cached 2026-10-06). The live page has
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

/// Why a pricing table was rejected.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PricingError {
    #[error("cannot read pricing file {path}: {reason}")]
    Read { path: String, reason: String },
    #[error("pricing config is not valid JSON for the pricing format: {0}")]
    Parse(String),
    #[error("pricing config has no price for model \"{0}\"")]
    MissingModel(String),
    #[error("pricing for \"{model}\": {field} must be finite, >= 0 and have at most 3 decimals")]
    InvalidRate { model: String, field: String },
    #[error("pricing for \"{0}\": long_prompt.above must be > 0")]
    InvalidThreshold(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LongPromptFile {
    above: u64,
    input: f64,
    output: f64,
    cache_write: f64,
    cache_read: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelFile {
    input: f64,
    output: f64,
    cache_write: f64,
    cache_read: f64,
    #[serde(default)]
    long_prompt: Option<LongPromptFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PricingFile {
    source: String,
    as_of: String,
    #[serde(default)]
    #[allow(dead_code)]
    unit: Option<String>,
    models: BTreeMap<String, ModelFile>,
}

/// A validated set of model prices. Construct only through [`PricingTable::seed`],
/// [`PricingTable::from_json`], [`PricingTable::from_file`] or [`PricingTable::from_env`].
#[derive(Clone, Debug, PartialEq)]
pub struct PricingTable {
    pub source: String,
    pub as_of: String,
    models: BTreeMap<String, ModelPricing>,
}

impl Default for PricingTable {
    fn default() -> Self {
        PricingTable::seed()
    }
}

fn valid_rate(r: f64) -> bool {
    let scaled = r * 1000.0;
    r.is_finite() && r >= 0.0 && scaled < 1e12 && (scaled.round() - scaled).abs() < 1e-6
}

impl PricingTable {
    /// The compiled-in dated seed ([`MODEL_PRICING`]).
    pub fn seed() -> Self {
        PricingTable {
            source: PRICING_SOURCE.to_owned(),
            as_of: PRICING_AS_OF.to_owned(),
            models: MODEL_PRICING
                .iter()
                .map(|(id, p)| ((*id).to_owned(), *p))
                .collect(),
        }
    }

    /// Parses and validates the JSON format in the module docs.
    pub fn from_json(s: &str) -> Result<Self, PricingError> {
        let f: PricingFile =
            serde_json::from_str(s).map_err(|e| PricingError::Parse(redact(&e.to_string())))?;
        let models = f
            .models
            .into_iter()
            .map(|(id, m)| {
                let long_prompt = m.long_prompt.map(|l| {
                    (
                        l.above,
                        Rates {
                            input: l.input,
                            output: l.output,
                            cache_write: l.cache_write,
                            cache_read: l.cache_read,
                        },
                    )
                });
                let p = ModelPricing {
                    input: m.input,
                    output: m.output,
                    cache_write: m.cache_write,
                    cache_read: m.cache_read,
                    long_prompt,
                };
                (id, p)
            })
            .collect();
        let t = PricingTable {
            source: f.source,
            as_of: f.as_of,
            models,
        };
        t.validate()?;
        Ok(t)
    }

    pub fn from_file(path: &Path) -> Result<Self, PricingError> {
        let s = std::fs::read_to_string(path).map_err(|e| PricingError::Read {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;
        PricingTable::from_json(&s)
    }

    /// The startup loader: the file named by `AI_PRICING_FILE`, or the validated seed.
    pub fn from_env() -> Result<Self, PricingError> {
        match std::env::var_os(PRICING_FILE_ENV).filter(|v| !v.is_empty()) {
            Some(path) => PricingTable::from_file(Path::new(&path)),
            None => {
                let t = PricingTable::seed();
                t.validate()?;
                Ok(t)
            }
        }
    }

    /// Every [`AiModel::ALL`] id is priced, and every rate is metering-exact.
    pub fn validate(&self) -> Result<(), PricingError> {
        for m in AiModel::ALL {
            if !self.models.contains_key(m.as_str()) {
                return Err(PricingError::MissingModel(m.as_str().to_owned()));
            }
        }
        for (id, p) in &self.models {
            let bad = |field: &str| PricingError::InvalidRate {
                model: id.clone(),
                field: field.to_owned(),
            };
            let check = |prefix: &str, r: &Rates| {
                for (name, v) in [
                    ("input", r.input),
                    ("output", r.output),
                    ("cache_write", r.cache_write),
                    ("cache_read", r.cache_read),
                ] {
                    if !valid_rate(v) {
                        return Err(bad(&format!("{prefix}{name}")));
                    }
                }
                Ok(())
            };
            check("", &p.base())?;
            if let Some((above, r)) = &p.long_prompt {
                if *above == 0 {
                    return Err(PricingError::InvalidThreshold(id.clone()));
                }
                check("long_prompt.", r)?;
            }
        }
        Ok(())
    }

    /// The pricing of an exact model id (no prefix or date-suffix matching).
    pub fn get(&self, model: &str) -> Option<&ModelPricing> {
        self.models.get(model)
    }

    /// The priced model ids, sorted.
    pub fn model_ids(&self) -> impl Iterator<Item = &str> {
        self.models.keys().map(String::as_str)
    }
}

/// Rates in thousandths of a micro-USD per token, so the sum is exact integer arithmetic.
/// [`PricingTable::validate`] guarantees at most three decimals.
fn milli(rate: f64) -> u128 {
    (rate * 1000.0).round() as u128
}

/// The cost of one response in whole micro-USD: halved for the Batch API, rounded half up.
/// Callers metering a response whose model is not priced use the requested model's id (and log
/// a `warn` at the metering site, ADR 0061).
pub fn cost_micro_usd(
    table: &PricingTable,
    model: &str,
    u: &AiUsage,
    batch: bool,
) -> Result<u64, AiError> {
    let p = table
        .get(model)
        .ok_or_else(|| AiError::UnknownModel(redact(model)))?;
    let prompt = u
        .input_tokens
        .saturating_add(u.cache_creation_tokens)
        .saturating_add(u.cache_read_tokens);
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
    /// The beta plan has the Pro budget (M5b ruling G1).
    pub fn for_plan(&self, plan: Plan) -> u64 {
        match plan {
            Plan::Free => self.free,
            Plan::Pro | Plan::Beta => self.pro,
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
