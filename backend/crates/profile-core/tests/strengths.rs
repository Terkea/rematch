//! R2 Task 10: strength exemplar extraction (TS M2 Task 9 cases plus R2 honesty cases).
//!
//! Fixture: `make_game()` (White player, 30 own moves at W 50, ΔW 0, quiet best lines with
//! `best_pv[0]` = the played move, nothing critical, no MultiPV, a draw). It has no strength.

mod common;

use chess_core::EngineScore;
use common::make_game::*;
use profile_core::types::{AnalysedGame, EnrichedMove, MultipvLine, StrengthKind, StrengthPly};
use profile_core::weakness::strengths::{
    MAX_PER_KIND, extract_strength_plies, extract_strength_plies_counted,
};

use StrengthKind::*;

fn sp(kind: StrengthKind, ply: u32) -> StrengthPly {
    StrengthPly { kind, ply }
}

fn pv_line(uci: &str) -> MultipvLine {
    MultipvLine {
        uci: uci.into(),
        score: EngineScore::Cp(0),
        pv: vec![uci.into()],
    }
}

/// A critical move whose MultiPV line 1 is the played move (`g1f3`).
fn only_move(m: &mut EnrichedMove, gap: Option<f64>) {
    m.f.is_critical = Some(true);
    m.f.multipv_gap = gap;
    m.raw.as_mut().unwrap().multipv = Some(vec![pv_line("g1f3"), pv_line("b1c3")]);
}

/// A best line winning `gain` pawns, started by the played move.
fn tactic_played(m: &mut EnrichedMove, gain: f64) {
    m.f.best = Some(line(gain, Some(3), None, &[]));
}

/// Own moves at the given plies get W `w` before and after.
fn with_w(mut g: AnalysedGame, plies: &[u32], w: f64) -> AnalysedGame {
    for &p in plies {
        set_w(&mut g.moves[at_ply(p)], w, w);
    }
    g
}

fn strengths(g: &AnalysedGame) -> Vec<StrengthPly> {
    extract_strength_plies(g)
}

// ------------------------------------------------------------------ TS M2 Task 9 cases

#[test]
fn ts1_critical_move_matching_line_1_is_only_move_found() {
    let g = with_move(make_game(), own(5), |m| only_move(m, Some(30.0)));
    assert_eq!(strengths(&g), vec![sp(OnlyMoveFound, 11)]);
}

#[test]
fn ts2_tactic_found_by_playing_best_pv_0_with_gain_3() {
    let g = with_move(make_game(), own(4), |m| tactic_played(m, 3.0));
    assert_eq!(strengths(&g), vec![sp(TacticFound, 9)]);
}

#[test]
fn ts3_won_winning_episode_without_critical_plies_is_conversion_at_start() {
    let g = with_score(with_w(make_game(), &[21, 23, 25], 87.0), 1.0);
    assert_eq!(strengths(&g), vec![sp(Conversion, 21)]);
}

#[test]
fn ts4_drawn_losing_episode_is_defence() {
    let g = with_score(with_w(make_game(), &[21, 23, 25], 12.0), 0.5);
    assert_eq!(strengths(&g), vec![sp(Defence, 21)]);
}

#[test]
fn ts5_four_critical_only_moves_give_three() {
    let mut g = make_game();
    for (k, gap) in [(1, 10.0), (2, 40.0), (3, 20.0), (4, 30.0)] {
        g = with_move(g, own(k), |m| only_move(m, Some(gap)));
    }
    // By gap descending: own(2) ply 5, own(4) ply 9, own(3) ply 7; own(1) ply 3 is dropped.
    assert_eq!(
        strengths(&g),
        vec![
            sp(OnlyMoveFound, 5),
            sp(OnlyMoveFound, 9),
            sp(OnlyMoveFound, 7)
        ]
    );
    assert_eq!(MAX_PER_KIND, 3);
}

