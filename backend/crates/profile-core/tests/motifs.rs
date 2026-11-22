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

// ---- Fix round 1 ----

#[test]
fn a_capture_fork_completed_past_the_window_is_a_fork() {
    // Nxd6+ wins a rook (depth 1) and forks king and queen; Nxb5 comes at ply 3.
    let a = tag_line(
        "4k3/8/3r4/1q6/2N5/8/8/7K w - - 0 1",
        &line(&["c4d6", "e8e7", "d6b5"]),
        Color::White,
        8,
    );
    assert_eq!(a.swing.depth, Some(1));
    assert_contains("capture fork", &a.tags, &[Fork]);
}

#[test]
fn a_promotion_fork_completed_past_the_window_is_a_fork() {
    // e8=N+ (depth 1: +2) forks king and queen; Nxc7 comes at ply 3.
    let a = tag_line(
        "8/2q1P3/5k2/8/8/8/8/K7 w - - 0 1",
        &line(&["e7e8n", "f6f5", "e8c7"]),
        Color::White,
        8,
    );
    assert_eq!(a.swing.depth, Some(1));
    assert_contains("promotion fork", &a.tags, &[Fork, Promotion]);
}

#[test]
fn a_fork_is_completed_only_by_the_forking_piece() {
    // Ne4 blocks the check and "forks" queen and bishop, but the knight is pinned to its king;
    // the rook, not the knight, takes the queen.
    let a = tag_line(
        "4r2k/8/3q4/6b1/8/2N2P2/8/3RK3 w - - 0 1",
        &line(&["c3e4", "h8g8", "d1d6"]),
        Color::White,
        8,
    );
    assert_eq!(a.swing.depth, Some(3));
    assert_lacks("pinned forker", &a.tags, &[Fork]);
}

#[test]
fn a_voluntary_reply_is_no_deflection() {
    // Rxe6 (not check) and the rook leaves b8 of its own accord; Qxb7 wins what it guarded.
    let a = tag_line(
        "1r4k1/1q6/4p3/3Q4/8/8/8/4R1K1 w - - 0 1",
        &line(&["e1e6", "b8c8", "d5b7"]),
        Color::White,
        8,
    );
    assert_eq!(a.swing.depth, Some(3));
    assert_lacks("voluntary reply", &a.tags, &[Deflection]);
}

#[test]
fn a_piece_defended_by_a_piece_is_not_hanging_even_in_double_check() {
    // Nxd6 is double check, so the bishop on f8 cannot recapture, but it did defend d6.
    let a = tag_line(
        "4kb2/8/3r4/8/4N3/8/8/4R2K w - - 0 1",
        &line(&["e4d6"]),
        Color::White,
        8,
    );
    assert_eq!(a.swing.depth, Some(1));
    assert_lacks("double check", &a.tags, &[HangingPiece]);
}

#[test]
fn a_mating_capture_defended_only_by_the_king_is_a_hanging_piece() {
    let a = tag_line(
        "7k/6r1/8/8/8/8/1B6/6QK w - - 0 1",
        &line(&["g1g7"]),
        Color::White,
        8,
    );
    assert_eq!(a.tags, vec![HangingPiece, Mate, MateIn1]);
}

/// `cycles` king shuffles (4 plies each) from `DEEP_MATE_FEN`, then the back-rank mate.
fn shuffle_then_mate(cycles: usize) -> Vec<String> {
    let mut pv = Vec::new();
    for _ in 0..cycles {
        pv.extend(line(&["e1e2", "h8g8", "e2e1", "g8h8"]));
    }
    pv.push("a1a8".to_owned());
    pv
}

#[test]
fn only_the_first_128_plies_are_replayed() {
    // Mate at ply 125: inside the cap.
    let inside = tag_line(DEEP_MATE_FEN, &shuffle_then_mate(31), Color::White, 8);
    assert_eq!(inside.swing.mate_in, Some(63));
    assert_contains("ply 125", &inside.tags, &[Mate, MateIn4Plus]);
    // Mate at ply 129: past the cap, so no swing and no tags.
    let past = tag_line(DEEP_MATE_FEN, &shuffle_then_mate(32), Color::White, 8);
    assert_eq!(past.swing.depth, None);
    assert_eq!(past.tags, Vec::<MotifTag>::new());
}

#[test]
fn a_queen_that_dodges_and_comes_back_is_no_fork() {
    // Nc7 forks Qa8 and Re8. The queen dodges to a7, the knight leaves (Nb5) and returns (Nc7,
    // now forking nothing), and the queen walks back to a8 where the knight takes it. The
    // capture is neither the forker's first move after the fork nor a capture of an unmoved
    // target, so the fork at ply 1 did not win the queen.
    let a = tag_line(
        "q3r2k/8/8/1N6/8/8/7K/8 w - - 0 1",
        &line(&[
            "b5c7", "a8a7", "c7b5", "a7b7", "b5c7", "b7a8", "c7a8", "e8a8",
        ]),
        Color::White,
        8,
    );
    assert_eq!(a.swing.depth, Some(7));
    assert_lacks("dodge and return", &a.tags, &[Fork]);
}

#[test]
fn a_target_that_dodges_and_returns_while_the_forker_waits_is_no_fork() {
    // Same fork; the queen dodges and comes back while the knight waits on c7. The target moved
    // between the fork and the capture, so the fork did not win it (AND timing rule).
    let a = tag_line(
        "q3r2k/8/8/1N6/8/8/7K/8 w - - 0 1",
        &line(&["b5c7", "a8a7", "h2h3", "a7a8", "c7a8", "e8a8"]),
        Color::White,
        8,
    );
    assert_eq!(a.swing.depth, Some(5));
    assert_lacks("forker waits, target returns", &a.tags, &[Fork]);
}

#[test]
fn a_forker_that_returns_to_re_fork_is_credited_with_the_new_fork() {
    // Nc7 forks Qa8 and Re8; the knight leaves and returns while both targets stay. The ply-1
    // fork does not count (the forker moved), but the return Nc7 at ply 5 is a fork of its own,
    // completed at once, so the line is still tagged. F6 below is the case with no re-fork.
    let a = tag_line(
        "q3r2k/8/8/1N6/8/8/7K/8 w - - 0 1",
        &line(&[
            "b5c7", "h8g8", "c7b5", "g8h8", "b5c7", "h8g8", "c7a8", "e8a8",
        ]),
        Color::White,
        8,
    );
    assert_contains("re-fork", &a.tags, &[Fork]);
}

#[test]
fn review_probe_f6_the_other_target_leaves_and_the_forker_returns_is_no_fork() {
    let a = tag_line(
        "q3r2k/8/8/1N6/8/8/7K/8 w - - 0 1",
        &line(&["b5c7", "e8e1", "c7b5", "h8g8", "b5c7", "g8h8", "c7a8"]),
        Color::White,
        8,
    );
    assert_lacks("F6", &a.tags, &[Fork]);
}

#[test]
fn a_fork_completed_at_once_is_still_a_fork() {
    // Nc7 forks Qa8 and Re8; Black plays a king move and the knight takes the queen at once.
    let a = tag_line(
        "q3r2k/8/8/1N6/8/8/7K/8 w - - 0 1",
        &line(&["b5c7", "h8g8", "c7a8", "e8a8"]),
        Color::White,
        8,
    );
    assert_contains("immediate fork", &a.tags, &[Fork]);
}
