//! R2 Task 9: per-game metrics (TS M2 Task 8 cases plus R2 honesty cases).
//!
//! Fixture: `make_game()` (White player, 30 own moves at W 50, ΔW 0, blitz 300+0 with clocks,
//! plies 1-20 opening, 21-60 middlegame, a draw). Every metric has a positive, a negative and a
//! no-evidence case.

mod common;

use chess_core::{Color, EngineScore, Judgement, Phase, TimeClass};
use common::make_game::*;
use profile_core::types::{
    AnalysedGame, BinomialObs, EndgameOutcome, EndgameType, MissCounts, MotifTag,
    PLAYER_GAME_METRICS_VERSION, PlatformId, PlayerGameMetrics, SeriesKey, StrengthKind,
    StrengthPly, WeaknessKey,
};
use profile_core::weakness::game_metrics::{compute_game_metrics, compute_game_metrics_counted};
use profile_core::weakness::observe_game::{T_WIN, observe_game};
use uuid::Uuid;

const ROOK_ENDING: &str = "8/5k2/8/8/3R4/8/2r5/4K3 w - - 0 1";
const KRVKP: &str = "8/8/4k3/8/4P3/8/4K3/7r w - - 0 1";

fn metrics(g: &AnalysedGame) -> PlayerGameMetrics {
    compute_game_metrics(g, &observe_game(g, T_WIN), vec![])
}

/// `make_game()` extended to `plies` plies with the same neutral pattern.
fn long_game(plies: usize) -> AnalysedGame {
    let mut g = make_game();
    while g.moves.len() < plies {
        let i = g.moves.len();
        let mut m = g.moves[i % 2].clone();
        m.rec.ply = i as u32 + 1;
        m.rec.phase = Phase::Middlegame;
        m.rec.clock_before = Some(150.0);
        if let Some(r) = m.raw.as_mut() {
            r.ply = m.rec.ply;
        }
        g.moves.push(m);
    }
    g
}

fn invalidate(g: &mut AnalysedGame, i: usize) {
    g.moves[i].rec.win_before = f64::NAN;
}

/// Plies 41.. become the endgame, with `fen` before the first endgame ply (41, White's).
fn with_endgame(mut g: AnalysedGame, fen: &str, entry_w: f64) -> AnalysedGame {
    for m in g.moves.iter_mut().filter(|m| m.rec.ply >= 41) {
        m.rec.phase = Phase::Endgame;
    }
    let i = at_ply(41);
    g.moves[i].rec.fen_before = fen.into();
    set_w(&mut g.moves[i], entry_w, entry_w);
    g
}

// ------------------------------------------------------------------ judgements and moves

#[test]
fn judgement_counts() {
    let g = with_move(make_game(), own(0), |m| lose(m, 20.0));
    let g = with_move(g, own(1), |m| lose(m, 12.0));
    let g = with_move(g, own(2), |m| lose(m, 6.0));
    // An opponent blunder is not the player's.
    let g = with_move(g, at_ply(10), |m| lose(m, 30.0));
    let m = metrics(&g);
    assert_eq!((m.blunders, m.mistakes, m.inaccuracies), (1, 1, 1));
    assert_eq!(m.moves, 30);
    assert_eq!(m.version, PLAYER_GAME_METRICS_VERSION);

    let m = metrics(&make_game());
    assert_eq!((m.blunders, m.mistakes, m.inaccuracies), (0, 0, 0));
    assert_eq!(m.moves, 30);
}

#[test]
fn invalid_moves_are_skipped_and_give_no_evidence() {
    let mut g = make_game();
    for i in 0..g.moves.len() {
        invalidate(&mut g, i);
    }
    g.ctx.accuracy = Some(f64::NAN);
    g.ctx.score = Some(f64::NAN);
    let s = compute_game_metrics_counted(&g, &observe_game(&g, T_WIN), vec![]);
    let m = &s.metrics;
    assert_eq!(s.skipped_moves, 60);
    assert!(s.invalid_game_accuracy);
    assert_eq!(m.moves, 0);
    assert_eq!(m.score, None);
    assert_eq!(m.accuracy, None);
    assert_eq!(m.accuracy_by_phase.opening, None);
    assert_eq!(m.quiet.moves, 0);
    assert_eq!(m.sharp.positions, 0);
    assert_eq!(m.missed_punish.opportunities, 0);
    assert_eq!(m.opening.win_at_opening_end, None);
    assert_eq!(m.opening.eval_at_opening_end_white_cp, None);
    assert_eq!(m.eval_trend_15_to_35, None);
    assert_eq!(m.critical_time_ratio, None);
    assert_eq!(m.time_per_move.opening, None);
    assert_eq!(m.time_in_opening, None);
    // Only clock-based W9a survives (T8: clock readings do not depend on the evals).
    assert!(m.weakness.keys().all(|k| *k == WeaknessKey::W9a));
}

