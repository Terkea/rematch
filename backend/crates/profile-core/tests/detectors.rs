//! R2 Task 8: weakness detectors W1-W12 (TS M2 Task 7 cases, plus the R2 honesty rules: nothing
//! claimed from zero evidence, denominators count only computable opportunities, invalid inputs
//! skipped and counted, no NaN or inf).

mod common;

use chess_core::{EngineScore, Judgement, Phase, TimeClass};
use common::make_game::*;
use profile_core::types::{
    AnalysedGame, BinomialObs, EndgameType, GameObservation, MotifTag, SeriesKey, SeriesObs,
    SessionInfo, WeaknessKey,
};
use profile_core::weakness::observe_game::{
    ObservationScan, T_WIN, observe_game, observe_game_counted, win_at_opening_end,
};
use profile_core::weakness::tilt::{tilt_residual_difference, tilt_residual_difference_counted};
use uuid::Uuid;

use WeaknessKey::*;

const EPS: f64 = 1e-9;

fn obs(g: &AnalysedGame) -> GameObservation {
    observe_game(g, T_WIN)
}

fn key(o: &GameObservation, k: WeaknessKey) -> &BinomialObs {
    o.binomial
        .get(&k)
        .unwrap_or_else(|| panic!("{k} absent; present: {:?}", o.binomial.keys()))
}

fn ev(o: &GameObservation, k: WeaknessKey) -> (u32, u32) {
    let b = key(o, k);
    (b.events, b.opportunities)
}

fn series<'a>(o: &'a GameObservation, k: &str) -> Option<&'a SeriesObs> {
    let k: SeriesKey = k.parse().unwrap();
    o.series.iter().find(|s| s.series_key == k)
}

fn all_finite(o: &GameObservation) -> bool {
    o.binomial.values().all(|b| b.epl.is_finite())
        && o.series.iter().all(|s| s.epl.is_finite())
        && o.w9d_rho.is_none_or(f64::is_finite)
}

/// The W1 move of TS M2 Task 7: player move 15 (ply 31), ΔW 20, refutation gain 3 at depth 1.
fn w1_game() -> AnalysedGame {
    with_move(make_game(), own(15), |m| {
        lose(m, 20.0);
        m.f.refutation = Some(line(3.0, Some(1), None, &[MotifTag::Fork]));
    })
}

// ---------------------------------------------------------------- neutral game

#[test]
fn neutral_game_has_no_events_and_only_computable_opportunities() {
    let o = obs(&make_game());
    assert_eq!(o.game_id, make_game().ctx.game_id);
    for (k, b) in &o.binomial {
        assert_eq!(b.events, 0, "{k}");
        assert_eq!(b.epl, 0.0, "{k}");
        assert!(b.exemplar_plies.is_empty(), "{k}");
        assert!(b.opportunities > 0, "{k} present with zero opportunities");
    }
    assert_eq!(ev(&o, W1), (0, 30));
    assert_eq!(ev(&o, W7Traps), (0, 1));
    assert_eq!(ev(&o, W7Prep), (0, 1));
    assert_eq!(ev(&o, W9a), (0, 1));
    // Middlegame own moves: plies 21..59 -> 20.
    assert_eq!(ev(&o, W10), (0, 20));
    // W4.allowed's opportunities are every eligible move (TS M2 Task 7).
    assert_eq!(ev(&o, W4Allowed), (0, 30));
    for absent in [
        W2, W3Shallow, W3Deep, W4Missed, W5, W6, W8Basics, W8Advanced, W9b, W9c, W11,
    ] {
        assert!(
            !o.binomial.contains_key(&absent),
            "{absent} has no opportunity"
        );
    }
    assert!(!o.binomial.contains_key(&W9d) && !o.binomial.contains_key(&W12));
    // Constant time spent and no MultiPV gap: no correlation is claimed.
    assert_eq!(o.w9d_rho, None);
    assert!(all_finite(&o));
}

// ---------------------------------------------------------------- W1 / W2

#[test]
fn w1_hung_material() {
    let o = obs(&w1_game());
    let w1 = key(&o, W1);
    assert_eq!((w1.events, w1.opportunities), (1, 30));
    assert!((w1.epl - 0.2).abs() < EPS);
    assert_eq!(w1.exemplar_plies, vec![31]);
}

#[test]
fn w1_needs_a_shallow_material_refutation() {
    for (label, l) in [
        ("depth 4", line(3.0, Some(4), None, &[])),
        ("gain 1", line(1.0, Some(1), None, &[])),
        ("mate", line(100.0, Some(3), Some(2), &[])),
        ("quiet", quiet_line()),
    ] {
        let g = with_move(make_game(), own(15), |m| {
            lose(m, 20.0);
            m.f.refutation = Some(l);
        });
        assert_eq!(ev(&obs(&g), W1), (0, 30), "{label}");
    }
    // Not decisive: ΔW 14.
    let g = with_move(make_game(), own(15), |m| {
        lose(m, 14.0);
        m.f.refutation = Some(line(3.0, Some(1), None, &[]));
    });
    assert_eq!(ev(&obs(&g), W1), (0, 30));
}

#[test]
fn decided_and_opponent_moves_are_not_w1_opportunities() {
    let mut g = with_move(make_game(), own(15), |m| {
        set_w(m, 95.0, 75.0);
        m.f.refutation = Some(line(3.0, Some(1), None, &[]));
    });
    // An opponent blunder with a hanging-piece refutation.
    g = with_move(g, own(15) + 1, |m| {
        lose(m, 30.0);
        m.f.refutation = Some(line(5.0, Some(1), None, &[]));
    });
    assert_eq!(ev(&obs(&g), W1), (0, 29));
}

