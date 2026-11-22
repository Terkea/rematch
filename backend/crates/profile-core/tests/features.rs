//! R2 Task 6: per-move features and enriched-move assembly (TS M2 Task 5 values, plus the R2
//! honesty rules: no substituted values, invalid inputs skipped and counted).

mod common;

use chess_core::board::Position;
use chess_core::move_record::build_move_records;
use chess_core::pgn::parse_pgn;
use chess_core::score::{flip_score, win_percent};
use chess_core::{ClockInfo, Color, EngineScore, MoveRecord, PositionEval, Terminal};
use common::positions::line;
use profile_core::board::null_move_fen;
use profile_core::features::{
    DIFFICULTY_MIN_PLY, assemble_enriched_moves, assemble_enriched_moves_counted,
    derive_move_features, first_capture_square, is_maia_model_id, maia_difficulty,
};
use profile_core::motifs::tag_line;
use profile_core::types::{Difficulty, MoveFeatures, MultipvLine, RawMoveFeatures, ThreatProbe};

const THREAT_FEN: &str = "4k3/8/8/r7/8/8/8/R3K3 b - - 0 1";

fn raw(ply: u32) -> RawMoveFeatures {
    RawMoveFeatures {
        ply,
        multipv: None,
        best_pv: Vec::new(),
        refutation_pv: Vec::new(),
        threat: None,
        tb_wdl_before: None,
        tb_wdl_after: None,
        difficulty: None,
    }
}

fn fen_after(fen: &str, uci: &str) -> String {
    Position::from_fen(fen)
        .unwrap()
        .play_uci(uci)
        .unwrap()
        .fen()
}

fn threat_raw(refutation: &[&str]) -> RawMoveFeatures {
    RawMoveFeatures {
        refutation_pv: line(refutation),
        threat: Some(ThreatProbe {
            null_fen: null_move_fen(THREAT_FEN).unwrap(),
            pv: line(&["a1a5"]),
            score: EngineScore::Cp(500),
        }),
        ..raw(1)
    }
}

fn threat_features(played: &str, refutation: &[&str]) -> MoveFeatures {
    let after = fen_after(THREAT_FEN, played);
    derive_move_features(
        Color::Black,
        THREAT_FEN,
        &after,
        played,
        Some(&threat_raw(refutation)),
    )
}

fn mpv(cps: &[i32]) -> Vec<MultipvLine> {
    let moves = ["e2e4", "d2d4", "g1f3", "c2c4"];
    cps.iter()
        .zip(moves)
        .map(|(&cp, uci)| MultipvLine {
            uci: uci.to_owned(),
            score: EngineScore::Cp(cp),
            pv: line(&[uci]),
        })
        .collect()
}

fn mpv_features(cps: &[i32], played: &str) -> MoveFeatures {
    let start = chess_core::START_FEN;
    let r = RawMoveFeatures {
        multipv: Some(mpv(cps)),
        ..raw(1)
    };
    derive_move_features(
        Color::White,
        start,
        &fen_after(start, played),
        played,
        Some(&r),
    )
}

// --- derive_move_features ---------------------------------------------------------------

#[test]
fn an_unaddressed_threat_existed_and_was_not_addressed() {
    let f = threat_features("e8e7", &["a1a5"]);
    assert_eq!(f.threat_existed, Some(true));
    assert_eq!(f.threat_addressed, Some(false));
    let threat = f.threat.unwrap();
    assert_eq!(threat.swing.gain, 5.0);
    assert_eq!(threat.swing.depth, Some(1));
}

#[test]
fn a_threat_met_by_the_move_is_addressed() {
    let f = threat_features("a5a1", &["e1d2"]);
    assert_eq!(f.threat_existed, Some(true));
    assert_eq!(f.threat_addressed, Some(true));
}

