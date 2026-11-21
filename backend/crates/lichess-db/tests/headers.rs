//! Dump header parsing and movetext SAN extraction.

use chess_core::pgn::parse_pgn;
use lichess_db::download::DumpError;
use lichess_db::headers::{DumpHeader, parse_dump_headers};
use lichess_db::movetext::movetext_sans;
use lichess_db::split::split_games;
use time::macros::datetime;

fn games() -> Vec<String> {
    let games = split_games(testkit::fixture("lichess-db/sample.pgn").lines());
    assert_eq!(games.len(), 5);
    games
}

fn header(i: usize) -> DumpHeader {
    parse_dump_headers(&games()[i]).expect("header")
}

#[test]
fn time_control_180_plus_2_gives_base_and_increment() {
    let h = header(0);
    assert_eq!(h.time_control, "180+2");
    assert_eq!(h.base_seconds, Some(180));
    assert_eq!(h.increment_seconds, Some(2));
    let h = header(1);
    assert_eq!((h.base_seconds, h.increment_seconds), (Some(600), Some(0)));
}

#[test]
fn lichess_game_id_comes_from_site() {
    let h = header(0);
    assert_eq!(h.site, "https://lichess.org/Fx1aB2cD");
    assert_eq!(h.lichess_game_id, "Fx1aB2cD");
    let ids: Vec<String> = (0..5).map(|i| header(i).lichess_game_id).collect();
    assert_eq!(
        ids,
        ["Fx1aB2cD", "Gy2bC3dE", "Hz3cD4eF", "Jk4dE5fG", "Lm5eF6gH"]
    );
}

/// Recent dumps (seen in a 2026-10 byte range) write `Site` as `https://lichess.org/game/<id>`.
#[test]
fn lichess_game_id_from_the_game_path_form_of_site() {
    let g = games()[0].replace(
        "[Site \"https://lichess.org/Fx1aB2cD\"]",
        "[Site \"https://lichess.org/game/Ab12Cd34\"]",
    );
    let h = parse_dump_headers(&g).unwrap();
    assert_eq!(h.site, "https://lichess.org/game/Ab12Cd34");
    assert_eq!(h.lichess_game_id, "Ab12Cd34");
}

#[test]
fn clock_and_eval_flags_follow_the_movetext() {
    let flags: Vec<(bool, bool)> = (0..5)
        .map(|i| {
            let h = header(i);
            (h.has_clock, h.has_eval)
        })
        .collect();
    assert_eq!(
        flags,
        [
            (true, true),
            (true, false),
            (true, false),
            (false, false),
            (true, false)
        ]
    );
}

#[test]
fn players_elos_result_termination_and_utc() {
    let h = header(0);
    assert_eq!(h.white, "FixtureAlice");
    assert_eq!(h.black, "FixtureBob");
    assert_eq!(h.white_elo, Some(1500));
    assert_eq!(h.black_elo, Some(1480));
    assert_eq!(h.result, "1-0");
    assert_eq!(h.termination.as_deref(), Some("Normal"));
    assert_eq!(h.utc, datetime!(2026-10-01 00:00:12 UTC));
    assert_eq!(header(1).termination.as_deref(), Some("Time forfeit"));
    assert_eq!(header(2).result, "1/2-1/2");
}

#[test]
fn correspondence_unknown_elo_and_missing_termination() {
    let g = games()[0]
        .replace("[TimeControl \"180+2\"]", "[TimeControl \"-\"]")
        .replace("[WhiteElo \"1500\"]", "[WhiteElo \"?\"]")
        .replace("[Termination \"Normal\"]\n", "");
    let h = parse_dump_headers(&g).unwrap();
    assert_eq!(h.time_control, "-");
    assert_eq!((h.base_seconds, h.increment_seconds), (None, None));
    assert_eq!(h.white_elo, None);
    assert_eq!(h.black_elo, Some(1480));
    assert_eq!(h.termination, None);
}

#[test]
fn escaped_values_crlf_and_last_duplicate_wins() {
    let pgn = "[Event \"Rated Blitz game\"]\r\n[Site \"https://lichess.org/abcdEFGH\"]\r\n\
               [White \"a\\\"b\\\\c\"]\r\n[Black \"x\"]\r\n[Black \"y\"]\r\n[Result \"*\"]\r\n\
               [UTCDate \"2026.10.31\"]\r\n[UTCTime \"23:59:59\"]\r\n[WhiteElo \"2000\"]\r\n\
               [BlackElo \"2001\"]\r\n[TimeControl \"15+0\"]\r\n\r\n1. e4 *\r\n";
    let h = parse_dump_headers(pgn).unwrap();
    assert_eq!(h.white, "a\"b\\c");
    assert_eq!(h.black, "y");
    assert_eq!(h.lichess_game_id, "abcdEFGH");
    assert_eq!(h.utc, datetime!(2026-10-31 23:59:59 UTC));
    assert_eq!((h.base_seconds, h.increment_seconds), (Some(15), Some(0)));
    assert!(!h.has_clock && !h.has_eval);
}