#[test]
fn duplicate_plies_are_dropped_and_counted() {
    let mut g = with_move(make_game(), own(3), |m| lose(m, 20.0));
    let copy = g.moves[own(3)].clone();
    g.moves.push(copy);
    let s = compute_game_metrics_counted(&g, &observe_game(&g, T_WIN), vec![]);
    assert_eq!(s.duplicate_plies, 2);
    assert_eq!(s.metrics.blunders, 0);
    assert_eq!(s.metrics.moves, 29);
}

#[test]
fn result_does_not_depend_on_move_order() {
    let g = with_move(make_game(), own(4), |m| lose(m, 20.0));
    let g = with_move(g, at_ply(10), |m| lose(m, 20.0));
    let mut r = g.clone();
    r.moves.reverse();
    assert_eq!(metrics(&g), metrics(&r));
}

// ------------------------------------------------------------------ accuracy

#[test]
fn accuracy_by_phase() {
    let mut g = make_game();
    g.moves.truncate(6); // own plies 1, 3, 5
    g.moves[own(0)].rec.accuracy = 100.0;
    g.moves[own(1)].rec.accuracy = 80.0;
    g.moves[own(2)].rec.accuracy = 60.0;
    g.moves[own(2)].rec.phase = Phase::Middlegame;
    // Opponent accuracy is not the player's.
    g.moves[at_ply(2)].rec.accuracy = 0.0;
    let m = metrics(&g);
    assert_eq!(m.accuracy_by_phase.opening, Some(90.0));
    assert_eq!(m.accuracy_by_phase.middlegame, Some(60.0));
    assert_eq!(m.accuracy_by_phase.endgame, None);
}

#[test]
fn invalid_move_accuracy_is_skipped_and_counted() {
    let mut g = make_game();
    g.moves.truncate(4);
    g.moves[own(0)].rec.accuracy = f64::NAN;
    g.moves[own(1)].rec.accuracy = 70.0;
    let s = compute_game_metrics_counted(&g, &observe_game(&g, T_WIN), vec![]);
    assert_eq!(s.metrics.accuracy_by_phase.opening, Some(70.0));
    assert_eq!(s.invalid_move_accuracy, 1);

    g.moves[own(1)].rec.accuracy = 101.0;
    let s = compute_game_metrics_counted(&g, &observe_game(&g, T_WIN), vec![]);
    assert_eq!(s.metrics.accuracy_by_phase.opening, None);
    assert_eq!(s.invalid_move_accuracy, 2);
}

#[test]
fn game_accuracy_is_the_valid_context_value() {
    let mut g = make_game();
    g.ctx.accuracy = Some(87.5);
    assert_eq!(metrics(&g).accuracy, Some(87.5));
    g.ctx.accuracy = None;
    assert_eq!(metrics(&g).accuracy, None);
    g.ctx.accuracy = Some(150.0);
    let s = compute_game_metrics_counted(&g, &observe_game(&g, T_WIN), vec![]);
    assert_eq!(s.metrics.accuracy, None);
    assert!(s.invalid_game_accuracy);
}

// ------------------------------------------------------------------ one-move blunders

#[test]
fn one_move_blunders() {
    // Refutation realised in one ply.
    let g = with_move(make_game(), own(5), |m| {
        lose(m, 20.0);
        m.f.refutation = Some(line(3.0, Some(1), None, &[]));
    });
    assert_eq!(metrics(&g).one_move_blunders, 1);

    // A missed mate in 1 that was not played.
    let g = with_move(make_game(), own(5), |m| {
        lose(m, 20.0);
        m.f.best = Some(line(100.0, Some(1), Some(1), &[MotifTag::MateIn1]));
    });
    assert_eq!(metrics(&g).one_move_blunders, 1);

    // Deeper refutation: a blunder, but not a one-move blunder.
    let g = with_move(make_game(), own(5), |m| {
        lose(m, 20.0);
        m.f.refutation = Some(line(3.0, Some(2), None, &[]));
    });
    let m = metrics(&g);
    assert_eq!((m.blunders, m.one_move_blunders), (1, 0));

    // Not decisive (ΔW 14.9).
    let g = with_move(make_game(), own(5), |m| {
        lose(m, 14.9);
        m.f.refutation = Some(line(3.0, Some(1), None, &[]));
    });
    assert_eq!(metrics(&g).one_move_blunders, 0);

    assert_eq!(metrics(&make_game()).one_move_blunders, 0);
}

#[test]
fn one_move_blunder_unknown_without_lines_is_counted_not_claimed() {
    let g = with_move(make_game(), own(5), |m| {
        lose(m, 20.0);
        m.f.refutation = None;
        m.f.best = None;
    });
    let s = compute_game_metrics_counted(&g, &observe_game(&g, T_WIN), vec![]);
    assert_eq!(s.metrics.one_move_blunders, 0);
    assert_eq!(s.unknown_one_move_blunders, 1);

    // One known line that already answers "yes" is enough.
    let g = with_move(make_game(), own(5), |m| {
        lose(m, 20.0);
        m.f.refutation = Some(line(3.0, Some(1), None, &[]));
        m.f.best = None;
    });
    let s = compute_game_metrics_counted(&g, &observe_game(&g, T_WIN), vec![]);
    assert_eq!(s.metrics.one_move_blunders, 1);
    assert_eq!(s.unknown_one_move_blunders, 0);

    // A known "no" refutation with an unknown best line could still be a missed mate in 1.
    let g = with_move(make_game(), own(5), |m| {
        lose(m, 20.0);
        m.f.refutation = Some(line(3.0, Some(2), None, &[]));
        m.f.best = None;
    });
    let s = compute_game_metrics_counted(&g, &observe_game(&g, T_WIN), vec![]);
    assert_eq!(s.metrics.one_move_blunders, 0);
    assert_eq!(s.unknown_one_move_blunders, 1);
}