#[test]
fn w1_without_a_refutation_line_is_left_out_only_when_it_could_be_an_event() {
    // A decisive move without a refutation line cannot be judged: not an opportunity.
    let g = with_move(make_game(), own(15), |m| {
        lose(m, 20.0);
        m.f.refutation = None;
    });
    assert_eq!(ev(&obs(&g), W1), (0, 29));
    // A move with ΔW < 15 is no W1 event whatever its line: still a (computable) opportunity.
    let g = with_move(make_game(), own(15), |m| m.f.refutation = None);
    assert_eq!(ev(&obs(&g), W1), (0, 30));
    // No refutation line anywhere and no decisive move: 30 computable non-events.
    let mut g = make_game();
    for m in &mut g.moves {
        m.f.refutation = None;
    }
    assert_eq!(ev(&obs(&g), W1), (0, 30));
}

#[test]
fn w2_takes_the_epl_from_w1() {
    let g = with_move(w1_game(), own(15), |m| {
        m.f.threat_existed = Some(true);
        m.f.threat_addressed = Some(false);
    });
    let o = obs(&g);
    let (w1, w2) = (key(&o, W1), key(&o, W2));
    assert_eq!((w2.events, w2.opportunities), (1, 1));
    assert!((w2.epl - 0.2).abs() < EPS);
    assert_eq!(w1.events, 1);
    assert_eq!(w1.epl, 0.0);
    assert_eq!(w2.exemplar_plies, vec![31]);
}

#[test]
fn w2_addressed_or_unknown_threats() {
    // Addressed: opportunity, no event.
    let g = with_move(w1_game(), own(15), |m| {
        m.f.threat_existed = Some(true);
        m.f.threat_addressed = Some(true);
    });
    assert_eq!(ev(&obs(&g), W2), (0, 1));
    // Unknown whether addressed (no refutation line) on a decisive move: not an opportunity.
    let g = with_move(w1_game(), own(15), |m| {
        m.f.threat_existed = Some(true);
        m.f.threat_addressed = None;
    });
    assert!(!obs(&g).binomial.contains_key(&W2));
    // Unknown whether addressed on a quiet move: no event possible, so a computable opportunity.
    let g = with_move(make_game(), own(15), |m| {
        m.f.threat_existed = Some(true);
        m.f.threat_addressed = None;
    });
    assert_eq!(ev(&obs(&g), W2), (0, 1));
    // No threat probe: no opportunity anywhere.
    let mut g = w1_game();
    for m in &mut g.moves {
        m.f.threat_existed = None;
        m.f.threat_addressed = None;
    }
    assert!(!obs(&g).binomial.contains_key(&W2));
}

// ---------------------------------------------------------------- W3

#[test]
fn w3_shallow_deep_and_in_top2() {
    let mk = |depth: u32, top2: Option<bool>| {
        with_move(make_game(), own(15), |m| {
            lose(m, 16.0);
            m.f.best = Some(line(3.0, Some(depth), None, &[MotifTag::Fork]));
            m.f.in_top2 = top2;
        })
    };
    let o = obs(&mk(3, None));
    assert_eq!(ev(&o, W3Shallow), (1, 1));
    assert!(!o.binomial.contains_key(&W3Deep));
    let o = obs(&mk(5, None));
    assert_eq!(ev(&o, W3Deep), (1, 1));
    assert!(!o.binomial.contains_key(&W3Shallow));
    // Depth 4 counts as deep (ADR-R2-4).
    assert_eq!(ev(&obs(&mk(4, None)), W3Deep), (1, 1));
    assert_eq!(ev(&obs(&mk(3, Some(true))), W3Shallow), (0, 1));
    assert_eq!(ev(&obs(&mk(3, Some(false))), W3Shallow), (1, 1));
}

#[test]
fn w3_mate_counts_and_small_gains_do_not() {
    let g = with_move(make_game(), own(15), |m| {
        lose(m, 30.0);
        m.f.best = Some(line(100.0, Some(5), Some(3), &[MotifTag::Mate]));
    });
    assert_eq!(ev(&obs(&g), W3Deep), (1, 1));
    let g = with_move(make_game(), own(15), |m| {
        lose(m, 30.0);
        m.f.best = Some(line(1.0, Some(1), None, &[]));
    });
    let o = obs(&g);
    assert!(!o.binomial.contains_key(&W3Shallow) && !o.binomial.contains_key(&W3Deep));
    // ΔW 14: an opportunity found well enough.
    let g = with_move(make_game(), own(15), |m| {
        lose(m, 14.0);
        m.f.best = Some(line(3.0, Some(2), None, &[]));
    });
    assert_eq!(ev(&obs(&g), W3Shallow), (0, 1));
}

#[test]
fn w3_takes_epl_after_w1_and_feeds_motif_missed() {
    let g = with_move(w1_game(), own(15), |m| {
        m.f.best = Some(line(3.0, Some(3), None, &[MotifTag::Fork]));
    });
    let o = obs(&g);
    assert!((key(&o, W1).epl - 0.2).abs() < EPS);
    assert_eq!(key(&o, W3Shallow).events, 1);
    assert_eq!(key(&o, W3Shallow).epl, 0.0);
    let s = series(&o, "motif.missed:fork").unwrap();
    assert_eq!((s.events, s.opportunities), (1, 1));
    assert!(series(&o, "motif.missed:mate").is_none());
}

/// EXP-0007 (#122): a line tagged only with a hidden motif still counts as a missed tactic (W3),
/// but builds no motif series, so nothing downstream can name the motif.
#[test]
fn a_hidden_motif_counts_for_w3_and_builds_no_series() {
    let g = with_move(w1_game(), own(15), |m| {
        m.f.best = Some(line(
            3.0,
            Some(3),
            None,
            &[MotifTag::Pin, MotifTag::Sacrifice],
        ));
        m.f.refutation = Some(line(3.0, Some(1), None, &[MotifTag::HangingPiece]));
    });
    let o = obs(&g);
    assert_eq!(key(&o, W3Shallow).events, 1);
    for s in &o.series {
        match s.series_key {
            SeriesKey::MotifMissed(t) | SeriesKey::MotifAllowed(t) => {
                assert!(t.is_player_visible(), "{}", s.series_key)
            }
            _ => {}
        }
    }
    assert!(series(&o, "motif.missed:pin").is_none());
    assert!(series(&o, "motif.allowed:hangingPiece").is_none());
    assert!(series(&o, "motif.allowed:fork").is_some());
}

