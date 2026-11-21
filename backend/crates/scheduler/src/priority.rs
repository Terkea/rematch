//! Tier priorities (R2 Task 26).

use crate::select::Plan;

/// Queue priority for an analyse job: `BASE * 2 + (0 if lichess evals exist, else 1)`.
/// Lower runs first. BASE: tier 0 {pro 1, free 2}, tier 3 {pro 3, free 4}, tier 1 is 5,
/// tier 2 is 10. Unknown tiers sort like tier 2.
pub fn job_priority(tier: u8, plan: Plan, has_lichess_evals: bool) -> i16 {
    let base: i16 = match (tier, plan) {
        (0, Plan::Pro) => 1,
        (0, Plan::Free) => 2,
        (3, Plan::Pro) => 3,
        (3, Plan::Free) => 4,
        (1, _) => 5,
        _ => 10,
    };
    base * 2 + i16::from(!has_lichess_evals)
}

/// The fair-scheduling tier of a stored priority.
pub fn fair_tier(priority: i16) -> i16 {
    priority / 2
}