#[test]
fn playing_the_mate_is_not_a_one_move_blunder() {
    let g = with_move(make_game(), own(5), |m| {
        lose(m, 20.0);
        m.rec.san = "Qh7#".into();
        m.f.refutation = Some(line(0.0, None, None, &[]));
        m.f.best = Some(line(100.0, Some(1), Some(1), &[MotifTag::MateIn1]));
    });
    assert_eq!(metrics(&g).one_move_blunders, 0);
}

// ------------------------------------------------------------------ missed punishment

#[test]
fn missed_punish_after_opponent_blunder() {
    let g = with_move(make_game(), at_ply(10), |m| lose(m, 20.0));
    let g = with_move(g, at_ply(11), |m| lose(m, 12.0));
    let m = metrics(&g);
    assert_eq!(
        m.missed_punish,
        MissCounts {
            opportunities: 1,
            misses: 1
        }
    );

    // Boundary: ΔW 10 is a miss, 9.9 is not.
    let base = with_move(make_game(), at_ply(10), |m| lose(m, 12.0)); // a mistake opens it too
    let g = with_move(base.clone(), at_ply(11), |m| lose(m, 10.0));
    assert_eq!(metrics(&g).missed_punish.misses, 1);
    let g = with_move(base, at_ply(11), |m| lose(m, 9.9));
    assert_eq!(
        metrics(&g).missed_punish,
        MissCounts {
            opportunities: 1,
            misses: 0
        }
    );

    // An opponent inaccuracy opens nothing.
    let g = with_move(make_game(), at_ply(10), |m| lose(m, 6.0));
    assert_eq!(metrics(&g).missed_punish.opportunities, 0);
    assert_eq!(metrics(&make_game()).missed_punish, MissCounts::default());
}

#[test]
fn missed_punish_needs_both_moves_valid() {
    let mut g = with_move(make_game(), at_ply(10), |m| lose(m, 20.0));
    g = with_move(g, at_ply(11), |m| lose(m, 12.0));
    let mut opp_bad = g.clone();
    invalidate(&mut opp_bad, at_ply(10));
    opp_bad.moves[at_ply(10)].rec.judgement = Some(Judgement::Blunder);
    assert_eq!(metrics(&opp_bad).missed_punish.opportunities, 0);

    let mut own_bad = g.clone();
    invalidate(&mut own_bad, at_ply(11));
    assert_eq!(metrics(&own_bad).missed_punish.opportunities, 0);

    // The previous ply missing entirely: no opportunity.
    let mut gap = g;
    gap.moves.remove(at_ply(10));
    assert_eq!(metrics(&gap).missed_punish.opportunities, 0);
}

// ------------------------------------------------------------------ ratings, score, termination

#[test]
fn expected_score_from_ratings() {
    let mut g = make_game();
    g.ctx.player_rating = Some(1500);
    g.ctx.opponent_rating = Some(1700);
    let m = metrics(&g);
    let e = m.expected_score.unwrap();
    assert_eq!((e * 10_000.0).round() / 10_000.0, 0.2403);
    assert_eq!(
        (m.player_rating, m.opponent_rating),
        (Some(1500), Some(1700))
    );

    g.ctx.opponent_rating = None;
    assert_eq!(metrics(&g).expected_score, None);

    g.ctx.opponent_rating = Some(0);
    let s = compute_game_metrics_counted(&g, &observe_game(&g, T_WIN), vec![]);
    assert_eq!(s.metrics.expected_score, None);
    assert!(s.invalid_ratings);
}

#[test]
fn score_is_the_valid_player_score() {
    assert_eq!(metrics(&with_score(make_game(), 1.0)).score, Some(1.0));
    assert_eq!(metrics(&make_game()).score, Some(0.5));
    let mut g = make_game();
    g.ctx.score = Some(0.3);
    assert_eq!(metrics(&g).score, None);
    g.ctx.score = None;
    assert_eq!(metrics(&g).score, None);
}

#[test]
fn lost_on_time() {
    let mut g = with_score(make_game(), 0.0);
    g.ctx.platform = PlatformId::Lichess;
    g.ctx.termination = Some("outoftime".into());
    assert!(metrics(&g).lost_on_time);

    let mut won = with_score(make_game(), 1.0);
    won.ctx.termination = Some("outoftime".into());
    assert!(!metrics(&won).lost_on_time);

    assert!(!metrics(&with_score(make_game(), 0.0)).lost_on_time);
}

