//! Difficulty providers (R2 Task 24, ADR-R2-13).
//!
//! - [`spread::SpreadDifficulty`] (`spread-v1`): derived from the MultiPV win% spread alone.
//! - [`maia::MaiaDifficulty`]: the Maia-2 HTTP service (`services/maia`), when `MAIA_URL` is set.
//! - [`select::select_difficulty_provider`]: Maia when it answers `/healthz`, else spread.

pub mod maia;
pub mod select;
pub mod spread;

/// A line is acceptable ("ok") when its mover win% is within this many points of the best line.
pub const OK_WINDOW: f64 = 5.0;

/// The best line's mover win%, or `None` without lines.
pub(crate) fn best_win(lines: &[(String, f64)]) -> Option<f64> {
    lines.iter().map(|(_, w)| *w).reduce(f64::max)
}
