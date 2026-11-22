//! R2 Task 25: critical-ply selection (TS M2 Task 22 `selectCriticalPlies`).

use chess_core::{Color, MoveRecord, Phase};
use enrich::critical::select_critical_plies;
use enrich::protocol::{
    ENRICHMENT_PROTOCOL, MAX_CRITICAL_PLIES, MULTIPV, MULTIPV_PROTOCOL, THREAT_NODES,
    THREAT_PROTOCOL,
};

fn rec(ply: u32, delta_w: f64) -> MoveRecord {
    MoveRecord {
        ply,
        color: if ply % 2 == 1 {
            Color::White
        } else {
            Color::Black
        },
        san: "e4".into(),
        uci: "e2e4".into(),
        fen_before: chess_core::START_FEN.into(),
        best_uci: None,
        win_before: 50.0,
        win_after: 50.0 - delta_w,
        delta_w,
        accuracy: 100.0,
        judgement: None,
        phase: Phase::Middlegame,
        clock_before: None,
        time_spent: None,
        decided: false,
    }
}

/// Plies 1..=7 with ΔW {3:6, 4:0, 5:12, 6:2, 7:1} and 0 elsewhere.
fn ts_records() -> Vec<MoveRecord> {
    let dw = |p: u32| match p {
        3 => 6.0,
        5 => 12.0,
        6 => 2.0,
        7 => 1.0,
        _ => 0.0,
    };
    (1..=7).map(|p| rec(p, dw(p))).collect()
}

const BOTH: &[Color] = &[Color::White, Color::Black];

#[test]
fn protocol_ids_are_exact() {
    assert_eq!(ENRICHMENT_PROTOCOL, "enr-v1");
    assert_eq!(MULTIPV_PROTOCOL, "sf19-n300k-mpv3-v1");
    assert_eq!(THREAT_PROTOCOL, "sf19-n50k-null-v1");
    assert_eq!(THREAT_NODES, 50_000);
    assert_eq!(MULTIPV, 3);
    assert_eq!(MAX_CRITICAL_PLIES, 30);
}

#[test]
fn selects_swings_and_the_ply_after_a_big_swing() {
    assert_eq!(
        select_critical_plies(&ts_records(), BOTH, 30),
        vec![3, 5, 6]
    );
}

#[test]
fn over_max_keeps_the_highest_delta_w() {
    assert_eq!(select_critical_plies(&ts_records(), BOTH, 2), vec![3, 5]);
}

#[test]
fn thresholds_are_inclusive() {
    // ΔW exactly 5 selects the ply; ΔW exactly 10 selects the next one too.
    let records = vec![
        rec(1, 4.999),
        rec(2, 5.0),
        rec(3, 10.0),
        rec(4, 0.0),
        rec(5, 9.999),
        rec(6, 0.0),
    ];
    // Ply 5 (ΔW 9.999) is critical itself, but ply 6 does not follow it.
    assert_eq!(select_critical_plies(&records, BOTH, 30), vec![2, 3, 4, 5]);
}

#[test]
fn only_requested_colours_are_selected() {
    // White plies are odd: 3 and 5 (6 is Black, selected only through ply 5's swing).
    assert_eq!(
        select_critical_plies(&ts_records(), &[Color::White], 30),
        vec![3, 5]
    );
    assert_eq!(
        select_critical_plies(&ts_records(), &[Color::Black], 30),
        vec![6]
    );
    assert!(select_critical_plies(&ts_records(), &[], 30).is_empty());
}

#[test]
fn ties_keep_the_lower_ply_and_output_is_ascending() {
    let records = vec![
        rec(1, 7.0),
        rec(2, 0.0),
        rec(3, 7.0),
        rec(4, 0.0),
        rec(5, 8.0),
    ];
    assert_eq!(select_critical_plies(&records, BOTH, 2), vec![1, 5]);
    assert!(select_critical_plies(&records, BOTH, 0).is_empty());
}
