//! R2 Task 7: episodes, time trouble, endgame type and sessions (TS M2 Task 6 values, plus the R2
//! honesty rules: no NaN or inf in output, invalid inputs skipped and counted, nothing claimed from
//! zero evidence).

use chess_core::{Color, MoveRecord, Phase, TimeClass};
use profile_core::board::{material_signature, piece_count};
use profile_core::types::{EndgameType, GameContext, PlatformId};
use profile_core::weakness::endgame::{
    BASIC_SIGNATURES, TB_MAX_PIECES, endgame_type, is_basic_endgame, is_basic_signature,
    is_tablebase_eligible,
};
use profile_core::weakness::episodes::{
    Episode, EpisodeKind, EpisodeOpts, Outcome, extract_episodes, extract_episodes_counted,
};
use profile_core::weakness::sessions::{
    SESSION_GAP_MINUTES, SessionGame, build_sessions, build_sessions_counted, est_duration_seconds,
    expected_score, expected_score_from_ratings,
};
use profile_core::weakness::time_trouble::{
    TimeTrouble, detect_time_trouble, detect_time_trouble_counted, is_resignation, is_time_loss,
    player_outcome, player_resigned,
};
use time::OffsetDateTime;
use time::macros::datetime;
use uuid::Uuid;

fn mv(ply: u32, win_before: f64, win_after: f64) -> MoveRecord {
    MoveRecord {
        ply,
        color: if ply % 2 == 1 {
            Color::White
        } else {
            Color::Black
        },
        san: "a3".into(),
        uci: "a2a3".into(),
        fen_before: "4k3/8/8/8/8/8/P7/4K3 w - - 0 1".into(),
        best_uci: None,
        win_before,
        win_after,
        delta_w: (win_before - win_after).max(0.0),
        accuracy: 100.0,
        judgement: None,
        phase: Phase::Middlegame,
        clock_before: None,
        time_spent: None,
        decided: false,
    }
}

/// White moves at `start, start+2, ...` with the given `win_before`; each `win_after` equals the
/// next own `win_before` (the last keeps its own), so no drop below 65 is implied by accident.
/// Black moves are interleaved with neutral values.
fn white_run(start: u32, wins: &[f64]) -> Vec<MoveRecord> {
    let mut out = Vec::new();
    for (i, w) in wins.iter().enumerate() {
        let ply = start + 2 * i as u32;
        let after = wins.get(i + 1).copied().unwrap_or(*w);
        out.push(mv(ply, *w, after));
        out.push(mv(ply + 1, 100.0 - after, 100.0 - after));
    }
    out
}

// ---------------------------------------------------------------- episodes

#[test]
fn winning_episode_drawn_with_drop_ply() {
    let mut moves = white_run(21, &[90.0, 88.0, 91.0, 70.0]);
    // Ply 27 (the 4th own move) has win_after 60.
    let m27 = moves.iter_mut().find(|m| m.ply == 27).unwrap();
    m27.win_after = 60.0;
    let (winning, losing) =
        extract_episodes(&moves, Color::White, Some(0.5), &EpisodeOpts::default());
    assert_eq!(
        winning,
        Some(Episode {
            kind: EpisodeKind::Winning,
            start_ply: 21,
            end_ply: 25,
            outcome: Outcome::Drawn,
            drop_ply: Some(27),
        })
    );
    assert_eq!(losing, None);
}

#[test]
fn winning_run_before_ply_21_is_no_episode() {
    let mut moves = white_run(15, &[90.0, 88.0, 91.0, 70.0]);
    moves.iter_mut().find(|m| m.ply == 21).unwrap().win_after = 60.0;
    let (winning, _) = extract_episodes(&moves, Color::White, Some(0.5), &EpisodeOpts::default());
    assert_eq!(winning, None);
}

