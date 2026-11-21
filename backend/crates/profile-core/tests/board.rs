//! R2 Task 4: board utilities, material swings and the null move (TS M2 Task 3 values).

mod common;

use chess_core::Color;
use chess_core::board::Position;
use common::positions::{FIXTURES, START, fixture, line};
use profile_core::board::{
    MATE_GAIN, PIECE_VALUE, attackers_of, defenders_of, is_en_prise, material_balance,
    material_signature, null_move_fen, piece_count, pinned_to, play_line, settled_plies,
    settled_swing,
};
use profile_core::types::Swing;

fn swing(gain: f64, depth: Option<u32>, mate_in: Option<u32>) -> Swing {
    Swing {
        gain,
        depth,
        mate_in,
    }
}

/// TS M2 Task 3's swing table (pov White, max 8 plies, settle 4).
const EXPECTED: &[(&str, f64, Option<u32>, Option<u32>)] = &[
    ("hanging", 9.0, Some(1), None),
    ("fork", 5.0, Some(3), None),
    ("pin", 2.0, Some(3), None),
    ("skewer", 9.0, Some(3), None),
    ("discovered", 9.0, Some(3), None),
    ("deflection", 4.0, Some(3), None),
    ("promotion", 8.0, Some(1), None),
    ("backRank", 100.0, Some(1), Some(1)),
    ("mateIn2", 100.0, Some(3), Some(2)),
    ("smothered", 100.0, Some(1), Some(1)),
    ("quiet", 0.0, None, None),
];

#[test]
fn settled_swing_matches_the_ts_m2_fixture_table() {
    assert_eq!(EXPECTED.len(), FIXTURES.len());
    for (name, gain, depth, mate_in) in EXPECTED {
        let (fen, pv) = fixture(name);
        assert_eq!(
            settled_swing(fen, &pv, Color::White, 8, 4),
            swing(*gain, *depth, *mate_in),
            "{name}"
        );
    }
}

#[test]
fn piece_values_and_mate_gain() {
    // Indexed by cozy_chess::Piece: pawn, knight, bishop, rook, queen, king.
    assert_eq!(PIECE_VALUE, [1, 3, 3, 5, 9, 0]);
    assert_eq!(MATE_GAIN, 100.0);
}

#[test]
fn wrong_pov_has_no_swing() {
    let (fen, pv) = fixture("fork");
    assert_eq!(
        settled_swing(fen, &pv, Color::Black, 8, 4),
        swing(0.0, None, None)
    );
}

#[test]
fn settled_plies_stop_the_recapture_chain_before_a_mate() {
    // Rd8+ Rxd8 Rxd8#: ply 1 settles after the recapture (-5), the mating ply is not material.
    let (fen, pv) = fixture("mateIn2");
    let plies = settled_plies(fen, &pv, Color::White, 8, 4);
    let got: Vec<(u32, i32, bool)> = plies.iter().map(|p| (p.ply, p.settled, p.mate)).collect();
    assert_eq!(got, [(1, -5, false), (3, 0, true)]);
    // Re8+ Rxe8 Qxb7: the recapture on e8 settles ply 1; Qxb7 is on another square.
    let (fen, pv) = fixture("deflection");
    let got: Vec<(u32, i32)> = settled_plies(fen, &pv, Color::White, 8, 4)
        .iter()
        .map(|p| (p.ply, p.settled))
        .collect();
    assert_eq!(got, [(1, -5), (3, 4)]);
    // An en-passant capture recaptures the pawn that just double-pushed.
    let got: Vec<(u32, i32)> = settled_plies(
        "4k3/8/8/8/3p4/8/4P3/4K3 w - - 0 1",
        &line(&["e2e4", "d4e3"]),
        Color::White,
        8,
        4,
    )
    .iter()
    .map(|p| (p.ply, p.settled))
    .collect();
    assert_eq!(got, [(1, -1)]);
}

/// `(ply, settled)` of every pov ply, defaults max 8 plies and settle 4.
fn settled(fen: &str, pv: &[&str]) -> Vec<(u32, i32)> {
    settled_plies(fen, &line(pv), Color::White, 8, 4)
        .iter()
        .map(|p| (p.ply, p.settled))
        .collect()
}

