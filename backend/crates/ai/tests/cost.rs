//! Port of `packages/ai/test/cost.test.ts` and the `UnknownModelError` block of
//! `test/client/fix-round-1.test.ts`, plus the pricing config (fix round 1).
//!
//! Sonnet 5.5 cache reads are 0.10 USD/MTok (0.05x input) on the live pricing page checked on
//! 2026-11-21 (ruling Q5), not the 0.20 the TS table and the brief carried, so the first two
//! expectations are 13000 and 6500 instead of 14000 and 7000 (ADR-R3-5).

use std::path::PathBuf;

use ai::client::cost::{
    AI_MONTHLY_BUDGET_MICRO_USD, Budgets, MODEL_PRICING, PRICING_AS_OF, PRICING_SOURCE,
    PricingError, PricingTable, check_budget, cost_micro_usd,
};
use ai::client::model::{AiModel, Models};
use ai::client::port::{AiError, AiUsage};
use ai::entitlements::Plan;

fn usage(input: u64, output: u64, cache_write: u64, cache_read: u64) -> AiUsage {
    AiUsage {
        input_tokens: input,
        output_tokens: output,
        cache_creation_tokens: cache_write,
        cache_read_tokens: cache_read,
    }
}

/// The seed table, as the TS `costMicroUsd` used its const.
fn cost(model: &str, u: &AiUsage, batch: bool) -> Result<u64, AiError> {
    cost_micro_usd(&PricingTable::seed(), model, u, batch)
}

const SAME: AiUsage = AiUsage {
    input_tokens: 1000,
    output_tokens: 500,
    cache_creation_tokens: 2000,
    cache_read_tokens: 10000,
};

#[test]
fn prices_sonnet_with_cache_tiers() {
    // 1000*2.00 + 500*10.00 + 2000*2.50 + 10000*0.10
    assert_eq!(cost("claude-sonnet-5-5", &SAME, false), Ok(13000));
}

#[test]
fn halves_for_batch() {
    assert_eq!(cost("claude-sonnet-5-5", &SAME, true), Ok(6500));
}

#[test]
fn prices_haiku() {
    let u = usage(1000, 1000, 0, 0);
    assert_eq!(cost("claude-haiku-5-5", &u, false), Ok(600));
}

#[test]
fn applies_the_haiku_long_prompt_tier_above_100k_prompt_tokens() {
    let u = usage(150_000, 1000, 0, 0);
    assert_eq!(cost("claude-haiku-5-5", &u, false), Ok(77500));
}

#[test]
fn counts_cache_tokens_toward_the_long_prompt_threshold() {
    let u = usage(1000, 0, 0, 200_000);
    assert_eq!(cost("claude-haiku-5-5", &u, false), Ok(500 + 10_000));
}

#[test]
fn long_prompt_tier_starts_strictly_above_the_threshold() {
    // Exactly 100 000 prompt tokens stays on the base tier.
    let u = usage(100_000, 0, 0, 0);
    assert_eq!(cost("claude-haiku-5-5", &u, false), Ok(10_000));
    let u = usage(100_001, 0, 0, 0);
    assert_eq!(cost("claude-haiku-5-5", &u, false), Ok(50_001));
}

#[test]
fn prompt_sum_saturates_instead_of_overflowing() {
    let u = usage(u64::MAX, 0, u64::MAX, u64::MAX);
    assert!(cost("claude-haiku-5-5", &u, false).is_ok());
}

#[test]
fn rounds_to_an_integer() {
    // 1*0.10 + 1*0.125 = 0.225, halved 0.1125 -> 0 (u64 by type); half up is checked below.
    let u = usage(1, 0, 1, 0);
    assert_eq!(cost("claude-haiku-5-5", &u, true), Ok(0));
    // 5 haiku output tokens = 2.5 -> 3 (half up); in batch 1.25 -> 1.
    let u = usage(0, 5, 0, 0);
    assert_eq!(cost("claude-haiku-5-5", &u, false), Ok(3));
    assert_eq!(cost("claude-haiku-5-5", &u, true), Ok(1));
    // 3 sonnet cache-write tokens in batch = 3.75 -> 4.
    assert_eq!(cost("claude-sonnet-5-5", &usage(0, 0, 3, 0), true), Ok(4));
}