#[test]
fn a_different_first_move_capturing_on_the_threat_square_is_not_addressed() {
    // Black ignores the threat (Kd8); White's refutation starts with a king move, then takes
    // on a5: a different first move, but the same first capture square.
    let fen = THREAT_FEN;
    let after = fen_after(fen, "e8d8");
    let r = threat_raw(&["e1d2", "d8c8", "a1a5"]);
    let f = derive_move_features(Color::Black, fen, &after, "e8d8", Some(&r));
    assert_eq!(f.refutation.as_ref().unwrap().swing.depth, Some(3));
    assert_eq!(f.threat_existed, Some(true));
    assert_eq!(f.threat_addressed, Some(false));
    // First capture square: the first capture by the line's side within its swing depth.
    let fcs = |pv: &[&str]| {
        let pv = line(pv);
        let a = tag_line(&after, &pv, Color::White, 8);
        first_capture_square(&after, &pv, Color::White, &a)
    };
    assert_eq!(fcs(&["a1a5"]).as_deref(), Some("a5"));
    assert_eq!(fcs(&["e1d2"]), None);
}

#[test]
fn multipv_with_one_good_move_is_critical() {
    let f = mpv_features(&[300, 20, 10], "e2e4");
    assert_eq!(f.is_critical, Some(true));
    assert_eq!(f.playable, Some(1));
    let gap = f.multipv_gap.unwrap();
    let want = win_percent(EngineScore::Cp(300)) - win_percent(EngineScore::Cp(20));
    assert!((gap - want).abs() < 1e-12, "{gap} vs {want}");
    assert_eq!(f.in_top2, Some(true));
}

#[test]
fn multipv_with_three_close_moves_is_not_critical() {
    let f = mpv_features(&[30, 25, 20], "d2d4");
    assert_eq!(f.is_critical, Some(false));
    assert_eq!(f.playable, Some(3));
    assert_eq!(f.in_top2, Some(true), "played lines[1].uci");
    let third = mpv_features(&[30, 25, 20], "g1f3");
    assert_eq!(third.in_top2, Some(false));
}

#[test]
fn multipv_lines_are_ranked_by_win_percent() {
    // Lines out of engine order still give a non-negative gap from the best.
    let f = mpv_features(&[20, 300, 10], "d2d4");
    assert!(f.multipv_gap.unwrap() > 15.0);
    assert_eq!(f.is_critical, Some(true));
    assert_eq!(f.in_top2, Some(true));
}

#[test]
fn a_single_multipv_line_has_no_gap() {
    let f = mpv_features(&[30], "e2e4");
    assert_eq!(f.multipv_gap, None);
    assert_eq!(f.is_critical, None);
    assert_eq!(f.playable, Some(1));
    assert_eq!(f.in_top2, Some(true));
}

#[test]
fn no_raw_gives_no_features() {
    let start = chess_core::START_FEN;
    let f = derive_move_features(Color::White, start, &fen_after(start, "e2e4"), "e2e4", None);
    assert_eq!(f.is_critical, None);
    assert_eq!(f.best, None);
    assert_eq!(f.threat_existed, None);
    assert_eq!(f, MoveFeatures::default());
}

#[test]
fn no_multipv_leaves_the_multipv_features_none() {
    let f = threat_features("e8e7", &["a1a5"]);
    assert_eq!(f.is_critical, None);
    assert_eq!(f.multipv_gap, None);
    assert_eq!(f.playable, None);
    assert_eq!(f.in_top2, None);
    let empty = RawMoveFeatures {
        multipv: Some(Vec::new()),
        ..raw(1)
    };
    let start = chess_core::START_FEN;
    let g = derive_move_features(
        Color::White,
        start,
        &fen_after(start, "e2e4"),
        "e2e4",
        Some(&empty),
    );
    assert_eq!((g.is_critical, g.playable, g.in_top2), (None, None, None));
}

#[test]
fn best_and_refutation_lines_are_tagged_for_the_right_side() {
    // Black's best line wins the rook on a1; White's refutation of Ke7 wins the rook on a5.
    let mut r = threat_raw(&["a1a5"]);
    r.best_pv = line(&["a5a1", "e1d2"]);
    let after = fen_after(THREAT_FEN, "e8e7");
    let f = derive_move_features(Color::Black, THREAT_FEN, &after, "e8e7", Some(&r));
    assert_eq!(f.best.unwrap().swing.gain, 5.0);
    assert_eq!(f.refutation.unwrap().swing.gain, 5.0);
}

