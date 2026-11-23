//! The computed focuses of the climb plan (TS `selectFocuses`, ADR-AI-7): deterministic code picks
//! them, Claude only narrates them.

use crate::input::types::{CoachingInput, RankedWeakness, WeaknessState};

/// The minimum confidence for a focus.
pub const FOCUS_MIN_CONFIDENCE: f64 = 0.7;
/// The focus id of the general plan, the only one allowed when no weakness qualifies.
pub const GENERAL_FOCUS: &str = "general";

/// Weakness ids by ascending rank (ties keep input order) with confidence ≥ 0.7 and state active,
/// emerging, relapsed or improving, never W12. At most 2 on the first report, 3 otherwise. Empty
/// means the general focus.
pub fn select_focuses(input: &CoachingInput, is_first_report: bool) -> Vec<String> {
    select_focuses_of(&input.weaknesses, is_first_report)
}

/// [`select_focuses`] over the ranked weaknesses alone (the lab's `cli lab rank`, ADR 0106).
pub fn select_focuses_of(weaknesses: &[RankedWeakness], is_first_report: bool) -> Vec<String> {
    let mut ws: Vec<_> = weaknesses
        .iter()
        .filter(|w| {
            w.confidence >= FOCUS_MIN_CONFIDENCE
                && w.id != "W12"
                && matches!(
                    w.state,
                    WeaknessState::Active
                        | WeaknessState::Emerging
                        | WeaknessState::Relapsed
                        | WeaknessState::Improving
                )
        })
        .collect();
    ws.sort_by_key(|w| w.rank);
    let max = if is_first_report { 2 } else { 3 };
    ws.into_iter().take(max).map(|w| w.id.clone()).collect()
}