#[test]
fn incomplete_best_line_coverage_drops_w3_and_w10_for_the_game() {
    // One eligible own move has no best line: the opportunity sets would be selected by outcome.
    let g = with_move(make_game(), own(20), |m| m.f.best = None);
    let scan = observe_game_counted(&g, T_WIN);
    let o = &scan.observation;
    assert!(!o.binomial.contains_key(&W10));
    assert!(!o.binomial.contains_key(&W3Shallow) && !o.binomial.contains_key(&W3Deep));
    assert!(
        o.series
            .iter()
            .all(|s| !matches!(s.series_key, SeriesKey::MotifMissed(_)))
    );
    assert_eq!(scan.moves_without_best_line, 1);
    // W1 is unaffected.
    assert_eq!(ev(o, W1), (0, 30));
}

// ---------------------------------------------------------------- W4

#[test]
fn w4_missed_mate() {
    let g = with_move(make_game(), own(15), |m| {
        set_w(m, 97.0, 85.0);
        m.score_before = EngineScore::Mate(2);
        m.score_after = EngineScore::Cp(400);
    });
    let o = obs(&g);
    assert_eq!(ev(&o, W4Missed), (1, 1));
    assert!((key(&o, W4Missed).epl - 0.12).abs() < EPS);
    // Still mating: opportunity, no event.
    let g = with_move(make_game(), own(15), |m| {
        set_w(m, 97.0, 97.0);
        m.score_before = EngineScore::Mate(2);
        m.score_after = EngineScore::Mate(1);
    });
    assert_eq!(ev(&obs(&g), W4Missed), (0, 1));
    // Mate in 5 is not an opportunity.
    let g = with_move(make_game(), own(15), |m| {
        set_w(m, 97.0, 85.0);
        m.score_before = EngineScore::Mate(5);
        m.score_after = EngineScore::Cp(400);
    });
    assert!(!obs(&g).binomial.contains_key(&W4Missed));
}

#[test]
fn w4_allowed_mate() {
    let g = with_move(make_game(), own(15), |m| {
        set_w(m, 55.0, 2.0);
        m.score_before = EngineScore::Cp(50);
        m.score_after = EngineScore::Mate(-3);
    });
    let o = obs(&g);
    assert_eq!(ev(&o, W4Allowed), (1, 30));
    // W4 takes the epl even when the move is also W1.
    let g = with_move(g, own(15), |m| {
        m.f.refutation = Some(line(3.0, Some(1), None, &[]));
    });
    let o = obs(&g);
    assert!((key(&o, W4Allowed).epl - 0.53).abs() < EPS);
    assert_eq!(key(&o, W1).epl, 0.0);
    assert_eq!(key(&o, W1).events, 1);
    // Already being mated (and decided): no opportunity, no event.
    let g = with_move(make_game(), own(15), |m| {
        set_w(m, 3.0, 2.0);
        m.score_before = EngineScore::Mate(-4);
        m.score_after = EngineScore::Mate(-3);
    });
    assert_eq!(ev(&obs(&g), W4Allowed), (0, 29));
}

// ---------------------------------------------------------------- W5 / W6

fn winning_run(g: AnalysedGame) -> AnalysedGame {
    let mut g = g;
    for k in 11..=13 {
        g = with_move(g, own(k), |m| set_w(m, 90.0, 90.0));
    }
    g
}

#[test]
fn w5_conversion_failure() {
    let o = obs(&winning_run(make_game()));
    let w5 = key(&o, W5);
    assert_eq!((w5.events, w5.opportunities), (1, 1));
    assert!((w5.epl - 0.5).abs() < EPS);
    let o = obs(&with_score(winning_run(make_game()), 1.0));
    assert_eq!(ev(&o, W5), (0, 1));
    assert_eq!(key(&o, W5).epl, 0.0);
    let o = obs(&with_score(winning_run(make_game()), 0.0));
    assert!((key(&o, W5).epl - 1.0).abs() < EPS);
}

#[test]
fn w5_and_w6_need_a_known_result() {
    let mut g = winning_run(make_game());
    g.ctx.score = None;
    g.ctx.result = "*".into();
    let scan = observe_game_counted(&g, T_WIN);
    assert!(!scan.observation.binomial.contains_key(&W5));
    assert!(scan.invalid_score);
}

#[test]
fn w6_defence() {
    let mut g = make_game();
    for k in 11..=13 {
        g = with_move(g, own(k), |m| set_w(m, 10.0, 10.0));
    }
    let o = obs(&with_score(g.clone(), 0.0));
    let w6 = key(&o, W6);
    assert_eq!((w6.events, w6.opportunities), (1, 1));
    assert!((w6.epl - 0.5).abs() < EPS);
    assert_eq!(ev(&obs(&g), W6), (0, 1)); // drawn: saved
    assert!(!obs(&make_game()).binomial.contains_key(&W6));
}

// ---------------------------------------------------------------- W7

#[test]
fn w7_traps_at_ply_15() {
    let g = with_move(make_game(), at_ply(15), |m| lose(m, 20.0));
    let o = obs(&g);
    assert_eq!(ev(&o, W7Traps), (1, 1));
    assert!((key(&o, W7Traps).epl - 0.2).abs() < EPS);
    assert_eq!(key(&o, W7Traps).exemplar_plies, vec![15]);
    // The same error at ply 21 is no trap.
    let g = with_move(make_game(), at_ply(21), |m| lose(m, 20.0));
    assert_eq!(ev(&obs(&g), W7Traps), (0, 1));
    // An opponent error at ply 16 is not the player's.
    let g = with_move(make_game(), at_ply(16), |m| lose(m, 20.0));
    assert_eq!(ev(&obs(&g), W7Traps), (0, 1));
}