#[test]
fn a_run_crossing_ply_21_starts_at_the_first_ply_from_21() {
    // Plies 17..25 all >= 85; only 21, 23, 25 are at ply >= 21, which is still 3 moves.
    let moves = white_run(17, &[90.0, 90.0, 90.0, 90.0, 90.0]);
    let (winning, _) = extract_episodes(&moves, Color::White, Some(1.0), &EpisodeOpts::default());
    let w = winning.unwrap();
    assert_eq!((w.start_ply, w.end_ply, w.outcome), (21, 25, Outcome::Won));
    assert_eq!(w.drop_ply, None);
    // Plies 19..23: only 21 and 23 are >= 21, so 2 moves, no episode.
    let moves = white_run(19, &[90.0, 90.0, 90.0, 50.0]);
    assert_eq!(
        extract_episodes(&moves, Color::White, Some(1.0), &EpisodeOpts::default()).0,
        None
    );
}

#[test]
fn two_own_moves_are_not_an_episode_and_the_threshold_is_inclusive() {
    let moves = white_run(21, &[90.0, 90.0, 50.0, 90.0, 90.0]);
    assert_eq!(
        extract_episodes(&moves, Color::White, Some(1.0), &EpisodeOpts::default()).0,
        None
    );
    let moves = white_run(21, &[85.0, 85.0, 85.0]);
    let w = extract_episodes(&moves, Color::White, Some(0.0), &EpisodeOpts::default())
        .0
        .unwrap();
    assert_eq!(w.outcome, Outcome::Lost);
    let moves = white_run(21, &[85.0, 84.9, 85.0]);
    assert_eq!(
        extract_episodes(&moves, Color::White, Some(0.0), &EpisodeOpts::default()).0,
        None
    );
}

#[test]
fn the_first_qualifying_run_is_the_episode() {
    let moves = white_run(21, &[90.0, 90.0, 90.0, 50.0, 90.0, 90.0, 90.0, 90.0]);
    let w = extract_episodes(&moves, Color::White, Some(1.0), &EpisodeOpts::default())
        .0
        .unwrap();
    assert_eq!((w.start_ply, w.end_ply), (21, 25));
    // The drop is searched from the start: ply 25's win_after is 50 (< 65).
    assert_eq!(w.drop_ply, Some(25));
}

#[test]
fn losing_episode_drawn_has_no_ply_floor() {
    let moves = white_run(3, &[10.0, 12.0, 9.0]);
    let (winning, losing) =
        extract_episodes(&moves, Color::White, Some(0.5), &EpisodeOpts::default());
    assert_eq!(winning, None);
    assert_eq!(
        losing,
        Some(Episode {
            kind: EpisodeKind::Losing,
            start_ply: 3,
            end_ply: 7,
            outcome: Outcome::Drawn,
            drop_ply: None,
        })
    );
}

#[test]
fn episodes_use_only_the_players_moves() {
    // White is winning (90) for 4 moves; Black's own win_before is then 10..: a losing episode
    // for Black, nothing for White's perspective of Black.
    let moves = white_run(21, &[90.0, 90.0, 90.0, 90.0]);
    let (bw, bl) = extract_episodes(&moves, Color::Black, Some(0.0), &EpisodeOpts::default());
    assert_eq!(bw, None);
    let bl = bl.unwrap();
    assert_eq!(
        (bl.start_ply, bl.end_ply, bl.outcome),
        (22, 28, Outcome::Lost)
    );
}

#[test]
fn episodes_need_a_known_outcome() {
    let moves = white_run(21, &[90.0, 90.0, 90.0]);
    let scan = extract_episodes_counted(&moves, Color::White, None, &EpisodeOpts::default());
    assert_eq!((scan.winning, scan.losing), (None, None));
    assert!(scan.invalid_score);
    for bad in [f64::NAN, 0.7, -1.0, f64::INFINITY] {
        let scan =
            extract_episodes_counted(&moves, Color::White, Some(bad), &EpisodeOpts::default());
        assert_eq!(scan.winning, None, "score {bad}");
        assert!(scan.invalid_score);
    }
    assert_eq!(Outcome::from_score(1.0), Some(Outcome::Won));
    assert_eq!(Outcome::from_score(0.5), Some(Outcome::Drawn));
    assert_eq!(Outcome::from_score(0.0), Some(Outcome::Lost));
    assert_eq!(Outcome::from_score(0.25), None);
}

