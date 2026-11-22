//! Strength exemplar extraction (R2 Task 10, rules TS M2 Task 9).
//!
//! A strength ply is evidence that the player did something well. Only the player's valid,
//! eligible moves (`|W_before − 50| < 40`, see [`GameView`]) can be one: a decided position shows
//! nothing, and an invalid or duplicated ply is unknown, never evidence.
//! - `only_move_found`: `is_critical == Some(true)` and the played move is MultiPV line 1.
//! - `tactic_found`: the best line is a tactic (gain ≥ 2 or a mate) and the played move is
//!   `raw.best_pv[0]`.
//! - `conversion`: a winning episode that was won; `defence`: a losing episode with score ≥ 0.5.
//!   The ply is the first eligible own critical ply inside the episode (`start_ply..=end_ply`),
//!   else `start_ply` when that is eligible, else none. No episode without a known result.
//! - At most [`MAX_PER_KIND`] per kind, by `multipv_gap` descending (unknown or invalid gaps
//!   last), then ply ascending; output grouped by kind in [`StrengthKind`] order.

use crate::types::{AnalysedGame, LineAnalysis, StrengthKind, StrengthPly};
use crate::weakness::detectors::{self as d, GameView, OwnMove, TACTIC_MIN_GAIN};
use crate::weakness::episodes::{Episode, Outcome};
use crate::weakness::observe_game::T_WIN;

/// At most this many plies per kind per game.
pub const MAX_PER_KIND: usize = 3;

/// [`extract_strength_plies`] plus what was left out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StrengthScan {
    pub plies: Vec<StrengthPly>,
    /// The player's moves that failed validation (never a strength).
    pub skipped_own_plies: u32,
    /// Moves (either colour) dropped because their ply appears more than once.
    pub duplicate_plies: u32,
    /// Candidate moves whose `multipv_gap` is non-finite or negative; they sort as unknown.
    pub invalid_gaps: u32,
}

/// The player's strength exemplar plies in `game`.
pub fn extract_strength_plies(game: &AnalysedGame) -> Vec<StrengthPly> {
    extract_strength_plies_counted(game).plies
}

/// [`extract_strength_plies`], also reporting skipped inputs.
pub fn extract_strength_plies_counted(game: &AnalysedGame) -> StrengthScan {
    let v = GameView::new(game);
    let eligible: Vec<&OwnMove> = v.own.iter().filter(|o| o.eligible).collect();
    let mut invalid_gaps = 0;

    let mut plies = Vec::new();
    let only: Vec<&OwnMove> = eligible
        .iter()
        .copied()
        .filter(|o| only_move_found(o))
        .collect();
    let tactics: Vec<&OwnMove> = eligible
        .iter()
        .copied()
        .filter(|o| tactic_found(o))
        .collect();
    for (kind, moves) in [
        (StrengthKind::OnlyMoveFound, only),
        (StrengthKind::TacticFound, tactics),
    ] {
        plies.extend(
            top(moves, &mut invalid_gaps)
                .into_iter()
                .map(|ply| StrengthPly { kind, ply }),
        );
    }

    let (winning, losing) = d::episodes(&v, Some(T_WIN));
    let converted = winning.filter(|e| e.outcome == Outcome::Won);
    let defended = losing.filter(|e| e.outcome != Outcome::Lost);
    for (kind, episode) in [
        (StrengthKind::Conversion, converted),
        (StrengthKind::Defence, defended),
    ] {
        if let Some(ply) = episode.and_then(|e| episode_ply(&eligible, &e)) {
            plies.push(StrengthPly { kind, ply });
        }
    }

    StrengthScan {
        plies,
        skipped_own_plies: v.skipped_own_plies.len() as u32,
        duplicate_plies: v.counts.duplicate_plies,
        invalid_gaps,
    }
}

fn only_move_found(o: &OwnMove) -> bool {
    o.m.f.is_critical == Some(true)
        && o.m
            .raw
            .as_ref()
            .and_then(|r| r.multipv.as_ref())
            .and_then(|lines| lines.first())
            .is_some_and(|l| l.uci == o.m.rec.uci)
}

fn is_tactic(l: &LineAnalysis) -> bool {
    l.swing.gain >= TACTIC_MIN_GAIN || l.swing.mate_in.is_some()
}

fn tactic_found(o: &OwnMove) -> bool {
    o.m.f.best.as_ref().is_some_and(is_tactic)
        && o.m
            .raw
            .as_ref()
            .and_then(|r| r.best_pv.first())
            .is_some_and(|u| *u == o.m.rec.uci)
}

/// The first eligible own critical ply inside the episode, else its start when eligible.
fn episode_ply(eligible: &[&OwnMove], e: &Episode) -> Option<u32> {
    eligible
        .iter()
        .find(|o| (e.start_ply..=e.end_ply).contains(&o.ply()) && o.m.f.is_critical == Some(true))
        .or_else(|| eligible.iter().find(|o| o.ply() == e.start_ply))
        .map(|o| o.ply())
}

/// The plies of the best [`MAX_PER_KIND`] moves: gap descending (unknown last), then ply.
fn top(moves: Vec<&OwnMove>, invalid_gaps: &mut u32) -> Vec<u32> {
    let mut keyed: Vec<(Option<f64>, u32)> = moves
        .into_iter()
        .map(|o| {
            let gap = match o.m.f.multipv_gap {
                Some(g) if g.is_finite() && g >= 0.0 => Some(g),
                Some(_) => {
                    *invalid_gaps += 1;
                    None
                }
                None => None,
            };
            (gap, o.ply())
        })
        .collect();
    keyed.sort_by(|(ga, pa), (gb, pb)| match (ga, gb) {
        (Some(a), Some(b)) => b.total_cmp(a).then(pa.cmp(pb)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => pa.cmp(pb),
    });
    keyed
        .into_iter()
        .take(MAX_PER_KIND)
        .map(|(_, p)| p)
        .collect()
}
