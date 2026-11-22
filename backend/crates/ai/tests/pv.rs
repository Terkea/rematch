//! Port of the `pvToSan` block of `.worktrees/m3/packages/ai/test/validator/extract.test.ts`, plus
//! `fen_after`, `strip_san` and chess.js's `move({from, to, promotion})` edge cases.

use ai::validator::pv::{fen_after, pv_to_san, strip_san};

const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| (*x).to_owned()).collect()
}

mod pv_to_san {
    use super::*;

    #[test]
    fn converts_and_stops_at_the_first_illegal_move() {
        assert_eq!(
            pv_to_san(START_FEN, &s(&["e2e4", "e7e5", "e1e8"])),
            s(&["e4", "e5"])
        );
    }

    #[test]
    fn never_throws() {
        assert_eq!(pv_to_san("garbage", &s(&["e2e4"])), Vec::<String>::new());
        assert_eq!(
            pv_to_san(START_FEN, &s(&["zz", "", "e2e4"])),
            Vec::<String>::new()
        );
        // Non-ASCII and over-long input must not panic on a char boundary.
        assert_eq!(
            pv_to_san(START_FEN, &s(&["e2é4", "e2e4e2e4", "\u{1F600}"])),
            Vec::<String>::new()
        );
    }

    #[test]
    fn handles_promotion() {
        assert_eq!(
            pv_to_san("8/P6k/8/8/8/8/8/K7 w - - 0 1", &s(&["a7a8q"])),
            s(&["a8=Q"])
        );
    }
}

mod chess_js_move_object {
    use super::*;

    /// chess.js `move({from, to, promotion})` ignores a promotion letter on a non-promotion move.
    #[test]
    fn a_promotion_suffix_on_a_plain_move_is_ignored() {
        assert_eq!(pv_to_san(START_FEN, &s(&["e2e4q"])), s(&["e4"]));
    }

    /// chess.js rejects king-takes-rook castling (`e1h1`); only the king's step is a move.
    #[test]
    fn king_takes_rook_castling_is_illegal() {
        let fen = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";
        assert_eq!(pv_to_san(fen, &s(&["e1h1"])), Vec::<String>::new());
        assert_eq!(pv_to_san(fen, &s(&["e1g1", "e8c8"])), s(&["O-O", "O-O-O"]));
        assert_eq!(fen_after(fen, "e1h1"), None);
    }

    /// The TS regex only takes lowercase squares and promotion letters.
    #[test]
    fn uppercase_is_not_uci() {
        assert_eq!(pv_to_san(START_FEN, &s(&["E2E4"])), Vec::<String>::new());
        assert_eq!(
            pv_to_san("8/P6k/8/8/8/8/8/K7 w - - 0 1", &s(&["a7a8Q"])),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_promotion_without_its_letter_is_illegal() {
        assert_eq!(
            pv_to_san("8/P6k/8/8/8/8/8/K7 w - - 0 1", &s(&["a7a8"])),
            Vec::<String>::new()
        );
    }
}

mod fen_after {
    use super::*;

    #[test]
    fn returns_the_fen_after_a_legal_move() {
        assert_eq!(
            fen_after(START_FEN, "e2e4").as_deref(),
            Some("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1")
        );
    }

    #[test]
    fn returns_none_when_illegal_or_the_fen_is_invalid() {
        assert_eq!(fen_after(START_FEN, "e2e5"), None);
        assert_eq!(fen_after("garbage", "e2e4"), None);
        assert_eq!(fen_after(START_FEN, "zz"), None);
    }
}

mod strip_san {
    use super::*;

    #[test]
    fn strips_trailing_check_mate_and_annotation_marks() {
        assert_eq!(strip_san("Qh5+"), "Qh5");
        assert_eq!(strip_san("Qxf7#"), "Qxf7");
        assert_eq!(strip_san("Nf3!?"), "Nf3");
        assert_eq!(strip_san("e4+!!"), "e4");
        assert_eq!(strip_san("O-O"), "O-O");
        assert_eq!(strip_san("+e4"), "+e4");
    }
}
