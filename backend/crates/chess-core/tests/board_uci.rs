//! R3 Task 7: in-place UCI play with chess.js SAN, checked against the chess.js 1.4.0 reference.
//!
//! The brief names this `Position::play_uci(&mut self, uci) -> Result<PlayedMove, IllegalMove>`.
//! That exact method already exists as [`Position::push_uci`] (R2 Task 4), because R1's
//! `play_uci(&self) -> Result<Position, ChessError>` keeps its signature for its callers (engine,
//! profile-core). This test pins the R3 contract on `push_uci`; see task-7-report.md.

use chess_core::board::Position;
use serde::Deserialize;

#[derive(Deserialize)]
struct Reference {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    start_fen: String,
    moves: Vec<RefMove>,
}

#[derive(Deserialize)]
struct RefMove {
    san: String,
    from: String,
    to: String,
    promotion: Option<String>,
    after: String,
}

fn reference() -> Reference {
    let text = include_str!("../../../fixtures/board/chessjs-reference.json");
    serde_json::from_str(text).expect("reference fixture parses")
}

#[test]
fn play_uci_matches_san_for_reference_positions() {
    let mut moves = 0;
    for case in reference().cases {
        let mut by_uci = Position::from_fen(&case.start_fen).expect("start fen");
        let mut by_san = by_uci.clone();
        for (i, m) in case.moves.iter().enumerate() {
            let ctx = format!("{} move {i} ({})", case.name, m.san);
            let uci = format!("{}{}{}", m.from, m.to, m.promotion.as_deref().unwrap_or(""));
            let played = by_uci
                .push_uci(&uci)
                .unwrap_or_else(|e| panic!("{ctx}: {e}"));
            assert_eq!(played.san, m.san, "{ctx}: san");
            assert_eq!(played.uci, uci, "{ctx}: uci");
            assert_eq!(by_uci.fen(), m.after, "{ctx}: after");
            let san = by_san
                .play_san(&m.san)
                .unwrap_or_else(|e| panic!("{ctx}: {e}"));
            assert_eq!(san, played, "{ctx}: identical to play_san");
            moves += 1;
        }
    }
    assert!(moves > 50, "the reference has {moves} moves");
}

#[test]
fn play_uci_rejects_illegal_and_malformed_moves_without_changing_the_position() {
    let mut pos = Position::start();
    let fen = pos.fen();
    for bad in ["e1e8", "e2e5", "zz", "", "e2e4qq", "E2E4"] {
        assert!(pos.push_uci(bad).is_err(), "{bad:?} must be rejected");
        assert_eq!(pos.fen(), fen, "{bad:?} left the position unchanged");
    }
}

#[test]
fn play_uci_accepts_castling_as_the_king_step_and_lowercase_promotion() {
    let mut pos = Position::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1").unwrap();
    assert_eq!(pos.push_uci("e1g1").unwrap().san, "O-O");
    assert_eq!(pos.push_uci("e8c8").unwrap().san, "O-O-O");
    let mut promo = Position::from_fen("8/P6k/8/8/8/8/8/K7 w - - 0 1").unwrap();
    assert!(
        promo.push_uci("a7a8").is_err(),
        "a promotion needs its suffix"
    );
    assert_eq!(promo.push_uci("a7a8q").unwrap().san, "a8=Q");
}