#[test]
fn ts6_decided_positions_give_none() {
    // Every candidate at W 95 (|W - 50| >= 40): critical only-moves, found tactics, a won
    // winning episode. None is eligible.
    let mut g = make_game();
    for k in 10..15 {
        g = with_move(g, own(k), |m| {
            set_w(m, 95.0, 95.0);
            only_move(m, Some(30.0));
            tactic_played(m, 3.0);
        });
    }
    let g = with_score(g, 1.0);
    assert_eq!(strengths(&g), vec![]);

    // And a lost-side mirror: W 5, a drawn losing episode, critical only-moves.
    let mut g = make_game();
    for k in 10..15 {
        g = with_move(g, own(k), |m| {
            set_w(m, 5.0, 5.0);
            only_move(m, Some(30.0));
        });
    }
    assert_eq!(strengths(&with_score(g, 0.5)), vec![]);
}

// ------------------------------------------------------------------ no evidence

#[test]
fn neutral_game_has_no_strength() {
    assert_eq!(strengths(&make_game()), vec![]);
    assert_eq!(strengths(&with_score(make_game(), 1.0)), vec![]);
}

#[test]
fn empty_game_has_no_strength() {
    let mut g = make_game();
    g.moves.clear();
    assert_eq!(strengths(&g), vec![]);
}

// ------------------------------------------------------------------ only_move_found

#[test]
fn only_move_needs_the_played_move_on_line_1() {
    let g = with_move(make_game(), own(5), |m| {
        only_move(m, Some(30.0));
        m.raw.as_mut().unwrap().multipv = Some(vec![pv_line("b1c3"), pv_line("g1f3")]);
    });
    assert_eq!(strengths(&g), vec![]);
}

#[test]
fn only_move_needs_is_critical_true() {
    for critical in [Some(false), None] {
        let g = with_move(make_game(), own(5), |m| {
            only_move(m, Some(30.0));
            m.f.is_critical = critical;
        });
        assert_eq!(strengths(&g), vec![], "is_critical {critical:?}");
    }
}

#[test]
fn only_move_needs_a_multipv() {
    for multipv in [None, Some(vec![])] {
        let g = with_move(make_game(), own(5), |m| {
            only_move(m, Some(30.0));
            m.raw.as_mut().unwrap().multipv = multipv.clone();
        });
        assert_eq!(strengths(&g), vec![]);
    }
    let g = with_move(make_game(), own(5), |m| {
        only_move(m, Some(30.0));
        m.raw = None;
    });
    assert_eq!(strengths(&g), vec![]);
}

#[test]
fn eligibility_boundary_is_strict_radius_40() {
    // W 89.9 is eligible, W 90 and W 10 are not, W 10.1 is.
    for (w, found) in [(89.9, true), (90.0, false), (10.0, false), (10.1, true)] {
        let g = with_move(make_game(), own(5), |m| {
            set_w(m, w, w);
            only_move(m, Some(30.0));
        });
        let want = if found {
            vec![sp(OnlyMoveFound, 11)]
        } else {
            vec![]
        };
        assert_eq!(strengths(&g), want, "W {w}");
    }
}

// ------------------------------------------------------------------ tactic_found

#[test]
fn tactic_needs_gain_2_or_a_mate() {
    for (gain, mate, found) in [(2.0, None, true), (1.99, None, false), (0.0, Some(2), true)] {
        let g = with_move(make_game(), own(4), |m| {
            m.f.best = Some(line(gain, Some(3), mate, &[]));
        });
        let want = if found {
            vec![sp(TacticFound, 9)]
        } else {
            vec![]
        };
        assert_eq!(strengths(&g), want, "gain {gain} mate {mate:?}");
    }
}

#[test]
fn tactic_needs_the_played_move_as_best_pv_0() {
    let g = with_move(make_game(), own(4), |m| {
        tactic_played(m, 3.0);
        m.raw.as_mut().unwrap().best_pv = vec!["b1c3".into(), "g1f3".into()];
    });
    assert_eq!(strengths(&g), vec![]);

    let g = with_move(make_game(), own(4), |m| {
        tactic_played(m, 3.0);
        m.raw.as_mut().unwrap().best_pv = vec![];
    });
    assert_eq!(strengths(&g), vec![]);

    let g = with_move(make_game(), own(4), |m| {
        tactic_played(m, 3.0);
        m.raw = None;
    });
    assert_eq!(strengths(&g), vec![]);
}

#[test]
fn tactic_needs_a_known_best_line() {
    let g = with_move(make_game(), own(4), |m| m.f.best = None);
    assert_eq!(strengths(&g), vec![]);
}

