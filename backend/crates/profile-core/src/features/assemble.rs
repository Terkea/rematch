//! Assembling EnrichedMove (R2 Task 6, TS M2 Task 5).

use chess_core::board::Position;
use chess_core::score::flip_score;
use chess_core::{Color, EngineScore, MoveRecord, PositionEval, Terminal};

use super::derive::{derive_move_features, maia_difficulty};
use crate::types::{EnrichedMove, RawMoveFeatures};

/// Inputs that were skipped during assembly, by reason.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkipCounts {
    /// Moves dropped because `evals` has no entry before or after them.
    pub moves_without_evals: u32,
    /// Final moves dropped because their move cannot be replayed to get `fen_after`.
    pub fen_after_unplayable: u32,
    /// `raw[i]` entries dropped because their `ply` is not `records[i].ply`.
    pub raw_ply_mismatch: u32,
    /// Stored difficulties dropped because they are not a finite Maia probability pair.
    pub difficulty_rejected: u32,
}

/// Assembled moves plus what was skipped.
#[derive(Clone, Debug, PartialEq)]
pub struct Assembly {
    pub moves: Vec<EnrichedMove>,
    pub skipped: SkipCounts,
}

/// [`assemble_enriched_moves_counted`] without the counts.
pub fn assemble_enriched_moves(
    records: &[MoveRecord],
    evals: &[PositionEval],
    raw: &[Option<RawMoveFeatures>],
    terminal: Option<Terminal>,
) -> Vec<EnrichedMove> {
    assemble_enriched_moves_counted(records, evals, raw, terminal).moves
}

fn pov(color: Color, s: EngineScore) -> EngineScore {
    match color {
        Color::White => s,
        Color::Black => flip_score(s),
    }
}

/// One [`EnrichedMove`] per record. `evals[i]` (White POV) is the position before
/// `records[i]`, as in R1's `build_move_records`; `raw[i]` belongs to `records[i]` (missing
/// entries are `None`). Scores are converted to the mover's POV, and a final ply that delivers
/// checkmate gets `score_after = Mate(1)`. `fen_after` is the next record's `fen_before`, or the
/// final move replayed on its `fen_before`.
///
/// Skipped and counted ([`SkipCounts`]): moves without both evals, a final move that cannot be
/// replayed, raw entries for another ply, and difficulties that are not valid Maia values. A
/// valid Maia difficulty at plies `≤ 10` is dropped by rule and not counted.
pub fn assemble_enriched_moves_counted(
    records: &[MoveRecord],
    evals: &[PositionEval],
    raw: &[Option<RawMoveFeatures>],
    terminal: Option<Terminal>,
) -> Assembly {
    let mut skipped = SkipCounts::default();
    let mut moves = Vec::with_capacity(records.len());
    let last = records.len().saturating_sub(1);
    for (i, rec) in records.iter().enumerate() {
        let (Some(before), Some(after)) = (evals.get(i), evals.get(i + 1)) else {
            skipped.moves_without_evals += 1;
            continue;
        };
        let fen_after = match records.get(i + 1) {
            Some(next) => next.fen_before.clone(),
            None => match Position::from_fen(&rec.fen_before)
                .ok()
                .and_then(|p| p.play_uci(&rec.uci).ok())
            {
                Some(p) => p.fen(),
                None => {
                    skipped.fen_after_unplayable += 1;
                    continue;
                }
            },
        };
        let score_before = pov(rec.color, before.score);
        let score_after = if terminal == Some(Terminal::Checkmate) && i == last {
            EngineScore::Mate(1)
        } else {
            pov(rec.color, after.score)
        };

        let mut raw_i = raw.get(i).cloned().flatten();
        if raw_i.as_ref().is_some_and(|r| r.ply != rec.ply) {
            skipped.raw_ply_mismatch += 1;
            raw_i = None;
        }
        if let Some(r) = raw_i.as_mut() {
            r.difficulty = match maia_difficulty(rec.ply, r.difficulty.as_ref()) {
                Ok(d) => d,
                Err(_) => {
                    skipped.difficulty_rejected += 1;
                    None
                }
            };
        }

        let f = derive_move_features(
            rec.color,
            &rec.fen_before,
            &fen_after,
            &rec.uci,
            raw_i.as_ref(),
        );
        moves.push(EnrichedMove {
            rec: rec.clone(),
            fen_after,
            score_before,
            score_after,
            raw: raw_i,
            f,
        });
    }
    Assembly { moves, skipped }
}
