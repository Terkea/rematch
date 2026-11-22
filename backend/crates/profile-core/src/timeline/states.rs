//! Weakness timeline states (R2 Task 14, TS M2 Task 18).
//!
//! [`next_state`] is the TS rule list, first match wins:
//! ```text
//! if prev in {resolved}:            return drift ? relapsed : resolved
//! if prev in {active,improving} && belowP40Streak >= 10: return resolved   (TS: also emerging, relapsed)
//! if prev in {active,emerging,relapsed} && (confirmedDown || (pImp ?? 0) >= 0.85): return improving
//! if prev == improving && !confirmedUp: return improving
//! if confidenceAbove >= 0.7:        return active
//! if (confirmedUp && !atOrBelowMedian) || cusumUp: return emerging   (TS: any confirmedUp)
//! if prev in {active,emerging,relapsed} && !atOrBelowMedian: return prev
//! return not_an_issue
//! ```
//! A NaN `confidence_above` or `p_imp` fails every comparison, so it claims nothing. An
//! infinite or out-of-range value is not a probability and is treated as NaN.

use crate::types::WeaknessState;

/// Version of the detector and state rules, stored on every history row.
pub const DETECTOR_VERSION: &str = "tl-v1";
/// Consecutive evaluated games below band P40 that resolve a weakness.
pub const RESOLVE_STREAK: u32 = 10;
/// `P_imp` at or above which a weakness is improving.
pub const IMPROVING_P_IMP: f64 = 0.85;
/// `P(theta > band median)` at or above which a weakness is active.
pub const ACTIVE_CONFIDENCE: f64 = 0.7;

/// Per-game inputs of [`next_state`] (TS `Signals`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Signals {
    /// A rise was confirmed at this game (evidence gate met).
    pub confirmed_up: bool,
    /// A fall was confirmed at this game (evidence gate met).
    pub confirmed_down: bool,
    /// CUSUM alarmed on the current segment.
    pub cusum_up: bool,
    /// `P(theta > band median)` of the current segment, in `[0, 1]`.
    pub confidence_above: f64,
    /// The current segment's rate is at or below the band median.
    pub at_or_below_median: bool,
    /// Consecutive games at which the segment rate was below band P40.
    pub below_p40_streak: u32,
    /// `P_imp` across the last confirmed change, if there is one.
    pub p_imp: Option<f64>,
    /// EWMA drift alarm since the weakness was resolved.
    pub drift: bool,
}

/// A probability, or NaN when the value is not one (so every comparison fails).
fn prob(x: f64) -> f64 {
    if (0.0..=1.0).contains(&x) {
        x
    } else {
        f64::NAN
    }
}

/// The next state after one game (TS `nextState`); see the module docs.
pub fn next_state(prev: Option<WeaknessState>, s: &Signals) -> WeaknessState {
    use WeaknessState::*;
    let tracked = matches!(prev, Some(Active | Emerging | Relapsed));
    if prev == Some(Resolved) {
        return if s.drift { Relapsed } else { Resolved };
    }
    // R2 deviation from TS (ADR-R2-6 §12): only an established weakness (`active`) or one
    // already improving can be resolved; `emerging` and `relapsed` must go through them.
    if matches!(prev, Some(Active | Improving)) && s.below_p40_streak >= RESOLVE_STREAK {
        return Resolved;
    }
    let p_imp = s.p_imp.map_or(0.0, prob);
    if tracked && (s.confirmed_down || p_imp >= IMPROVING_P_IMP) {
        return Improving;
    }
    if prev == Some(Improving) && !s.confirmed_up {
        return Improving;
    }
    if prob(s.confidence_above) >= ACTIVE_CONFIDENCE {
        return Active;
    }
    // R2 deviation (ADR-R2-6 §12): a confirmed rise is a weakness only above the band median.
    if (s.confirmed_up && !s.at_or_below_median) || s.cusum_up {
        return Emerging;
    }
    if tracked
        && !s.at_or_below_median
        && let Some(p) = prev
    {
        return p;
    }
    NotAnIssue
}