#[test]
fn an_empty_pv_is_unknown_not_a_quiet_line() {
    // Lichess evals carry no PV: no line, so nothing is claimed about it.
    let after = fen_after(THREAT_FEN, "e8e7");
    let f = derive_move_features(
        Color::Black,
        THREAT_FEN,
        &after,
        "e8e7",
        Some(&threat_raw(&[])),
    );
    assert_eq!(f.best, None);
    assert_eq!(f.refutation, None);
    assert_eq!(f.threat_existed, Some(true));
    assert_eq!(f.threat_addressed, None, "no refutation line, so unknown");
}

#[test]
fn an_illegal_pv_or_null_fen_is_unknown() {
    let after = fen_after(THREAT_FEN, "e8e7");
    let mut r = threat_raw(&["h1h8"]);
    r.best_pv = line(&["zz99"]);
    let f = derive_move_features(Color::Black, THREAT_FEN, &after, "e8e7", Some(&r));
    assert_eq!(f.best, None);
    assert_eq!(f.refutation, None);

    let mut bad = threat_raw(&["a1a5"]);
    bad.threat.as_mut().unwrap().null_fen = "not a fen".to_owned();
    let g = derive_move_features(Color::Black, THREAT_FEN, &after, "e8e7", Some(&bad));
    assert_eq!(g.threat, None);
    assert_eq!(g.threat_existed, None);
    assert_eq!(g.threat_addressed, None);
}

#[test]
fn no_threat_probe_gives_no_threat_features() {
    let mut r = threat_raw(&["a1a5"]);
    r.threat = None;
    let after = fen_after(THREAT_FEN, "e8e7");
    let f = derive_move_features(Color::Black, THREAT_FEN, &after, "e8e7", Some(&r));
    assert_eq!(f.threat, None);
    assert_eq!(f.threat_existed, None);
    assert_eq!(f.threat_addressed, None);
    assert!(f.refutation.is_some());
}

#[test]
fn a_harmless_threat_line_did_not_exist_and_counts_as_addressed() {
    let mut r = threat_raw(&["a1a5"]);
    r.threat.as_mut().unwrap().pv = line(&["e1d2"]);
    let after = fen_after(THREAT_FEN, "e8e7");
    let f = derive_move_features(Color::Black, THREAT_FEN, &after, "e8e7", Some(&r));
    assert_eq!(f.threat_existed, Some(false));
    assert_eq!(f.threat_addressed, Some(true));
}

// --- difficulty (owner override: Maia only) --------------------------------------------

fn diff(model: &str, p_best: f64, p_ok: f64) -> Difficulty {
    Difficulty {
        p_best,
        p_ok,
        model: model.to_owned(),
    }
}

#[test]
fn difficulty_is_maia_only_and_never_in_the_opening_plies() {
    assert_eq!(DIFFICULTY_MIN_PLY, 11);
    let maia = diff("maia2-rapid-1.0", 0.4, 0.9);
    assert_eq!(maia_difficulty(11, Some(&maia)), Ok(Some(maia.clone())));
    assert_eq!(maia_difficulty(10, Some(&maia)), Ok(None), "plies <= 10");
    assert_eq!(maia_difficulty(30, None), Ok(None), "Maia down");
    assert!(maia_difficulty(30, Some(&diff("spread-v1", 0.5, 0.6))).is_err());
    assert!(maia_difficulty(30, Some(&diff("maia2-rapid-1.0", f64::NAN, 0.6))).is_err());
    assert!(maia_difficulty(30, Some(&diff("maia2-rapid-1.0", 0.5, 1.5))).is_err());
    assert!(maia_difficulty(30, Some(&diff("maia2-rapid-1.0", 0.5, f64::INFINITY))).is_err());
}