#[test]
fn w7_traps_needs_an_own_move_by_ply_20() {
    let mut g = make_game();
    g.moves.retain(|m| m.rec.ply > 20);
    assert!(!obs(&g).binomial.contains_key(&W7Traps));
}

#[test]
fn w7_prep_from_the_win_at_opening_end() {
    // Last opening ply is 20 (Black): player POV = 100 - Black's W after.
    let g = with_move(make_game(), at_ply(20), |m| set_w(m, 50.0, 65.0));
    assert!((win_at_opening_end(&g).unwrap() - 35.0).abs() < EPS);
    let o = obs(&g);
    assert_eq!(ev(&o, W7Prep), (1, 1));
    let g = with_move(make_game(), at_ply(20), |m| set_w(m, 50.0, 59.0));
    assert_eq!(ev(&obs(&g), W7Prep), (0, 1));
    // No opening phase: unknown, no opportunity.
    let mut g = make_game();
    for m in &mut g.moves {
        m.rec.phase = Phase::Middlegame;
    }
    assert_eq!(win_at_opening_end(&g), None);
    assert!(!obs(&g).binomial.contains_key(&W7Prep));
}

#[test]
fn w7_prep_epl_is_own_opening_loss_not_claimed_earlier() {
    let mut g = with_move(make_game(), at_ply(20), |m| set_w(m, 50.0, 70.0));
    g = with_move(g, at_ply(13), |m| lose(m, 4.0));
    g = with_move(g, at_ply(15), |m| lose(m, 20.0)); // a trap: W7.traps takes it
    let o = obs(&g);
    assert!((key(&o, W7Prep).epl - 0.04).abs() < EPS);
    assert!((key(&o, W7Traps).epl - 0.2).abs() < EPS);
}

// ---------------------------------------------------------------- W8

const KRVKP: &str = "8/8/4k3/8/4P3/8/4K3/7r w - - 0 1";

fn tb(g: AnalysedGame, fen: &str, before: i8, after: i8) -> AnalysedGame {
    with_move(g, own(25), |m| {
        m.rec.fen_before = fen.into();
        m.rec.phase = Phase::Endgame;
        let raw = m.raw.as_mut().unwrap();
        raw.tb_wdl_before = Some(before);
        raw.tb_wdl_after = Some(after);
    })
}

#[test]
fn w8_basics_tb_drop() {
    let o = obs(&tb(make_game(), KRVKP, 1, 0));
    assert_eq!(ev(&o, W8Basics), (1, 1));
    assert!(!o.binomial.contains_key(&W8Advanced));
    assert_eq!(ev(&obs(&tb(make_game(), KRVKP, 0, 0)), W8Basics), (0, 1));
    assert_eq!(ev(&obs(&tb(make_game(), KRVKP, -1, -1)), W8Basics), (0, 1));
}

#[test]
fn w8_advanced_for_other_signatures() {
    // KBNvK is not a basic signature.
    let o = obs(&tb(make_game(), "8/8/4k3/8/8/8/4K3/5BN1 w - - 0 1", 1, 0));
    assert_eq!(ev(&o, W8Advanced), (1, 1));
    assert!(!o.binomial.contains_key(&W8Basics));
}

#[test]
fn w8_skips_partial_and_invalid_tablebase_data() {
    let g = with_move(make_game(), own(25), |m| {
        m.rec.fen_before = KRVKP.into();
        m.raw.as_mut().unwrap().tb_wdl_before = Some(1);
    });
    assert!(!obs(&g).binomial.contains_key(&W8Basics));
    // Out-of-range WDL, and a tablebase value on the 32-man start position: invalid, counted.
    let scan = observe_game_counted(&tb(make_game(), KRVKP, 2, 0), T_WIN);
    assert!(!scan.observation.binomial.contains_key(&W8Basics));
    assert_eq!(scan.invalid_tablebase, 1);
    let scan = observe_game_counted(&tb(make_game(), START, 1, 0), T_WIN);
    assert!(!scan.observation.binomial.contains_key(&W8Advanced));
    assert_eq!(scan.invalid_tablebase, 1);
}

// ---------------------------------------------------------------- W9

#[test]
fn w9a_time_trouble() {
    let mut g = make_game();
    for k in 15..30 {
        g = with_move(g, own(k), |m| m.rec.clock_before = Some(20.0));
    }
    g = with_move(g, own(14), |m| lose(m, 10.0)); // before the trouble
    g = with_move(g, own(20), |m| lose(m, 6.0));
    let o = obs(&g);
    let w9a = key(&o, W9a);
    assert_eq!((w9a.events, w9a.opportunities), (1, 1));
    assert!((w9a.epl - 0.06).abs() < EPS);
    assert_eq!(w9a.exemplar_plies, vec![31]);
}

#[test]
fn w9a_not_for_daily_games() {
    let mut g = make_game();
    g.ctx.time_class = TimeClass::Daily;
    let o = obs(&g);
    assert!(!o.binomial.contains_key(&W9a));
    assert!(!o.binomial.contains_key(&W9b));
}

#[test]
fn w9b_time_pressure_collapse() {
    let g = with_move(make_game(), own(20), |m| {
        m.rec.clock_before = Some(20.0);
        lose(m, 16.0);
    });
    let o = obs(&g);
    assert_eq!(ev(&o, W9b), (1, 1));
    assert!((key(&o, W9b).epl - 0.16).abs() < EPS);
    let g = with_move(make_game(), own(20), |m| {
        m.rec.clock_before = Some(20.0);
        lose(m, 3.0);
    });
    assert_eq!(ev(&obs(&g), W9b), (0, 1));
    // Classical is not a fast time control.
    let mut g = with_move(make_game(), own(20), |m| m.rec.clock_before = Some(20.0));
    g.ctx.time_class = TimeClass::Classical;
    assert!(!obs(&g).binomial.contains_key(&W9b));
}