#[test]
fn exposes_the_model_map() {
    assert_eq!(Models::REPORT.as_str(), "claude-sonnet-5-5");
    assert_eq!(Models::CHAT.as_str(), "claude-sonnet-5-5");
    assert_eq!(Models::EXPLAIN.as_str(), "claude-haiku-5-5");
    assert_eq!(Models::WEEKLY.as_str(), "claude-haiku-5-5");
    // Every model we call has a seed price, and nothing else does.
    let priced: Vec<&str> = MODEL_PRICING.iter().map(|(id, _)| *id).collect();
    let models: Vec<&str> = AiModel::ALL.iter().map(|m| m.as_str()).collect();
    assert_eq!(priced, models);
}

#[test]
fn pricing_table_is_the_dated_one() {
    let t = PricingTable::seed();
    assert_eq!(t.validate(), Ok(()));
    assert_eq!(t.source, PRICING_SOURCE);
    assert_eq!(t.as_of, PRICING_AS_OF);
    let s = t.get("claude-sonnet-5-5").unwrap();
    assert_eq!(
        (s.input, s.output, s.cache_write, s.cache_read),
        (2.0, 10.0, 2.5, 0.1)
    );
    assert!(s.long_prompt.is_none());
    let h = t.get("claude-haiku-5-5").unwrap();
    assert_eq!(
        (h.input, h.output, h.cache_write, h.cache_read),
        (0.1, 0.5, 0.125, 0.01)
    );
    let (above, r) = h.long_prompt.unwrap();
    assert_eq!(above, 100_000);
    assert_eq!(
        (r.input, r.output, r.cache_write, r.cache_read),
        (0.5, 2.5, 0.625, 0.05)
    );
}

// ---- pricing config ---------------------------------------------------------------------------

fn seed_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config/pricing.json")
}

#[test]
fn shipped_config_file_equals_the_seed() {
    let t = PricingTable::from_file(&seed_file()).unwrap();
    assert_eq!(t, PricingTable::seed());
}

const SONNET: &str =
    r#""claude-sonnet-5-5": {"input": 2.0, "output": 10.0, "cache_write": 2.5, "cache_read": 0.1}"#;
const HAIKU: &str = r#""claude-haiku-5-5": {"input": 0.1, "output": 0.5, "cache_write": 0.125, "cache_read": 0.01}"#;