#[test]
fn non_finite_win_values_are_skipped_counted_and_break_runs() {
    let mut moves = white_run(21, &[90.0, 90.0, 90.0, 90.0, 90.0]);
    moves.iter_mut().find(|m| m.ply == 25).unwrap().win_before = f64::NAN;
    let scan = extract_episodes_counted(&moves, Color::White, Some(1.0), &EpisodeOpts::default());
    // 21, 23 | NaN | 27, 29: no run of 3.
    assert_eq!(scan.winning, None);
    assert_eq!(scan.skipped_moves, 1);
    // A non-finite win_after is never a drop.
    let mut moves = white_run(21, &[90.0, 90.0, 90.0]);
    moves.iter_mut().find(|m| m.ply == 23).unwrap().win_after = f64::NEG_INFINITY;
    let scan = extract_episodes_counted(&moves, Color::White, Some(1.0), &EpisodeOpts::default());
    assert_eq!(scan.winning.unwrap().drop_ply, None);
    assert_eq!(scan.skipped_moves, 1);
}

#[test]
fn episode_opts_default_values() {
    let o = EpisodeOpts::default();
    assert_eq!(
        (o.t_win, o.t_lose, o.min_moves, o.min_ply),
        (85.0, 15.0, 3, 21)
    );
    let custom = EpisodeOpts {
        min_moves: 2,
        ..EpisodeOpts::default()
    };
    let moves = white_run(21, &[90.0, 90.0, 50.0]);
    assert!(
        extract_episodes(&moves, Color::White, Some(1.0), &custom)
            .0
            .is_some()
    );
}

// ---------------------------------------------------------------- time trouble

/// White moves at plies 1, 3, .., `last_ply`, each with clock 200 except `ply` with `clock`.
fn clocked(last_ply: u32, ply: u32, clock: f64) -> Vec<MoveRecord> {
    let mut out = Vec::new();
    let mut p = 1;
    while p <= last_ply {
        let mut w = mv(p, 50.0, 50.0);
        w.clock_before = Some(if p == ply { clock } else { 200.0 });
        out.push(w);
        let mut b = mv(p + 1, 50.0, 50.0);
        b.clock_before = Some(5.0); // the opponent's clock never counts
        out.push(b);
        p += 2;
    }
    out
}

#[test]
fn time_trouble_at_move_31() {
    let moves = clocked(61, 61, 25.0);
    assert_eq!(
        detect_time_trouble(&moves, Color::White, Some(300)),
        TimeTrouble {
            in_trouble: Some(true),
            first_ply: Some(61)
        }
    );
}

#[test]
fn clock_above_ten_percent_is_no_trouble() {
    let moves = clocked(61, 61, 31.0);
    assert_eq!(
        detect_time_trouble(&moves, Color::White, Some(300)),
        TimeTrouble {
            in_trouble: Some(false),
            first_ply: None
        }
    );
    // Strictly below 10%: exactly 30 of 300 is not trouble.
    let moves = clocked(61, 61, 30.0);
    assert_eq!(
        detect_time_trouble(&moves, Color::White, Some(300)).in_trouble,
        Some(false)
    );
}

#[test]
fn late_low_clock_with_few_moves_left_is_no_trouble() {
    // Ply 89 = move 45, own moves 89, 91, 93 remain: 3 < 10.
    let moves = clocked(93, 89, 25.0);
    assert_eq!(
        detect_time_trouble(&moves, Color::White, Some(300)).in_trouble,
        Some(false)
    );
    // Ten own moves remaining (89..=107, the low-clock move included) is trouble.
    let moves = clocked(107, 89, 25.0);
    assert_eq!(
        detect_time_trouble(&moves, Color::White, Some(300)),
        TimeTrouble {
            in_trouble: Some(true),
            first_ply: Some(89)
        }
    );
    // Nine remaining is not.
    let moves = clocked(105, 89, 25.0);
    assert_eq!(
        detect_time_trouble(&moves, Color::White, Some(300)).in_trouble,
        Some(false)
    );
}

