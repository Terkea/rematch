//! Game phase detection (port of `phase.ts`).

use crate::Phase;

/// The opening lasts at least this many plies.
pub const OPENING_MIN_PLY: u32 = 20;
/// The opening never lasts longer than this many plies.
pub const OPENING_MAX_PLY: u32 = 30;

#[derive(Default)]
struct Material {
    white: u32,
    black: u32,
    queens: u32,
}

fn non_pawn_material(fen: &str) -> Material {
    let board = fen.split(' ').next().unwrap_or("");
    let mut m = Material::default();
    for ch in board.chars() {
        let value = match ch.to_ascii_lowercase() {
            'q' => 9,
            'r' => 5,
            'b' | 'n' => 3,
            _ => continue,
        };
        if ch.is_ascii_uppercase() {
            m.white += value;
        } else {
            m.black += value;
        }
        if ch.eq_ignore_ascii_case(&'q') {
            m.queens += 1;
        }
    }
    m
}

fn is_endgame(fen: &str) -> bool {
    let m = non_pawn_material(fen);
    if m.white <= 13 && m.black <= 13 {
        return true;
    }
    m.queens == 0 && m.white + m.black <= 26
}

/// Phase of each position. `fens_before[i]` is the position before ply `i + 1`.
/// The opening lasts until ply `max(20, book_exit_ply)`, capped at 30. Afterwards
/// the phase is endgame by material, else middlegame, and never goes backwards.
pub fn detect_phases(fens_before: &[&str], book_exit_ply: u32) -> Vec<Phase> {
    let opening_end = book_exit_ply.clamp(OPENING_MIN_PLY, OPENING_MAX_PLY);
    let mut reached_endgame = false;
    fens_before
        .iter()
        .enumerate()
        .map(|(i, fen)| {
            if (i as u32) < opening_end {
                return Phase::Opening;
            }
            if !reached_endgame && is_endgame(fen) {
                reached_endgame = true;
            }
            if reached_endgame {
                Phase::Endgame
            } else {
                Phase::Middlegame
            }
        })
        .collect()
}