#[test]
fn w9c_rushing_critical_moves() {
    let mut g = with_move(make_game(), own(20), |m| {
        m.f.is_critical = Some(true);
        m.rec.time_spent = Some(1.0);
        lose(m, 12.0);
    });
    g = with_move(g, own(21), |m| m.f.is_critical = Some(true));
    let o = obs(&g);
    assert_eq!(ev(&o, W9c), (1, 2));
    // An inaccuracy only is no event; a slow mistake is no event.
    let g = with_move(make_game(), own(20), |m| {
        m.f.is_critical = Some(true);
        m.rec.time_spent = Some(1.0);
        lose(m, 6.0);
    });
    assert_eq!(ev(&obs(&g), W9c), (0, 1));
    let g = with_move(make_game(), own(20), |m| {
        m.f.is_critical = Some(true);
        m.rec.time_spent = Some(4.0);
        lose(m, 12.0);
        m.rec.judgement = Some(Judgement::Mistake);
    });
    assert_eq!(ev(&obs(&g), W9c), (0, 1));
}

#[test]
fn w9d_rho_is_one_for_matching_ranks() {
    let mut g = make_game();
    for k in 0..10 {
        g = with_move(g, own(10 + k), |m| {
            m.rec.time_spent = Some(1.0 + k as f64);
            m.f.multipv_gap = Some(1.0 + k as f64);
        });
    }
    let rho = obs(&g).w9d_rho.unwrap();
    assert!((rho - 1.0).abs() < EPS);
    // Nine points are too few.
    let g = with_move(g, own(19), |m| m.f.multipv_gap = None);
    assert_eq!(obs(&g).w9d_rho, None);
}

#[test]
fn no_clocks_w9_keys_absent() {
    let mut g = without_clocks(make_game());
    for k in 0..10 {
        g = with_move(g, own(10 + k), |m| {
            m.f.multipv_gap = Some(1.0 + k as f64);
            m.f.is_critical = Some(true);
        });
    }
    let o = obs(&g);
    for k in [W9a, W9b, W9c] {
        assert!(!o.binomial.contains_key(&k), "{k}");
    }
    assert_eq!(o.w9d_rho, None);
}

#[test]
fn invalid_clocks_are_skipped_and_counted() {
    let mut g = with_move(make_game(), own(20), |m| {
        m.rec.clock_before = Some(f64::NAN);
        lose(m, 16.0);
    });
    g = with_move(g, own(21), |m| m.rec.time_spent = Some(-1.0));
    let scan = observe_game_counted(&g, T_WIN);
    assert!(!scan.observation.binomial.contains_key(&W9b));
    assert_eq!(scan.invalid_clocks, 1);
    assert_eq!(scan.invalid_time_spent, 1);
    assert!(all_finite(&scan.observation));
}

// ---------------------------------------------------------------- W10

#[test]
fn w10_positional_drift() {
    let g = with_move(make_game(), own(20), |m| lose(m, 6.0));
    let o = obs(&g);
    assert_eq!(ev(&o, W10), (1, 20));
    assert!((key(&o, W10).epl - 0.06).abs() < EPS);
    let g = with_move(make_game(), own(20), |m| lose(m, 4.0));
    assert_eq!(ev(&obs(&g), W10), (0, 20));
}

#[test]
fn w10_excludes_critical_tactical_and_unbalanced_positions() {
    let g = with_move(make_game(), own(20), |m| {
        lose(m, 6.0);
        m.f.is_critical = Some(true);
    });
    assert_eq!(ev(&obs(&g), W10), (0, 19));
    let g = with_move(make_game(), own(20), |m| {
        lose(m, 6.0);
        m.f.best = Some(line(3.0, Some(2), None, &[]));
    });
    assert_eq!(ev(&obs(&g), W10), (0, 19));
    let g = with_move(make_game(), own(20), |m| {
        lose(m, 6.0);
        m.score_before = EngineScore::Cp(151);
    });
    assert_eq!(ev(&obs(&g), W10), (0, 19));
    let g = with_move(make_game(), own(20), |m| {
        lose(m, 6.0);
        m.score_before = EngineScore::Mate(9);
    });
    assert_eq!(ev(&obs(&g), W10), (0, 19));
    // Opening moves are never W10 opportunities.
    let g = with_move(make_game(), own(2), |m| lose(m, 6.0));
    assert_eq!(ev(&obs(&g), W10), (0, 20));
}

// ---------------------------------------------------------------- W11

#[test]
fn w11_easy_move_errors() {
    let g = with_move(make_game(), own(20), |m| {
        lose(m, 20.0);
        m.raw.as_mut().unwrap().difficulty = Some(maia(0.9));
    });
    let o = obs(&g);
    assert_eq!(ev(&o, W11), (1, 1));
    assert!((key(&o, W11).epl - 0.2).abs() < EPS);
    let g = with_move(make_game(), own(20), |m| {
        lose(m, 20.0);
        m.raw.as_mut().unwrap().difficulty = Some(maia(0.7));
    });
    assert!(!obs(&g).binomial.contains_key(&W11));
}

#[test]
fn w11_difficulty_is_maia_only_and_never_substituted() {
    // A non-Maia model is invalid: skipped and counted, never used.
    let g = with_move(make_game(), own(20), |m| {
        lose(m, 20.0);
        let mut d = maia(0.9);
        d.model = "spread-v1".into();
        m.raw.as_mut().unwrap().difficulty = Some(d);
    });
    let scan = observe_game_counted(&g, T_WIN);
    assert!(!scan.observation.binomial.contains_key(&W11));
    assert_eq!(scan.invalid_difficulty, 1);
    // No difficulty at all: no opportunity.
    let g = with_move(make_game(), own(20), |m| {
        lose(m, 20.0);
        m.raw = None;
    });
    assert!(!obs(&g).binomial.contains_key(&W11));
    // Maia at ply <= 10 is not used.
    let g = with_move(make_game(), own(2), |m| {
        m.raw.as_mut().unwrap().difficulty = Some(maia(0.9));
    });
    assert!(!obs(&g).binomial.contains_key(&W11));
}

