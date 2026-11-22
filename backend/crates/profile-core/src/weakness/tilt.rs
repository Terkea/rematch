//! Tilt and W12 residuals (R2 Task 8, TS M2 Task 7 `tiltResidualDifference`).
//!
//! W12 is continuous and never ranked: the caller compares the two residual samples (score minus
//! the rating-expected score). "Tilted" games are the 4th or later game of a session, or a game
//! right after a loss in the same session.

use crate::types::SessionInfo;

/// Games from this index in a session on count as tilted.
pub const TILT_MIN_INDEX: u32 = 4;

/// The residual split, plus what was left out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TiltSplit {
    pub tilted: Vec<f64>,
    pub other: Vec<f64>,
    /// Games without a residual (unknown score or ratings).
    pub unknown: u32,
    /// Games with a non-finite residual.
    pub invalid: u32,
}

/// `(tilted, other)` residuals, in input order.
pub fn tilt_residual_difference(sessions: &[SessionInfo]) -> (Vec<f64>, Vec<f64>) {
    let s = tilt_residual_difference_counted(sessions);
    (s.tilted, s.other)
}

pub fn tilt_residual_difference_counted(sessions: &[SessionInfo]) -> TiltSplit {
    let mut out = TiltSplit::default();
    for s in sessions {
        match s.residual {
            None => out.unknown += 1,
            Some(r) if !r.is_finite() => out.invalid += 1,
            Some(r) if s.index_in_session >= TILT_MIN_INDEX || s.after_loss => out.tilted.push(r),
            Some(r) => out.other.push(r),
        }
    }
    out
}