#[test]
fn move_39_counts_and_move_40_needs_remaining_moves() {
    // Ply 77 = move 39, last own move: trouble.
    assert_eq!(
        detect_time_trouble(&clocked(77, 77, 10.0), Color::White, Some(300)).first_ply,
        Some(77)
    );
    // Ply 79 = move 40, last own move: not trouble.
    assert_eq!(
        detect_time_trouble(&clocked(79, 79, 10.0), Color::White, Some(300)).in_trouble,
        Some(false)
    );
}

#[test]
fn black_move_numbers_and_first_ply() {
    let mut moves = clocked(61, 0, 0.0);
    for m in moves.iter_mut().filter(|m| m.color == Color::Black) {
        m.clock_before = Some(if m.ply >= 40 { 20.0 } else { 200.0 });
    }
    let t = detect_time_trouble(&moves, Color::Black, Some(300));
    assert_eq!(
        t,
        TimeTrouble {
            in_trouble: Some(true),
            first_ply: Some(40)
        }
    );
}

#[test]
fn no_clocks_or_no_base_is_unknown() {
    let moves = white_run(1, &[50.0; 30]);
    let unknown = TimeTrouble {
        in_trouble: None,
        first_ply: None,
    };
    assert_eq!(
        detect_time_trouble(&moves, Color::White, Some(300)),
        unknown
    );
    let moves = clocked(61, 61, 25.0);
    assert_eq!(detect_time_trouble(&moves, Color::White, None), unknown);
    assert_eq!(detect_time_trouble(&moves, Color::White, Some(0)), unknown);
    assert_eq!(detect_time_trouble(&[], Color::White, Some(300)), unknown);
}

#[test]
fn invalid_clocks_are_skipped_and_counted() {
    let mut moves = clocked(61, 61, 25.0);
    moves.iter_mut().find(|m| m.ply == 61).unwrap().clock_before = Some(f64::NAN);
    moves.iter_mut().find(|m| m.ply == 59).unwrap().clock_before = Some(-3.0);
    let scan = detect_time_trouble_counted(&moves, Color::White, Some(300));
    assert_eq!(scan.trouble.in_trouble, Some(false));
    assert_eq!(scan.skipped_clocks, 2);
    // Every own clock invalid: no evidence, so unknown.
    for m in moves.iter_mut().filter(|m| m.color == Color::White) {
        m.clock_before = Some(f64::INFINITY);
    }
    let scan = detect_time_trouble_counted(&moves, Color::White, Some(300));
    assert_eq!(scan.trouble.in_trouble, None);
    assert_eq!(scan.skipped_clocks, 31);
}

// ---------------------------------------------------------------- endgame

#[test]
fn rook_endgame() {
    assert_eq!(
        endgame_type("8/5k2/8/8/3R4/8/2r5/4K3 w - - 0 1"),
        Some(EndgameType::Rook)
    );
}

#[test]
fn endgame_types_by_non_pawn_material() {
    let cases = [
        ("4k3/4p3/8/8/8/8/4P3/4K3 w - - 0 1", EndgameType::Pawn),
        ("4k3/8/8/8/8/8/8/4K3 w - - 0 1", EndgameType::Pawn),
        ("4k3/4p3/8/8/8/8/4P3/R3K3 w - - 0 1", EndgameType::Rook),
        ("2b1k3/8/8/8/8/8/8/1N2K3 w - - 0 1", EndgameType::Minor),
        ("4k3/8/8/8/8/8/8/1B2KB2 w - - 0 1", EndgameType::Minor),
        ("3qk3/8/8/8/8/8/8/3QK3 w - - 0 1", EndgameType::Queen),
        ("3rk3/8/8/8/8/8/8/1B2K3 w - - 0 1", EndgameType::Mixed),
        ("3rk3/8/8/8/8/8/8/3QK3 w - - 0 1", EndgameType::Mixed),
        ("3qk3/8/8/8/8/8/8/1N2K3 w - - 0 1", EndgameType::Mixed),
    ];
    for (fen, want) in cases {
        assert_eq!(endgame_type(fen), Some(want), "{fen}");
    }
}