// ------------------------------------------------------------------ observation pass-through

#[test]
fn weakness_equals_observation() {
    let g = with_move(make_game(), own(5), |m| {
        lose(m, 20.0);
        m.f.refutation = Some(line(3.0, Some(1), None, &[]));
    });
    let obs = observe_game(&g, T_WIN);
    let m = compute_game_metrics(&g, &obs, vec![]);
    assert_eq!(m.weakness[&WeaknessKey::W1], obs.binomial[&WeaknessKey::W1]);
    assert_eq!(m.weakness, obs.binomial);
    assert_eq!(m.weakness[&WeaknessKey::W1].events, 1);
}

#[test]
fn observation_of_another_game_is_refused() {
    let g = with_move(make_game(), own(5), |m| lose(m, 20.0));
    let mut obs = observe_game(&g, T_WIN);
    obs.game_id = Uuid::from_u128(0xdead);
    obs.w9d_rho = Some(0.5);
    let s = compute_game_metrics_counted(&g, &obs, vec![]);
    assert!(s.observation_mismatch);
    assert!(s.metrics.weakness.is_empty());
    assert!(s.metrics.missed_by_motif.is_empty());
    assert_eq!(s.metrics.w9d_rho, None);
    assert_eq!(s.metrics.tactics.opportunities, 0);
    // Move-level metrics are still computed from the game itself.
    assert_eq!(s.metrics.blunders, 1);
}

#[test]
fn invalid_observation_rows_are_skipped_and_counted() {
    let g = make_game();
    let mut obs = observe_game(&g, T_WIN);
    obs.binomial.insert(
        WeaknessKey::W2,
        BinomialObs {
            events: 3,
            opportunities: 2,
            epl: 0.0,
            exemplar_plies: vec![],
        },
    );
    obs.binomial.insert(
        WeaknessKey::W11,
        BinomialObs {
            events: 0,
            opportunities: 2,
            epl: f64::NAN,
            exemplar_plies: vec![],
        },
    );
    obs.binomial.insert(
        WeaknessKey::W5,
        BinomialObs {
            events: 0,
            opportunities: 0,
            epl: 0.0,
            exemplar_plies: vec![],
        },
    );
    obs.w9d_rho = Some(f64::NAN);
    let s = compute_game_metrics_counted(&g, &obs, vec![]);
    // Three rows plus the NaN rho.
    assert_eq!(s.invalid_observation_rows, 4);
    assert!(!s.metrics.weakness.contains_key(&WeaknessKey::W2));
    assert!(!s.metrics.weakness.contains_key(&WeaknessKey::W11));
    assert!(!s.metrics.weakness.contains_key(&WeaknessKey::W5));
    assert!(s.metrics.weakness.contains_key(&WeaknessKey::W1));
    assert_eq!(s.metrics.w9d_rho, None);

    obs.w9d_rho = Some(0.25);
    assert_eq!(compute_game_metrics(&g, &obs, vec![]).w9d_rho, Some(0.25));
}

#[test]
fn invalid_motif_series_rows_are_skipped_and_counted() {
    let g = with_move(make_game(), own(5), |m| {
        m.f.best = Some(line(3.0, Some(1), None, &[MotifTag::Fork]));
        lose(m, 16.0);
    });
    let mut obs = observe_game(&g, T_WIN);
    let fork = obs
        .series
        .iter_mut()
        .find(|s| s.series_key == SeriesKey::MotifMissed(MotifTag::Fork))
        .unwrap();
    fork.events = 2; // more events than opportunities (1)
    let s = compute_game_metrics_counted(&g, &obs, vec![]);
    assert!(!s.metrics.missed_by_motif.contains_key(&MotifTag::Fork));
    assert_eq!(s.invalid_observation_rows, 1);

    let fork = obs
        .series
        .iter_mut()
        .find(|s| s.series_key == SeriesKey::MotifMissed(MotifTag::Fork))
        .unwrap();
    fork.events = 1;
    fork.epl = f64::INFINITY;
    let s = compute_game_metrics_counted(&g, &obs, vec![]);
    assert!(!s.metrics.missed_by_motif.contains_key(&MotifTag::Fork));
    assert_eq!(s.invalid_observation_rows, 1);
}

#[test]
fn strengths_are_passed_through() {
    let g = make_game();
    let strengths = vec![
        StrengthPly {
            kind: StrengthKind::OnlyMoveFound,
            ply: 23,
        },
        StrengthPly {
            kind: StrengthKind::Defence,
            ply: 41,
        },
    ];
    let m = compute_game_metrics(&g, &observe_game(&g, T_WIN), strengths.clone());
    assert_eq!(m.strengths, strengths);
    assert!(metrics(&g).strengths.is_empty());
}