fn config(models: &str) -> String {
    format!(r#"{{"source": "test", "as_of": "2026-12-01", "models": {{{models}}}}}"#)
}

#[test]
fn a_config_override_changes_the_cost() {
    let sonnet_up = r#""claude-sonnet-5-5": {"input": 3.0, "output": 15.0, "cache_write": 3.75, "cache_read": 0.3}"#;
    let t = PricingTable::from_json(&config(&format!("{sonnet_up}, {HAIKU}"))).unwrap();
    // 1000*3 + 500*15 + 2000*3.75 + 10000*0.3
    assert_eq!(
        cost_micro_usd(&t, "claude-sonnet-5-5", &SAME, false),
        Ok(21000)
    );
    assert_eq!(t.as_of, "2026-12-01");
    // Haiku without a long-prompt tier in this config stays on the base rate above 100k.
    let u = usage(150_000, 0, 0, 0);
    assert_eq!(
        cost_micro_usd(&t, "claude-haiku-5-5", &u, false),
        Ok(15_000)
    );
    // A file override works the same way.
    let path = std::env::temp_dir().join(format!("ai-pricing-{}.json", std::process::id()));
    std::fs::write(&path, config(&format!("{sonnet_up}, {HAIKU}"))).unwrap();
    let from_file = PricingTable::from_file(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(from_file, t);
}

#[test]
fn a_missing_model_fails_validation() {
    assert_eq!(
        PricingTable::from_json(&config(SONNET)),
        Err(PricingError::MissingModel("claude-haiku-5-5".into()))
    );
    assert_eq!(
        PricingTable::from_json(&config(HAIKU)),
        Err(PricingError::MissingModel("claude-sonnet-5-5".into()))
    );
}

#[test]
fn rates_with_more_than_three_decimals_fail_validation() {
    let bad = r#""claude-sonnet-5-5": {"input": 2.0, "output": 10.0, "cache_write": 2.5, "cache_read": 0.1005}"#;
    assert_eq!(
        PricingTable::from_json(&config(&format!("{bad}, {HAIKU}"))),
        Err(PricingError::InvalidRate {
            model: "claude-sonnet-5-5".into(),
            field: "cache_read".into()
        })
    );
    let neg = r#""claude-sonnet-5-5": {"input": -2.0, "output": 10.0, "cache_write": 2.5, "cache_read": 0.1}"#;
    assert!(matches!(
        PricingTable::from_json(&config(&format!("{neg}, {HAIKU}"))),
        Err(PricingError::InvalidRate { .. })
    ));
    let long = r#""claude-haiku-5-5": {"input": 0.1, "output": 0.5, "cache_write": 0.125, "cache_read": 0.01,
        "long_prompt": {"above": 100000, "input": 0.5, "output": 2.5, "cache_write": 0.6255, "cache_read": 0.05}}"#;
    assert_eq!(
        PricingTable::from_json(&config(&format!("{SONNET}, {long}"))),
        Err(PricingError::InvalidRate {
            model: "claude-haiku-5-5".into(),
            field: "long_prompt.cache_write".into()
        })
    );
    let zero = r#""claude-haiku-5-5": {"input": 0.1, "output": 0.5, "cache_write": 0.125, "cache_read": 0.01,
        "long_prompt": {"above": 0, "input": 0.5, "output": 2.5, "cache_write": 0.625, "cache_read": 0.05}}"#;
    assert_eq!(
        PricingTable::from_json(&config(&format!("{SONNET}, {zero}"))),
        Err(PricingError::InvalidThreshold("claude-haiku-5-5".into()))
    );
}

#[test]
fn unknown_keys_and_bad_files_fail() {
    let typo = r#""claude-sonnet-5-5": {"input": 2.0, "output": 10.0, "cache_write": 2.5, "cache_reads": 0.1}"#;
    assert!(matches!(
        PricingTable::from_json(&config(&format!("{typo}, {HAIKU}"))),
        Err(PricingError::Parse(_))
    ));
    assert!(matches!(
        PricingTable::from_file(&PathBuf::from("/nonexistent/pricing.json")),
        Err(PricingError::Read { .. })
    ));
}

#[test]
fn from_env_without_a_file_is_the_validated_seed() {
    // The test environment does not set AI_PRICING_FILE.
    if std::env::var_os("AI_PRICING_FILE").is_none() {
        assert_eq!(PricingTable::from_env(), Ok(PricingTable::seed()));
    }
}

// ---- budgets and unknown models ---------------------------------------------------------------

#[test]
fn hard_stops_at_the_plan_budget() {
    let b = &AI_MONTHLY_BUDGET_MICRO_USD;
    assert_eq!(check_budget(2_000_000, Plan::Pro, b), Err(AiError::Budget));
    assert_eq!(check_budget(1_999_999, Plan::Pro, b), Ok(()));
    assert_eq!(check_budget(200_000, Plan::Free, b), Err(AiError::Budget));
    assert_eq!(check_budget(199_999, Plan::Free, b), Ok(()));
    assert_eq!(
        AI_MONTHLY_BUDGET_MICRO_USD,
        Budgets {
            free: 200_000,
            pro: 2_000_000
        }
    );
}

#[test]
fn is_thrown_for_an_unpriced_model() {
    let u = usage(1, 1, 0, 0);
    assert_eq!(
        cost("claude-nope", &u, false),
        Err(AiError::UnknownModel("claude-nope".into()))
    );
    assert_eq!(
        cost("toString", &u, false),
        Err(AiError::UnknownModel("toString".into()))
    );
    // A dated snapshot id is not our model.
    assert!(matches!(
        cost("claude-sonnet-5-5-20261001", &u, false),
        Err(AiError::UnknownModel(_))
    ));
}
