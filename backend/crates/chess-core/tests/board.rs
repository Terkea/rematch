//! chess.js 1.4.0 parity tests for `chess_core::board::Position`.
//! The reference fixture was generated with chess.js 1.4.0 (see ADR 0037).

use chess_core::Color;
use chess_core::board::Position;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct Reference {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    start_fen: String,
    moves: Vec<RefMove>,
    #[serde(rename = "final")]
    fin: Final,
}

#[derive(Deserialize)]
struct RefMove {
    san: String,
    from: String,
    to: String,
    promotion: Option<String>,
    before: String,
    after: String,
}

#[derive(Deserialize)]
struct Final {
    checkmate: bool,
    stalemate: bool,
    insufficient_material: bool,
    threefold: bool,
}

fn reference() -> Reference {
    let text = include_str!("../../../fixtures/board/chessjs-reference.json");
    serde_json::from_str(text).expect("reference fixture parses")
}

fn threefold(counts: &HashMap<String, u32>, pos: &Position) -> bool {
    counts.get(&pos.repetition_key()).copied().unwrap_or(0) >= 3
}

fn count(counts: &mut HashMap<String, u32>, pos: &Position) {
    *counts.entry(pos.repetition_key()).or_default() += 1;
}

#[test]
fn fen_matches_chess_js_for_reference_positions() {
    let reference = reference();
    assert!(reference.cases.len() >= 13);
    for case in &reference.cases {
        let mut pos = Position::from_fen(&case.start_fen)
            .unwrap_or_else(|e| panic!("{}: start fen: {e}", case.name));
        assert_eq!(pos.fen(), case.start_fen, "{}: start fen", case.name);
        let mut counts = HashMap::new();
        count(&mut counts, &pos);
        for (i, m) in case.moves.iter().enumerate() {
            let ctx = format!("{} move {} ({})", case.name, i, m.san);
            assert_eq!(pos.fen(), m.before, "{ctx}: before");
            let played = pos
                .play_san(&m.san)
                .unwrap_or_else(|e| panic!("{ctx}: {e}"));
            assert_eq!(played.san, m.san, "{ctx}: san");
            let uci = format!("{}{}{}", m.from, m.to, m.promotion.as_deref().unwrap_or(""));
            assert_eq!(played.uci, uci, "{ctx}: uci");
            assert_eq!(pos.fen(), m.after, "{ctx}: after");
            count(&mut counts, &pos);
        }
        let n = &case.name;
        assert_eq!(pos.is_checkmate(), case.fin.checkmate, "{n}: mate");
        assert_eq!(pos.is_stalemate(), case.fin.stalemate, "{n}: stalemate");
        assert_eq!(
            pos.is_insufficient_material(),
            case.fin.insufficient_material,
            "{n}: insufficient"
        );
        assert_eq!(
            threefold(&counts, &pos),
            case.fin.threefold,
            "{n}: threefold"
        );
    }
}

#[test]
fn castling_uci_uses_king_destination() {
    let mut pos = Position::start();
    for san in ["e4", "e5", "Nf3", "Nc6", "Bc4", "Bc5"] {
        pos.play_san(san).unwrap();
    }
    assert_eq!(pos.play_san("O-O").unwrap().uci, "e1g1");
    for san in ["Nf6", "d3", "d6", "Bg5", "Qe7", "Nc3", "Bd7", "Qd2"] {
        pos.play_san(san).unwrap();
    }
    assert_eq!(pos.play_san("O-O-O").unwrap().uci, "e8c8");

    let mut pos = Position::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1").unwrap();
    assert_eq!(pos.play_san("O-O-O").unwrap().uci, "e1c1");
    assert_eq!(pos.play_san("O-O").unwrap().uci, "e8g8");
}

#[test]
fn ep_square_only_when_legal_capture() {
    let mut pos = Position::start();
    pos.play_san("e4").unwrap();
    assert!(pos.fen().contains(" b KQkq - 0 1"), "{}", pos.fen());
    for san in ["a6", "e5", "d5"] {
        pos.play_san(san).unwrap();
    }
    assert!(pos.fen().contains(" w KQkq d6 0 3"), "{}", pos.fen());
    // The capture would expose the king to the rook: chess.js prints '-'.
    let mut pos = Position::from_fen("k7/3p4/8/K3P2r/8/8/8/8 b - - 0 1").unwrap();
    pos.play_san("d5").unwrap();
    assert_eq!(pos.fen(), "k7/8/8/K2pP2r/8/8/8/8 w - - 0 2");
}