#[test]
fn invalid_fen_has_no_endgame_type() {
    for fen in [
        "",
        "not a fen",
        "8/8/8/8/8/8/8/8 w - - 0 1",
        "4k3/8/8/8/8/8/8/3QK3 w - -",
    ] {
        assert_eq!(endgame_type(fen), None, "{fen:?}");
        assert_eq!(is_tablebase_eligible(fen), None, "{fen:?}");
    }
}

#[test]
fn basic_signatures_are_the_ts_set_and_all_tablebase_sized() {
    assert_eq!(
        BASIC_SIGNATURES,
        ["KPvK", "KQvK", "KRvK", "KRvKP", "KRPvKR", "KQvKP", "KPvKP"]
    );
    assert_eq!(TB_MAX_PIECES, 5);
    // One legal position per basic signature: its signature, piece count, tablebase eligibility
    // and endgame type agree.
    let positions = [
        ("4k3/8/8/8/8/8/4P3/4K3 w - - 0 1", "KPvK", EndgameType::Pawn),
        ("4k3/8/8/8/8/8/8/3QK3 w - - 0 1", "KQvK", EndgameType::Queen),
        ("4k3/8/8/8/8/8/8/R3K3 w - - 0 1", "KRvK", EndgameType::Rook),
        (
            "8/8/4k3/8/4P3/8/4K3/7r w - - 0 1",
            "KRvKP",
            EndgameType::Rook,
        ),
        (
            "3rk3/8/8/8/8/8/4P3/R3K3 w - - 0 1",
            "KRPvKR",
            EndgameType::Rook,
        ),
        (
            "4k3/4p3/8/8/8/8/8/3QK3 w - - 0 1",
            "KQvKP",
            EndgameType::Queen,
        ),
        (
            "4k3/4p3/8/8/8/8/4P3/4K3 w - - 0 1",
            "KPvKP",
            EndgameType::Pawn,
        ),
    ];
    for (fen, sig, kind) in positions {
        assert_eq!(material_signature(fen), sig, "{fen}");
        assert!(is_basic_signature(&material_signature(fen)), "{fen}");
        assert!(piece_count(fen) <= TB_MAX_PIECES, "{fen}");
        assert_eq!(is_tablebase_eligible(fen), Some(true), "{fen}");
        assert_eq!(endgame_type(fen), Some(kind), "{fen}");
    }
    for sig in BASIC_SIGNATURES {
        // Signature letters are the pieces: their count is the position's piece count.
        assert!(
            sig.chars().filter(|c| *c != 'v').count() as u32 <= TB_MAX_PIECES,
            "{sig}"
        );
    }
    assert!(!is_basic_signature("KBNvK"));
    assert!(!is_basic_signature("KRvKB"));
}

#[test]
fn tablebase_eligibility_is_five_pieces_or_fewer() {
    // 5 pieces: eligible; 6 pieces: not.
    assert_eq!(
        is_tablebase_eligible("3rk3/8/8/8/8/8/4P3/R3K3 w - - 0 1"),
        Some(true)
    );
    assert_eq!(
        is_tablebase_eligible("3rk3/4p3/8/8/8/8/4P3/R3K3 w - - 0 1"),
        Some(false)
    );
    // Basic endgame: a basic signature in a tablebase-sized position.
    assert_eq!(
        is_basic_endgame("8/8/4k3/8/4P3/8/4K3/7r w - - 0 1"),
        Some(true)
    );
    assert_eq!(
        is_basic_endgame("2b1k3/8/8/8/8/8/8/1N2K3 w - - 0 1"),
        Some(false)
    );
    assert_eq!(
        is_basic_endgame("3rk3/4p3/8/8/8/8/4P3/R3K3 w - - 0 1"),
        Some(false)
    );
    assert_eq!(is_basic_endgame("bad"), None);
}

