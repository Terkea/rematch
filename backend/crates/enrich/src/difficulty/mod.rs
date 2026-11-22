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

use futures::future::BoxFuture;
use profile_core::types::Difficulty;
use serde::{Deserialize, Serialize};

use crate::types::DifficultyInput;

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
    /// Maia kept giving an invalid answer for this position while healthy: after
    /// `reenrich::MAX_DIFFICULTY_ATTEMPTS` attempts it is final (Task 25).
    ModelError,
}

impl PendingReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MaiaNotConfigured => "maia_not_configured",
            Self::MaiaUnavailable => "maia_unavailable",
            Self::Timeout => "timeout",
            Self::InvalidResponse => "invalid_response",
        }
    }
}

impl NotApplicableReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpeningPly => "opening_ply",
            Self::InvalidFen => "invalid_fen",
            Self::NoLegalMoves => "no_legal_moves",
            Self::NoBestMove => "no_best_move",
            Self::ModelError => "model_error",
        }
    }
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

    /// `move_features.difficulty_status`: `ok`, `pending_maia` or `not_applicable`.
    pub fn status(&self) -> &'static str {
        match self {
            Self::Ok { .. } => "ok",
            Self::PendingMaia { .. } => "pending_maia",
            Self::NotApplicable { .. } => "not_applicable",
        }
    }

    /// `move_features.difficulty_reason`: the pending or not-applicable reason; `None` when ok.
    pub fn reason(&self) -> Option<&'static str> {
        match self {
            Self::Ok { .. } => None,
            Self::PendingMaia { reason } => Some(reason.as_str()),
            Self::NotApplicable { reason } => Some(reason.as_str()),
        }
    }
}

/// Where enrichment gets difficulty from (Task 25). The only implementation is
/// [`select::DifficultyEstimator`], so difficulty is Maia's or absent: never estimated otherwise.
pub trait DifficultySource: Send + Sync {
    /// One outcome per input, in order. Never fails: when Maia cannot answer, the outcome is
    /// `pending_maia`.
    fn estimate<'a>(
        &'a self,
        inputs: &'a [DifficultyInput],
    ) -> BoxFuture<'a, Vec<DifficultyOutcome>>;

    /// A fresh `GET /healthz` (with an exact Maia model id): true when Maia can answer now.
    fn healthy(&self) -> BoxFuture<'_, bool>;
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