#[test]
fn repetition_key_ep_follows_adjacent_pawn_rule() {
    // Pinned ep capture: the FEN prints '-', but chess.js's hash still carries
    // the ep file because an enemy pawn stands next to the pushed pawn.
    let mut pinned = Position::from_fen("k7/3p4/8/K3P2r/8/8/8/8 b - - 0 1").unwrap();
    pinned.play_san("d5").unwrap();
    let plain = Position::from_fen("k7/8/8/K2pP2r/8/8/8/8 w - - 0 2").unwrap();
    assert_ne!(pinned.repetition_key(), plain.repetition_key());

    // No adjacent enemy pawn: no ep component, same key as the plain position.
    let mut a = Position::start();
    a.play_san("e4").unwrap();
    let b =
        Position::from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1").unwrap();
    assert_eq!(a.repetition_key(), b.repetition_key());
}

#[test]
fn san_accepts_suffixes_and_zero_castling() {
    let mut pos = Position::start();
    for san in ["e4", "e5", "Qh5", "Nc6", "Bc4", "Nf6"] {
        pos.play_san(san).unwrap();
    }
    assert_eq!(pos.play_san("Qxf7#").unwrap().san, "Qxf7#");
    let mut pos = Position::start();
    assert_eq!(pos.play_san("e4!?").unwrap().san, "e4");
    assert_eq!(pos.play_san("e5?!").unwrap().san, "e5");
    let mut pos = Position::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1").unwrap();
    assert_eq!(pos.play_san("0-0").unwrap().san, "O-O");
    assert_eq!(pos.play_san("0-0-0").unwrap().san, "O-O-O");
}

#[test]
fn san_accepts_over_disambiguated_and_coordinate_forms() {
    // chess.js's permissive parser.
    let cases = [
        ("Ngf3", "Nf3"),
        ("Ng1f3", "Nf3"),
        ("N1f3", "Nf3"),
        ("e2e4", "e4"),
        ("Pe2-e4", "e4"),
        ("g1f3", "Nf3"),
        ("Ng1-f3", "Nf3"),
    ];
    for (input, san) in cases {
        let mut pos = Position::start();
        assert_eq!(pos.play_san(input).unwrap().san, san, "{input}");
    }
    let mut pos = Position::from_fen("7k/1P6/8/8/8/8/8/K7 w - - 0 1").unwrap();
    let played = pos.play_san("b7b8q").unwrap();
    assert_eq!(
        (played.san.as_str(), played.uci.as_str()),
        ("b8=Q+", "b7b8q")
    );
    let mut pos = Position::start();
    assert!(pos.play_san("Nge7").is_err());
}

#[test]
fn illegal_san_is_rejected() {
    let mut pos = Position::start();
    assert!(pos.play_san("Ke9").is_err());
    assert!(pos.play_san("e5").is_err());
    assert!(pos.play_san("").is_err());
    assert!(pos.play_san("Qd4").is_err());
    assert!(pos.play_san("O-O").is_err());
    // A rejected move leaves the position unchanged.
    assert_eq!(
        pos.fen(),
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
    );
}

#[test]
fn terminal_flags_match_chess_js() {
    let reference = reference();
    let by_name = |n: &str| reference.cases.iter().find(|c| c.name == n).unwrap();
    assert!(by_name("a_scholars_mate").fin.checkmate);
    assert!(by_name("h_sam_loyd_stalemate").fin.stalemate);
    assert!(by_name("i_threefold").fin.threefold);

    let mut pos = Position::start();
    let mut counts = HashMap::new();
    count(&mut counts, &pos);
    for san in ["Nf3", "Nf6", "Ng1", "Ng8", "Nf3", "Nf6", "Ng1", "Ng8"] {
        pos.play_san(san).unwrap();
        count(&mut counts, &pos);
    }
    assert!(threefold(&counts, &pos));

    let fen = |s: &str| Position::from_fen(s).unwrap();
    // Bishops on opposite colours: not insufficient.
    assert!(!fen("8/8/8/8/1b6/8/B1k5/K7 b - - 0 1").is_insufficient_material());
    // K+B v K.
    assert!(fen("8/8/8/8/8/3k4/8/K1B5 w - - 0 1").is_insufficient_material());
    // Bishops on the same colour (a1 and c1... b2/c1 dark and light checked by fixture).
    assert!(fen("8/8/8/8/8/3k4/1b6/K1B5 w - - 0 1").is_insufficient_material());
    assert!(fen("8/8/8/8/8/3k4/8/K7 w - - 0 1").is_insufficient_material());
    assert!(fen("8/8/8/8/8/3k4/8/KN6 w - - 0 1").is_insufficient_material());
    assert!(!fen("8/8/8/8/8/3k4/8/K1R5 w - - 0 1").is_insufficient_material());
    assert!(!fen("8/8/8/8/8/3k4/p7/K7 w - - 0 1").is_insufficient_material());
    // Two knights: chess.js says sufficient.
    assert!(!fen("8/8/8/8/8/3k4/8/KN1N4 w - - 0 1").is_insufficient_material());
}

