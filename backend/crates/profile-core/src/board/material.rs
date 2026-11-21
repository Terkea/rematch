//! Material counting and swings (R2 Task 4).

use chess_core::Color;
use cozy_chess::Piece;

use super::line::{LineStep, play_line};
use super::parse_square;
use crate::types::Swing;

/// Piece values in pawns, indexed by `cozy_chess::Piece`: pawn 1, knight 3, bishop 3, rook 5,
/// queen 9, king 0.
pub const PIECE_VALUE: [i32; 6] = [1, 3, 3, 5, 9, 0];

/// `Swing.gain` of a line that mates.
pub const MATE_GAIN: f64 = 100.0;

/// A settled material change of at least this many pawns realises a swing.
const MIN_SWING: i32 = 2;

/// Value of a cozy-chess piece.
pub(crate) fn value_of(p: Piece) -> i32 {
    PIECE_VALUE[p as usize]
}

/// Value of a FEN piece letter (either case); 0 for kings and unknown letters.
pub fn piece_value(c: char) -> i32 {
    match c.to_ascii_lowercase() {
        'p' => value_of(Piece::Pawn),
        'n' => value_of(Piece::Knight),
        'b' => value_of(Piece::Bishop),
        'r' => value_of(Piece::Rook),
        'q' => value_of(Piece::Queen),
        _ => 0,
    }
}

/// The pieces of a FEN's placement field as `(file, rank, letter)` (0-based, `a1 = (0, 0)`).
/// Reads only the first field; malformed parts are skipped, never a panic.
pub(crate) fn placement_pieces(fen: &str) -> Vec<(u8, u8, char)> {
    let field = fen.split_whitespace().next().unwrap_or("");
    let mut out = Vec::new();
    for (row, rank_text) in field.split('/').take(8).enumerate() {
        let rank = 7 - row as u8;
        let mut file: u32 = 0;
        for c in rank_text.chars() {
            if let Some(d) = c.to_digit(10) {
                file += d;
                continue;
            }
            if "pnbrqkPNBRQK".contains(c) && file < 8 {
                out.push((file as u8, rank, c));
            }
            file += 1;
        }
    }
    out
}

fn owner(c: char) -> Color {
    if c.is_ascii_uppercase() {
        Color::White
    } else {
        Color::Black
    }
}

/// Material of `pov` minus material of the opponent, in pawns (`PIECE_VALUE`).
pub fn material_balance(fen: &str, pov: Color) -> i32 {
    placement_pieces(fen)
        .into_iter()
        .map(|(_, _, c)| {
            let v = piece_value(c);
            if owner(c) == pov { v } else { -v }
        })
        .sum()
}

/// Number of pieces on the board, kings included.
pub fn piece_count(fen: &str) -> u32 {
    placement_pieces(fen).len() as u32
}

/// Material signature such as `"KRPvKR"`: each side as K, then Q, R, B, N, P by count; the
/// stronger side first (more material, then the larger piece counts in Q, R, B, N, P order), so
/// the signature does not depend on which colour holds which material.
pub fn material_signature(fen: &str) -> String {
    const ORDER: [char; 5] = ['q', 'r', 'b', 'n', 'p'];
    // (material, counts in ORDER) per colour.
    let mut sides = [(0i32, [0u32; 5]), (0i32, [0u32; 5])];
    for (_, _, c) in placement_pieces(fen) {
        let side = &mut sides[usize::from(owner(c) == Color::Black)];
        if let Some(i) = ORDER.iter().position(|o| *o == c.to_ascii_lowercase()) {
            side.0 += piece_value(c);
            side.1[i] += 1;
        }
    }
    let render = |(_, counts): &(i32, [u32; 5])| {
        let mut s = String::from("K");
        for (letter, n) in ORDER.iter().zip(counts) {
            for _ in 0..*n {
                s.push(letter.to_ascii_uppercase());
            }
        }
        s
    };
    let [white, black] = &sides;
    let (strong, weak) = if white >= black {
        (white, black)
    } else {
        (black, white)
    };
    format!("{}v{}", render(strong), render(weak))
}

/// One `pov` ply of a line with its settled material change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettledPly {
    /// 1-based ply index in the line.
    pub ply: u32,
    /// Balance after the ply and its recapture chain, minus the balance before the line.
    pub settled: i32,
    /// The ply delivers checkmate.
    pub mate: bool,
}

