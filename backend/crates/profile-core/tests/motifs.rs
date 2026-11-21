//! R2 Task 5: the clean-room motif tagger (TS M2 Task 4 assertions, translated).

mod common;

use chess_core::Color;
use common::positions::{fixture, line};
use profile_core::motifs::tag_line;
use profile_core::types::{LineAnalysis, MotifTag};

use MotifTag::*;

fn tag(name: &str) -> Vec<MotifTag> {
    let (fen, pv) = fixture(name);
    tag_line(fen, &pv, Color::White, 8).tags
}

fn assert_contains(name: &str, tags: &[MotifTag], want: &[MotifTag]) {
    for t in want {
        assert!(tags.contains(t), "{name}: {tags:?} lacks {t:?}");
    }
}

fn assert_lacks(name: &str, tags: &[MotifTag], unwanted: &[MotifTag]) {
    for t in unwanted {
        assert!(!tags.contains(t), "{name}: {tags:?} has {t:?}");
    }
}

#[test]
fn hanging_is_exactly_hanging_piece() {
    assert_eq!(tag("hanging"), vec![HangingPiece]);
}

#[test]
fn fork_is_a_fork_not_a_hanging_piece() {
    let t = tag("fork");
    assert_contains("fork", &t, &[Fork]);
    assert_lacks("fork", &t, &[HangingPiece]);
}

#[test]
fn pin_and_skewer_are_told_apart() {
    let pin = tag("pin");
    assert_contains("pin", &pin, &[Pin]);
    assert_lacks("pin", &pin, &[Skewer]);
    let skewer = tag("skewer");
    assert_contains("skewer", &skewer, &[Skewer]);
    assert_lacks("skewer", &skewer, &[Pin]);
}

#[test]
fn discovered_attack() {
    assert_contains("discovered", &tag("discovered"), &[DiscoveredAttack]);
}

#[test]
fn deflection_is_also_a_sacrifice() {
    assert_contains("deflection", &tag("deflection"), &[Deflection, Sacrifice]);
}

#[test]
fn promotion() {
    assert_contains("promotion", &tag("promotion"), &[Promotion]);
}

#[test]
fn back_rank_mate_in_one() {
    assert_contains("backRank", &tag("backRank"), &[Mate, MateIn1, BackRankMate]);
}

#[test]
fn mate_in_two_is_a_mating_net_and_a_sacrifice_but_no_deflection() {
    let t = tag("mateIn2");
    assert_contains(
        "mateIn2",
        &t,
        &[Mate, MateIn2, MatingNet, BackRankMate, Sacrifice],
    );
    assert_lacks("mateIn2", &t, &[Deflection]);
}

#[test]
fn smothered_mate_is_not_a_back_rank_mate() {
    let t = tag("smothered");
    assert_contains("smothered", &t, &[MateIn1, SmotheredMate]);
    assert_lacks("smothered", &t, &[BackRankMate]);
}

#[test]
fn quiet_line_has_no_tags() {
    assert_eq!(tag("quiet"), Vec::<MotifTag>::new());
}

#[test]
fn no_swing_no_tags() {
    let (fen, pv) = fixture("fork");
    let a = tag_line(fen, &pv, Color::Black, 8);
    assert_eq!(a.swing.depth, None);
    assert_eq!(a.tags, Vec::<MotifTag>::new());
    // A horizon too short to see the fork's capture has no swing either.
    assert_eq!(
        tag_line(fen, &pv, Color::White, 2).tags,
        Vec::<MotifTag>::new()
    );
}

#[test]
fn tags_are_in_declaration_order_without_duplicates() {
    for name in [
        "hanging",
        "fork",
        "pin",
        "skewer",
        "discovered",
        "deflection",
        "promotion",
        "backRank",
        "mateIn2",
        "smothered",
    ] {
        let t = tag(name);
        assert!(
            t.windows(2).all(|w| w[0] < w[1]),
            "{name}: {t:?} not strictly ordered"
        );
    }
}

#[test]
fn the_swing_is_task_4s_settled_swing() {
    let (fen, pv) = fixture("mateIn2");
    let a = tag_line(fen, &pv, Color::White, 8);
    assert_eq!(a.swing.gain, 100.0);
    assert_eq!(a.swing.depth, Some(3));
    assert_eq!(a.swing.mate_in, Some(2));
}

const DEEP_MATE_FEN: &str = "7k/6pp/1p6/8/8/8/8/R3K3 w - - 0 1";