#[test]
fn an_opponent_recapture_that_mates_stays_in_the_chain() {
    // Rxe1 Rxe1#: the mate is the opponent's, so its material counts (+3 then -5 = -2).
    let fen = "k3r3/8/8/8/8/8/5PPP/3Rn1K1 w - - 0 1";
    let plies = settled_plies(fen, &line(&["d1e1", "e8e1"]), Color::White, 8, 4);
    let got: Vec<(u32, i32, bool)> = plies.iter().map(|p| (p.ply, p.settled, p.mate)).collect();
    assert_eq!(got, [(1, -2, false)]);
    assert_eq!(
        settled_swing(fen, &line(&["d1e1", "e8e1"]), Color::White, 8, 4),
        swing(-2.0, None, None)
    );
}

#[test]
fn the_settle_cap_still_takes_the_recapture_of_a_pov_capture() {
    // Six captures on d5. Ply 1's chain reaches the cap at ply 5 (a pov capture), so the
    // recapture at ply 6 is included: -2, not +1.
    let fen = "6k1/8/1np2n2/3p4/4PN2/2N5/8/6K1 w - - 0 1";
    let pv = ["e4d5", "c6d5", "c3d5", "f6d5", "f4d5", "b6d5"];
    assert_eq!(settled(fen, &pv), [(1, -2), (3, -2), (5, -2)]);
}

#[test]
fn a_promotion_inside_a_chain_counts_the_new_piece() {
    // Rxd8 Rxd8 cxd8=Q: B0 = -4, the queen settles the exchange at +9.
    let fen = "3r1r2/2P4k/8/8/8/8/8/3R2K1 w - - 0 1";
    let pv = ["d1d8", "f8d8", "c7d8q"];
    assert_eq!(settled(fen, &pv), [(1, 13), (3, 13)]);
    let steps = play_line(fen, &line(&pv), None);
    assert_eq!(
        (steps[2].captured, steps[2].promotion),
        (Some('r'), Some('q'))
    );
}

#[test]
fn an_x_ray_chain_of_four_captures_settles_on_the_last() {
    // Rxd5 Rxd5 Rxd5 Qxd5: both recaptures come from behind a piece that just left the file.
    let fen = "3q2k1/3r4/8/3n4/8/8/3R4/3R2K1 w - - 0 1";
    let pv = ["d2d5", "d7d5", "d1d5", "d8d5"];
    assert_eq!(settled(fen, &pv), [(1, -2), (3, -2)]);
}

/// Black shuffles Kh8-g8; White shuffles Ke1-e2 and finally mates with Ra8.
const DEEP_MATE_FEN: &str = "7k/6pp/1p6/8/8/8/8/R3K3 w - - 0 1";

#[test]
fn a_mate_past_the_horizon_counts_with_depth_at_the_mating_ply() {
    let mate_in_5 = [
        "e1e2", "h8g8", "e2e1", "g8h8", "e1e2", "h8g8", "e2e1", "g8h8", "a1a8",
    ];
    assert_eq!(
        settled_swing(DEEP_MATE_FEN, &line(&mate_in_5), Color::White, 8, 4),
        swing(MATE_GAIN, Some(9), Some(5))
    );
    // mateIn4Plus territory: mate in 6 at ply 11.
    let mate_in_6 = [
        "e1e2", "h8g8", "e2e1", "g8h8", "e1e2", "h8g8", "e2e1", "g8h8", "e1e2", "b6b5", "a1a8",
    ];
    let s = settled_swing(DEEP_MATE_FEN, &line(&mate_in_6), Color::White, 8, 4);
    assert_eq!(s, swing(MATE_GAIN, Some(11), Some(6)));
    assert!(s.mate_in.is_some_and(|m| m >= 4));
    // The opponent's mate is no gain for pov.
    assert_eq!(
        settled_swing(DEEP_MATE_FEN, &line(&mate_in_5), Color::Black, 8, 4),
        swing(0.0, None, None)
    );
}

