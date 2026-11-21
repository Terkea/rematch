//! Assembling EnrichedMove (R2 Task 6, TS M2 Task 5).

use chess_core::board::Position;
use chess_core::score::flip_score;
use chess_core::{Color, EngineScore, MoveRecord, PositionEval, Terminal};

use super::derive::{derive_move_features, maia_difficulty, threat_probe_matches};
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
    /// Stored difficulties dropped because they are not a valid Maia result
    /// ([`maia_difficulty`]).
    pub difficulty_rejected: u32,
    /// Moves dropped because an eval they use is out of range (`|cp| > 100 000` or
    /// `|mate| > 1000`); checked before any score is flipped.
    pub evals_out_of_range: u32,
    /// Moves dropped because their `fen_before` or `fen_after` does not parse.
    pub moves_with_invalid_fen: u32,
    /// Threat probes dropped because their null FEN is not `null_move_fen(fen_before)`.
    pub threat_fen_mismatch: u32,
    /// `Some` raw entries at indexes `≥ records.len()` (they belong to no record).
    pub raw_beyond_records: u32,
}

/// Largest centipawn magnitude accepted from an eval.
pub const MAX_EVAL_CP: i32 = 100_000;

/// Largest mate distance accepted from an eval.
pub const MAX_EVAL_MATE: i32 = 1000;

/// The eval's score is in the accepted range (so negating it cannot overflow).
pub fn eval_in_range(s: EngineScore) -> bool {
    match s {
        EngineScore::Cp(cp) => (-MAX_EVAL_CP..=MAX_EVAL_CP).contains(&cp),
        EngineScore::Mate(m) => (-MAX_EVAL_MATE..=MAX_EVAL_MATE).contains(&m),
    }
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
/// Skipped and counted ([`SkipCounts`]): moves without both evals or with an out-of-range eval,
/// moves whose FENs do not parse, a final move that cannot be replayed, raw entries for another
/// ply or beyond the records, threat probes for another position, and difficulties that are not
/// valid Maia values. A valid Maia difficulty at plies `≤ 10` is dropped by rule and not counted.
pub fn assemble_enriched_moves_counted(
    records: &[MoveRecord],
    evals: &[PositionEval],
    raw: &[Option<RawMoveFeatures>],
    terminal: Option<Terminal>,
) -> Assembly {
    let mut skipped = SkipCounts::default();
    let mut moves = Vec::with_capacity(records.len());
    let last = records.len().saturating_sub(1);
    let beyond = raw.get(records.len()..).unwrap_or_default();
    skipped.raw_beyond_records =
        u32::try_from(beyond.iter().filter(|r| r.is_some()).count()).unwrap_or(u32::MAX);
    for (i, rec) in records.iter().enumerate() {
        let (Some(before), Some(after)) = (evals.get(i), evals.get(i + 1)) else {
            skipped.moves_without_evals += 1;
            continue;
        };
        let mates_now = terminal == Some(Terminal::Checkmate) && i == last;
        if !eval_in_range(before.score) || (!mates_now && !eval_in_range(after.score)) {
            skipped.evals_out_of_range += 1;
            continue;
        }
        if Position::from_fen(&rec.fen_before).is_err()
            || records
                .get(i + 1)
                .is_some_and(|next| Position::from_fen(&next.fen_before).is_err())
        {
            skipped.moves_with_invalid_fen += 1;
            continue;
        }
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
        let score_after = if mates_now {
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
            if r.threat
                .as_ref()
                .is_some_and(|t| !threat_probe_matches(&rec.fen_before, t))
            {
                skipped.threat_fen_mismatch += 1;
                r.threat = None;
            }
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