// ---------------------------------------------------------------- series

#[test]
fn motif_allowed_fork_series() {
    let o = obs(&w1_game());
    let s = series(&o, "motif.allowed:fork").unwrap();
    assert_eq!((s.events, s.opportunities), (1, 30));
    assert!((s.epl - 0.2).abs() < EPS);
    let s = series(&o, "motif.allowed:mate").unwrap();
    assert_eq!((s.events, s.opportunities), (0, 30));
    assert!(series(&o, "motif.allowed:pin").is_none());
}

#[test]
fn every_binomial_key_has_its_series_row() {
    let o = obs(&w1_game());
    for (k, b) in &o.binomial {
        let s = series(&o, k.as_str()).unwrap();
        assert_eq!((s.events, s.opportunities), (b.events, b.opportunities));
        assert_eq!(s.epl, b.epl);
    }
    let weakness_rows = o
        .series
        .iter()
        .filter(|s| matches!(s.series_key, SeriesKey::Weakness(_)))
        .count();
    assert_eq!(weakness_rows, o.binomial.len());
}

#[test]
fn opening_series() {
    let o = obs(&make_game());
    let s = series(&o, "opening:white:italian-game").unwrap();
    assert_eq!((s.events, s.opportunities), (0, 1));
    let o = obs(&with_score(make_game(), 0.0));
    let s = series(&o, "opening:white:italian-game").unwrap();
    assert_eq!((s.events, s.opportunities), (1, 1));
    let mut g = make_game();
    g.opening_line_key = None;
    assert!(
        obs(&g)
            .series
            .iter()
            .all(|s| !matches!(s.series_key, SeriesKey::Opening(_)))
    );
    let mut g = make_game();
    g.ctx.score = None;
    assert!(
        obs(&g)
            .series
            .iter()
            .all(|s| !matches!(s.series_key, SeriesKey::Opening(_)))
    );
}

/// From ply 41 on the game is a rook endgame entered at `entry` (player POV win before).
fn rook_endgame(entry: f64, score: f64) -> AnalysedGame {
    let mut g = with_score(make_game(), score);
    for m in g.moves.iter_mut().filter(|m| m.rec.ply >= 41) {
        m.rec.phase = Phase::Endgame;
        m.rec.fen_before = "4k3/r7/8/8/8/8/R7/4K3 w - - 0 1".into();
    }
    with_move(g, at_ply(41), |m| set_w(m, entry, entry))
}

#[test]
fn endgame_series() {
    let o = obs(&rook_endgame(75.0, 0.5));
    let s = series(&o, "endgame:rook").unwrap();
    assert_eq!((s.events, s.opportunities), (1, 1));
    assert_eq!(
        series(&obs(&rook_endgame(75.0, 1.0)), "endgame:rook")
            .unwrap()
            .events,
        0
    );
    assert_eq!(
        series(&obs(&rook_endgame(50.0, 0.0)), "endgame:rook")
            .unwrap()
            .events,
        1
    );
    assert_eq!(
        series(&obs(&rook_endgame(50.0, 0.5)), "endgame:rook")
            .unwrap()
            .events,
        0
    );
    // Entered lost: no opportunity.
    assert!(series(&obs(&rook_endgame(20.0, 0.0)), "endgame:rook").is_none());
    // No endgame reached.
    assert!(
        obs(&make_game())
            .series
            .iter()
            .all(|s| !matches!(s.series_key, SeriesKey::Endgame(_)))
    );
    let _ = EndgameType::Rook;
}

// ---------------------------------------------------------------- composition

#[test]
fn exemplar_plies_are_events_by_delta_w_desc() {
    let mut g = make_game();
    for (k, dw) in [(12, 20.0), (16, 40.0), (14, 20.0)] {
        g = with_move(g, own(k), |m| {
            lose(m, dw);
            m.f.refutation = Some(line(3.0, Some(1), None, &[]));
        });
    }
    let w1 = key(&obs(&g), W1).clone();
    assert_eq!(w1.events, 3);
    assert_eq!(w1.exemplar_plies, vec![33, 25, 29]);
    assert!((w1.epl - 0.8).abs() < EPS);
}

#[test]
fn invalid_moves_are_skipped_and_counted() {
    let mut g = with_move(w1_game(), own(3), |m| m.rec.win_before = f64::NAN);
    g = with_move(g, own(4), |m| m.rec.delta_w = f64::INFINITY);
    g = with_move(g, own(5), |m| m.score_before = EngineScore::Cp(1_000_000));
    let scan: ObservationScan = observe_game_counted(&g, T_WIN);
    assert_eq!(scan.skipped_moves, 3);
    assert_eq!(ev(&scan.observation, W1), (1, 27));
    assert!(all_finite(&scan.observation));
}

#[test]
fn observation_does_not_depend_on_move_order() {
    let g = with_move(w1_game(), own(20), |m| lose(m, 6.0));
    let mut r = g.clone();
    r.moves.reverse();
    assert_eq!(obs(&g), obs(&r));
}

#[test]
fn an_invalid_t_win_finds_no_winning_episode() {
    let g = winning_run(make_game());
    for t in [f64::NAN, 40.0, 101.0] {
        let scan = observe_game_counted(&g, t);
        assert!(!scan.observation.binomial.contains_key(&W5), "{t}");
        assert!(scan.invalid_t_win);
    }
    assert_eq!(T_WIN, 85.0);
}

#[test]
fn an_empty_game_claims_nothing() {
    let mut g = make_game();
    g.moves.clear();
    g.opening_line_key = None;
    let o = obs(&g);
    assert!(o.binomial.is_empty(), "{:?}", o.binomial.keys());
    assert!(o.series.is_empty());
    assert_eq!(o.w9d_rho, None);
}