// ------------------------------------------------------------------ conversion and defence

#[test]
fn conversion_uses_the_first_own_critical_ply_inside_the_episode() {
    let g = with_w(make_game(), &[21, 23, 25, 27], 87.0);
    let g = with_move(g, at_ply(25), |m| m.f.is_critical = Some(true));
    let g = with_move(g, at_ply(27), |m| m.f.is_critical = Some(true));
    // A critical ply before the episode is not inside it.
    let g = with_move(g, at_ply(19), |m| m.f.is_critical = Some(true));
    assert_eq!(strengths(&with_score(g, 1.0)), vec![sp(Conversion, 25)]);
}

#[test]
fn episode_range_includes_its_end_ply_and_nothing_after() {
    // Episode 21..=25: the critical end ply 25 counts.
    let g = with_w(make_game(), &[21, 23, 25], 87.0);
    let g = with_move(g, at_ply(25), |m| m.f.is_critical = Some(true));
    assert_eq!(strengths(&with_score(g, 1.0)), vec![sp(Conversion, 25)]);
    // A critical ply right after the episode (27, back at W 50) does not.
    let g = with_w(make_game(), &[21, 23, 25], 87.0);
    let g = with_move(g, at_ply(27), |m| m.f.is_critical = Some(true));
    assert_eq!(strengths(&with_score(g, 1.0)), vec![sp(Conversion, 21)]);
}

#[test]
fn conversion_skips_an_ineligible_critical_ply() {
    let g = with_w(make_game(), &[21, 23, 25], 87.0);
    let g = with_w(g, &[23], 92.0);
    let g = with_move(g, at_ply(23), |m| m.f.is_critical = Some(true));
    assert_eq!(strengths(&with_score(g, 1.0)), vec![sp(Conversion, 21)]);
}

#[test]
fn conversion_with_an_ineligible_start_and_no_eligible_critical_ply_is_none() {
    let g = with_w(make_game(), &[21, 23, 25], 87.0);
    let g = with_w(g, &[21], 92.0);
    assert_eq!(strengths(&with_score(g, 1.0)), vec![]);
}

#[test]
fn winning_episode_not_won_is_no_conversion() {
    for score in [0.5, 0.0] {
        let g = with_score(with_w(make_game(), &[21, 23, 25], 87.0), score);
        assert_eq!(strengths(&g), vec![], "score {score}");
    }
}

#[test]
fn unknown_result_gives_no_conversion_or_defence() {
    let mut g = with_w(make_game(), &[21, 23, 25], 87.0);
    g.ctx.score = None;
    assert_eq!(strengths(&g), vec![]);
    let mut g = with_w(make_game(), &[21, 23, 25], 12.0);
    g.ctx.score = Some(f64::NAN);
    assert_eq!(strengths(&g), vec![]);
}

#[test]
fn losing_episode_lost_is_no_defence_and_won_is_defence() {
    let g = with_score(with_w(make_game(), &[21, 23, 25], 12.0), 0.0);
    assert_eq!(strengths(&g), vec![]);
    let g = with_score(with_w(make_game(), &[21, 23, 25], 12.0), 1.0);
    assert_eq!(strengths(&g), vec![sp(Defence, 21)]);
}

#[test]
fn defence_uses_the_first_own_critical_ply_inside_the_episode() {
    let g = with_w(make_game(), &[21, 23, 25], 12.0);
    let g = with_move(g, at_ply(23), |m| m.f.is_critical = Some(true));
    // An opponent's critical ply inside the episode is not the player's.
    let g = with_move(g, at_ply(22), |m| m.f.is_critical = Some(true));
    assert_eq!(strengths(&with_score(g, 0.5)), vec![sp(Defence, 23)]);
}

#[test]
fn too_short_episode_is_none() {
    let g = with_score(with_w(make_game(), &[21, 23], 87.0), 1.0);
    assert_eq!(strengths(&g), vec![]);
}

// ------------------------------------------------------------------ player moves only, valid only

#[test]
fn opponent_moves_are_never_strengths() {
    let g = with_move(make_game(), at_ply(12), |m| {
        only_move(m, Some(30.0));
        tactic_played(m, 3.0);
    });
    assert_eq!(strengths(&g), vec![]);
}

