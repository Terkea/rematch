//! Precision targets (R2 Task 26).

use profile_core::constants::{RANKABLE, min_evidence};
use profile_core::types::WeaknessKey;

/// One weakness estimate, reduced to what the stopping rule needs.
#[derive(Clone, Debug)]
pub struct PrecisionInput {
    pub key: WeaknessKey,
    pub ci_low: f64,
    pub ci_high: f64,
    pub band_median: Option<f64>,
    pub n_unweighted: f64,
}

/// True when every rankable estimate has `width <= 0.25 * median` or enough raw evidence.
/// Unrankable keys are ignored; with no rankable estimate the answer is false.
pub fn is_precise(est: &[PrecisionInput]) -> bool {
    let mut any = false;
    for e in est.iter().filter(|e| RANKABLE.contains(&e.key)) {
        any = true;
        let width = e.ci_high - e.ci_low;
        let tight = e.band_median.is_some_and(|m| width <= 0.25 * m + 1e-12);
        if !(tight || e.n_unweighted >= min_evidence(e.key)) {
            return false;
        }
    }
    any
}
