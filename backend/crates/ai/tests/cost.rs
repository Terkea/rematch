//! Port of `packages/ai/test/cost.test.ts` and the `UnknownModelError` block of
//! `test/client/fix-round-1.test.ts`.
//!
//! Sonnet 5.5 cache reads are 0.10 USD/MTok (0.05x input) on the live pricing page checked on
//! 2026-11-21 (ruling Q5), not the 0.20 the TS table and the brief carried, so the first two
//! expectations are 13000 and 6500 instead of 14000 and 7000 (ADR-R3-5).

use ai::client::cost::{
    AI_MONTHLY_BUDGET_MICRO_USD, Budgets, MODEL_PRICING, check_budget, cost_micro_usd, pricing,
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

const SAME: AiUsage = AiUsage {
    input_tokens: 1000,
    output_tokens: 500,
    cache_creation_tokens: 2000,
    cache_read_tokens: 10000,
};

#[test]
fn prices_sonnet_with_cache_tiers() {
    // 1000*2.00 + 500*10.00 + 2000*2.50 + 10000*0.10
    assert_eq!(cost_micro_usd("claude-sonnet-5-5", &SAME, false), Ok(13000));
}

#[test]
fn halves_for_batch() {
    assert_eq!(cost_micro_usd("claude-sonnet-5-5", &SAME, true), Ok(6500));
}

#[test]
fn prices_haiku() {
    let u = usage(1000, 1000, 0, 0);
    assert_eq!(cost_micro_usd("claude-haiku-5-5", &u, false), Ok(600));
}

#[test]
fn applies_the_haiku_long_prompt_tier_above_100k_prompt_tokens() {
    let u = usage(150_000, 1000, 0, 0);
    assert_eq!(cost_micro_usd("claude-haiku-5-5", &u, false), Ok(77500));
}

#[test]
fn counts_cache_tokens_toward_the_long_prompt_threshold() {
    let u = usage(1000, 0, 0, 200_000);
    assert_eq!(
        cost_micro_usd("claude-haiku-5-5", &u, false),
        Ok(500 + 10_000)
    );
}

#[test]
fn long_prompt_tier_starts_strictly_above_the_threshold() {
    // Exactly 100 000 prompt tokens stays on the base tier.
    let u = usage(100_000, 0, 0, 0);
    assert_eq!(cost_micro_usd("claude-haiku-5-5", &u, false), Ok(10_000));
    let u = usage(100_001, 0, 0, 0);
    assert_eq!(cost_micro_usd("claude-haiku-5-5", &u, false), Ok(50_001));
}

#[test]
fn rounds_to_an_integer() {
    // 1*0.10 + 1*0.125 = 0.225, halved 0.1125 -> 0 (u64 by type); half up is checked below.
    let u = usage(1, 0, 1, 0);
    assert_eq!(cost_micro_usd("claude-haiku-5-5", &u, true), Ok(0));
    // 5 haiku output tokens = 2.5 -> 3 (half up); in batch 1.25 -> 1.
    let u = usage(0, 5, 0, 0);
    assert_eq!(cost_micro_usd("claude-haiku-5-5", &u, false), Ok(3));
    assert_eq!(cost_micro_usd("claude-haiku-5-5", &u, true), Ok(1));
    // 1 sonnet cache-write token in batch = 1.25 -> 1; 3 = 3.75 -> 4.
    assert_eq!(
        cost_micro_usd("claude-sonnet-5-5", &usage(0, 0, 3, 0), true),
        Ok(4)
    );
}

#[test]
fn exposes_the_model_map() {
    assert_eq!(Models::REPORT.as_str(), "claude-sonnet-5-5");
    assert_eq!(Models::CHAT.as_str(), "claude-sonnet-5-5");
    assert_eq!(Models::EXPLAIN.as_str(), "claude-haiku-5-5");
    assert_eq!(Models::WEEKLY.as_str(), "claude-haiku-5-5");
    // Every model we call has a price, and nothing else does.
    let priced: Vec<&str> = MODEL_PRICING.iter().map(|(id, _)| *id).collect();
    let models: Vec<&str> = AiModel::ALL.iter().map(|m| m.as_str()).collect();
    assert_eq!(priced, models);
}

#[test]
fn pricing_table_is_the_dated_one() {
    let s = pricing("claude-sonnet-5-5").unwrap();
    assert_eq!(
        (s.input, s.output, s.cache_write, s.cache_read),
        (2.0, 10.0, 2.5, 0.1)
    );
    assert!(s.long_prompt.is_none());
    let h = pricing("claude-haiku-5-5").unwrap();
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
        cost_micro_usd("claude-nope", &u, false),
        Err(AiError::UnknownModel("claude-nope".into()))
    );
    assert_eq!(
        cost_micro_usd("toString", &u, false),
        Err(AiError::UnknownModel("toString".into()))
    );
    // A dated snapshot id is not our model.
    assert!(matches!(
        cost_micro_usd("claude-sonnet-5-5-20261001", &u, false),
        Err(AiError::UnknownModel(_))
    ));
}