#[test]
fn json_shape_is_camel_case_version_1() {
    let m = metrics(&make_game());
    let v = serde_json::to_value(&m).unwrap();
    assert_eq!(v["version"], 1);
    assert!(v.get("accuracyByPhase").is_some());
    assert!(v.get("oneMoveBlunders").is_some());
    assert!(v.get("missedPunish").is_some());
    assert!(v.get("evalTrend15to35").is_some());
    assert!(v["opening"].get("evalAtOpeningEndWhiteCp").is_some());
    let back: PlayerGameMetrics = serde_json::from_value(v).unwrap();
    assert_eq!(back, m);
}

// ------------------------------------------------------------------ sharp positions

#[test]
fn sharp_counts_eligible_critical_moves() {
    let g = with_move(make_game(), own(5), |m| {
        m.f.is_critical = Some(true);
        lose(m, 6.0);
    });
    let g = with_move(g, own(6), |m| m.f.is_critical = Some(true));
    // Unknown criticality and decided positions are not sharp positions.
    let g = with_move(g, own(7), |m| {
        m.f.is_critical = None;
        lose(m, 20.0);
    });
    let g = with_move(g, own(8), |m| {
        m.f.is_critical = Some(true);
        set_w(m, 95.0, 70.0);
    });
    let m = metrics(&g);
    assert_eq!((m.sharp.positions, m.sharp.errors), (2, 1));
    assert_eq!(metrics(&make_game()).sharp.positions, 0);
}

// ------------------------------------------------------------------ eval trend

#[test]
fn eval_trend_is_player_w_after_ply_70_minus_after_ply_30() {
    let mut g = long_game(70);
    // Plies 30 and 70 are Black's: player (White) W = 100 − Black's W.
    g.moves[at_ply(30)].rec.win_after = 60.0;
    g.moves[at_ply(70)].rec.win_after = 30.0;
    assert_eq!(metrics(&g).eval_trend_15_to_35, Some(30.0));

    assert_eq!(metrics(&long_game(70)).eval_trend_15_to_35, Some(0.0));
    // Too short: ply 70 was never played.
    assert_eq!(metrics(&make_game()).eval_trend_15_to_35, None);
    let mut bad = long_game(70);
    invalidate(&mut bad, at_ply(70));
    assert_eq!(metrics(&bad).eval_trend_15_to_35, None);
    let mut bad = long_game(70);
    invalidate(&mut bad, at_ply(30));
    assert_eq!(metrics(&bad).eval_trend_15_to_35, None);
}

// ------------------------------------------------------------------ time

#[test]
fn critical_time_ratio() {
    let g = with_move(make_game(), own(5), |m| {
        m.f.is_critical = Some(true);
        m.rec.time_spent = Some(10.0);
    });
    // Unknown criticality is in neither group.
    let g = with_move(g, own(6), |m| {
        m.f.is_critical = None;
        m.rec.time_spent = Some(100.0);
    });
    assert_eq!(metrics(&g).critical_time_ratio, Some(2.0));

    // No critical move: no ratio.
    assert_eq!(metrics(&make_game()).critical_time_ratio, None);

    // Non-critical moves took no time: no ratio rather than infinity.
    let mut z = with_move(make_game(), own(5), |m| m.f.is_critical = Some(true));
    for (k, m) in z.moves.iter_mut().enumerate() {
        if k != own(5) {
            m.rec.time_spent = Some(0.0);
        }
    }
    assert_eq!(metrics(&z).critical_time_ratio, None);
    assert_eq!(metrics(&without_clocks(g)).critical_time_ratio, None);
}

#[test]
fn time_per_move_and_time_in_opening() {
    let g = with_move(make_game(), own(12), |m| m.rec.time_spent = Some(25.0));
    let m = metrics(&g);
    assert_eq!(m.time_per_move.opening, Some(5.0));
    // Middlegame own moves are plies 21..59 (20 moves): (19·5 + 25) / 20.
    assert_eq!(m.time_per_move.middlegame, Some(6.0));
    assert_eq!(m.time_per_move.endgame, None);
    // 10 own opening moves of 5 s.
    assert_eq!(m.time_in_opening, Some(50.0));

    let m = metrics(&without_clocks(make_game()));
    assert_eq!(m.time_per_move.opening, None);
    assert_eq!(m.time_in_opening, None);

    // One own opening move without a time: the total is unknown, not an undercount.
    let g = with_move(make_game(), own(3), |m| m.rec.time_spent = None);
    let m = metrics(&g);
    assert_eq!(m.time_in_opening, None);
    assert_eq!(m.time_per_move.opening, Some(5.0));
    let mut g = make_game();
    invalidate(&mut g, own(3));
    assert_eq!(metrics(&g).time_in_opening, None);
}