#[test]
fn black_player_sees_its_own_moves() {
    let mut g = w1_game();
    g.ctx.player_color = chess_core::Color::Black;
    let g = with_move(g, own(15) + 1, |m| {
        lose(m, 20.0);
        m.f.refutation = Some(line(3.0, Some(1), None, &[]));
    });
    assert_eq!(ev(&obs(&g), W1), (1, 30));
}

// ---------------------------------------------------------------- W12 tilt

fn session(i: u32, after_loss: bool, residual: Option<f64>) -> SessionInfo {
    SessionInfo {
        game_id: Uuid::from_u128(u128::from(i)),
        session_id: 1,
        index_in_session: i,
        after_loss,
        residual,
    }
}

#[test]
fn tilt_splits_residuals() {
    let s = [
        session(1, false, Some(0.1)),
        session(2, true, Some(-0.4)),
        session(3, false, Some(0.2)),
        session(4, false, Some(-0.3)),
        session(5, false, None),
        session(6, false, Some(f64::NAN)),
    ];
    let (tilted, other) = tilt_residual_difference(&s);
    assert_eq!(tilted, vec![-0.4, -0.3]);
    assert_eq!(other, vec![0.1, 0.2]);
    let split = tilt_residual_difference_counted(&s);
    assert_eq!(split.unknown, 1);
    assert_eq!(split.invalid, 1);
    assert_eq!(tilt_residual_difference(&[]), (vec![], vec![]));
}

// ---------------------------------------------------------------- fix round 1: honesty filters

#[test]
fn w3_coverage_guard_bites_on_a_real_tactic() {
    // A missed fork on own move 15, so W3 and motif.missed:fork have evidence...
    let tactic = with_move(make_game(), own(15), |m| {
        lose(m, 16.0);
        m.f.best = Some(line(3.0, Some(2), None, &[MotifTag::Fork]));
    });
    let o = obs(&tactic);
    assert_eq!(ev(&o, W3Shallow), (1, 1));
    assert!(series(&o, "motif.missed:fork").is_some());
    // ...until another eligible own move lacks its best line: then W3 is left out entirely.
    let gap = with_move(tactic, own(20), |m| m.f.best = None);
    let o = obs(&gap);
    assert!(!o.binomial.contains_key(&W3Shallow));
    assert!(!o.binomial.contains_key(&W3Deep));
    assert!(series(&o, "motif.missed:fork").is_none());
    // A decided move without a best line does not count against coverage.
    let decided = with_move(gap, own(20), |m| set_w(m, 95.0, 95.0));
    assert_eq!(ev(&obs(&decided), W3Shallow), (1, 1));
}

#[test]
fn motif_allowed_leaves_out_decisive_moves_without_a_refutation() {
    let g = with_move(make_game(), own(15), |m| {
        lose(m, 20.0);
        m.f.refutation = None;
    });
    let o = obs(&g);
    for t in ["fork", "discoveredAttack", "mateIn1"] {
        let s = series(&o, &format!("motif.allowed:{t}")).unwrap();
        assert_eq!((s.events, s.opportunities), (0, 29), "{t}");
    }
    // A non-decisive move without a refutation is still a computable opportunity.
    let g = with_move(make_game(), own(15), |m| m.f.refutation = None);
    let s = series(&obs(&g), "motif.allowed:fork").map(|s| s.opportunities);
    assert_eq!(s, Some(30));
}

#[test]
fn w9c_unknown_criticality_is_no_opportunity() {
    // Without MultiPV, `is_critical` is None on almost every move: none of them may enter W9c.
    let mut g = make_game();
    for m in &mut g.moves {
        m.f.is_critical = None;
    }
    assert!(!obs(&g).binomial.contains_key(&W9c));
    let g = with_move(g, own(20), |m| {
        m.f.is_critical = Some(true);
        m.rec.time_spent = Some(1.0);
        lose(m, 12.0);
    });
    assert_eq!(ev(&obs(&g), W9c), (1, 1));
}

// ---------------------------------------------------------------- fix round 1: boundaries

#[test]
fn eligibility_boundary_is_strictly_under_40_from_50() {
    for (w, eligible) in [(89.9, true), (90.0, false), (10.1, true), (10.0, false)] {
        let g = with_move(make_game(), own(15), |m| set_w(m, w, w));
        let opps = if eligible { 30 } else { 29 };
        assert_eq!(ev(&obs(&g), W1), (0, opps), "W_before {w}");
    }
}

#[test]
fn decisive_boundary_is_15() {
    for (dw, event) in [(15.0, 1), (14.9, 0)] {
        let g = with_move(make_game(), own(15), |m| {
            lose(m, dw);
            m.f.refutation = Some(line(3.0, Some(3), None, &[]));
        });
        assert_eq!(ev(&obs(&g), W1), (event, 30), "ΔW {dw}");
    }
}

#[test]
fn w4_missed_includes_mate_in_4() {
    let g = with_move(make_game(), own(15), |m| {
        set_w(m, 97.0, 85.0);
        m.score_before = EngineScore::Mate(4);
        m.score_after = EngineScore::Cp(400);
    });
    assert_eq!(ev(&obs(&g), W4Missed), (1, 1));
}

#[test]
fn w5_and_w6_thresholds_are_inclusive() {
    for (w, present) in [(85.0, true), (84.9, false)] {
        let mut g = make_game();
        for k in 11..=13 {
            g = with_move(g, own(k), |m| set_w(m, w, w));
        }
        assert_eq!(obs(&g).binomial.contains_key(&W5), present, "W {w}");
    }
    for (w, present) in [(15.0, true), (15.1, false)] {
        let mut g = make_game();
        for k in 11..=13 {
            g = with_move(g, own(k), |m| set_w(m, w, w));
        }
        assert_eq!(obs(&g).binomial.contains_key(&W6), present, "W {w}");
    }
}