#[test]
fn clock_text_in_a_header_value_is_not_a_clock() {
    let g = games()[3].replace(
        "[Opening \"Vienna Game\"]",
        "[Opening \"Vienna Game [%clk 0:01:00] [%eval 0.1]\"]",
    );
    let h = parse_dump_headers(&g).unwrap();
    assert!(!h.has_clock && !h.has_eval);
}

#[test]
fn missing_or_malformed_required_headers_are_decode_errors() {
    let g = &games()[0];
    for (from, to) in [
        ("[Site \"https://lichess.org/Fx1aB2cD\"]\n", ""),
        ("[White \"FixtureAlice\"]\n", ""),
        ("[Result \"1-0\"]\n", ""),
        ("[UTCDate \"2026.10.01\"]", "[UTCDate \"2026.13.01\"]"),
        ("[UTCTime \"00:00:12\"]", "[UTCTime \"noon\"]"),
        ("[TimeControl \"180+2\"]\n", ""),
        (
            "[Site \"https://lichess.org/Fx1aB2cD\"]",
            "[Site \"https://lichess.org/\"]",
        ),
        ("[Round \"-\"]", "[Round \"-]"),
    ] {
        let broken = g.replace(from, to);
        assert_ne!(&broken, g, "replacement {from:?} did not apply");
        assert!(
            matches!(parse_dump_headers(&broken), Err(DumpError::Decode(_))),
            "{from:?} -> {to:?} should be a decode error"
        );
    }
}

#[test]
fn movetext_sans_skips_comments_variations_nags_numbers_and_results() {
    let text = "1. e4 {[%clk 0:03:00]} (1. d4 d5) 1... e5 $1 2. Nf3 *";
    assert_eq!(movetext_sans(text, 3), ["e4", "e5", "Nf3"]);
    assert_eq!(movetext_sans(text, 2), ["e4", "e5"]);
    assert_eq!(movetext_sans(text, 0), Vec::<&str>::new());
    assert_eq!(movetext_sans(text, 100), ["e4", "e5", "Nf3"]);
}

#[test]
fn movetext_sans_handles_nesting_semicolons_glued_tokens_and_suffixes() {
    let text = "1.e4 (1. d4 (1. c4 {a ) b} c5) d5 ; line ) comment\n) e5!? 2.Nf3{x}Nc6 \
                3. Bb5 $14 a6?! 4. Ba4 1-0 5. Qh5";
    assert_eq!(
        movetext_sans(text, usize::MAX),
        ["e4", "e5", "Nf3", "Nc6", "Bb5", "a6", "Ba4"]
    );
}

/// Tokens are returned as written. Old dumps (2013) over-disambiguate (`Rd1d2` where chess.js
/// writes `R1d2`); `Position::play_san` accepts both, so callers replay the raw token.
#[test]
fn movetext_sans_returns_tokens_as_written() {
    assert_eq!(
        movetext_sans("21. Rd1d2 Rc8c6 22. Nc1d3 *", 10),
        ["Rd1d2", "Rc8c6", "Nc1d3"]
    );
}

#[test]
fn movetext_sans_of_a_full_game_skips_the_headers() {
    let g = &games()[3];
    assert_eq!(
        movetext_sans(g, usize::MAX),
        ["e4", "e5", "Qh5", "Nc6", "Bc4", "Nf6", "Qxf7#"]
    );
    assert_eq!(movetext_sans(&games()[0], 4), ["e4", "e5", "Nf3", "Nc6"]);
}

/// The fixture games are legal and our SAN tokenizer agrees with R1's PGN parser on every ply.
#[test]
fn fixture_sans_match_the_chess_core_parser() {
    for g in games() {
        let parsed = parse_pgn(&g).expect("legal fixture game");
        let sans: Vec<&str> = parsed.plies.iter().map(|p| p.san.as_str()).collect();
        assert!(!sans.is_empty());
        assert_eq!(movetext_sans(&g, usize::MAX), sans);
    }
}