#[test]
fn a_mate_in_six_past_the_horizon_is_tagged_without_panicking() {
    let pv = line(&[
        "e1e2", "h8g8", "e2e1", "g8h8", "e1e2", "h8g8", "e2e1", "g8h8", "e1e2", "b6b5", "a1a8",
    ]);
    let a = tag_line(DEEP_MATE_FEN, &pv, Color::White, 8);
    assert_eq!(a.swing.depth, Some(11));
    assert_eq!(a.swing.mate_in, Some(6));
    assert_eq!(a.tags, vec![Mate, MateIn4Plus, MatingNet, BackRankMate]);
    // A tiny horizon replays the whole PV too.
    let short = tag_line(DEEP_MATE_FEN, &pv, Color::White, 1);
    assert_eq!(short.tags, a.tags);
}

#[test]
fn a_late_mate_pattern_counts_after_an_early_material_swing() {
    // Hanging queen won at ply 1 (depth 1), the back-rank mate only comes at ply 13.
    let fen = "7k/6pp/1p6/8/q7/8/8/R3K3 w - - 0 1";
    let pv = line(&[
        "a1a4", "h8g8", "a4a1", "g8h8", "e1e2", "h8g8", "e2e1", "g8h8", "e1e2", "h8g8", "e2e1",
        "g8h8", "a1a8",
    ]);
    let a = tag_line(fen, &pv, Color::White, 8);
    assert_eq!(a.swing.depth, Some(1));
    assert_eq!(
        a.tags,
        vec![HangingPiece, Mate, MateIn4Plus, MatingNet, BackRankMate]
    );
}

#[test]
fn a_defended_capture_is_not_a_hanging_piece() {
    // Qxd5 is met by ...exd5: no swing at all.
    let a = tag_line(
        "4k3/8/4p3/3q4/8/8/8/3QK3 w - - 0 1",
        &line(&["d1d5", "e6d5"]),
        Color::White,
        8,
    );
    assert_eq!(a.tags, Vec::<MotifTag>::new());
    // Rxd5 wins a knight that only the king could take back, but Kxd5 is illegal (the bishop
    // on b3 guards d5), so the knight hangs.
    let b = tag_line(
        "8/8/4k3/3n4/8/1B6/8/3RK3 w - - 0 1",
        &line(&["d1d5"]),
        Color::White,
        8,
    );
    assert_eq!(b.tags, vec![HangingPiece]);
}

#[test]
fn invalid_input_gives_no_tags() {
    let a = tag_line("not a fen", &line(&["e2e4"]), Color::White, 8);
    assert_eq!(a.tags, Vec::<MotifTag>::new());
    let (fen, _) = fixture("hanging");
    assert_eq!(
        tag_line(fen, &[], Color::White, 8).tags,
        Vec::<MotifTag>::new()
    );
}

#[test]
fn line_analysis_json_is_flat_with_camel_case_tags() {
    let (fen, pv) = fixture("backRank");
    let a: LineAnalysis = tag_line(fen, &pv, Color::White, 8);
    let json = serde_json::to_value(&a).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "gain": 100.0, "depth": 1, "mateIn": 1,
            "tags": ["mate", "mateIn1", "backRankMate"]
        })
    );
}

/// The exact tag set of every fixture: no rule fires where it should not.
#[test]
fn every_fixture_has_exactly_its_own_motifs() {
    let expected: &[(&str, &[MotifTag])] = &[
        ("hanging", &[HangingPiece]),
        ("fork", &[Fork]),
        ("pin", &[Pin]),
        ("skewer", &[Skewer]),
        ("discovered", &[DiscoveredAttack]),
        ("deflection", &[Deflection, Sacrifice]),
        ("promotion", &[Promotion]),
        ("backRank", &[Mate, MateIn1, BackRankMate]),
        (
            "mateIn2",
            &[Sacrifice, Mate, MateIn2, MatingNet, BackRankMate],
        ),
        ("smothered", &[Mate, MateIn1, SmotheredMate]),
        ("quiet", &[]),
    ];
    assert_eq!(expected.len(), common::positions::FIXTURES.len());
    for (name, tags) in expected {
        assert_eq!(tag(name), tags.to_vec(), "{name}");
    }
}

#[test]
fn a_forking_piece_that_is_en_prise_is_no_fork() {
    // Same fork, but the bishop on e5 guards c7, so Nc7+ puts the knight en prise.
    let a = tag_line(
        "r3k3/8/8/1N2b3/8/8/8/4K3 w - - 0 1",
        &line(&["b5c7", "e8e7", "c7a8"]),
        Color::White,
        8,
    );
    assert_eq!(a.swing.depth, Some(3));
    assert_lacks("en prise fork", &a.tags, &[Fork]);
}