// --- assemble_enriched_moves ------------------------------------------------------------

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

/// R1's Scholar's-mate evals (`chess-core/tests/move_record.rs`).
fn scholars_evals() -> Vec<PositionEval> {
    let mut e: Vec<PositionEval> = [20, 30, 25, 30, 40, 50]
        .iter()
        .map(|&c| ev(EngineScore::Cp(c), None))
        .collect();
    e.push(ev(EngineScore::Mate(1), Some("h5f7")));
    e.push(ev(EngineScore::Cp(1000), None));
    e
}

fn scholars() -> (chess_core::ParsedGame, Vec<MoveRecord>, Vec<PositionEval>) {
    let game = parse_pgn(&fixture("scholars-mate.pgn")).unwrap();
    let evals = scholars_evals();
    let clock = Some(ClockInfo {
        base_seconds: 180.0,
        increment_seconds: 0.0,
    });
    let records = build_move_records(&game, &evals, clock).unwrap();
    (game, records, evals)
}

#[test]
fn assembles_scholars_mate() {
    let (game, records, evals) = scholars();
    let raws = vec![None; records.len()];
    let moves = assemble_enriched_moves(&records, &evals, &raws, game.terminal);
    assert_eq!(moves.len(), 7);
    assert_eq!(moves[6].score_after, EngineScore::Mate(1));
    assert_eq!(moves[1].score_before, flip_score(evals[1].score));
    assert_eq!(moves[0].score_before, evals[0].score);
    assert_eq!(moves[5].score_after, flip_score(evals[6].score));
    assert_eq!(moves[0].fen_after, moves[1].rec.fen_before);
    // The final ply's FEN is replayed from its own move.
    assert_eq!(moves[6].fen_after, game.plies[6].fen_after);
    assert!(
        moves
            .iter()
            .all(|m| m.raw.is_none() && m.f == MoveFeatures::default())
    );
}

#[test]
fn no_checkmate_terminal_keeps_the_engine_score() {
    let (_, records, evals) = scholars();
    let moves = assemble_enriched_moves(&records, &evals, &[], None);
    assert_eq!(moves[6].score_after, EngineScore::Cp(1000));
    let mated = assemble_enriched_moves(&records, &evals, &[], Some(Terminal::Draw));
    assert_eq!(mated[6].score_after, EngineScore::Cp(1000));
}

#[test]
fn raw_features_are_attached_to_their_ply() {
    let (game, records, evals) = scholars();
    let mut raws: Vec<Option<RawMoveFeatures>> = vec![None; records.len()];
    // Ply 2 (black, ...e5): MultiPV in black's POV.
    raws[1] = Some(RawMoveFeatures {
        multipv: Some(vec![
            MultipvLine {
                uci: "e7e5".to_owned(),
                score: EngineScore::Cp(-30),
                pv: line(&["d7d6"]),
            },
            MultipvLine {
                uci: "c7c5".to_owned(),
                score: EngineScore::Cp(-35),
                pv: line(&["c7c5"]),
            },
        ]),
        ..raw(2)
    });
    let counted = assemble_enriched_moves_counted(&records, &evals, &raws, game.terminal);
    assert_eq!(counted.skipped, Default::default());
    let m = &counted.moves[1];
    assert_eq!(m.raw, raws[1]);
    assert_eq!(m.f.in_top2, Some(true));
    assert_eq!(m.f.playable, Some(2));
    assert_eq!(m.f.is_critical, Some(false));
}