/// The square a step captures on: its destination, or the passed pawn's square for en passant.
fn capture_square(step: &LineStep) -> Option<String> {
    step.captured?;
    let to_occupied = parse_square(&step.to).is_some_and(|to| {
        let (file, rank) = (to.file() as u8, to.rank() as u8);
        placement_pieces(&step.fen_before)
            .iter()
            .any(|(f, r, _)| (*f, *r) == (file, rank))
    });
    if to_occupied {
        return Some(step.to.clone());
    }
    let file = step.to.get(0..1)?;
    let rank = step.from.get(1..2)?;
    Some(format!("{file}{rank}"))
}

/// Index of the last step of the recapture chain that starts at the `pov` step `i`.
///
/// The chain follows PV moves that capture on the previous move's destination square, at most
/// `settle_extra` of them. A `pov` mate ends it (mate is credited as mate, not as material); an
/// opponent's mating recapture stays in. When the cap lands right after a `pov` capture, the
/// opponent's recapture on that square is still taken, so the chain never stops one ply short of
/// the answer to our capture.
fn chain_end(steps: &[LineStep], i: usize, pov: Color, settle_extra: usize) -> usize {
    let recaptured = |end: usize| match (steps.get(end), steps.get(end + 1)) {
        (Some(prev), Some(next)) => {
            !(next.is_mate && next.mover == pov)
                && capture_square(next).as_deref() == Some(prev.to.as_str())
        }
        _ => false,
    };
    let mut end = i;
    while end - i < settle_extra && recaptured(end) {
        end += 1;
    }
    let capped_after_pov_capture = end - i == settle_extra
        && steps
            .get(end)
            .is_some_and(|s| s.mover == pov && s.captured.is_some());
    if capped_after_pov_capture && recaptured(end) {
        end += 1;
    }
    end
}

/// `settled(k)` for every `pov` ply k ≤ `max_plies` of already played steps.
fn settled_of_steps(
    steps: &[LineStep],
    b0: i32,
    pov: Color,
    max_plies: usize,
    settle_extra: usize,
) -> Vec<SettledPly> {
    let mut out = Vec::new();
    for (i, step) in steps.iter().enumerate().take(max_plies) {
        if step.mover != pov {
            continue;
        }
        let after = steps
            .get(chain_end(steps, i, pov, settle_extra))
            .map_or(b0, |s| material_balance(&s.fen_after, pov));
        out.push(SettledPly {
            ply: u32::try_from(i + 1).unwrap_or(u32::MAX),
            settled: after - b0,
            mate: step.is_mate,
        });
    }
    out
}

/// `settled(k)` for every `pov` ply k ≤ `max_plies` of the line (TS M2 Task 3): the balance after
/// ply k, extended through its recapture chain (see `chain_end`; the chain may run past
/// `max_plies`), minus the balance before the line.
pub fn settled_plies(
    fen: &str,
    pv: &[String],
    pov: Color,
    max_plies: usize,
    settle_extra: usize,
) -> Vec<SettledPly> {
    // The chain needs at most `settle_extra` plies plus the recapture after the cap.
    let limit = max_plies.saturating_add(settle_extra).saturating_add(1);
    let steps = play_line(fen, pv, Some(limit));
    settled_of_steps(
        &steps,
        material_balance(fen, pov),
        pov,
        max_plies,
        settle_extra,
    )
}

/// Material swing that a PV realises for `pov` (TS M2 Task 3; defaults `max_plies` 8,
/// `settle_extra` 4; controller ruling on mates after the R2 T4 review).
///
/// A `pov` mate is detected on the whole PV, whatever `max_plies` is: then `gain = MATE_GAIN` and
/// `mate_in = mating ply.div_ceil(2)`. `depth` is the first `pov` ply k ≤ `max_plies` with
/// `settled(k) ≥ 2`; without one it falls back to the mating ply, so a deep mate has a deep depth.
/// Otherwise `gain` is the largest `settled(k)` (0 without `pov` plies).
pub fn settled_swing(
    fen: &str,
    pv: &[String],
    pov: Color,
    max_plies: usize,
    settle_extra: usize,
) -> Swing {
    let steps = play_line(fen, pv, None);
    let plies = settled_of_steps(
        &steps,
        material_balance(fen, pov),
        pov,
        max_plies,
        settle_extra,
    );
    // A mate ends the line, so only the last step can be one.
    let mating_ply = steps
        .last()
        .filter(|s| s.is_mate && s.mover == pov)
        .map(|_| u32::try_from(steps.len()).unwrap_or(u32::MAX));
    let depth = plies
        .iter()
        .find(|p| p.settled >= MIN_SWING)
        .map(|p| p.ply)
        .or(mating_ply);
    let gain = match mating_ply {
        Some(_) => MATE_GAIN,
        None => plies.iter().map(|p| p.settled).max().map_or(0.0, f64::from),
    };
    Swing {
        gain,
        depth,
        mate_in: mating_ply.map(|p| p.div_ceil(2)),
    }
}