#[test]
fn time_trouble_and_clock_buckets() {
    let m = metrics(&make_game());
    assert_eq!(m.time_trouble, Some(false));
    // Own clocks run 300..155: all above 60 s.
    assert_eq!((m.high_clock.moves, m.high_clock.decisive), (30, 0));
    assert_eq!((m.low_clock.moves, m.low_clock.decisive), (0, 0));

    let g = with_move(make_game(), own(20), |m| {
        m.rec.clock_before = Some(20.0);
        lose(m, 20.0);
    });
    let g = with_move(g, own(21), |m| m.rec.clock_before = Some(29.0));
    let g = with_move(g, own(22), |m| {
        m.rec.clock_before = Some(45.0);
        lose(m, 20.0);
    });
    let obs = observe_game(&g, T_WIN);
    let m = compute_game_metrics(&g, &obs, vec![]);
    assert_eq!(m.time_trouble, Some(true));
    assert_eq!((m.low_clock.moves, m.low_clock.decisive), (2, 1));
    assert_eq!(
        m.low_clock.moves,
        obs.binomial[&WeaknessKey::W9b].opportunities
    );
    // 45 s is in neither bucket.
    assert_eq!((m.high_clock.moves, m.high_clock.decisive), (27, 0));

    let m = metrics(&without_clocks(make_game()));
    assert_eq!(m.time_trouble, None);
    assert_eq!((m.low_clock.moves, m.high_clock.moves), (0, 0));

    let mut daily = make_game();
    daily.ctx.time_class = TimeClass::Daily;
    let m = metrics(&daily);
    assert_eq!(m.time_trouble, None);
    assert_eq!((m.low_clock.moves, m.high_clock.moves), (0, 0));
}

// ------------------------------------------------------------------ episodes

fn winning_run(g: AnalysedGame) -> AnalysedGame {
    let mut g = g;
    for k in 10..13 {
        set_w(&mut g.moves[own(k)], 90.0, 90.0);
    }
    g
}

#[test]
fn conversion_from_the_winning_episode() {
    let m = metrics(&with_score(winning_run(make_game()), 1.0));
    assert!(m.conversion.episode);
    assert_eq!(m.conversion.converted, Some(true));

    let g = with_move(winning_run(make_game()), own(12), |m| set_w(m, 90.0, 60.0));
    let m = metrics(&with_score(g, 0.5));
    assert!(m.conversion.episode);
    assert_eq!(m.conversion.converted, Some(false));
    assert_eq!(m.conversion.drop_ply, Some(25));

    let m = metrics(&make_game());
    assert!(!m.conversion.episode);
    assert_eq!(m.conversion.converted, None);

    let mut g = winning_run(make_game());
    g.ctx.score = None;
    assert!(!metrics(&g).conversion.episode);
}

#[test]
fn resilience_from_the_losing_episode() {
    let mut g = make_game();
    for k in 10..13 {
        set_w(&mut g.moves[own(k)], 10.0, 10.0);
    }
    let m = metrics(&with_score(g.clone(), 0.5));
    assert!(m.resilience.episode);
    assert_eq!(m.resilience.saved, Some(true));
    let m = metrics(&with_score(g, 0.0));
    assert_eq!(m.resilience.saved, Some(false));
    let m = metrics(&make_game());
    assert!(!m.resilience.episode);
    assert_eq!(m.resilience.saved, None);
}

// ------------------------------------------------------------------ tactics

#[test]
fn tactics_missed_and_allowed() {
    let g = with_move(make_game(), own(5), |m| {
        m.f.best = Some(line(3.0, Some(1), None, &[MotifTag::Fork]));
        lose(m, 16.0);
    });
    let g = with_move(g, own(6), |m| {
        m.f.best = Some(line(3.0, Some(5), None, &[MotifTag::Pin]));
    });
    let g = with_move(g, own(8), |m| {
        lose(m, 20.0);
        m.f.refutation = Some(line(3.0, Some(3), None, &[MotifTag::Fork]));
    });
    let obs = observe_game(&g, T_WIN);
    let m = compute_game_metrics(&g, &obs, vec![]);
    assert_eq!(m.tactics.opportunities, 2);
    assert_eq!(m.tactics.missed, 1);
    assert_eq!(m.tactics.allowed, 1);
    assert_eq!(
        m.missed_by_motif[&MotifTag::Fork],
        MissCounts {
            opportunities: 1,
            misses: 1
        }
    );
    assert_eq!(
        m.missed_by_motif[&MotifTag::Pin],
        MissCounts {
            opportunities: 1,
            misses: 0
        }
    );
    assert!(!m.missed_by_motif.contains_key(&MotifTag::Skewer));

    let m = metrics(&make_game());
    assert_eq!(
        (m.tactics.opportunities, m.tactics.missed, m.tactics.allowed),
        (0, 0, 0)
    );
    assert!(m.missed_by_motif.is_empty());
}

#[test]
fn tactics_allowed_needs_a_tactical_refutation() {
    // A decisive move whose refutation wins nothing: allowed no tactic.
    let g = with_move(make_game(), own(8), |m| lose(m, 20.0));
    assert_eq!(metrics(&g).tactics.allowed, 0);
    // A mate counts as a tactic.
    let g = with_move(make_game(), own(8), |m| {
        lose(m, 20.0);
        m.f.refutation = Some(line(0.0, Some(3), Some(2), &[MotifTag::MateIn2]));
    });
    assert_eq!(metrics(&g).tactics.allowed, 1);
    // A decided position is not counted.
    let g = with_move(make_game(), own(8), |m| {
        set_w(m, 95.0, 70.0);
        m.f.refutation = Some(line(3.0, Some(1), None, &[]));
    });
    assert_eq!(metrics(&g).tactics.allowed, 0);
    // Unknown refutation on a decisive move: counted, not claimed.
    let g = with_move(make_game(), own(8), |m| {
        lose(m, 20.0);
        m.f.refutation = None;
    });
    let s = compute_game_metrics_counted(&g, &observe_game(&g, T_WIN), vec![]);
    assert_eq!(s.metrics.tactics.allowed, 0);
    assert_eq!(s.unknown_tactics_allowed, 1);
}

