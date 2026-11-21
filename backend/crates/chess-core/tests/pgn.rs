//! PGN parser tests: ports of `packages/chess-core/test/pgn.test.ts`, the
//! review-finding cases the TS version only failed soft on, and parity with
//! chess.js 1.4.0 on recorded Lichess and chess.com games.

use chess_core::pgn::{PgnError, parse_pgn};
use chess_core::{Color, Terminal};
use serde::Deserialize;

fn fixture(name: &str) -> String {
    let path = format!("{}/../../fixtures/pgn/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn clocks(pgn: &str) -> Vec<Option<f64>> {
    parse_pgn(pgn)
        .expect("parses")
        .plies
        .iter()
        .map(|p| p.clock_seconds)
        .collect()
}

fn sans(pgn: &str) -> Vec<String> {
    parse_pgn(pgn)
        .expect("parses")
        .plies
        .iter()
        .map(|p| p.san.clone())
        .collect()
}

// ---- ports of pgn.test.ts ----

#[test]
fn parses_scholars_mate_with_clocks() {
    let game = parse_pgn(&fixture("scholars-mate.pgn")).unwrap();
    assert_eq!(game.plies.len(), 7);
    let first = &game.plies[0];
    assert_eq!(first.ply, 1);
    assert_eq!(first.color, Color::White);
    assert_eq!(first.san, "e4");
    assert_eq!(first.uci, "e2e4");
    assert_eq!(first.clock_seconds, Some(178.0));
    assert_eq!(first.fen_before, game.start_fen);
    assert_eq!(
        first.fen_after,
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1"
    );
    assert_eq!(game.plies[1].color, Color::Black);
    assert_eq!(game.plies[1].clock_seconds, Some(177.0));
    assert_eq!(game.plies[5].clock_seconds, Some(160.0));
    assert_eq!(game.plies[6].uci, "h5f7");
    assert_eq!(game.plies[6].clock_seconds, Some(169.0));
    assert_eq!(game.terminal, Some(Terminal::Checkmate));
    assert_eq!(game.header("TimeControl"), Some("180"));
}

#[test]
fn parses_game_without_clocks() {
    let game = parse_pgn(&fixture("no-clocks.pgn")).unwrap();
    assert_eq!(game.plies.len(), 7);
    assert!(game.plies.iter().all(|p| p.clock_seconds.is_none()));
}

#[test]
fn rejects_chess960() {
    let err = parse_pgn(&fixture("chess960.pgn")).unwrap_err();
    assert!(err.is_unsupported());
    assert!(matches!(err, PgnError::UnsupportedVariant(_)));
}

#[test]
fn rejects_non_standard_variant_headers_and_setup_fen_games() {
    let err = parse_pgn("[Variant \"Atomic\"]\n\n1. e4 *").unwrap_err();
    assert_eq!(err.to_string(), "Unsupported variant: Atomic");
    assert!(err.is_unsupported());

    let err = parse_pgn("[SetUp \"1\"]\n[FEN \"8/8/8/8/8/8/8/K6k w - - 0 1\"]\n\n*").unwrap_err();
    assert!(err.is_unsupported());
    assert_eq!(
        err.to_string(),
        "Games from a custom starting position are not supported"
    );
    // FEN alone is enough.
    assert!(
        parse_pgn("[FEN \"8/8/8/8/8/8/8/K6k w - - 0 1\"]\n\n*")
            .unwrap_err()
            .is_unsupported()
    );
}

#[test]
fn accepts_explicit_standard_variant_and_reports_promotions() {
    let game = parse_pgn(
        "[Variant \"Standard\"]\n\n1. e4 {[%clk 0:00:59.5]} d5 2. exd5 c6 3. dxc6 Nf6 4. cxb7 Nbd7 5. bxa8=Q Nb6 *",
    )
    .unwrap();
    // Clocks are by move index: only the first move has one.
    assert!((game.plies[0].clock_seconds.unwrap() - 59.5).abs() < 1e-5);
    assert!(game.plies[1..].iter().all(|p| p.clock_seconds.is_none()));
    assert_eq!(game.plies[8].uci, "b7a8q");
    assert_eq!(game.terminal, None);
}

#[test]
fn fractional_clock_is_read() {
    let c = clocks("1. e4 {[%clk 0:00:59.5]} e5 {[%clk 1:00:00]} *");
    assert_eq!(c, vec![Some(59.5), Some(3600.0)]);
}

#[test]
fn variant_chess_is_standard_and_case_and_space_insensitive() {
    assert!(parse_pgn("[Variant \"Chess\"]\n\n1. e4 *").is_ok());
    assert!(parse_pgn("[Variant \" STANDARD \"]\n\n1. e4 *").is_ok());
    assert!(
        parse_pgn("[Variant \"From Position\"]\n\n1. e4 *")
            .unwrap_err()
            .is_unsupported()
    );
}

// ---- brief-specified additions ----

#[test]
fn nested_variations_do_not_shift_clocks() {
    let c = clocks("1. e4 {[%clk 0:03:00]} (1. d4 (1. c4) 1... d5) 1... e5 {[%clk 0:02:59]} *");
    assert_eq!(c, vec![Some(180.0), Some(179.0)]);
}

#[test]
fn deeply_nested_variations_and_variation_clocks_are_ignored() {
    let pgn = "1. e4 {[%clk 0:03:00]} (1. d4 {[%clk 0:09:09]} (1. c4 (1. b4 (1. a4))) 1... d5) \
               (1. Nf3) 1... e5 {[%clk 0:02:59]} 2. Nf3 {[%clk 0:02:58]} *";
    assert_eq!(sans(pgn), vec!["e4", "e5", "Nf3"]);
    assert_eq!(clocks(pgn), vec![Some(180.0), Some(179.0), Some(178.0)]);
}

#[test]
fn glued_move_numbers_parse() {
    assert_eq!(parse_pgn("1.e4 e5 2.Nf3 *").unwrap().plies.len(), 3);
}

#[test]
fn illegal_move_is_not_unsupported() {
    let err = parse_pgn("1. e4 e5 2. Ke9 *").unwrap_err();
    assert!(!err.is_unsupported());
    assert_eq!(
        err,
        PgnError::IllegalMove {
            ply: 3,
            token: "Ke9".to_string()
        }
    );
}

#[test]
fn semicolon_comments_and_nags_are_ignored() {
    let pgn = "1. e4 $1 ; a comment with (parens and {braces\ne5 $14 $2\n2. Nf3 {[%clk 0:01:00]} *";
    assert_eq!(sans(pgn), vec!["e4", "e5", "Nf3"]);
    assert_eq!(clocks(pgn), vec![None, None, Some(60.0)]);
}

// ---- review findings ----

#[test]
fn comments_containing_parentheses_do_not_break_variation_stripping() {
    let pgn = "1. e4 {best (by far) [%clk 0:03:00]} e5 {[%clk 0:02:59] :-)} \
               2. Nf3 {( [%clk 0:02:58]} (2. f4 {)} exf4) 2... Nc6 {[%clk 0:02:50]} *";
    assert_eq!(sans(pgn), vec!["e4", "e5", "Nf3", "Nc6"]);
    assert_eq!(
        clocks(pgn),
        vec![Some(180.0), Some(179.0), Some(178.0), Some(170.0)]
    );
}

#[test]
fn san_glued_to_comment_brace() {
    let pgn =
        "1. e4{[%clk 0:03:00]} e5{[%clk 0:02:59]} 2. Nf3{[%clk 0:02:58]}Nc6{[%clk 0:02:57]} *";
    assert_eq!(sans(pgn), vec!["e4", "e5", "Nf3", "Nc6"]);
    assert_eq!(
        clocks(pgn),
        vec![Some(180.0), Some(179.0), Some(178.0), Some(177.0)]
    );
}

#[test]
fn eval_and_clk_in_one_comment() {
    let pgn = "1. e4 { [%eval 0.15] [%clk 0:03:00] } e5 {[%clk 0:02:59.5] [%eval #3]} *";
    assert_eq!(clocks(pgn), vec![Some(180.0), Some(179.5)]);
    let pgn = "1. e4 { [%eval 0.15,12] [%emt 0:00:01] } *";
    assert_eq!(clocks(pgn), vec![None]);
}

#[test]
fn nags_in_all_positions() {
    let pgn = "1. e4 $1 {[%clk 0:03:00]} $2 e5 $3 $4 {[%clk 0:02:59]} 2. Nf3 $5 {[%clk 0:02:58]} Nc6 $6 {[%clk 0:02:57]} *";
    assert_eq!(sans(pgn), vec!["e4", "e5", "Nf3", "Nc6"]);
    assert_eq!(
        clocks(pgn),
        vec![Some(180.0), Some(179.0), Some(178.0), Some(177.0)]
    );
}

#[test]
fn annotation_suffixes_are_accepted_and_normalised() {
    assert_eq!(
        sans("1. e4! e5?! 2. Nf3!! Nc6?? *"),
        vec!["e4", "e5", "Nf3", "Nc6"]
    );
}

#[test]
fn black_move_numbers_with_ellipsis() {
    let pgn = "1. e4 1... e5 2. Nf3 2... Nc6 3.Bb5 3...a6 *";
    assert_eq!(sans(pgn), vec!["e4", "e5", "Nf3", "Nc6", "Bb5", "a6"]);
    assert_eq!(sans("1.e4 1...e5 *"), vec!["e4", "e5"]);
}

#[test]
fn headers_with_escaped_quotes_and_crlf() {
    let pgn = "[Event \"The \\\"Big\\\" One \\\\ x\"]\r\n[White \"a\"]\r\n\r\n1. e4 {[%clk 0:03:00]}\r\ne5 {[%clk 0:02:59]} *\r\n";
    let game = parse_pgn(pgn).unwrap();
    assert_eq!(game.header("Event"), Some("The \"Big\" One \\ x"));
    assert_eq!(game.header("White"), Some("a"));
    assert_eq!(game.header("Missing"), None);
    assert_eq!(game.plies.len(), 2);
    assert_eq!(game.plies[1].clock_seconds, Some(179.0));
}

#[test]
fn header_block_ends_at_first_non_header_line() {
    // A tag-like line after the movetext has started is not a header.
    let game = parse_pgn("[A \"1\"]\n1. e4\n[B \"2\"]\ne5 *");
    // "[B "2"]" is then movetext and not a legal move.
    assert!(matches!(game, Err(PgnError::IllegalMove { .. })));
    let game = parse_pgn("[A \"1\"]\n\n[B \"2\"]\n\n1. e4 e5 *").unwrap();
    assert_eq!(game.header("A"), Some("1"));
    assert_eq!(game.header("B"), Some("2"));
}

#[test]
fn castling_with_zeros_and_results_are_handled() {
    let pgn = "1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 4. 0-0 Nf6 1/2-1/2";
    let s = sans(pgn);
    assert_eq!(s[6], "O-O");
    assert_eq!(s.last().unwrap(), "Nf6");
}

#[test]
fn malformed_input_is_reported() {
    assert!(matches!(
        parse_pgn("1. e4 {unterminated").unwrap_err(),
        PgnError::Malformed(_)
    ));
    assert!(matches!(
        parse_pgn("1. e4 (1. d4").unwrap_err(),
        PgnError::Malformed(_)
    ));
    assert!(matches!(
        parse_pgn("1. e4 ) e5").unwrap_err(),
        PgnError::Malformed(_)
    ));
}

#[test]
fn empty_movetext_is_an_empty_game() {
    let game = parse_pgn("[Event \"x\"]\n\n*").unwrap();
    assert!(game.plies.is_empty());
    assert_eq!(game.terminal, None);
}

// ---- terminals ----

#[test]
fn stalemate_is_detected() {
    let pgn = "1. e3 a5 2. Qh5 Ra6 3. Qxa5 h5 4. h4 Rah6 5. Qxc7 f6 6. Qxd7+ Kf7 \
               7. Qxb7 Qd3 8. Qxb8 Qh7 9. Qxc8 Kg6 10. Qe6 1/2-1/2";
    assert_eq!(parse_pgn(pgn).unwrap().terminal, Some(Terminal::Stalemate));
}

#[test]
fn threefold_repetition_counts_the_start_position() {
    let pgn = "1. Nf3 Nf6 2. Ng1 Ng8 3. Nf3 Nf6 4. Ng1 Ng8 1/2-1/2";
    assert_eq!(parse_pgn(pgn).unwrap().terminal, Some(Terminal::Draw));
    // Only twice: not a draw.
    let pgn = "1. Nf3 Nf6 2. Ng1 Ng8 3. Nf3 Nf6 *";
    assert_eq!(parse_pgn(pgn).unwrap().terminal, None);
}

// ---- real-world parity with chess.js 1.4.0 ----

#[derive(Deserialize)]
struct Reference {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    pgn: String,
    daily: bool,
    #[serde(rename = "noClocks", default)]
    no_clocks: bool,
    terminal: Option<String>,
    /// san, uci, fen before, fen after, clock.
    plies: Vec<(String, String, String, String, Option<f64>)>,
}

#[test]
fn recorded_games_match_chessjs() {
    let reference: Reference =
        serde_json::from_str(&fixture("realworld-reference.json")).expect("reference json");
    assert!(reference.cases.len() >= 8);
    let terminals: Vec<Option<&str>> = reference
        .cases
        .iter()
        .map(|c| c.terminal.as_deref())
        .collect();
    for want in [Some("checkmate"), Some("stalemate"), Some("draw"), None] {
        assert!(
            terminals.contains(&want),
            "reference lacks terminal {want:?}"
        );
    }
    for case in &reference.cases {
        let game = parse_pgn(&case.pgn).unwrap_or_else(|e| panic!("{}: {e}", case.name));
        assert_eq!(game.plies.len(), case.plies.len(), "{}", case.name);
        for (i, (got, want)) in game.plies.iter().zip(&case.plies).enumerate() {
            let at = format!("{} ply {}", case.name, i + 1);
            assert_eq!(got.ply as usize, i + 1, "{at}");
            assert_eq!(got.san, want.0, "{at}");
            assert_eq!(got.uci, want.1, "{at}");
            assert_eq!(got.fen_before, want.2, "{at}");
            assert_eq!(got.fen_after, want.3, "{at}");
            assert_eq!(got.clock_seconds, want.4, "{at}");
            if case.no_clocks {
                assert!(got.clock_seconds.is_none(), "{at}: no clocks in source");
            } else if case.daily {
                assert!(got.clock_seconds.is_none(), "{at}: daily has no clocks");
            } else {
                assert!(got.clock_seconds.is_some(), "{at}: missing clock");
            }
        }
        let terminal = game.terminal.map(|t| match t {
            Terminal::Checkmate => "checkmate",
            Terminal::Stalemate => "stalemate",
            Terminal::Draw => "draw",
        });
        assert_eq!(terminal, case.terminal.as_deref(), "{}", case.name);
    }
}

// ---- escape lines, clocks, unicode, size ----

#[test]
fn percent_escape_lines_are_rejected_like_chessjs() {
    // chess.js 1.4.0 throws on a line starting with `%` (the PGN spec says
    // to ignore it); we match chess.js.
    for pgn in [
        "1. e4 e5
% escaped Nf3
2. Nf3 Nc6
",
        "%first line
1. e4 e5
",
        "1. e4 e5
%
2. Nf3 Nc6
",
    ] {
        assert!(matches!(
            parse_pgn(pgn).unwrap_err(),
            PgnError::Malformed(_)
        ));
    }
    // A `%` inside a comment is fine.
    assert_eq!(
        sans(
            "1. e4 {50% sure
% x} e5 *"
        ),
        ["e4", "e5"]
    );
}

#[test]
fn clock_with_two_digit_hours() {
    let c = clocks("1. e4 {[%clk 10:00:00]} e5 {[%clk 100:01:02.5]} *");
    assert_eq!(c, [Some(36000.0), Some(100.0 * 3600.0 + 62.5)]);
}

#[test]
fn non_ascii_comment_bytes() {
    let pgn = "1. e4 {Schöne Eröffnung ♟ 日本語 [%clk 0:01:00]} e5 {é} 2. Nf3 ; ünï
Nc6 *";
    assert_eq!(sans(pgn), ["e4", "e5", "Nf3", "Nc6"]);
    assert_eq!(clocks(pgn)[0], Some(60.0));
}

#[test]
fn long_game_parses_quickly() {
    let mut pgn = String::new();
    for n in 1..=260 {
        pgn.push_str(&format!("{n}. Nf3 Nf6 {n}. Ng1 Ng8 "));
    }
    // numbers are ignored; 1040 plies of knight shuffling
    let start = std::time::Instant::now();
    let game = parse_pgn(&pgn).expect("parses");
    assert_eq!(game.plies.len(), 1040);
    assert!(
        start.elapsed() < std::time::Duration::from_secs(1),
        "{:?}",
        start.elapsed()
    );
}