#[test]
fn clock_and_side_accessors() {
    let mut pos = Position::start();
    assert_eq!(pos.side_to_move(), Color::White);
    assert_eq!(pos.halfmove_clock(), 0);
    pos.play_san("Nf3").unwrap();
    assert_eq!(pos.side_to_move(), Color::Black);
    assert_eq!(pos.halfmove_clock(), 1);
}

#[test]
fn from_fen_rejects_garbage() {
    assert!(Position::from_fen("not a fen").is_err());
    assert!(Position::from_fen("").is_err());
}

#[test]
fn play_uci_applies_legal_moves_and_rejects_illegal() {
    let pos = Position::start();
    let next = pos.play_uci("e2e4").unwrap();
    assert_eq!(
        next.fen(),
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1"
    );
    // The receiver is unchanged.
    assert_eq!(pos.side_to_move(), Color::White);
    assert!(pos.play_uci("e2e5").is_err());
    assert!(pos.play_uci("e7e5").is_err());
    assert!(pos.play_uci("zz").is_err());
    assert!(pos.play_uci("").is_err());
    assert!(pos.play_uci("e2e4q").is_err());
}

#[test]
fn play_uci_handles_castling_in_both_forms() {
    let pos = Position::from_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1").unwrap();
    let short = "r3k2r/8/8/8/8/8/8/R4RK1 b kq - 1 1";
    let long = "r3k2r/8/8/8/8/8/8/2KR3R b kq - 1 1";
    // chess.js style: king destination.
    assert_eq!(pos.play_uci("e1g1").unwrap().fen(), short);
    assert_eq!(pos.play_uci("e1c1").unwrap().fen(), long);
    // Engine (king takes own rook) style.
    assert_eq!(pos.play_uci("e1h1").unwrap().fen(), short);
    assert_eq!(pos.play_uci("e1a1").unwrap().fen(), long);
    let black = Position::from_fen("r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1").unwrap();
    let want = "r4rk1/8/8/8/8/8/8/R3K2R w KQ - 1 2";
    assert_eq!(black.play_uci("e8g8").unwrap().fen(), want);
    assert_eq!(black.play_uci("e8h8").unwrap().fen(), want);
    // A two-file king step that is not castling stays illegal.
    let none = Position::from_fen("4k3/8/8/8/8/8/8/4K3 w - - 0 1").unwrap();
    assert!(none.play_uci("e1g1").is_err());
}

#[test]
fn play_uci_promotion_and_en_passant() {
    let pos = Position::from_fen("7k/1P6/8/8/8/8/8/K7 w - - 0 1").unwrap();
    assert!(pos.play_uci("b7b8").is_err(), "promotion needs a suffix");
    assert_eq!(
        pos.play_uci("b7b8q").unwrap().fen(),
        "1Q5k/8/8/8/8/8/8/K7 b - - 0 1"
    );
    let mut ep = Position::start();
    for san in ["e4", "a6", "e5", "d5"] {
        ep.play_san(san).unwrap();
    }
    let after = ep.play_uci("e5d6").unwrap();
    assert_eq!(
        after.fen(),
        "rnbqkbnr/1pp1pppp/p2P4/8/8/8/PPPP1PPP/RNBQKBNR b KQkq - 0 3"
    );
}