#[test]
fn w7_boundaries() {
    // A trap at ply 19 (the player's last move by ply 20) counts.
    let g = with_move(make_game(), at_ply(19), |m| lose(m, 20.0));
    assert_eq!(ev(&obs(&g), W7Traps), (1, 1));
    // For Black, ply 20 is an own move and still a trap; ply 22 is not.
    for (ply, event) in [(20, 1), (22, 0)] {
        let mut g = with_move(make_game(), at_ply(ply), |m| lose(m, 20.0));
        g.ctx.player_color = chess_core::Color::Black;
        assert_eq!(ev(&obs(&g), W7Traps), (event, 1), "Black ply {ply}");
    }
    // Opening-end W 40 is "worse", 40.1 is not. Ply 20 is Black's: player W = 100 - Black's W.
    for (black_w, event) in [(60.0, 1), (60.1, 1), (59.9, 0)] {
        let g = with_move(make_game(), at_ply(20), |m| set_w(m, 50.0, black_w));
        assert_eq!(ev(&obs(&g), W7Prep), (event, 1), "Black W {black_w}");
    }
}

#[test]
fn w7_prep_exemplar_is_the_last_opening_ply() {
    let g = with_move(make_game(), at_ply(20), |m| set_w(m, 50.0, 65.0));
    assert_eq!(key(&obs(&g), W7Prep).exemplar_plies, vec![20]);
    let g = with_move(make_game(), at_ply(20), |m| set_w(m, 50.0, 55.0));
    assert!(key(&obs(&g), W7Prep).exemplar_plies.is_empty());
}

#[test]
fn w9b_clock_boundary_is_strictly_under_30() {
    for (clock, opp) in [(29.9, true), (30.0, false), (30.1, false)] {
        let g = with_move(make_game(), own(20), |m| m.rec.clock_before = Some(clock));
        assert_eq!(obs(&g).binomial.contains_key(&W9b), opp, "clock {clock}");
    }
}

#[test]
fn w9c_rush_boundary_is_strictly_under_half_the_median() {
    // The median own time spent is 5, so the bound is 2.5.
    for (t, event) in [(2.4, 1), (2.5, 0)] {
        let g = with_move(make_game(), own(20), |m| {
            m.f.is_critical = Some(true);
            m.rec.time_spent = Some(t);
            lose(m, 12.0);
        });
        assert_eq!(ev(&obs(&g), W9c), (event, 1), "time {t}");
    }
}

#[test]
fn w10_boundaries() {
    for (cp, opps) in [(150, 20), (-150, 20), (151, 19), (-151, 19)] {
        let g = with_move(make_game(), own(20), |m| {
            m.score_before = EngineScore::Cp(cp)
        });
        assert_eq!(ev(&obs(&g), W10), (0, opps), "cp {cp}");
    }
    for (dw, event) in [(5.0, 1), (4.9, 0)] {
        let g = with_move(make_game(), own(20), |m| lose(m, dw));
        assert_eq!(ev(&obs(&g), W10), (event, 20), "ΔW {dw}");
    }
}

#[test]
fn w11_p_ok_boundary_is_inclusive() {
    for (p, present) in [(0.8, true), (0.79, false), (0.81, true)] {
        let g = with_move(make_game(), own(20), |m| {
            m.raw.as_mut().unwrap().difficulty = Some(maia(p));
        });
        assert_eq!(obs(&g).binomial.contains_key(&W11), present, "p_ok {p}");
    }
}

#[test]
fn endgame_series_boundaries() {
    let s = |entry, score| series(&obs(&rook_endgame(entry, score)), "endgame:rook").cloned();
    assert_eq!(s(70.0, 0.5).unwrap().events, 1);
    assert_eq!(s(69.9, 0.5).unwrap().events, 0);
    assert_eq!(
        s(30.0, 0.0).map(|r| (r.events, r.opportunities)),
        Some((1, 1))
    );
    assert!(s(29.9, 0.0).is_none());
}

// ---------------------------------------------------------------- fix round 1: invalid input

#[test]
fn an_invalid_own_move_breaks_an_episode_run() {
    // 90, invalid, 90, 90: only two valid moves after the break, so no 3-move winning episode.
    let mut g = make_game();
    for k in [11, 13, 14] {
        g = with_move(g, own(k), |m| set_w(m, 90.0, 90.0));
    }
    let g = with_move(g, own(12), |m| m.rec.win_before = f64::NAN);
    let scan = observe_game_counted(&g, T_WIN);
    assert!(!scan.observation.binomial.contains_key(&W5));
    assert_eq!(scan.skipped_moves, 1);
    // With the middle move valid, the run is 4 moves long.
    let g = with_move(g, own(12), |m| set_w(m, 90.0, 90.0));
    assert!(obs(&g).binomial.contains_key(&W5));
    // An invalid move whose W values look fine (out-of-range score) breaks the run too.
    let g = with_move(g, own(12), |m| m.score_before = EngineScore::Cp(1_000_000));
    let scan = observe_game_counted(&g, T_WIN);
    assert!(!scan.observation.binomial.contains_key(&W5));
    assert_eq!(scan.skipped_moves, 1);
}

#[test]
fn a_skipped_early_own_move_makes_no_trap_unknown() {
    let g = with_move(make_game(), at_ply(9), |m| m.rec.delta_w = f64::NAN);
    assert!(!obs(&g).binomial.contains_key(&W7Traps));
    // A trap found elsewhere by ply 20 still counts.
    let g = with_move(g, at_ply(15), |m| lose(m, 20.0));
    assert_eq!(ev(&obs(&g), W7Traps), (1, 1));
}

#[test]
fn duplicate_plies_are_dropped_and_counted() {
    let mut g = w1_game();
    let mut copy = g.moves[own(15)].clone();
    lose(&mut copy, 3.0);
    g.moves.push(copy);
    let scan = observe_game_counted(&g, T_WIN);
    assert_eq!(scan.duplicate_plies, 2);
    assert_eq!(ev(&scan.observation, W1), (0, 29));
    let mut r = g.clone();
    r.moves.reverse();
    assert_eq!(observe_game(&r, T_WIN), scan.observation);
}
