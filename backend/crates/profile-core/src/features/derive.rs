//! Deriving MoveFeatures from RawMoveFeatures (R2 Task 6, TS M2 Task 5).

use chess_core::Color;
use chess_core::board::Position;
use chess_core::score::win_percent;

use crate::board::play_line;
use crate::motifs::tag_line;
use crate::types::{Difficulty, LineAnalysis, MoveFeatures, MultipvLine, RawMoveFeatures};

/// A move is critical when the best MultiPV line beats the second by at least this many W%.
pub const CRITICAL_GAP: f64 = 15.0;

/// A MultiPV line is playable when its W% is within this many points of the best line.
pub const PLAYABLE_WINDOW: f64 = 5.0;

/// A threat exists when its line gains at least this many pawns (a mate counts) ...
pub const THREAT_MIN_GAIN: f64 = 2.0;

/// ... within this many plies.
pub const THREAT_MAX_DEPTH: u32 = 4;

/// Swing horizon used when tagging best, refutation and threat lines.
pub const LINE_MAX_PLIES: usize = 8;

/// First ply that may carry a difficulty: Maia is out of distribution for plies `≤ 10`.
pub const DIFFICULTY_MIN_PLY: u32 = 11;

/// A stored difficulty that is not a finite Maia probability pair.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[error("invalid difficulty: {0}")]
pub struct InvalidDifficulty(pub String);

fn opponent(c: Color) -> Color {
    match c {
        Color::White => Color::Black,
        Color::Black => Color::White,
    }
}

/// The analysed line, or `None` when there is no line to analyse: an empty PV, an invalid FEN,
/// or a first move that is illegal there.
fn analyse(fen: &str, pv: &[String], pov: Color) -> Option<LineAnalysis> {
    if pv.is_empty() || Position::from_fen(fen).is_err() || play_line(fen, pv, Some(1)).is_empty() {
        return None;
    }
    Some(tag_line(fen, pv, pov, LINE_MAX_PLIES))
}

/// The line realises a material gain of at least 2 (or a mate) within 4 plies.
fn is_threat(a: &LineAnalysis) -> bool {
    a.swing.gain >= THREAT_MIN_GAIN && a.swing.depth.is_some_and(|d| d <= THREAT_MAX_DEPTH)
}

/// The square of the first capture by `pov` in `pv` from `fen`, within the line's swing depth
/// (capped at [`THREAT_MAX_DEPTH`] plies; with no swing, the first 4 plies). `None` when that
/// prefix has no capture by `pov`, or the line cannot be replayed.
pub fn first_capture_square(fen: &str, pv: &[String], pov: Color) -> Option<String> {
    let depth = tag_line(fen, pv, pov, LINE_MAX_PLIES)
        .swing
        .depth
        .unwrap_or(THREAT_MAX_DEPTH)
        .min(THREAT_MAX_DEPTH);
    let plies = usize::try_from(depth).unwrap_or(0);
    play_line(fen, pv, Some(plies))
        .into_iter()
        .find(|s| s.mover == pov && s.captured.is_some())
        .map(|s| s.to)
}

/// MultiPV features; `None` without lines. Lines are ranked by mover W% (stable, so the engine
/// order is kept when it is already sorted, as Stockfish's is).
fn multipv_features(lines: &[MultipvLine], uci: &str, f: &mut MoveFeatures) {
    if lines.is_empty() {
        return;
    }
    let mut ranked: Vec<(f64, &MultipvLine)> =
        lines.iter().map(|l| (win_percent(l.score), l)).collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    let best = ranked.first().map_or(0.0, |(w, _)| *w);
    if let Some((second, _)) = ranked.get(1) {
        let gap = best - second;
        f.multipv_gap = Some(gap);
        f.is_critical = Some(gap >= CRITICAL_GAP);
    }
    let playable = ranked
        .iter()
        .filter(|(w, _)| *w >= best - PLAYABLE_WINDOW)
        .count();
    f.playable = u32::try_from(playable).ok();
    f.in_top2 = Some(ranked.iter().take(2).any(|(_, l)| l.uci == uci));
}

/// Features of one move (TS M2 Task 5): `best` for the mover from `fen_before`, `refutation`
/// for the opponent from `fen_after`, `threat` for the opponent from the null-move FEN.
///
/// `threat_existed` = threat gain ≥ 2 within 4 plies (`None` without a valid threat probe).
/// `threat_addressed` = not (the threat existed and the refutation also wins ≥ 2 within 4 plies
/// and either starts with the threat's first move or makes its first capture on the threat's
/// first capture square; two capture-free lines, i.e. mates, count as the same square, as in
/// TS's `null === null`). It is `Some(true)` when no threat existed, and `None` when a threat
/// existed but there is no refutation line to judge it by.
pub fn derive_move_features(
    color: Color,
    fen_before: &str,
    fen_after: &str,
    uci: &str,
    raw: Option<&RawMoveFeatures>,
) -> MoveFeatures {
    let Some(raw) = raw else {
        return MoveFeatures::default();
    };
    let them = opponent(color);
    let mut f = MoveFeatures::default();
    if let Some(lines) = &raw.multipv {
        multipv_features(lines, uci, &mut f);
    }
    f.best = analyse(fen_before, &raw.best_pv, color);
    f.refutation = analyse(fen_after, &raw.refutation_pv, them);
    let probe = raw.threat.as_ref();
    f.threat = probe.and_then(|t| analyse(&t.null_fen, &t.pv, them));

    if let (Some(threat), Some(probe)) = (&f.threat, probe) {
        let existed = is_threat(threat);
        f.threat_existed = Some(existed);
        f.threat_addressed = if !existed {
            Some(true)
        } else {
            f.refutation.as_ref().map(|refutation| {
                let same_start = raw.refutation_pv.first() == probe.pv.first();
                let same_square = || {
                    first_capture_square(fen_after, &raw.refutation_pv, them)
                        == first_capture_square(&probe.null_fen, &probe.pv, them)
                };
                !(is_threat(refutation) && (same_start || same_square()))
            })
        };
    }
    f
}

/// The difficulty to use for `ply`: Maia only (owner override), `None` when Maia was down
/// (`d` is `None`) or at plies `≤ 10`. A stored value from another model (the removed
/// `spread-v1`) or with a probability outside `[0, 1]` (or non-finite) is an error, so the
/// caller can skip and count it.
pub fn maia_difficulty(
    ply: u32,
    d: Option<&Difficulty>,
) -> Result<Option<Difficulty>, InvalidDifficulty> {
    let Some(d) = d else {
        return Ok(None);
    };
    if !d.model.starts_with("maia") {
        return Err(InvalidDifficulty(format!(
            "model {:?} is not Maia",
            d.model
        )));
    }
    let unit = |p: f64| p.is_finite() && (0.0..=1.0).contains(&p);
    if !unit(d.p_best) || !unit(d.p_ok) {
        return Err(InvalidDifficulty(format!(
            "probabilities ({}, {}) outside [0, 1]",
            d.p_best, d.p_ok
        )));
    }
    Ok((ply >= DIFFICULTY_MIN_PLY).then(|| d.clone()))
}
