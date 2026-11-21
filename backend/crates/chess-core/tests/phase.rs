//! Port of `packages/chess-core/test/phase.test.ts`.

use chess_core::Phase;
use chess_core::phase::{OPENING_MAX_PLY, OPENING_MIN_PLY, detect_phases};

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const ROOK_ENDING: &str = "8/5k2/8/8/3R4/8/2r5/4K3 w - - 0 1";
const QR_EACH: &str = "3qr1k1/pppp1ppp/8/8/8/8/PPPP1PPP/3QR1K1 w - - 0 1";
// Queenless, 16 per side (R+R+B+N): 32 total, over both limits.
const QUEENLESS_32: &str = "1rbn1rk1/pppppppp/8/8/8/8/PPPPPPPP/1RBN1RK1 w - - 0 1";
// Queenless, white 16 and black 10 (R+R): 26 total, only the total rule applies.
const QUEENLESS_26: &str = "1r3rk1/pppppppp/8/8/8/8/PPPPPPPP/1RBN1RK1 w - - 0 1";

fn repeat(fen: &'static str, n: usize) -> Vec<&'static str> {
    vec![fen; n]
}

#[test]
fn constants_match_ts() {
    assert_eq!((OPENING_MIN_PLY, OPENING_MAX_PLY), (20, 30));
}

#[test]
fn is_opening_through_ply_20_and_middlegame_from_21_by_default() {
    let p = detect_phases(&repeat(START, 25), OPENING_MIN_PLY);
    assert!(p[..20].iter().all(|x| *x == Phase::Opening));
    assert!(p[20..].iter().all(|x| *x == Phase::Middlegame));
}

#[test]
fn detects_an_endgame_once_the_opening_is_over() {
    let p = detect_phases(&repeat(ROOK_ENDING, 22), 20);
    assert_eq!(p[19], Phase::Opening);
    assert_eq!(p[20], Phase::Endgame);
    assert_eq!(p[21], Phase::Endgame);
}

#[test]
fn treats_q_r_each_28_total_14_per_side_as_middlegame() {
    assert_eq!(
        detect_phases(&repeat(QR_EACH, 22), 20)[21],
        Phase::Middlegame
    );
}

#[test]
fn applies_the_queenless_total_material_rule() {
    assert_eq!(
        detect_phases(&repeat(QUEENLESS_32, 22), 20)[21],
        Phase::Middlegame
    );
    assert_eq!(
        detect_phases(&repeat(QUEENLESS_26, 22), 20)[21],
        Phase::Endgame
    );
}

#[test]
fn never_goes_backwards_from_endgame() {
    let mut fens = repeat(ROOK_ENDING, 21);
    fens.extend([QR_EACH, START]);
    let p = detect_phases(&fens, 20);
    assert_eq!(&p[20..], &[Phase::Endgame; 3]);
}

#[test]
fn extends_the_opening_to_book_exit_ply() {
    let p = detect_phases(&repeat(START, 30), 26);
    assert_eq!(p[25], Phase::Opening);
    assert_eq!(p[26], Phase::Middlegame);
}

#[test]
fn caps_the_opening_at_ply_30() {
    let p = detect_phases(&repeat(START, 33), 40);
    assert_eq!(p[29], Phase::Opening);
    assert_eq!(p[30], Phase::Middlegame);
}