#[test]
fn tactics_follow_the_whole_game_best_line_rule() {
    let g = with_move(make_game(), own(5), |m| {
        m.f.best = Some(line(3.0, Some(1), None, &[MotifTag::Fork]));
        lose(m, 16.0);
    });
    let g = with_move(g, own(9), |m| m.f.best = None);
    let m = metrics(&g);
    assert_eq!((m.tactics.opportunities, m.tactics.missed), (0, 0));
    assert!(m.missed_by_motif.is_empty());
}

// ------------------------------------------------------------------ quiet moves

#[test]
fn quiet_loss_over_w10_moves() {
    let g = with_move(make_game(), own(15), |m| lose(m, 6.0));
    // A critical move is not quiet.
    let g = with_move(g, own(16), |m| {
        m.f.is_critical = Some(true);
        lose(m, 8.0);
    });
    let obs = observe_game(&g, T_WIN);
    let m = compute_game_metrics(&g, &obs, vec![]);
    // Own middlegame moves: plies 21..59 = 20, minus the critical one.
    assert_eq!(m.quiet.moves, 19);
    assert_eq!(m.quiet.moves, obs.binomial[&WeaknessKey::W10].opportunities);
    assert_eq!(m.quiet.loss, 6.0);

    assert_eq!(metrics(&make_game()).quiet.loss, 0.0);
    assert_eq!(metrics(&make_game()).quiet.moves, 20);

    // Unbalanced position (cp 200) is not quiet; a missing best line drops the game (W10 rule).
    let g = with_move(make_game(), own(15), |m| {
        m.score_before = EngineScore::Cp(200)
    });
    assert_eq!(metrics(&g).quiet.moves, 19);
    let g = with_move(make_game(), own(15), |m| m.f.best = None);
    let m = metrics(&g);
    assert_eq!((m.quiet.moves, m.quiet.loss), (0, 0.0));
}

// ------------------------------------------------------------------ opening

#[test]
fn opening_metrics() {
    let m = metrics(&make_game());
    assert_eq!(m.opening.line_key.as_deref(), Some("white:italian-game"));
    assert_eq!(m.opening.book_exit_ply, Some(8));
    assert_eq!(m.opening.win_at_opening_end, Some(50.0));
    assert_eq!(m.opening.out_worse, Some(false));
    assert_eq!(m.opening.eval_at_opening_end_white_cp, Some(0));

    // Ply 20 is the last opening ply (Black's): the player's W is 100 − Black's.
    let g = with_move(make_game(), at_ply(20), |m| {
        m.rec.win_after = 70.0;
        m.score_after = EngineScore::Cp(200);
    });
    let m = metrics(&g);
    assert_eq!(m.opening.win_at_opening_end, Some(30.0));
    assert_eq!(m.opening.out_worse, Some(true));
    assert_eq!(m.opening.eval_at_opening_end_white_cp, Some(-200));

    // Boundary: W 40 is out worse.
    let g = with_move(make_game(), at_ply(20), |m| m.rec.win_after = 60.0);
    assert_eq!(metrics(&g).opening.out_worse, Some(true));

    // A mate for Black is −1000 for White (R-8).
    let g = with_move(make_game(), at_ply(20), |m| {
        m.score_after = EngineScore::Mate(3)
    });
    assert_eq!(
        metrics(&g).opening.eval_at_opening_end_white_cp,
        Some(-1000)
    );

    let mut bad = make_game();
    invalidate(&mut bad, at_ply(20));
    let m = metrics(&bad);
    assert_eq!(m.opening.win_at_opening_end, None);
    assert_eq!(m.opening.out_worse, None);
    assert_eq!(m.opening.eval_at_opening_end_white_cp, None);
}

#[test]
fn opening_metrics_for_a_black_player() {
    let mut g = make_game();
    g.ctx.player_color = Color::Black;
    // Black played ply 20 and holds W 70 after it; White's view of the score is −150.
    g.moves[at_ply(20)].rec.win_after = 70.0;
    g.moves[at_ply(20)].score_after = EngineScore::Cp(150);
    let m = metrics(&g);
    assert_eq!(m.opening.win_at_opening_end, Some(70.0));
    assert_eq!(m.opening.eval_at_opening_end_white_cp, Some(-150));
    assert_eq!(m.moves, 30);
}

// ------------------------------------------------------------------ endgame