// ---------------------------------------------------------------- sessions

fn sg(at: OffsetDateTime, score: Option<f64>, expected: Option<f64>) -> SessionGame {
    SessionGame {
        game_id: Uuid::new_v4(),
        played_at: at,
        est_duration_seconds: 180.0,
        score,
        expected_score: expected,
    }
}

#[test]
fn sessions_split_on_a_15_minute_gap() {
    let games = [
        sg(datetime!(2026-11-01 10:00 UTC), Some(1.0), Some(0.5)),
        sg(datetime!(2026-11-01 10:04 UTC), Some(0.0), Some(0.5)),
        sg(datetime!(2026-11-01 11:00 UTC), Some(0.5), Some(0.5)),
    ];
    assert_eq!(SESSION_GAP_MINUTES, 15);
    let s = build_sessions(&games, SESSION_GAP_MINUTES);
    assert_eq!(
        s.iter().map(|x| x.session_id).collect::<Vec<_>>(),
        [1, 1, 2]
    );
    assert_eq!(
        s.iter().map(|x| x.index_in_session).collect::<Vec<_>>(),
        [1, 2, 1]
    );
    assert_eq!(
        s.iter().map(|x| x.game_id).collect::<Vec<_>>(),
        games.iter().map(|g| g.game_id).collect::<Vec<_>>()
    );
}

#[test]
fn sessions_sort_by_played_at_and_track_losses_and_residuals() {
    let a = sg(datetime!(2026-11-01 10:00 UTC), Some(0.0), Some(0.25));
    let b = sg(datetime!(2026-11-01 10:04 UTC), Some(1.0), Some(0.25));
    let c = sg(datetime!(2026-11-01 10:08 UTC), Some(0.5), None);
    let d = sg(datetime!(2026-11-01 12:00 UTC), Some(1.0), Some(0.5));
    let s = build_sessions(&[d.clone(), b.clone(), a.clone(), c.clone()], 15);
    assert_eq!(
        s.iter().map(|x| x.game_id).collect::<Vec<_>>(),
        [a.game_id, b.game_id, c.game_id, d.game_id]
    );
    // after_loss: the previous game of the same session was lost.
    assert_eq!(
        s.iter().map(|x| x.after_loss).collect::<Vec<_>>(),
        [false, true, false, false]
    );
    assert_eq!(
        s.iter().map(|x| x.residual).collect::<Vec<_>>(),
        [Some(-0.25), Some(0.75), None, Some(0.5)]
    );
}

#[test]
fn a_loss_does_not_carry_into_the_next_session() {
    let a = sg(datetime!(2026-11-01 10:00 UTC), Some(0.0), None);
    let b = sg(datetime!(2026-11-01 13:00 UTC), Some(1.0), None);
    let s = build_sessions(&[a, b], 15);
    assert_eq!(s[1].session_id, 2);
    assert!(!s[1].after_loss);
}

#[test]
fn the_gap_counts_from_the_estimated_end_of_the_previous_game() {
    // Previous game 10:00 lasting 600 s ends 10:10; next at 10:24 is 14 min later: same session.
    let mut a = sg(datetime!(2026-11-01 10:00 UTC), None, None);
    a.est_duration_seconds = 600.0;
    let b = sg(datetime!(2026-11-01 10:24 UTC), None, None);
    let c = sg(datetime!(2026-11-01 10:42 UTC), None, None); // b ends 10:27: 15 min, new session
    let s = build_sessions(&[a, b, c], 15);
    assert_eq!(
        s.iter().map(|x| x.session_id).collect::<Vec<_>>(),
        [1, 1, 2]
    );
    // Unknown scores never claim a loss or a residual.
    assert!(s.iter().all(|x| !x.after_loss && x.residual.is_none()));
}