#[test]
fn a_material_swing_inside_the_horizon_sets_depth_before_a_deep_mate() {
    // Hanging queen won at ply 1, then the same mating pattern far past the horizon.
    let fen = "7k/6pp/1p6/8/q7/8/8/R3K3 w - - 0 1";
    let pv = [
        "a1a4", "h8g8", "a4a1", "g8h8", "e1e2", "h8g8", "e2e1", "g8h8", "e1e2", "h8g8", "e2e1",
        "g8h8", "a1a8",
    ];
    assert_eq!(
        settled_swing(fen, &line(&pv), Color::White, 8, 4),
        swing(MATE_GAIN, Some(1), Some(7))
    );
}

#[test]
fn swing_horizon_limits_the_counted_plies() {
    let (fen, pv) = fixture("fork");
    assert_eq!(
        settled_swing(fen, &pv, Color::White, 2, 4),
        swing(0.0, None, None)
    );
}

#[test]
fn play_line_stops_at_the_first_illegal_move() {
    assert_eq!(play_line(START, &line(&["e2e4", "e2e4"]), None).len(), 1);
    assert_eq!(play_line(START, &line(&["e2e4", "e7e5"]), Some(1)).len(), 1);
    assert!(play_line("not a fen", &line(&["e2e4"]), None).is_empty());
    assert!(play_line(START, &line(&["zz", "e2e4"]), None).is_empty());
}

#[test]
fn play_line_reports_captures_checks_mates_and_promotions() {
    let (fen, pv) = fixture("hanging");
    let step = &play_line(fen, &pv, None)[0];
    assert_eq!(step.uci, "d1d5");
    assert_eq!(step.san, "Qxd5");
    assert_eq!(step.fen_before, fen);
    assert_eq!(step.fen_after, "4k3/8/8/3Q4/8/8/8/4K3 b - - 0 1");
    assert_eq!(step.mover, Color::White);
    assert_eq!(step.captured, Some('q'));
    assert_eq!((step.from.as_str(), step.to.as_str()), ("d1", "d5"));
    assert!(!step.is_check && !step.is_mate);
    assert_eq!(step.promotion, None);

    let (fen, pv) = fixture("fork");
    let steps = play_line(fen, &pv, None);
    assert_eq!(steps.len(), 3);
    assert!(steps[0].is_check && !steps[0].is_mate);
    assert_eq!(steps[1].mover, Color::Black);
    assert_eq!(steps[2].captured, Some('r'));
    assert_eq!(steps[1].fen_after, steps[2].fen_before);

    let (fen, pv) = fixture("backRank");
    let step = &play_line(fen, &pv, None)[0];
    assert!(step.is_check && step.is_mate);
    assert_eq!(step.san, "Rd8#");

    let (fen, pv) = fixture("promotion");
    let step = &play_line(fen, &pv, None)[0];
    assert_eq!(step.promotion, Some('q'));
    assert_eq!(step.san, "e8=Q");
    assert_eq!(step.captured, None);
}

#[test]
fn play_line_reports_en_passant_and_canonical_castling() {
    let fen = "rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3";
    let step = &play_line(fen, &line(&["e5d6"]), None)[0];
    assert_eq!(step.captured, Some('p'));
    assert_eq!(step.san, "exd6");

    let fen = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";
    let step = &play_line(fen, &line(&["e1h1"]), None)[0];
    assert_eq!(
        (step.uci.as_str(), step.san.as_str(), step.to.as_str()),
        ("e1g1", "O-O", "g1")
    );
    assert_eq!(step.captured, None);
}

#[test]
fn material_counts_and_signature() {
    let kpk = "8/4k3/8/4K3/4P3/8/8/8 w - - 0 1";
    assert_eq!(material_signature(kpk), "KPvK");
    assert_eq!(piece_count(kpk), 3);
    assert_eq!(material_balance(kpk, Color::White), 1);
    assert_eq!(material_balance(kpk, Color::Black), -1);
    assert_eq!(piece_count(START), 32);
    assert_eq!(material_balance(START, Color::White), 0);
    // The stronger side comes first whatever its colour.
    assert_eq!(
        material_signature("8/8/4k3/4r3/4P3/4K3/4R3/8 b - - 0 1"),
        "KRPvKR"
    );
    assert_eq!(
        material_signature("8/8/4k3/4R3/4p3/4K3/4r3/8 w - - 0 1"),
        "KRPvKR"
    );
    assert_eq!(
        material_signature("8/8/4k3/4n3/8/4K3/4B3/8 w - - 0 1"),
        "KBvKN"
    );
}