#[test]
fn endgame_outcomes() {
    let cases = [
        (80.0, 1.0, Some(EndgameOutcome::Converted)),
        (80.0, 0.5, Some(EndgameOutcome::FailedConversion)),
        (70.0, 0.0, Some(EndgameOutcome::FailedConversion)),
        (20.0, 0.5, Some(EndgameOutcome::Held)),
        (30.0, 1.0, Some(EndgameOutcome::Held)),
        (20.0, 0.0, Some(EndgameOutcome::FailedHold)),
        (50.0, 0.0, None),
    ];
    for (entry, score, outcome) in cases {
        let g = with_score(with_endgame(make_game(), ROOK_ENDING, entry), score);
        let m = metrics(&g);
        assert_eq!(m.endgame.kind, Some(EndgameType::Rook), "{entry} {score}");
        assert_eq!(m.endgame.entry_win, Some(entry), "{entry} {score}");
        assert_eq!(m.endgame.outcome, outcome, "{entry} {score}");
    }
    let m = metrics(&make_game());
    assert_eq!(m.endgame.kind, None);
    assert_eq!(m.endgame.entry_win, None);
    assert_eq!(m.endgame.outcome, None);
    assert_eq!(m.accuracy_by_phase.endgame, None);
}

#[test]
fn endgame_without_valid_inputs_claims_nothing() {
    // Unknown result: no outcome.
    let mut g = with_endgame(make_game(), ROOK_ENDING, 80.0);
    g.ctx.score = None;
    let m = metrics(&g);
    assert_eq!(m.endgame.entry_win, Some(80.0));
    assert_eq!(m.endgame.outcome, None);

    // Invalid FEN: no type.
    let g = with_endgame(make_game(), "not a fen", 80.0);
    assert_eq!(metrics(&g).endgame.kind, None);

    // Invalid entry move: nothing about the entry.
    let mut g = with_score(with_endgame(make_game(), ROOK_ENDING, 80.0), 1.0);
    invalidate(&mut g, at_ply(41));
    let m = metrics(&g);
    assert_eq!(m.endgame.entry_win, None);
    assert_eq!(m.endgame.outcome, None);
}

#[test]
fn tablebase_positions_and_errors() {
    let g = with_endgame(make_game(), KRVKP, 50.0);
    let g = with_move(g, at_ply(41), |m| {
        let r = m.raw.as_mut().unwrap();
        r.tb_wdl_before = Some(1);
        r.tb_wdl_after = Some(0);
    });
    let g = with_move(g, at_ply(43), |m| {
        m.rec.fen_before = KRVKP.into();
        let r = m.raw.as_mut().unwrap();
        r.tb_wdl_before = Some(0);
        r.tb_wdl_after = Some(0);
    });
    // Out of range: invalid, not a position.
    let g = with_move(g, at_ply(45), |m| {
        m.rec.fen_before = KRVKP.into();
        let r = m.raw.as_mut().unwrap();
        r.tb_wdl_before = Some(2);
        r.tb_wdl_after = Some(0);
    });
    let obs = observe_game(&g, T_WIN);
    let m = compute_game_metrics(&g, &obs, vec![]);
    assert_eq!((m.endgame.tb_positions, m.endgame.tb_errors), (2, 1));
    let w8 = |k| {
        obs.binomial
            .get(&k)
            .map_or((0, 0), |b| (b.opportunities, b.events))
    };
    let (bo, be) = w8(WeaknessKey::W8Basics);
    let (ao, ae) = w8(WeaknessKey::W8Advanced);
    assert_eq!((bo + ao, be + ae), (2, 1));

    let m = metrics(&make_game());
    assert_eq!((m.endgame.tb_positions, m.endgame.tb_errors), (0, 0));
}

// ------------------------------------------------------------------ resignation

fn resigned(mut g: AnalysedGame) -> AnalysedGame {
    g = with_score(g, 0.0);
    g.ctx.platform = PlatformId::Lichess;
    g.ctx.termination = Some("resign".into());
    g
}

#[test]
fn resigned_saveable() {
    // Last ply 60 is Black's at W 50: the player's final W is 50.
    assert_eq!(
        metrics(&resigned(make_game())).resigned_saveable,
        Some(true)
    );

    // Boundary: final W 25 is saveable, 24.9 is not.
    let g = with_move(resigned(make_game()), at_ply(60), |m| {
        m.rec.win_after = 75.0
    });
    assert_eq!(metrics(&g).resigned_saveable, Some(true));
    let g = with_move(resigned(make_game()), at_ply(60), |m| {
        m.rec.win_after = 75.1
    });
    assert_eq!(metrics(&g).resigned_saveable, Some(false));

    // No resignation by the player: no claim.
    assert_eq!(metrics(&make_game()).resigned_saveable, None);
    let mut opp = resigned(make_game());
    opp = with_score(opp, 1.0);
    assert_eq!(metrics(&opp).resigned_saveable, None);

    // The final position unknown: no claim.
    let mut g = resigned(make_game());
    invalidate(&mut g, at_ply(60));
    assert_eq!(metrics(&g).resigned_saveable, None);
    let mut g = resigned(make_game());
    g.moves.clear();
    assert_eq!(metrics(&g).resigned_saveable, None);
}