#[test]
fn invalid_inputs_are_skipped_and_counted() {
    let (game, records, evals) = scholars();
    let mut raws: Vec<Option<RawMoveFeatures>> = vec![None; records.len()];
    raws[0] = Some(raw(5)); // wrong ply
    raws[2] = Some(RawMoveFeatures {
        difficulty: Some(diff("spread-v1", 0.5, 0.5)),
        ..raw(3)
    });
    raws[3] = Some(RawMoveFeatures {
        difficulty: Some(diff("maia2-rapid-1.0", 0.5, 0.5)),
        ..raw(4)
    });
    // Evals one short: the final ply has no eval after it.
    let short = &evals[..evals.len() - 1];
    let counted = assemble_enriched_moves_counted(&records, short, &raws, game.terminal);
    assert_eq!(counted.moves.len(), 6);
    assert_eq!(counted.skipped.moves_without_evals, 1);
    assert_eq!(counted.skipped.raw_ply_mismatch, 1);
    assert_eq!(counted.skipped.difficulty_rejected, 1);
    assert_eq!(counted.moves[0].raw, None);
    assert_eq!(counted.moves[2].raw.as_ref().unwrap().difficulty, None);
    // A valid Maia result before ply 11 is dropped by rule, not counted as invalid.
    assert_eq!(counted.moves[3].raw.as_ref().unwrap().difficulty, None);
}

#[test]
fn an_unplayable_final_move_is_skipped_and_counted() {
    let (game, mut records, evals) = scholars();
    records[6].uci = "a1a8".to_owned();
    let counted = assemble_enriched_moves_counted(&records, &evals, &[], game.terminal);
    assert_eq!(counted.moves.len(), 6);
    assert_eq!(counted.skipped.fen_after_unplayable, 1);
}

#[test]
fn a_maia_difficulty_past_the_opening_is_kept() {
    let (_, mut records, mut evals) = scholars();
    // Renumber so the last ply counts as ply 11.
    for r in &mut records {
        r.ply += 4;
    }
    evals.truncate(8);
    let d = diff("maia2-blitz-2", 0.25, 0.75);
    let mut raws: Vec<Option<RawMoveFeatures>> = vec![None; records.len()];
    raws[6] = Some(RawMoveFeatures {
        difficulty: Some(d.clone()),
        ..raw(11)
    });
    let moves = assemble_enriched_moves(&records, &evals, &raws, None);
    assert_eq!(moves[6].raw.as_ref().unwrap().difficulty, Some(d));
}

#[test]
fn features_never_hold_non_finite_numbers() {
    for cps in [[i32::MAX, i32::MIN, 0], [0, 0, 0]] {
        let f = mpv_features(&cps, "e2e4");
        assert!(f.multipv_gap.unwrap().is_finite());
        serde_json::to_string(&f).unwrap();
    }
}

// --- fix round 1: exact Maia ids and input validation ----------------------------------

#[test]
fn maia_is_recognised_by_its_exact_model_id() {
    for ok in ["maia2-rapid-1", "maia2-blitz-0.11.0", "maia2-rapid-1.0"] {
        assert!(is_maia_model_id(ok), "{ok}");
    }
    for bad in [
        "maia",
        "maia-fake",
        "maiaspread-v1",
        "maia1-1500",
        "maia2-classical-0.11.0",
        "maia2-rapid-",
        "maia2-rapid-1.",
        "maia2-rapid-.1",
        "maia2-rapid-1..2",
        "maia2-rapid-v1",
        "maia2-rapid-1 ",
        "spread-v1",
        "",
    ] {
        assert!(!is_maia_model_id(bad), "{bad:?}");
        assert!(
            maia_difficulty(30, Some(&diff(bad, 0.2, 0.5))).is_err(),
            "{bad:?}"
        );
    }
}

#[test]
fn p_best_above_p_ok_is_rejected_and_counted() {
    assert!(maia_difficulty(30, Some(&diff("maia2-rapid-1", 0.6, 0.5))).is_err());
    assert!(maia_difficulty(30, Some(&diff("maia2-rapid-1", 0.5 + 1e-12, 0.5))).is_ok());
    let (_, mut records, evals) = scholars();
    for r in &mut records {
        r.ply += 4;
    }
    let mut raws: Vec<Option<RawMoveFeatures>> = vec![None; records.len()];
    raws[6] = Some(RawMoveFeatures {
        difficulty: Some(diff("maia2-rapid-1", 0.6, 0.5)),
        ..raw(11)
    });
    let counted = assemble_enriched_moves_counted(&records, &evals, &raws, None);
    assert_eq!(counted.skipped.difficulty_rejected, 1);
    assert_eq!(counted.moves[6].raw.as_ref().unwrap().difficulty, None);
}