#[test]
fn null_move_flips_the_side_and_keeps_the_clocks() {
    assert_eq!(
        null_move_fen("4k3/8/8/r7/8/8/8/R3K3 b - - 0 1").as_deref(),
        Some("4k3/8/8/r7/8/8/8/R3K3 w - - 0 1")
    );
    // Nonzero clocks are kept as they are (a pass is not a move).
    assert_eq!(
        null_move_fen("4k3/8/8/r7/8/8/8/R3K3 b - - 7 42").as_deref(),
        Some("4k3/8/8/r7/8/8/8/R3K3 w - - 7 42")
    );
}

#[test]
fn null_move_fen_returns_none_in_check() {
    let fen = "4k3/8/8/8/8/8/8/R3K2r w - - 0 1";
    assert!(Position::from_fen(fen).unwrap().is_check());
    assert_eq!(null_move_fen(fen), None);
    assert_eq!(null_move_fen("not a fen"), None);
}

#[test]
fn null_move_clears_en_passant() {
    assert_eq!(
        null_move_fen("rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3").as_deref(),
        Some("rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR b KQkq - 0 3")
    );
}

#[test]
fn pinned_to_finds_the_pinner_and_the_piece_behind() {
    let (fen, pv) = fixture("pin");
    let after = &play_line(fen, &pv, Some(1))[0].fen_after;
    assert_eq!(
        pinned_to(after, "e5"),
        Some(("e1".to_owned(), "e8".to_owned()))
    );
    // The pawn in front of the knight is not pinned; empty squares and kings never are.
    assert_eq!(pinned_to(after, "d6"), None);
    assert_eq!(pinned_to(after, "e4"), None);
    assert_eq!(pinned_to(after, "e8"), None);
    // Relative pin: a knight in front of its queen.
    assert_eq!(
        pinned_to("4k3/8/4q3/4n3/8/8/8/4R1K1 b - - 0 1", "e5"),
        Some(("e1".to_owned(), "e6".to_owned()))
    );
    // A bishop does not pin along a file.
    assert_eq!(pinned_to("4k3/8/8/4n3/8/8/8/4B1K1 b - - 0 1", "e5"), None);
}

#[test]
fn attackers_defenders_and_en_prise() {
    let (fen, pv) = fixture("pin");
    let after = &play_line(fen, &pv, Some(1))[0].fen_after;
    assert_eq!(attackers_of(after, "e5", Color::White), ["f4", "e1"]);
    assert_eq!(defenders_of(after, "e5", Color::Black), ["d6"]);
    // Attacked by a cheaper pawn: en prise although defended.
    assert!(is_en_prise(after, "e5"));
    // Before f4 only the rook attacks the defended knight.
    assert!(!is_en_prise(fen, "e5"));
    // An undefended queen.
    let (fen, _) = fixture("hanging");
    assert!(is_en_prise(fen, "d5"));
    assert!(defenders_of(fen, "d5", Color::Black).is_empty());
    assert!(!is_en_prise(fen, "e4"));
}

#[test]
fn play_uci_castling() {
    let mut pos = Position::start();
    for san in ["e4", "e5", "Nf3", "Nc6", "Bc4", "Bc5"] {
        pos.play_san(san).unwrap();
    }
    let mut king_takes_rook = pos.clone();
    let played = pos.push_uci("e1g1").unwrap();
    assert_eq!((played.san.as_str(), played.uci.as_str()), ("O-O", "e1g1"));
    let played = king_takes_rook.push_uci("e1h1").unwrap();
    assert_eq!((played.san.as_str(), played.uci.as_str()), ("O-O", "e1g1"));
    assert_eq!(pos.fen(), king_takes_rook.fen());
    assert_eq!(pos.board().side_to_move(), cozy_chess::Color::Black);

    let before = pos.fen();
    assert!(pos.push_uci("e8e6").is_err());
    assert!(pos.push_uci("zz").is_err());
    assert_eq!(
        pos.fen(),
        before,
        "an illegal move leaves the position unchanged"
    );
}
