//! Per-move records from a parsed game and its position evaluations (port of `move-record.ts`).

use crate::judgement::judge;
use crate::pgn::{ParsedGame, Terminal};
use crate::phase::{OPENING_MIN_PLY, detect_phases};
use crate::score::{flip_score, move_accuracy, win_percent};
use crate::{ClockInfo, Color, EngineScore, MoveRecord, PositionEval};

/// The game was already decided when `|win_before - 50| >= DECIDED_THRESHOLD`.
pub const DECIDED_THRESHOLD: f64 = 40.0;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MoveRecordError {
    #[error("Expected {expected} evals, got {got}")]
    WrongEvalCount { expected: usize, got: usize },
}

/// `evals[i]` is the evaluation (white's point of view) of the position before ply
/// `i + 1`, so there are `plies.len() + 1` entries; the last is the final position.
pub fn build_move_records(
    game: &ParsedGame,
    evals: &[PositionEval],
    clock: Option<ClockInfo>,
) -> Result<Vec<MoveRecord>, MoveRecordError> {
    build_move_records_with_book_exit(game, evals, clock, OPENING_MIN_PLY)
}

/// As [`build_move_records`], with the ply at which the opening book was left.
pub fn build_move_records_with_book_exit(
    game: &ParsedGame,
    evals: &[PositionEval],
    clock: Option<ClockInfo>,
    book_exit_ply: u32,
) -> Result<Vec<MoveRecord>, MoveRecordError> {
    let plies = &game.plies;
    if evals.len() != plies.len() + 1 {
        return Err(MoveRecordError::WrongEvalCount {
            expected: plies.len() + 1,
            got: evals.len(),
        });
    }
    let fens: Vec<&str> = plies.iter().map(|p| p.fen_before.as_str()).collect();
    let phases = detect_phases(&fens, book_exit_ply);

    // Without any clock comment in the PGN there is no clock to report, even when
    // a time control is known.
    let pgn_has_clocks = plies.iter().any(|p| p.clock_seconds.is_some());
    let base = clock.filter(|_| pgn_has_clocks).map(|c| c.base_seconds);
    let mut previous = [base, base]; // [white, black]

    let last = plies.len().saturating_sub(1);
    Ok(plies
        .iter()
        .enumerate()
        .map(|(i, ply)| {
            let eval_before = &evals[i];
            let eval_after = &evals[i + 1];
            let pov = |s: EngineScore| {
                if ply.color == Color::White {
                    s
                } else {
                    flip_score(s)
                }
            };
            let before = pov(eval_before.score);
            // A delivered checkmate has no meaningful engine score: the mover has won.
            let delivers_mate = game.terminal == Some(Terminal::Checkmate) && i == last;
            let after = if delivers_mate {
                EngineScore::Mate(1)
            } else {
                pov(eval_after.score)
            };

            let win_before = win_percent(before);
            let win_after = win_percent(after);

            let slot = usize::from(ply.color == Color::Black);
            let clock_before = previous[slot];
            let time_spent = match (clock, clock_before, ply.clock_seconds) {
                (Some(c), Some(cb), Some(cs)) => Some((cb - cs + c.increment_seconds).max(0.0)),
                _ => None,
            };
            previous[slot] = if clock.is_some() {
                ply.clock_seconds
            } else {
                None
            };

            MoveRecord {
                ply: ply.ply,
                color: ply.color,
                san: ply.san.clone(),
                uci: ply.uci.clone(),
                fen_before: ply.fen_before.clone(),
                best_uci: eval_before.best_uci.clone(),
                win_before,
                win_after,
                delta_w: (win_before - win_after).max(0.0),
                accuracy: move_accuracy(win_before, win_after),
                judgement: judge(before, after),
                phase: phases[i],
                clock_before,
                time_spent,
                decided: (win_before - 50.0).abs() >= DECIDED_THRESHOLD,
            }
        })
        .collect())
}
