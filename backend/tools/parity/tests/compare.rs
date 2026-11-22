//! `parity compare` rules (Task 24 step 3), on in-memory snapshots.

use parity::compare::{EvalRow, GameRow, MoveRow, Snapshot, diff};

fn game(id: &str) -> GameRow {
    GameRow {
        platform: "lichess".into(),
        platform_game_id: id.into(),
        analysis_status: "done".into(),
        analysis_protocol: Some("sf19-n300k".into()),
        white_accuracy: Some(80.5),
        black_accuracy: Some(70.25),
    }
}

fn mv(ply: i32) -> MoveRow {
    MoveRow {
        ply,
        color: if ply % 2 == 1 { "white" } else { "black" }.into(),
        san: "e4".into(),
        uci: "e2e4".into(),
        fen_before: "fen".into(),
        best_uci: Some("e2e4".into()),
        win_before: 50.0,
        win_after: 51.0,
        delta_w: 1.0,
        accuracy: 99.0,
        judgement: None,
        phase: "opening".into(),
        clock_before: Some(180.0),
        time_spent: Some(1.5),
        decided: false,
    }
}

fn eval(fen: &str) -> EvalRow {
    EvalRow {
        fen_key: fen.into(),
        protocol: "sf19-n300k".into(),
        score_cp: Some(20),
        score_mate: None,
        best_uci: Some("e2e4".into()),
        pv: vec!["e2e4".into(), "e7e5".into()],
        source: "stockfish".into(),
    }
}

fn snapshot() -> Snapshot {
    let mut s = Snapshot::default();
    s.insert_game(game("g1"), vec![mv(1), mv(2)]);
    s.insert_eval(eval("k1"));
    s
}

#[test]
fn identical_snapshots_have_no_mismatches_and_count_everything() {
    let r = diff(&snapshot(), &snapshot());
    assert!(r.mismatches.is_empty(), "{:?}", r.mismatches);
    assert_eq!((r.games, r.moves, r.evals), (1, 2, 1));
    assert_eq!(
        r.summary(),
        "parity: 1 games, 2 moves, 1 evals compared, 0 mismatches"
    );
}

#[test]
fn move_floats_within_1e9_match_and_beyond_mismatch() {
    let mut b = snapshot();
    b.moves_mut("lichess", "g1")[0].win_before += 5e-10;
    assert!(diff(&snapshot(), &b).mismatches.is_empty());
    b.moves_mut("lichess", "g1")[0].win_before += 5e-9;
    let r = diff(&snapshot(), &b);
    assert_eq!(r.mismatches.len(), 1);
    assert!(
        r.mismatches[0].starts_with("g1 ply 1 win_before: 50 != 50.00000000"),
        "{}",
        r.mismatches[0]
    );
}

#[test]
fn move_accuracy_tolerance_is_1e6() {
    let mut b = snapshot();
    b.moves_mut("lichess", "g1")[1].accuracy += 5e-7;
    assert!(diff(&snapshot(), &b).mismatches.is_empty());
    b.moves_mut("lichess", "g1")[1].accuracy += 1e-6;
    assert_eq!(diff(&snapshot(), &b).mismatches.len(), 1);
}

#[test]
fn exact_move_columns_report_each_difference() {
    let mut b = snapshot();
    {
        let m = &mut b.moves_mut("lichess", "g1")[1];
        m.judgement = Some("blunder".into());
        m.best_uci = None;
        m.time_spent = Some(1.6);
        m.decided = true;
    }
    let r = diff(&snapshot(), &b);
    assert_eq!(
        r.mismatches,
        vec![
            "g1 ply 2 best_uci: e2e4 != NULL",
            "g1 ply 2 judgement: NULL != blunder",
            "g1 ply 2 time_spent: 1.5 != 1.6",
            "g1 ply 2 decided: false != true",
        ]
    );
}

#[test]
fn a_different_move_count_is_one_mismatch() {
    let mut b = snapshot();
    b.moves_mut("lichess", "g1").pop();
    let r = diff(&snapshot(), &b);
    assert_eq!(r.mismatches, vec!["g1 moves count: 2 != 1"]);
}

#[test]
fn game_columns_use_their_own_tolerances() {
    let mut b = snapshot();
    b.game_mut("lichess", "g1").white_accuracy = Some(80.5 + 5e-7);
    assert!(diff(&snapshot(), &b).mismatches.is_empty());
    b.game_mut("lichess", "g1").black_accuracy = None;
    b.game_mut("lichess", "g1").analysis_status = "failed".into();
    let r = diff(&snapshot(), &b);
    assert_eq!(
        r.mismatches,
        vec![
            "g1 game analysis_status: done != failed",
            "g1 game black_accuracy: 70.25 != NULL",
        ]
    );
}

#[test]
fn games_present_on_one_side_only_are_reported() {
    let mut b = snapshot();
    b.insert_game(game("g2"), vec![]);
    let r = diff(&snapshot(), &b);
    assert_eq!(r.mismatches, vec!["g2 game: missing != present"]);
}

#[test]
fn eval_key_sets_and_columns_are_compared_exactly() {
    let mut b = snapshot();
    b.insert_eval(eval("k2"));
    b.eval_mut("k1", "sf19-n300k").pv = vec!["e2e4".into()];
    b.eval_mut("k1", "sf19-n300k").source = "lichess".into();
    let r = diff(&snapshot(), &b);
    assert_eq!(
        r.mismatches,
        vec![
            "eval k1 [sf19-n300k] pv: {e2e4,e7e5} != {e2e4}",
            "eval k1 [sf19-n300k] source: stockfish != lichess",
            "eval k2 [sf19-n300k]: missing != present",
        ]
    );
    assert_eq!(r.evals, 2);
}
