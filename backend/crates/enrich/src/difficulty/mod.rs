//! Move difficulty from Maia-2 only (R2 Task 24, ADR-R2-13).
//!
//! The owner's direction: difficulty comes only from Maia (or Stockfish), never from simple
//! engine-based estimates. So there is no fallback estimator. When Maia cannot answer, the
//! outcome says why, and nothing is guessed:
//!
//! - [`DifficultyOutcome::Ok`]: Maia's estimate, tagged with the Maia model id.
//! - [`DifficultyOutcome::PendingMaia`]: Maia was not configured, unavailable, timed out or gave
//!   an invalid answer. `difficulty = None`; the position is re-enriched once Maia is healthy
//!   (Task 25 hand-off). Analysis is never blocked and Stockfish evaluations are unaffected.
//! - [`DifficultyOutcome::NotApplicable`]: Maia cannot estimate this position, and never will
//!   (an opening ply Maia was not trained on, no legal moves, an invalid FEN, no best move).
//!   Never pending.
//!
//! - [`maia::MaiaDifficulty`]: the HTTP client for `services/maia`.
//! - [`select::DifficultyEstimator`]: the production entry point (`select_difficulty_provider`).

pub mod maia;
pub mod select;

use profile_core::types::Difficulty;
use serde::{Deserialize, Serialize};

/// A line is acceptable ("ok") when its mover win% is within this many points of the best line.
pub const OK_WINDOW: f64 = 5.0;

/// Maia-2 is trained only on half-moves with 0-based index `>= MAIA_FIRST_PLY`
/// (`first_n_moves: 10` in maia2's `maia2-training.yaml`; `process_per_game` skips `i < 10`).
pub const MAIA_FIRST_PLY: u32 = 10;

/// Why [`half_move_index`] has no index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlyIndexError {
    /// The FEN does not have six fields, a `w`/`b` side or a numeric move number >= 1.
    Unparsable,
    /// The move number is numeric but the index does not fit in `u32`.
    Overflow,
}

/// 0-based half-move index of the move about to be played in `fen`, from its side to move and
/// fullmove number: `2 * (fullmove - 1) + (black to move)`, with checked arithmetic.
pub fn half_move_index(fen: &str) -> Result<u32, PlyIndexError> {
    let fields: Vec<&str> = fen.split_whitespace().collect();
    let [_, side, _, _, _, fullmove] = fields.as_slice() else {
        return Err(PlyIndexError::Unparsable);
    };
    let black = match *side {
        "w" => 0,
        "b" => 1,
        _ => return Err(PlyIndexError::Unparsable),
    };
    if fullmove.is_empty() || !fullmove.bytes().all(|b| b.is_ascii_digit()) {
        return Err(PlyIndexError::Unparsable);
    }
    // All digits: a parse failure can only be an overflow.
    let fullmove: u32 = fullmove.parse().map_err(|_| PlyIndexError::Overflow)?;
    fullmove
        .checked_sub(1)
        .ok_or(PlyIndexError::Unparsable)?
        .checked_mul(2)
        .and_then(|i| i.checked_add(black))
        .ok_or(PlyIndexError::Overflow)
}

/// True for the opening plies Maia was not trained on. An unparsable or overflowing FEN is not
/// an opening ply (the estimator reports an overflow as `invalid_fen`; the service rejects the rest).
pub fn is_maia_opening_ply(fen: &str) -> bool {
    half_move_index(fen).is_ok_and(|i| i < MAIA_FIRST_PLY)
}

/// Why a difficulty is pending (retry when Maia is healthy).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingReason {
    /// `MAIA_URL` is not set.
    MaiaNotConfigured,
    /// Connection refused, DNS, non-2xx answer (5xx included), or `/healthz` failing.
    MaiaUnavailable,
    /// The request exceeded its timeout.
    Timeout,
    /// The answer, or this item of it, failed validation.
    InvalidResponse,
}

/// Why Maia cannot estimate a position. Final: never re-enriched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotApplicableReason {
    /// Half-move index below [`MAIA_FIRST_PLY`]: Maia was not trained on it.
    OpeningPly,
    /// The service rejected the FEN.
    InvalidFen,
    /// Checkmate or stalemate.
    NoLegalMoves,
    /// The input has no best move to score.
    NoBestMove,
}

/// The difficulty result for one position, as stored by Task 25.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DifficultyOutcome {
    Ok { difficulty: Difficulty },
    PendingMaia { reason: PendingReason },
    NotApplicable { reason: NotApplicableReason },
}

impl DifficultyOutcome {
    /// The estimate, when Maia produced one. Its `model` is always the Maia model id.
    pub fn difficulty(&self) -> Option<&Difficulty> {
        match self {
            Self::Ok { difficulty } => Some(difficulty),
            _ => None,
        }
    }

    /// True when the position should be re-enriched once Maia is healthy.
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::PendingMaia { .. })
    }
}

/// The lines with duplicate moves removed, keeping each move's highest win%.
pub(crate) fn dedupe_lines(lines: &[(String, f64)]) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = Vec::with_capacity(lines.len());
    for (uci, win) in lines {
        match out.iter_mut().find(|(u, _)| u == uci) {
            Some(existing) => existing.1 = existing.1.max(*win),
            None => out.push((uci.clone(), *win)),
        }
    }
    out
}

/// The best line's mover win%, or `None` without lines.
pub(crate) fn best_win(lines: &[(String, f64)]) -> Option<f64> {
    lines.iter().map(|(_, w)| *w).reduce(f64::max)
}