#[test]
fn fen_round_trips_large_clocks_unchanged() {
    // cozy-chess caps the halfmove clock at 100; Position must not.
    let fen = "4k3/8/8/8/8/8/8/R3K3 w - - 150 80";
    let pos = Position::from_fen(fen).unwrap();
    assert_eq!(pos.fen(), fen);
    assert_eq!(pos.halfmove_clock(), 150);
}

#[test]
fn halfmove_clock_counts_past_100() {
    let mut pos = Position::from_fen("4k3/8/8/8/8/8/8/R3K3 w - - 96 60").unwrap();
    for (i, san) in ["Ra2", "Kd7", "Ra3", "Ke7", "Ra4", "Kd7"]
        .iter()
        .enumerate()
    {
        pos.play_san(san).unwrap();
        assert_eq!(pos.halfmove_clock(), 97 + i as u32, "after {san}");
    }
    assert_eq!(pos.halfmove_clock(), 102);
    assert_eq!(pos.fen(), "8/3k4/8/8/R7/8/8/4K3 w - - 102 63");
    // A capture still resets it.
    let mut cap = Position::from_fen("4k3/8/8/p7/8/8/8/R3K3 w - - 120 70").unwrap();
    cap.play_san("Rxa5").unwrap();
    assert_eq!(cap.halfmove_clock(), 0);
}

#[test]
fn coordinate_promotion_without_piece_picks_knight_like_chess_js() {
    // chess.js 1.4.0: move('e7e8') promotes to a knight (first of its
    // promotion list). Not reachable from Lichess/chess.com PGN, but pinned.
    let mut pos = Position::from_fen("7k/4P3/8/8/8/8/8/K7 w - - 0 1").unwrap();
    let played = pos.play_san("e7e8").unwrap();
    assert_eq!(
        (played.san.as_str(), played.uci.as_str()),
        ("e8=N", "e7e8n")
    );
    // An explicit piece still wins.
    let mut pos = Position::from_fen("7k/4P3/8/8/8/8/8/K7 w - - 0 1").unwrap();
    assert_eq!(pos.play_san("e7e8q").unwrap().san, "e8=Q+");
}

#[test]
fn fixture_covers_required_scenarios() {
    let reference = reference();
    let mut ep = 0;
    let mut printed_ep = 0;
    let (mut wk, mut wq, mut bk, mut bq) = (0, 0, 0, 0);
    let (mut mates, mut stales, mut threes) = (0, 0, 0);
    let mut square_disamb = 0;
    for case in &reference.cases {
        mates += case.fin.checkmate as u32;
        stales += case.fin.stalemate as u32;
        threes += case.fin.threefold as u32;
        for m in &case.moves {
            let fields: Vec<&str> = m.before.split(' ').collect();
            let white = fields[1] == "w";
            if m.after.split(' ').nth(3) != Some("-") {
                printed_ep += 1;
            }
            // En passant: a pawn capture onto the previously printed ep square.
            if m.san.contains('x') && Some(m.to.as_str()) == Some(fields[3]) {
                let pawn_move = m.san.chars().next().is_some_and(|c| c.is_ascii_lowercase());
                ep += pawn_move as u32;
            }
            match (m.san.starts_with("O-O-O"), m.san.starts_with("O-O"), white) {
                (true, _, true) => wq += 1,
                (true, _, false) => bq += 1,
                (_, true, true) => wk += 1,
                (_, true, false) => bk += 1,
                _ => {}
            }
            let b = m.san.as_bytes();
            if b.len() >= 5
                && b"NBRQK".contains(&b[0])
                && b[1].is_ascii_lowercase()
                && b[2].is_ascii_digit()
            {
                square_disamb += 1;
            }
        }
    }
    assert!(ep >= 10, "ep captures {ep}");
    assert!(printed_ep >= 10, "printed ep squares {printed_ep}");
    assert!(wk > 0 && wq > 0 && bk > 0 && bq > 0, "{wk} {wq} {bk} {bq}");
    assert!(
        mates >= 5 && stales >= 3 && threes >= 3,
        "{mates} {stales} {threes}"
    );
    assert!(square_disamb >= 3, "square disambiguation {square_disamb}");
    assert!(
        reference
            .cases
            .iter()
            .any(|c| c.name.starts_with("real_lichess_"))
    );
    assert!(
        reference
            .cases
            .iter()
            .any(|c| c.name.starts_with("real_chesscom_"))
    );
}