#[test]
fn invalid_session_inputs_are_skipped_and_counted() {
    let mut bad_duration = sg(datetime!(2026-11-01 10:02 UTC), Some(0.0), None);
    bad_duration.est_duration_seconds = f64::NAN;
    let mut negative = sg(datetime!(2026-11-01 10:03 UTC), Some(0.0), None);
    negative.est_duration_seconds = -1.0;
    let bad_score = sg(datetime!(2026-11-01 10:04 UTC), Some(f64::NAN), Some(0.5));
    let bad_expected = sg(
        datetime!(2026-11-01 10:08 UTC),
        Some(1.0),
        Some(f64::INFINITY),
    );
    let first = sg(datetime!(2026-11-01 10:00 UTC), Some(1.0), Some(0.5));
    let build = build_sessions_counted(
        &[
            first.clone(),
            bad_duration,
            negative,
            bad_score.clone(),
            bad_expected.clone(),
        ],
        15,
    );
    assert_eq!(build.skipped_games, 2);
    assert_eq!(build.invalid_scores, 2);
    let ids: Vec<_> = build.sessions.iter().map(|x| x.game_id).collect();
    assert_eq!(
        ids,
        [first.game_id, bad_score.game_id, bad_expected.game_id]
    );
    assert_eq!(
        build
            .sessions
            .iter()
            .map(|x| x.index_in_session)
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
    // An invalid score is not a loss and has no residual.
    assert!(!build.sessions[2].after_loss);
    assert!(build.sessions.iter().skip(1).all(|x| x.residual.is_none()));
    assert!(
        build
            .sessions
            .iter()
            .all(|x| x.residual.is_none_or(f64::is_finite))
    );
    assert!(build_sessions(&[], 15).is_empty());
}

#[test]
fn estimated_duration_is_base_plus_40_increments() {
    assert_eq!(est_duration_seconds(Some(180), Some(0)), Some(180.0));
    assert_eq!(est_duration_seconds(Some(180), Some(2)), Some(260.0));
    assert_eq!(est_duration_seconds(Some(600), None), Some(600.0));
    assert_eq!(est_duration_seconds(None, Some(2)), None);
}

#[test]
fn expected_score_is_the_elo_formula() {
    assert_eq!(expected_score(1500.0, 1500.0), 0.5);
    assert!((expected_score(1500.0, 1700.0) - 0.240_253_073_352_042).abs() < 1e-12);
    assert!((expected_score(1700.0, 1500.0) + expected_score(1500.0, 1700.0) - 1.0).abs() < 1e-12);
    assert_eq!(
        expected_score_from_ratings(Some(1500), Some(1500)),
        Some(0.5)
    );
    assert_eq!(expected_score_from_ratings(None, Some(1500)), None);
    assert_eq!(expected_score_from_ratings(Some(1500), None), None);
    // Extreme gaps stay finite.
    let e = expected_score_from_ratings(Some(0), Some(i32::MAX)).unwrap();
    assert!(e.is_finite() && (0.0..=1.0).contains(&e));
}

// ---------------------------------------------------------------- time losses and resignations

fn ctx(platform: PlatformId, color: Color, result: &str, termination: Option<&str>) -> GameContext {
    GameContext {
        game_id: Uuid::new_v4(),
        platform_game_id: "g1".into(),
        platform,
        played_at: datetime!(2026-11-01 10:00 UTC),
        time_class: TimeClass::Blitz,
        base_seconds: Some(180),
        increment_seconds: Some(0),
        rated: true,
        player_color: color,
        player_rating: None,
        opponent_rating: None,
        result: result.into(),
        score: None,
        termination: termination.map(str::to_owned),
        accuracy: None,
        protocol: "sf19-n300k-v1".into(),
    }
}

#[test]
fn chesscom_won_on_time_is_a_time_loss_for_the_loser() {
    let c = ctx(
        PlatformId::Chesscom,
        Color::Black,
        "1-0",
        Some("Hikaru won on time"),
    );
    assert!(is_time_loss(&c));
    // The winner did not lose on time.
    let c = ctx(
        PlatformId::Chesscom,
        Color::White,
        "1-0",
        Some("Hikaru won on time"),
    );
    assert!(!is_time_loss(&c));
    let c = ctx(
        PlatformId::Chesscom,
        Color::Black,
        "1-0",
        Some("hikaru WON ON TIME"),
    );
    assert!(is_time_loss(&c));
}

#[test]
fn lichess_outoftime_and_timeout_are_time_losses() {
    for t in ["outoftime", "timeout"] {
        assert!(
            is_time_loss(&ctx(PlatformId::Lichess, Color::White, "0-1", Some(t))),
            "{t}"
        );
        assert!(
            !is_time_loss(&ctx(PlatformId::Lichess, Color::Black, "0-1", Some(t))),
            "{t}"
        );
    }
    assert!(!is_time_loss(&ctx(
        PlatformId::Lichess,
        Color::White,
        "0-1",
        Some("mate")
    )));
    assert!(!is_time_loss(&ctx(
        PlatformId::Lichess,
        Color::White,
        "0-1",
        None
    )));
    // A chess.com phrase on Lichess and a Lichess status on chess.com do not match.
    assert!(!is_time_loss(&ctx(
        PlatformId::Lichess,
        Color::White,
        "0-1",
        Some("x won on time")
    )));
    assert!(!is_time_loss(&ctx(
        PlatformId::Chesscom,
        Color::White,
        "0-1",
        Some("outoftime")
    )));
}

#[test]
fn a_time_loss_needs_a_known_lost_result() {
    // Unknown result: nothing is claimed.
    assert!(!is_time_loss(&ctx(
        PlatformId::Lichess,
        Color::White,
        "*",
        Some("outoftime")
    )));
    // A drawn game (timeout vs insufficient material) is not a time loss: who flagged is unknown.
    assert!(!is_time_loss(&ctx(
        PlatformId::Lichess,
        Color::White,
        "1/2-1/2",
        Some("outoftime")
    )));
    assert!(!is_time_loss(&ctx(
        PlatformId::Chesscom,
        Color::White,
        "1/2-1/2",
        Some("Game drawn by timeout vs insufficient material")
    )));
}

#[test]
fn resignations() {
    assert!(is_resignation(&ctx(
        PlatformId::Lichess,
        Color::White,
        "0-1",
        Some("resign")
    )));
    assert!(is_resignation(&ctx(
        PlatformId::Chesscom,
        Color::White,
        "1-0",
        Some("Hikaru won by resignation")
    )));
    assert!(!is_resignation(&ctx(
        PlatformId::Lichess,
        Color::White,
        "0-1",
        Some("mate")
    )));
    assert!(!is_resignation(&ctx(
        PlatformId::Lichess,
        Color::White,
        "0-1",
        None
    )));
    // player_resigned: a resignation the player lost.
    assert!(player_resigned(&ctx(
        PlatformId::Lichess,
        Color::White,
        "0-1",
        Some("resign")
    )));
    assert!(!player_resigned(&ctx(
        PlatformId::Lichess,
        Color::Black,
        "0-1",
        Some("resign")
    )));
    assert!(!player_resigned(&ctx(
        PlatformId::Lichess,
        Color::White,
        "*",
        Some("resign")
    )));
}

#[test]
fn player_outcome_reads_the_result_from_the_players_side() {
    let o = |c, r| player_outcome(&ctx(PlatformId::Lichess, c, r, None));
    assert_eq!(o(Color::White, "1-0"), Some(Outcome::Won));
    assert_eq!(o(Color::Black, "1-0"), Some(Outcome::Lost));
    assert_eq!(o(Color::Black, "0-1"), Some(Outcome::Won));
    assert_eq!(o(Color::White, "1/2-1/2"), Some(Outcome::Drawn));
    assert_eq!(o(Color::White, "*"), None);
    assert_eq!(o(Color::White, ""), None);
}
