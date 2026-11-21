//! Spread-based fallback provider (R2 Task 24).
//!
//! With `lines` MultiPV lines and `playable` of them within [`OK_WINDOW`] of the best:
//! `p_ok = playable / lines` and `p_best = 1 / playable`. Fewer than two lines give `None`.

use futures::future::BoxFuture;

use super::{OK_WINDOW, best_win};
use crate::types::{DifficultyError, DifficultyEstimate, DifficultyInput, DifficultyProvider};

pub const SPREAD_MODEL: &str = "spread-v1";

#[derive(Clone, Copy, Debug, Default)]
pub struct SpreadDifficulty;

impl SpreadDifficulty {
    /// `(p_best, p_ok)` for one input.
    pub fn estimate_one(input: &DifficultyInput) -> DifficultyEstimate {
        let lines = input.lines.as_deref()?;
        if lines.len() < 2 {
            return None;
        }
        let best = best_win(lines)?;
        let playable = lines.iter().filter(|(_, w)| *w >= best - OK_WINDOW).count();
        // The best line is always playable, so `playable >= 1` unless a win% is NaN.
        if playable == 0 {
            return None;
        }
        Some((1.0 / playable as f64, playable as f64 / lines.len() as f64))
    }
}

impl DifficultyProvider for SpreadDifficulty {
    fn model(&self) -> &str {
        SPREAD_MODEL
    }

    fn estimate<'a>(
        &'a self,
        inputs: &'a [DifficultyInput],
    ) -> BoxFuture<'a, Result<Vec<DifficultyEstimate>, DifficultyError>> {
        Box::pin(async move { Ok(inputs.iter().map(Self::estimate_one).collect()) })
    }
}