#[test]
fn out_of_range_evals_are_skipped_and_counted() {
    let (game, records, mut evals) = scholars();
    evals[2].score = EngineScore::Cp(i32::MIN); // before ply 3, after ply 2
    evals[4].score = EngineScore::Mate(1001); // before ply 5, after ply 4
    let counted = assemble_enriched_moves_counted(&records, &evals, &[], game.terminal);
    assert_eq!(counted.skipped.evals_out_of_range, 4);
    let plies: Vec<u32> = counted.moves.iter().map(|m| m.rec.ply).collect();
    assert_eq!(plies, vec![1, 6, 7]);
    // The bounds themselves are in range.
    let (_, records, mut evals) = scholars();
    evals[2].score = EngineScore::Cp(-100_000);
    evals[4].score = EngineScore::Mate(-1000);
    let ok = assemble_enriched_moves_counted(&records, &evals, &[], None);
    assert_eq!(ok.skipped, Default::default());
}

#[test]
fn a_threat_probe_for_another_position_is_dropped_and_counted() {
    let mut r = threat_raw(&["a1a5"]);
    r.threat.as_mut().unwrap().null_fen = "4k3/8/8/r7/8/8/8/R3K3 w - - 0 2".to_owned();
    let after = fen_after(THREAT_FEN, "e8e7");
    let f = derive_move_features(Color::Black, THREAT_FEN, &after, "e8e7", Some(&r));
    assert_eq!(
        (f.threat, f.threat_existed, f.threat_addressed),
        (None, None, None)
    );

    let (game, records, evals) = scholars();
    let mut raws: Vec<Option<RawMoveFeatures>> = vec![None; records.len()];
    raws[1] = Some(RawMoveFeatures {
        threat: Some(ThreatProbe {
            null_fen: null_move_fen(THREAT_FEN).unwrap(),
            pv: line(&["a1a5"]),
            score: EngineScore::Cp(500),
        }),
        ..raw(2)
    });
    raws[2] = Some(RawMoveFeatures {
        threat: Some(ThreatProbe {
            null_fen: null_move_fen(&records[2].fen_before).unwrap(),
            pv: line(&["d7d6"]),
            score: EngineScore::Cp(0),
        }),
        ..raw(3)
    });
    let counted = assemble_enriched_moves_counted(&records, &evals, &raws, game.terminal);
    assert_eq!(counted.skipped.threat_fen_mismatch, 1);
    assert_eq!(counted.moves[1].raw.as_ref().unwrap().threat, None);
    assert!(counted.moves[2].raw.as_ref().unwrap().threat.is_some());
}

#[test]
fn raw_entries_beyond_the_records_are_counted() {
    let (game, records, evals) = scholars();
    let mut raws: Vec<Option<RawMoveFeatures>> = vec![None; records.len()];
    raws.push(Some(raw(8)));
    raws.push(Some(raw(9)));
    raws.push(None);
    let counted = assemble_enriched_moves_counted(&records, &evals, &raws, game.terminal);
    assert_eq!(counted.skipped.raw_beyond_records, 2);
    assert_eq!(counted.moves.len(), 7);
}

#[test]
fn moves_with_an_unparseable_fen_are_skipped_and_counted() {
    let (game, mut records, evals) = scholars();
    records[3].fen_before = "garbage".to_owned();
    let counted = assemble_enriched_moves_counted(&records, &evals, &[], game.terminal);
    // Ply 4 has no valid fen_before, and ply 3's fen_after is that same garbage.
    assert_eq!(counted.skipped.moves_with_invalid_fen, 2);
    let plies: Vec<u32> = counted.moves.iter().map(|m| m.rec.ply).collect();
    assert_eq!(plies, vec![1, 2, 5, 6, 7]);
}
