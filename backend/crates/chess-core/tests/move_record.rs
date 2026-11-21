//! Port of `packages/chess-core/test/move-record.test.ts`, plus the TS-review fixes.

use chess_core::move_record::{
    DECIDED_THRESHOLD, MoveRecordError, build_move_records, build_move_records_with_book_exit,
};
use chess_core::pgn::parse_pgn;
use chess_core::{ClockInfo, Color, EngineScore, Judgement, MoveRecord, Phase, PositionEval};

fn fixture(name: &str) -> String {
    let path = format!("{}/../../fixtures/pgn/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn ev(score: EngineScore, best: Option<&str>) -> PositionEval {
    PositionEval {
        score,
        best_uci: best.map(str::to_string),
        pv: best.map(|b| vec![b.to_string()]).unwrap_or_default(),
    }
}

fn cps(values: &[i32]) -> Vec<PositionEval> {
    values
        .iter()
        .map(|&c| ev(EngineScore::Cp(c), None))
        .collect()
}

fn scholars_evals() -> Vec<PositionEval> {
    let mut e = cps(&[20, 30, 25, 30, 40, 50]);
    e.push(ev(EngineScore::Mate(1), Some("h5f7")));
    e.push(ev(EngineScore::Cp(1000), None));
    e
}

fn clock(base: f64, inc: f64) -> Option<ClockInfo> {
    Some(ClockInfo {
        base_seconds: base,
        increment_seconds: inc,
    })
}

fn scholars_records() -> Vec<MoveRecord> {
    let game = parse_pgn(&fixture("scholars-mate.pgn")).unwrap();
    build_move_records(&game, &scholars_evals(), clock(180.0, 0.0)).unwrap()
}

#[test]
fn builds_one_record_per_ply() {
    let r = scholars_records();
    assert_eq!(r.len(), 7);
    assert_eq!((r[0].ply, r[0].color), (1, Color::White));
    assert_eq!((r[0].san.as_str(), r[0].uci.as_str()), ("e4", "e2e4"));
    assert_eq!(r[0].phase, Phase::Opening);
}

#[test]
fn judges_the_blunder_that_allows_mate_from_the_movers_point_of_view() {
    let nf6 = &scholars_records()[5];
    assert_eq!(nf6.san, "Nf6");
    assert_eq!(nf6.color, Color::Black);
    assert_eq!(nf6.judgement, Some(Judgement::Blunder));
    assert!(nf6.delta_w > 40.0);
    assert!((nf6.win_before - 45.4).abs() < 0.05, "{}", nf6.win_before);
    assert!(!nf6.decided);
}

#[test]
fn does_not_penalise_the_mating_move_and_marks_it_decided() {
    let mate = &scholars_records()[6];
    assert_eq!(mate.judgement, None);
    assert_eq!(mate.delta_w, 0.0);
    assert_eq!(mate.accuracy, 100.0);
    assert!(mate.decided);
    assert_eq!(mate.best_uci.as_deref(), Some("h5f7"));
}

#[test]
fn computes_time_spent_from_the_previous_clock_of_the_same_colour() {
    let r = scholars_records();
    assert_eq!(r[0].clock_before, Some(180.0));
    assert_eq!(r[0].time_spent, Some(2.0));
    assert_eq!(r[2].clock_before, Some(178.0));
    assert_eq!(r[2].time_spent, Some(3.0));
}

#[test]
fn adds_the_increment_back_to_time_spent() {
    let game = parse_pgn(&fixture("scholars-mate.pgn")).unwrap();
    let r = build_move_records(&game, &scholars_evals(), clock(180.0, 2.0)).unwrap();
    assert_eq!(r[0].time_spent, Some(4.0));
}

#[test]
fn time_spent_with_increment_uses_base_for_the_first_move_of_each_colour() {
    let game = parse_pgn(&fixture("scholars-mate.pgn")).unwrap();
    let r = build_move_records(&game, &scholars_evals(), clock(180.0, 2.0)).unwrap();
    // White: 180 -> 178 (+2), then 178 -> 175 (+2).
    assert_eq!(
        (r[0].clock_before, r[0].time_spent),
        (Some(180.0), Some(4.0))
    );
    assert_eq!(
        (r[2].clock_before, r[2].time_spent),
        (Some(178.0), Some(5.0))
    );
    // Black also starts from base: 180 -> 177 (+2), then 177 -> 174 (+2).
    assert_eq!(
        (r[1].clock_before, r[1].time_spent),
        (Some(180.0), Some(5.0))
    );
    assert_eq!(
        (r[3].clock_before, r[3].time_spent),
        (Some(177.0), Some(5.0))
    );
}

#[test]
fn leaves_clock_fields_none_without_clock_info() {
    let game = parse_pgn(&fixture("scholars-mate.pgn")).unwrap();
    let r = build_move_records(&game, &scholars_evals(), None).unwrap();
    assert!(
        r.iter()
            .all(|r| r.clock_before.is_none() && r.time_spent.is_none())
    );
}

#[test]
fn leaves_time_spent_none_for_games_without_clock_comments() {
    let game = parse_pgn(&fixture("no-clocks.pgn")).unwrap();
    let r = build_move_records(&game, &scholars_evals(), clock(180.0, 0.0)).unwrap();
    assert!(r.iter().all(|r| r.time_spent.is_none()));
}

#[test]
fn clock_before_is_none_when_clock_config_given_but_pgn_has_no_clocks() {
    let game = parse_pgn(&fixture("no-clocks.pgn")).unwrap();
    let r = build_move_records(&game, &scholars_evals(), clock(180.0, 0.0)).unwrap();
    assert!(r.iter().all(|r| r.clock_before.is_none()));
}

#[test]
fn rejects_an_evals_array_of_the_wrong_length() {
    let game = parse_pgn(&fixture("scholars-mate.pgn")).unwrap();
    let e = scholars_evals();
    assert_eq!(
        build_move_records(&game, &e[1..], None),
        Err(MoveRecordError::WrongEvalCount {
            expected: 8,
            got: 7
        })
    );
}

#[test]
fn checkmate_by_black_on_the_final_ply_is_not_judged() {
    let game = parse_pgn("[Result \"0-1\"]\n\n1. f3 e5 2. g4 Qh4# 0-1").expect("parses");
    assert_eq!(game.plies.len(), 4);
    // A misleading final eval (white +500) must be ignored: black delivered mate.
    let mut e = cps(&[0, 0, 0, -1000]);
    e.push(ev(EngineScore::Cp(500), None));
    let r = build_move_records(&game, &e, None).unwrap();
    let mate = &r[3];
    assert_eq!(mate.color, Color::Black);
    assert_eq!(mate.judgement, None);
    assert_eq!(mate.delta_w, 0.0);
    assert_eq!(mate.accuracy, 100.0);
    assert!(mate.win_after > 97.0, "{}", mate.win_after);
    assert!(mate.decided);
}

#[test]
fn resignation_does_not_get_the_checkmate_override() {
    let game = parse_pgn("[Result \"1-0\"]\n\n1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 1-0").unwrap();
    assert_eq!(game.terminal, None);
    // Final eval is white +1000: black's last move is a blunder, not a mate.
    let mut e = cps(&[0; 6]);
    e.push(ev(EngineScore::Cp(1000), None));
    let r = build_move_records(&game, &e, None).unwrap();
    let last = r.last().unwrap();
    assert_eq!(last.color, Color::Black);
    assert_eq!(last.judgement, Some(Judgement::Blunder));
    assert!(last.delta_w > 40.0);
}

#[test]
fn win_percent_is_from_the_movers_point_of_view() {
    let game = parse_pgn(&fixture("no-clocks.pgn")).unwrap();
    let r = build_move_records(&game, &cps(&[100; 8]), None).unwrap();
    // White +100cp is above 50% for white and below 50% for black.
    assert!(r[0].win_before > 50.0);
    assert!(r[1].win_before < 50.0);
    assert!((r[0].win_before + r[1].win_before - 100.0).abs() < 1e-9);
}

#[test]
fn delta_w_is_never_negative() {
    let game = parse_pgn(&fixture("no-clocks.pgn")).unwrap();
    // White's eval rises on ply 1, so win_after > win_before and deltaW clamps to 0.
    let r = build_move_records(&game, &scholars_evals(), None).unwrap();
    assert!(r[0].win_after > r[0].win_before);
    assert_eq!(r[0].delta_w, 0.0);
    assert!(r.iter().all(|r| r.delta_w >= 0.0));
}

#[test]
fn decided_threshold_is_forty_points_from_even() {
    assert_eq!(DECIDED_THRESHOLD, 40.0);
    let game = parse_pgn(&fixture("no-clocks.pgn")).unwrap();
    // 597cp is just over 90%, 596cp just under.
    let at = |cp: i32, ply: usize| {
        let mut e = cps(&[0; 8]);
        e[ply] = ev(EngineScore::Cp(cp), None);
        build_move_records(&game, &e, None).unwrap()[ply].clone()
    };
    assert!(at(597, 0).decided);
    assert!(!at(596, 0).decided);
    // Black to move: white -597 is +597 for black.
    assert!(at(-597, 1).decided);
    assert!(!at(-596, 1).decided);
    // Lost positions count too.
    assert!(at(-597, 0).decided);
}

#[test]
fn book_exit_ply_is_plumbed_through_to_phases() {
    let moves = "1. Nf3 Nf6 2. Ng1 Ng8 3. Nf3 Nf6 4. Ng1 Ng8 5. Nf3 Nf6 6. Ng1 Ng8 \
                 7. Nf3 Nf6 8. Ng1 Ng8 9. Nf3 Nf6 10. Ng1 Ng8 11. Nf3 Nf6 12. Ng1 Ng8 \
                 13. Nf3 Nf6 14. Ng1 Ng8 15. Nf3 Nf6 16. Ng1 Ng8 *";
    let game = parse_pgn(moves).unwrap();
    assert_eq!(game.plies.len(), 32);
    let e = cps(&[0; 33]);
    let default = build_move_records(&game, &e, None).unwrap();
    assert_eq!(default[19].phase, Phase::Opening);
    assert_eq!(default[20].phase, Phase::Middlegame);
    let late = build_move_records_with_book_exit(&game, &e, None, 26).unwrap();
    assert_eq!(late[25].phase, Phase::Opening);
    assert_eq!(late[26].phase, Phase::Middlegame);
    // Capped at 30.
    let capped = build_move_records_with_book_exit(&game, &e, None, 99).unwrap();
    assert_eq!(capped[29].phase, Phase::Opening);
    assert_eq!(capped[30].phase, Phase::Middlegame);
}

#[test]
fn parse_pgn_output_is_the_canonical_parsed_game() {
    fn takes_canonical(game: &chess_core::ParsedGame) -> usize {
        game.plies.len()
    }
    let game = parse_pgn(&fixture("scholars-mate.pgn")).unwrap();
    assert_eq!(takes_canonical(&game), 7);
    let r = build_move_records(&game, &scholars_evals(), None).unwrap();
    assert_eq!(r.len(), 7);
}

#[test]
fn header_last_wins_for_duplicate_tags() {
    let game = parse_pgn("[Event \"A\"]\n[Event \"B\"]\n\n1. e4 *").unwrap();
    assert_eq!(game.header("Event"), Some("B"));
    assert_eq!(game.headers.len(), 2);
}