#[test]
fn black_player_gets_its_own_plies() {
    let mut g = with_move(make_game(), at_ply(12), |m| only_move(m, Some(30.0)));
    g = with_move(g, own(5), |m| only_move(m, Some(30.0)));
    g.ctx.player_color = chess_core::Color::Black;
    assert_eq!(strengths(&g), vec![sp(OnlyMoveFound, 12)]);
}

#[test]
fn invalid_moves_are_skipped_and_counted() {
    let g = with_move(make_game(), own(5), |m| {
        only_move(m, Some(30.0));
        tactic_played(m, 3.0);
        m.rec.win_before = f64::NAN;
    });
    let scan = extract_strength_plies_counted(&g);
    assert_eq!(scan.plies, vec![]);
    assert_eq!(scan.skipped_own_plies, 1);

    let g = with_move(make_game(), own(5), |m| {
        only_move(m, Some(30.0));
        m.score_before = EngineScore::Cp(1_000_000);
    });
    assert_eq!(strengths(&g), vec![]);
}

#[test]
fn duplicated_plies_are_dropped_and_counted() {
    let mut g = with_move(make_game(), own(5), |m| only_move(m, Some(30.0)));
    let copy = g.moves[own(5)].clone();
    g.moves.push(copy);
    let scan = extract_strength_plies_counted(&g);
    assert_eq!(scan.plies, vec![]);
    assert_eq!(scan.duplicate_plies, 2);
}

// ------------------------------------------------------------------ ordering and ties

#[test]
fn equal_gaps_break_ties_by_ply() {
    let mut g = make_game();
    for k in [7, 3, 5, 9] {
        g = with_move(g, own(k), |m| only_move(m, Some(25.0)));
    }
    assert_eq!(
        strengths(&g),
        vec![
            sp(OnlyMoveFound, 7),
            sp(OnlyMoveFound, 11),
            sp(OnlyMoveFound, 15)
        ]
    );
}

#[test]
fn unknown_or_invalid_gaps_sort_after_known_gaps_and_are_counted() {
    let mut g = make_game();
    g = with_move(g, own(1), |m| only_move(m, None));
    g = with_move(g, own(2), |m| only_move(m, Some(f64::NAN)));
    g = with_move(g, own(3), |m| only_move(m, Some(5.0)));
    g = with_move(g, own(4), |m| only_move(m, Some(-1.0)));
    let scan = extract_strength_plies_counted(&g);
    // own(3) ply 7 first; then the unknown gaps by ply: 3, 5 (9 is cut by the cap).
    assert_eq!(
        scan.plies,
        vec![
            sp(OnlyMoveFound, 7),
            sp(OnlyMoveFound, 3),
            sp(OnlyMoveFound, 5)
        ]
    );
    assert_eq!(scan.invalid_gaps, 2);
}

#[test]
fn kinds_are_capped_independently_and_ordered_by_kind() {
    let mut g = make_game();
    for k in 10..15 {
        g = with_move(g, own(k), |m| {
            only_move(m, Some(10.0 + k as f64));
            tactic_played(m, 3.0);
        });
    }
    // A defence too: plies 41, 43, 45 at W 12 in a draw.
    let g = with_score(with_w(g, &[41, 43, 45], 12.0), 0.5);
    let got = strengths(&g);
    // own(14) ply 29, own(13) ply 27, own(12) ply 25 by gap; same plies for tactics.
    assert_eq!(
        got,
        vec![
            sp(OnlyMoveFound, 29),
            sp(OnlyMoveFound, 27),
            sp(OnlyMoveFound, 25),
            sp(TacticFound, 29),
            sp(TacticFound, 27),
            sp(TacticFound, 25),
            sp(Defence, 41),
        ]
    );
}

#[test]
fn result_does_not_depend_on_move_order() {
    let mut g = make_game();
    for (k, gap) in [(1, 10.0), (2, 40.0), (3, 20.0), (4, 30.0)] {
        g = with_move(g, own(k), |m| only_move(m, Some(gap)));
    }
    let g = with_score(with_w(g, &[41, 43, 45], 12.0), 0.5);
    let want = strengths(&g);
    let mut r = g.clone();
    r.moves.reverse();
    assert_eq!(strengths(&r), want);
}
