//! R2 Task 14: weakness timeline states, onsets and cause hints.
//!
//! Block 1 ports TS M2 Task 18's assertions. Block 2 is a table of every `next_state` rule for
//! every previous state. Block 3 pins the honesty rules: no change and no `p_change` is
//! reported below the evidence gate (zero evidence included), invalid points are skipped and
//! counted, no NaN or inf reaches a row, and the output is deterministic. Block 4 holds seeded
//! property tests (no impossible transition, deterministic and order-independent output).

use coaching_input::{CandidateCause, CauseKind, CauseValue, LIMITS};
use profile_core::stats::rng::Rng;
use profile_core::timeline::bocpd::DEFAULT_HAZARD;
use profile_core::timeline::causes::{CauseGame, MIN_CAUSE_GAMES, cause_hints};
use profile_core::timeline::series::{
    CONFIRM_P_CHANGE, CONFIRM_REL_CHANGE, RowKind, SERIES_WINDOW_GAMES, SeriesOutcome, SeriesPoint,
    SeriesRef, TimelineRow, evaluate_series,
};
use profile_core::timeline::states::{DETECTOR_VERSION, Signals, next_state};
use profile_core::types::WeaknessState;
use profile_core::types::WeaknessState::{
    Active, Emerging, Improving, NotAnIssue, Relapsed, Resolved,
};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

fn base_time() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap()
}

/// Points `i = 0..` with game id `i + 1` and one game per hour.
fn points(obs: &[(u32, u32)]) -> Vec<SeriesPoint> {
    obs.iter()
        .enumerate()
        .map(|(i, &(events, opportunities))| SeriesPoint {
            game_id: Uuid::from_u128(i as u128 + 1),
            played_at: base_time() + Duration::hours(i as i64),
            events,
            opportunities,
            protocol: "sf19-n300k-v1".to_owned(),
        })
        .collect()
}

fn repeat(n: usize, o: (u32, u32)) -> Vec<(u32, u32)> {
    vec![o; n]
}

fn alternating(n: usize, a: (u32, u32), b: (u32, u32)) -> Vec<(u32, u32)> {
    (0..n).map(|i| if i % 2 == 0 { a } else { b }).collect()
}

fn ts_ref() -> SeriesRef {
    SeriesRef {
        median: 0.02,
        p40: 0.017,
        pooled_rate: 0.02,
    }
}

const N0: f64 = 300.0;
const MIN_EVIDENCE: f64 = 1000.0;

fn eval(obs: &[(u32, u32)]) -> SeriesOutcome {
    evaluate_series(&points(obs), Some(&ts_ref()), N0, MIN_EVIDENCE)
}

fn up_obs() -> Vec<(u32, u32)> {
    let mut v = repeat(60, (1, 50));
    v.extend(repeat(60, (3, 50)));
    v
}

fn improve_obs() -> Vec<(u32, u32)> {
    let mut v = up_obs();
    v.extend(alternating(80, (0, 50), (1, 50)));
    v
}

fn relapse_obs() -> Vec<(u32, u32)> {
    let mut v = improve_obs();
    v.extend(repeat(30, (3, 50)));
    v
}

/// The sequence of states the rows move through (`to` of every transition row).
fn state_path(o: &SeriesOutcome) -> Vec<WeaknessState> {
    o.rows
        .iter()
        .filter(|r| r.kind == RowKind::Transition)
        .map(|r| r.to)
        .collect()
}

fn index_of(path: &[WeaknessState], s: WeaknessState) -> Option<usize> {
    path.iter().position(|&x| x == s)
}

fn quiet() -> Signals {
    Signals {
        confirmed_up: false,
        confirmed_down: false,
        cusum_up: false,
        confidence_above: 0.0,
        at_or_below_median: true,
        below_p40_streak: 0,
        p_imp: None,
        drift: false,
    }
}

/// The transitions the TS rule list can produce, written independently of `next_state`.
fn possible(prev: Option<WeaknessState>, next: WeaknessState) -> bool {
    match prev {
        Some(Resolved) => matches!(next, Resolved | Relapsed),
        Some(Active | Emerging | Relapsed) => {
            next == prev.unwrap()
                || matches!(next, Resolved | Improving | Active | Emerging | NotAnIssue)
        }
        // Leaving `improving` needs a confirmed rise, which is at least `emerging`.
        Some(Improving) => matches!(next, Resolved | Improving | Active | Emerging),
        Some(NotAnIssue) | None => matches!(next, Active | Emerging | NotAnIssue),
    }
}

fn game_index(id: Uuid) -> usize {
    id.as_u128() as usize - 1
}

/// Every honesty invariant of a series outcome.
fn assert_honest(o: &SeriesOutcome, min_evidence: f64, label: &str) {
    let mut state: Option<WeaknessState> = None;
    for (i, r) in o.rows.iter().enumerate() {
        let finite = |x: Option<f64>| x.is_none_or(|v| v.is_finite());
        assert!(
            finite(r.p_change) && finite(r.rate_before) && finite(r.rate_after),
            "{label}: row {i} has a non-finite value: {r:?}"
        );
        assert!(
            r.evidence.confidence.is_finite() && (0.0..=1.0).contains(&r.evidence.confidence),
            "{label}: row {i} confidence out of range: {r:?}"
        );
        assert!(r.evidence.k <= r.evidence.n, "{label}: row {i} k > n");
        // A p_change (and the rates of a change) appear only with a confirmed change that meets
        // the evidence gate.
        if r.p_change.is_some() || r.rate_before.is_some() || r.rate_after.is_some() {
            let p = r.p_change.expect("rates come with a p_change");
            assert!(
                (CONFIRM_P_CHANGE..=1.0).contains(&p),
                "{label}: row {i} p_change {p} below the confirmation level"
            );
            assert!(
                r.evidence.n as f64 >= min_evidence,
                "{label}: row {i} reports a change below min evidence: {r:?}"
            );
            let (b, a) = (r.rate_before.unwrap(), r.rate_after.unwrap());
            assert!((0.0..=1.0).contains(&b) && (0.0..=1.0).contains(&a));
            let rel = if b > 0.0 {
                (a / b - 1.0).abs()
            } else {
                f64::INFINITY
            };
            assert!(
                rel >= CONFIRM_REL_CHANGE,
                "{label}: row {i} relative change {rel} below the rule"
            );
        }
        match r.kind {
            RowKind::Transition => {
                assert_eq!(r.from, state, "{label}: row {i} does not chain");
                assert!(
                    r.onset.is_none(),
                    "{label}: transition row {i} has an onset"
                );
                if i > 0 {
                    assert_ne!(r.from, Some(r.to), "{label}: row {i} is not a change");
                }
                assert!(
                    possible(r.from, r.to),
                    "{label}: impossible transition {:?} -> {:?}",
                    r.from,
                    r.to
                );
                state = Some(r.to);
            }
            RowKind::Onset => {
                assert_eq!(
                    Some(r.to),
                    state,
                    "{label}: onset row {i} is not the current state"
                );
                let on = r.onset.as_ref().expect("onset row has an onset");
                assert!(on.low <= on.played_at && on.played_at <= on.high);
                assert!(
                    on.high <= r.at_played_at,
                    "{label}: onset after the confirming game"
                );
                assert!(r.p_change.is_some(), "{label}: onset row without p_change");
            }
        }
    }
    if let Some(first) = o.rows.first() {
        assert_eq!(first.from, None, "{label}: first row must have from = None");
        assert_eq!(first.kind, RowKind::Transition);
    }
    assert_eq!(
        state.unwrap_or(NotAnIssue),
        o.state,
        "{label}: final state mismatch"
    );
    assert!(
        o.p_imp
            .is_none_or(|p| p.is_finite() && (0.0..=1.0).contains(&p))
    );
}

// ---------------------------------------------------------------------------------------------
// Block 1: TS M2 Task 18
// ---------------------------------------------------------------------------------------------

#[test]
fn detector_version_and_window_cap() {
    assert_eq!(DETECTOR_VERSION, "tl-v1");
    const {
        assert!(
            SERIES_WINDOW_GAMES <= 100,
            "the BOCPD window must stay at or below 100"
        )
    };
    assert_eq!(DEFAULT_HAZARD, 1.0 / 200.0);
    assert_eq!(CONFIRM_P_CHANGE, 0.8);
    assert_eq!(CONFIRM_REL_CHANGE, 0.15);
}

#[test]
fn ts_next_state_table() {
    let cases: [(Option<WeaknessState>, Signals, WeaknessState); 6] = [
        (
            Some(Resolved),
            Signals {
                drift: true,
                ..quiet()
            },
            Relapsed,
        ),
        (
            Some(Active),
            Signals {
                below_p40_streak: 10,
                ..quiet()
            },
            Resolved,
        ),
        (
            Some(Active),
            Signals {
                confirmed_down: true,
                ..quiet()
            },
            Improving,
        ),
        (
            None,
            Signals {
                confidence_above: 0.75,
                ..quiet()
            },
            Active,
        ),
        (
            Some(NotAnIssue),
            Signals {
                cusum_up: true,
                ..quiet()
            },
            Emerging,
        ),
        (None, quiet(), NotAnIssue),
    ];
    for (prev, s, want) in cases {
        assert_eq!(next_state(prev, &s), want, "prev {prev:?}, {s:?}");
    }
}

#[test]
fn ts_stable_series_is_not_an_issue() {
    let o = eval(&repeat(100, (1, 50)));
    assert_eq!(o.rows.len(), 1, "{:?}", o.rows);
    let r = &o.rows[0];
    assert_eq!(
        (r.kind, r.from, r.to),
        (RowKind::Transition, None, NotAnIssue)
    );
    assert_eq!(r.at_game_id, Uuid::from_u128(1));
    assert_eq!(r.p_change, None);
    assert_eq!(o.state, NotAnIssue);
    assert_eq!(o.p_imp, None);
    assert_honest(&o, MIN_EVIDENCE, "stable");
}

#[test]
fn ts_up_series_is_active_with_an_onset_at_game_61() {
    let pts = points(&up_obs());
    let o = evaluate_series(&pts, Some(&ts_ref()), N0, MIN_EVIDENCE);
    assert_eq!(o.state, Active, "{:?}", o.rows);
    let onsets: Vec<&TimelineRow> = o.rows.iter().filter(|r| r.kind == RowKind::Onset).collect();
    assert_eq!(onsets.len(), 1, "{:?}", o.rows);
    let on = onsets[0].onset.as_ref().unwrap();
    let idx = game_index(on.game_id);
    assert!(
        idx.abs_diff(60) <= 2,
        "onset at points[{idx}], want points[60] +- 2"
    );
    assert_eq!(on.played_at, pts[idx].played_at);
    // The onset row dates the change behind the current state.
    assert_eq!((onsets[0].from, onsets[0].to), (Some(NotAnIssue), Active));
    let rb = onsets[0].rate_before.unwrap();
    let ra = onsets[0].rate_after.unwrap();
    assert!(
        (rb - 0.02).abs() < 0.003 && (ra - 0.06).abs() < 0.006,
        "{rb} {ra}"
    );
    assert!(onsets[0].evidence.n as f64 >= MIN_EVIDENCE);
    assert_eq!(o.p_imp.map(|p| p < 0.05), Some(true), "p_imp {:?}", o.p_imp);
    assert_honest(&o, MIN_EVIDENCE, "up");
}

#[test]
fn ts_improvement_goes_through_improving_to_resolved() {
    let o = eval(&improve_obs());
    let path = state_path(&o);
    let imp = index_of(&path, Improving).expect("improving appears");
    let res = index_of(&path, Resolved).expect("resolved appears");
    assert!(imp < res, "{path:?}");
    assert_eq!(o.state, Resolved);
    // The confirmed down change is dated near game 121 (points[120]).
    let downs: Vec<_> = o
        .rows
        .iter()
        .filter(|r| r.kind == RowKind::Onset)
        .filter(|r| r.rate_after < r.rate_before)
        .collect();
    assert_eq!(downs.len(), 1, "{:?}", o.rows);
    let idx = game_index(downs[0].onset.as_ref().unwrap().game_id);
    assert!(idx.abs_diff(120) <= 2, "down onset at points[{idx}]");
    assert!(o.p_imp.unwrap() >= 0.85, "p_imp {:?}", o.p_imp);
    assert_honest(&o, MIN_EVIDENCE, "improve");
}

#[test]
fn ts_relapse_after_resolved() {
    let o = eval(&relapse_obs());
    let path = state_path(&o);
    let res = index_of(&path, Resolved).expect("resolved appears");
    let rel = index_of(&path, Relapsed).expect("relapsed appears");
    assert!(res < rel, "{path:?}");
    // Drift alarms after the first bad games following resolution (game 201 onwards).
    let relapse_row = o.rows.iter().find(|r| r.to == Relapsed).unwrap();
    assert!(game_index(relapse_row.at_game_id) >= 200);
    assert_honest(&o, MIN_EVIDENCE, "relapse");
}

#[test]
fn ts_no_reference_is_only_not_an_issue() {
    for obs in [up_obs(), improve_obs(), relapse_obs(), repeat(40, (5, 10))] {
        let o = evaluate_series(&points(&obs), None, N0, MIN_EVIDENCE);
        assert_eq!(o.rows.len(), 1);
        assert_eq!((o.rows[0].from, o.rows[0].to), (None, NotAnIssue));
        assert_eq!(o.rows[0].p_change, None);
        assert_eq!(o.rows[0].evidence.confidence, 0.0);
        assert_eq!((o.state, o.p_imp), (NotAnIssue, None));
    }
}

// ---------------------------------------------------------------------------------------------
// Block 2: every next_state rule for every previous state
// ---------------------------------------------------------------------------------------------

#[test]
fn next_state_every_transition() {
    let all_prev: [Option<WeaknessState>; 7] = [
        None,
        Some(NotAnIssue),
        Some(Emerging),
        Some(Active),
        Some(Improving),
        Some(Resolved),
        Some(Relapsed),
    ];
    let drift = Signals {
        drift: true,
        ..quiet()
    };
    let streak = Signals {
        below_p40_streak: 10,
        ..quiet()
    };
    let streak9 = Signals {
        below_p40_streak: 9,
        ..quiet()
    };
    let down = Signals {
        confirmed_down: true,
        ..quiet()
    };
    let pimp = Signals {
        p_imp: Some(0.85),
        ..quiet()
    };
    let pimp_low = Signals {
        p_imp: Some(0.849),
        ..quiet()
    };
    let up = Signals {
        confirmed_up: true,
        ..quiet()
    };
    let conf = Signals {
        confidence_above: 0.7,
        ..quiet()
    };
    let conf_low = Signals {
        confidence_above: 0.699,
        ..quiet()
    };
    let cusum = Signals {
        cusum_up: true,
        ..quiet()
    };
    let above = Signals {
        at_or_below_median: false,
        ..quiet()
    };
    // Rule precedence probes.
    let streak_and_down = Signals {
        below_p40_streak: 12,
        confirmed_down: true,
        ..quiet()
    };
    let down_and_conf = Signals {
        confirmed_down: true,
        confidence_above: 0.9,
        ..quiet()
    };
    let up_and_conf = Signals {
        confirmed_up: true,
        confidence_above: 0.9,
        ..quiet()
    };
    let conf_and_cusum = Signals {
        confidence_above: 0.8,
        cusum_up: true,
        ..quiet()
    };
    let everything = Signals {
        confirmed_up: true,
        confirmed_down: true,
        cusum_up: true,
        confidence_above: 1.0,
        at_or_below_median: false,
        below_p40_streak: 50,
        p_imp: Some(1.0),
        drift: true,
    };

    use std::collections::HashMap;
    // (signals name, signals, expected next state per previous state in `all_prev` order).
    let table: Vec<(&str, Signals, [WeaknessState; 7])> = vec![
        //                      None        NotAnIssue  Emerging   Active     Improving  Resolved  Relapsed
        (
            "quiet",
            quiet(),
            [
                NotAnIssue, NotAnIssue, NotAnIssue, NotAnIssue, Improving, Resolved, NotAnIssue,
            ],
        ),
        (
            "drift",
            drift,
            [
                NotAnIssue, NotAnIssue, NotAnIssue, NotAnIssue, Improving, Relapsed, NotAnIssue,
            ],
        ),
        (
            "streak10",
            streak,
            [
                NotAnIssue, NotAnIssue, Resolved, Resolved, Resolved, Resolved, Resolved,
            ],
        ),
        (
            "streak9",
            streak9,
            [
                NotAnIssue, NotAnIssue, NotAnIssue, NotAnIssue, Improving, Resolved, NotAnIssue,
            ],
        ),
        (
            "down",
            down,
            [
                NotAnIssue, NotAnIssue, Improving, Improving, Improving, Resolved, Improving,
            ],
        ),
        (
            "pimp",
            pimp,
            [
                NotAnIssue, NotAnIssue, Improving, Improving, Improving, Resolved, Improving,
            ],
        ),
        (
            "pimp_low",
            pimp_low,
            [
                NotAnIssue, NotAnIssue, NotAnIssue, NotAnIssue, Improving, Resolved, NotAnIssue,
            ],
        ),
        (
            "up",
            up,
            [
                Emerging, Emerging, Emerging, Emerging, Emerging, Resolved, Emerging,
            ],
        ),
        (
            "conf",
            conf,
            [Active, Active, Active, Active, Improving, Resolved, Active],
        ),
        (
            "conf_low",
            conf_low,
            [
                NotAnIssue, NotAnIssue, NotAnIssue, NotAnIssue, Improving, Resolved, NotAnIssue,
            ],
        ),
        (
            "cusum",
            cusum,
            [
                Emerging, Emerging, Emerging, Emerging, Improving, Resolved, Emerging,
            ],
        ),
        (
            "above",
            above,
            [
                NotAnIssue, NotAnIssue, Emerging, Active, Improving, Resolved, Relapsed,
            ],
        ),
        (
            "streak_and_down",
            streak_and_down,
            [
                NotAnIssue, NotAnIssue, Resolved, Resolved, Resolved, Resolved, Resolved,
            ],
        ),
        (
            "down_and_conf",
            down_and_conf,
            [
                Active, Active, Improving, Improving, Improving, Resolved, Improving,
            ],
        ),
        (
            "up_and_conf",
            up_and_conf,
            [Active, Active, Active, Active, Active, Resolved, Active],
        ),
        (
            "conf_and_cusum",
            conf_and_cusum,
            [Active, Active, Active, Active, Improving, Resolved, Active],
        ),
        (
            "everything",
            everything,
            [
                Active, Active, Resolved, Resolved, Resolved, Relapsed, Resolved,
            ],
        ),
    ];
    let mut seen: HashMap<(Option<WeaknessState>, WeaknessState), usize> = HashMap::new();
    for (name, s, want) in &table {
        for (prev, w) in all_prev.iter().zip(want) {
            assert_eq!(next_state(*prev, s), *w, "case {name}, prev {prev:?}");
            *seen.entry((*prev, *w)).or_default() += 1;
        }
    }
    // Every possible transition is exercised by at least one case.
    for prev in all_prev {
        for next in WeaknessState::ALL {
            if possible(prev, next) {
                assert!(
                    seen.contains_key(&(prev, next)),
                    "transition {prev:?} -> {next:?} is not covered by the table"
                );
            }
        }
    }
}

#[test]
fn next_state_ignores_non_finite_signals() {
    for prev in [None, Some(Active), Some(Improving)] {
        let nan = Signals {
            confidence_above: f64::NAN,
            p_imp: Some(f64::NAN),
            ..quiet()
        };
        let want = if prev == Some(Improving) {
            Improving
        } else {
            NotAnIssue
        };
        assert_eq!(next_state(prev, &nan), want);
    }
    let inf = Signals {
        confidence_above: f64::INFINITY,
        ..quiet()
    };
    assert_eq!(
        next_state(None, &inf),
        NotAnIssue,
        "infinite confidence claims nothing"
    );
}

// ---------------------------------------------------------------------------------------------
// Block 3: honesty rules
// ---------------------------------------------------------------------------------------------

#[test]
fn zero_evidence_reports_no_change_and_no_p_change() {
    // BOCPD's p_change from zero evidence is the prior hazard mass (about 0.22 at window 50,
    // far more for long windows); none of it may surface.
    let mut obs = repeat(150, (0, 0));
    let o = eval(&obs);
    assert_eq!(o.rows.len(), 1);
    assert_eq!((o.rows[0].from, o.rows[0].to), (None, NotAnIssue));
    assert_eq!(o.rows[0].p_change, None);
    assert_eq!((o.rows[0].evidence.k, o.rows[0].evidence.n), (0, 0));
    assert_eq!(o.rows[0].evidence.confidence, 0.0);
    assert_eq!((o.state, o.p_imp), (NotAnIssue, None));

    // Zero evidence after a real segment changes nothing either.
    let mut v = repeat(60, (1, 50));
    v.append(&mut obs);
    let o = eval(&v);
    assert!(
        o.rows
            .iter()
            .all(|r| r.kind == RowKind::Transition && r.p_change.is_none())
    );
    assert_eq!(o.state, NotAnIssue);

    // Zero-event games are evidence of a low rate, not of a weakness.
    let o = eval(&repeat(200, (0, 50)));
    assert_eq!(o.rows.len(), 1);
    assert_eq!(o.state, NotAnIssue);
    assert_eq!(o.rows[0].evidence.confidence, 0.0);
}

#[test]
fn a_change_below_min_evidence_is_not_reported() {
    // 15 high games = 750 opportunities since the onset, below min evidence 1000.
    let mut v = repeat(60, (1, 50));
    v.extend(repeat(15, (3, 50)));
    let o = eval(&v);
    assert!(
        o.rows.iter().all(|r| r.kind == RowKind::Transition),
        "{:?}",
        o.rows
    );
    assert!(
        o.rows
            .iter()
            .all(|r| r.p_change.is_none() && r.rate_before.is_none())
    );
    assert_eq!(o.p_imp, None);
    assert_honest(&o, MIN_EVIDENCE, "below gate");

    // The same series with a looser gate confirms the change, so the gate is what held it back.
    let loose = evaluate_series(&points(&v), Some(&ts_ref()), N0, 500.0);
    assert!(
        loose.rows.iter().any(|r| r.kind == RowKind::Onset),
        "{:?}",
        loose.rows
    );
    assert_honest(&loose, 500.0, "loose gate");

    // A gate that cannot be met (NaN or infinite) confirms nothing.
    for gate in [f64::NAN, f64::INFINITY] {
        let o = evaluate_series(&points(&relapse_obs()), Some(&ts_ref()), N0, gate);
        assert!(
            o.rows
                .iter()
                .all(|r| r.kind == RowKind::Transition && r.p_change.is_none())
        );
        assert_eq!(o.p_imp, None);
    }
}

#[test]
fn few_games_claim_no_active_state() {
    // One very bad game must not make a weakness active: confidence needs min evidence.
    let o = eval(&[(5, 50)]);
    assert_eq!(o.state, NotAnIssue);
    assert_eq!(o.rows[0].evidence.confidence, 0.0);
    let o = eval(&repeat(19, (5, 50)));
    assert_eq!(o.state, NotAnIssue, "{:?}", o.rows);
    // With min evidence met (20 games x 50) the same rate is active.
    let o = eval(&repeat(20, (5, 50)));
    assert_eq!(o.state, Active, "{:?}", o.rows);
    assert_honest(&o, MIN_EVIDENCE, "bad start");
}

#[test]
fn invalid_points_are_skipped_and_counted() {
    let clean = eval(&relapse_obs());
    assert_eq!(clean.invalid_points, 0);
    // Insert points with events > opportunities after every 7th game; their game ids come
    // after all clean ones, and their times sit between neighbours.
    let mut pts = points(&relapse_obs());
    let n = pts.len();
    let mut extra = Vec::new();
    for (j, i) in (0..n).step_by(7).enumerate() {
        extra.push(SeriesPoint {
            game_id: Uuid::from_u128(10_000 + j as u128),
            played_at: pts[i].played_at + Duration::minutes(30),
            events: 4,
            opportunities: 3,
            protocol: "sf19-n300k-v1".to_owned(),
        });
    }
    let n_extra = extra.len() as u32;
    pts.extend(extra);
    let dirty = evaluate_series(&pts, Some(&ts_ref()), N0, MIN_EVIDENCE);
    assert_eq!(dirty.invalid_points, n_extra);
    assert_eq!(dirty.rows, clean.rows);
    assert_eq!((dirty.state, dirty.p_imp), (clean.state, clean.p_imp));

    // Only invalid points: nothing at all is claimed.
    let o = evaluate_series(
        &points(&[(3, 1), (9, 0)]),
        Some(&ts_ref()),
        N0,
        MIN_EVIDENCE,
    );
    assert_eq!(o.invalid_points, 2);
    assert!(o.rows.is_empty());
    assert_eq!((o.state, o.p_imp), (NotAnIssue, None));
    let o = evaluate_series(&[], Some(&ts_ref()), N0, MIN_EVIDENCE);
    assert!(o.rows.is_empty());
    assert_eq!(o.state, NotAnIssue);
}

#[test]
fn invalid_reference_or_prior_strength_claims_nothing_or_stays_finite() {
    for r in [
        SeriesRef {
            median: f64::NAN,
            ..ts_ref()
        },
        SeriesRef {
            p40: f64::INFINITY,
            ..ts_ref()
        },
        SeriesRef {
            pooled_rate: -0.1,
            ..ts_ref()
        },
        SeriesRef {
            pooled_rate: 1.5,
            ..ts_ref()
        },
        SeriesRef {
            median: 2.0,
            ..ts_ref()
        },
    ] {
        let o = evaluate_series(&points(&relapse_obs()), Some(&r), N0, MIN_EVIDENCE);
        assert_eq!(o.rows.len(), 1, "{r:?}");
        assert_eq!(o.state, NotAnIssue);
    }
    // Boundary references and odd n0 values still give finite, honest output.
    for (r, n0) in [
        (
            SeriesRef {
                median: 0.0,
                p40: 0.0,
                pooled_rate: 0.0,
            },
            N0,
        ),
        (
            SeriesRef {
                median: 1.0,
                p40: 1.0,
                pooled_rate: 1.0,
            },
            N0,
        ),
        (ts_ref(), 0.0),
        (ts_ref(), f64::NAN),
        (ts_ref(), -5.0),
        (ts_ref(), 1e300),
    ] {
        let o = evaluate_series(&points(&relapse_obs()), Some(&r), n0, MIN_EVIDENCE);
        assert_honest(&o, MIN_EVIDENCE, &format!("{r:?} n0 {n0}"));
    }
}

#[test]
fn onset_rows_are_dated_inside_their_bounds_and_huge_counts_stay_finite() {
    let o = eval(&relapse_obs());
    for r in o.rows.iter().filter(|r| r.kind == RowKind::Onset) {
        let on = r.onset.as_ref().unwrap();
        assert!(on.low <= on.played_at && on.played_at <= on.high, "{on:?}");
    }
    // Opportunity counts near u32::MAX.
    let mut v = repeat(30, (40_000_000, u32::MAX));
    v.extend(repeat(30, (u32::MAX / 2, u32::MAX)));
    let o = evaluate_series(&points(&v), Some(&ts_ref()), N0, MIN_EVIDENCE);
    assert_honest(&o, MIN_EVIDENCE, "huge");
}

// ---------------------------------------------------------------------------------------------
// Block 4: seeded properties
// ---------------------------------------------------------------------------------------------

#[test]
fn property_next_state_never_makes_an_impossible_transition() {
    let mut rng = Rng::new(14);
    let pick = |rng: &mut Rng, xs: &[f64]| xs[(rng.next_f64() * xs.len() as f64) as usize];
    let prevs: [Option<WeaknessState>; 7] = [
        None,
        Some(NotAnIssue),
        Some(Emerging),
        Some(Active),
        Some(Improving),
        Some(Resolved),
        Some(Relapsed),
    ];
    for _ in 0..20_000 {
        let s = Signals {
            confirmed_up: rng.next_f64() < 0.3,
            confirmed_down: rng.next_f64() < 0.3,
            cusum_up: rng.next_f64() < 0.3,
            confidence_above: pick(&mut rng, &[0.0, 0.5, 0.69, 0.7, 0.95, f64::NAN, -1.0, 2.0]),
            at_or_below_median: rng.next_f64() < 0.5,
            below_p40_streak: (rng.next_f64() * 20.0) as u32,
            p_imp: if rng.next_f64() < 0.3 {
                None
            } else {
                Some(pick(&mut rng, &[0.0, 0.5, 0.84, 0.85, 1.0, f64::NAN]))
            },
            drift: rng.next_f64() < 0.3,
        };
        for prev in prevs {
            let next = next_state(prev, &s);
            assert!(possible(prev, next), "{prev:?} -> {next:?} with {s:?}");
            assert_eq!(next_state(prev, &s), next, "deterministic");
        }
    }
}

/// A random piecewise-constant series with 1-5 segments, random opportunities, a few
/// zero-information games and occasional invalid points.
fn random_points(rng: &mut Rng) -> Vec<SeriesPoint> {
    let mut obs = Vec::new();
    let segments = 1 + (rng.next_f64() * 5.0) as usize;
    for _ in 0..segments {
        let len = 5 + (rng.next_f64() * 60.0) as usize;
        let rate = [0.0, 0.005, 0.01, 0.02, 0.04, 0.08, 0.3][(rng.next_f64() * 7.0) as usize];
        for _ in 0..len {
            let n = if rng.next_f64() < 0.05 {
                0
            } else {
                10 + (rng.next_f64() * 60.0) as u32
            };
            let k = (0..n).filter(|_| rng.next_f64() < rate).count() as u32;
            let (k, n) = if rng.next_f64() < 0.02 {
                (n + 1, n)
            } else {
                (k, n)
            };
            obs.push((k, n));
        }
    }
    points(&obs)
}

#[test]
fn property_series_are_honest_deterministic_and_order_independent() {
    let mut rng = Rng::new(1414);
    let mut onsets = 0;
    for case in 0..60 {
        let pts = random_points(&mut rng);
        let refs = [
            ts_ref(),
            SeriesRef {
                median: 0.04,
                p40: 0.03,
                pooled_rate: 0.045,
            },
        ];
        let r = refs[case % 2];
        let gate = [300.0, 1000.0, 2500.0][case % 3];
        let o = evaluate_series(&pts, Some(&r), N0, gate);
        assert_honest(&o, gate, &format!("case {case}"));
        onsets += o.rows.iter().filter(|r| r.kind == RowKind::Onset).count();
        // Same input, same output.
        assert_eq!(
            evaluate_series(&pts, Some(&r), N0, gate),
            o,
            "case {case}: not deterministic"
        );
        // Input order does not matter (points are replayed by time, then game id).
        let mut shuffled = pts.clone();
        for i in (1..shuffled.len()).rev() {
            let j = (rng.next_f64() * (i + 1) as f64) as usize;
            shuffled.swap(i, j);
        }
        assert_eq!(
            evaluate_series(&shuffled, Some(&r), N0, gate),
            o,
            "case {case}: order"
        );
        // No reference: only not_an_issue.
        let none = evaluate_series(&pts, None, N0, gate);
        assert!(none.rows.len() <= 1);
        assert_eq!(none.state, NotAnIssue);
    }
    assert!(
        onsets >= 10,
        "the generator must exercise confirmed changes ({onsets})"
    );
}

// ---------------------------------------------------------------------------------------------
// Cause hints
// ---------------------------------------------------------------------------------------------

fn cg(tc: &str, opp: Option<i32>, line: Option<&str>, day: i64, protocol: &str) -> CauseGame {
    CauseGame {
        tc: tc.to_owned(),
        opponent_rating: opp,
        line_key: line.map(str::to_owned),
        played_at: base_time() + Duration::days(day),
        protocol: protocol.to_owned(),
    }
}

/// `n` games, one per day from `day0`, all alike.
fn games(n: usize, tc: &str, opp: Option<i32>, line: Option<&str>, day0: i64) -> Vec<CauseGame> {
    (0..n)
        .map(|i| cg(tc, opp, line, day0 + i as i64, "sf19-n300k-v1"))
        .collect()
}

fn kinds(c: &[CandidateCause]) -> Vec<CauseKind> {
    c.iter().map(|c| c.kind).collect()
}

fn assert_valid_causes(c: &[CandidateCause]) {
    assert!(c.len() <= LIMITS.candidate_causes);
    for cause in c {
        let chars = cause.description.chars().count();
        assert!((1..=LIMITS.description_chars).contains(&chars), "{cause:?}");
        for v in [&cause.before, &cause.after] {
            match v {
                CauseValue::Text(s) => assert!(s.chars().count() <= LIMITS.label_chars),
                CauseValue::Number(x) => assert!(x.is_finite()),
            }
        }
        // A cause is a correlate, never presented as proven.
        let d = cause.description.to_lowercase();
        for word in [
            "caused",
            "because",
            "due to",
            "proven",
            "proof",
            "the reason",
        ] {
            assert!(!d.contains(word), "description claims causation: {d}");
        }
    }
}

#[test]
fn ts_cause_time_control_mix() {
    let before = games(20, "300+0", Some(1500), None, 0);
    let mut after = games(15, "180+0", Some(1500), None, 20);
    after.extend(games(5, "300+0", Some(1500), None, 35));
    let c = cause_hints(&before, &after);
    assert_eq!(kinds(&c), vec![CauseKind::TimeControlMix], "{c:?}");
    assert_eq!(c[0].before, CauseValue::Text("300+0".to_owned()));
    assert_eq!(c[0].after, CauseValue::Text("180+0".to_owned()));
    assert_eq!(c[0].description, "Most games moved from 300+0 to 180+0");
    assert_valid_causes(&c);
}

#[test]
fn ts_cause_rating_level() {
    let before = games(20, "300+0", Some(1500), None, 0);
    let after = games(20, "300+0", Some(1600), None, 20);
    let c = cause_hints(&before, &after);
    assert_eq!(kinds(&c), vec![CauseKind::RatingLevel], "{c:?}");
    assert_eq!(
        (c[0].before.clone(), c[0].after.clone()),
        (CauseValue::Number(1500.0), CauseValue::Number(1600.0))
    );
    assert_valid_causes(&c);
    // A 74-point shift is below the trigger; games without a rating do not count.
    let after = games(20, "300+0", Some(1574), None, 20);
    assert!(cause_hints(&before, &after).is_empty());
    let mut after = games(20, "300+0", Some(1500), None, 20);
    after.extend(games(20, "300+0", None, None, 40));
    assert!(cause_hints(&before, &after).is_empty());
}

#[test]
fn ts_cause_new_opening() {
    let before = games(20, "300+0", Some(1500), Some("white:italian"), 0);
    let mut after = games(12, "300+0", Some(1500), Some("white:italian"), 20);
    after.extend(games(8, "300+0", Some(1500), Some("white:london"), 32));
    let c = cause_hints(&before, &after);
    assert_eq!(kinds(&c), vec![CauseKind::NewOpening], "{c:?}");
    assert_eq!(c[0].before, CauseValue::Number(0.0));
    assert_eq!(c[0].after, CauseValue::Number(0.4));
    assert!(
        c[0].description.contains("white:london"),
        "{}",
        c[0].description
    );
    assert_valid_causes(&c);
}

#[test]
fn ts_cause_equal_inputs_give_nothing() {
    let g = games(20, "300+0", Some(1500), Some("white:italian"), 0);
    assert!(cause_hints(&g, &g).is_empty());
    assert!(cause_hints(&[], &[]).is_empty());
}

#[test]
fn cause_session_length_and_protocol() {
    // Before: one game per day. After: four games per day.
    let before = games(20, "300+0", Some(1500), None, 0);
    let after: Vec<CauseGame> = (0..20)
        .map(|i| cg("300+0", Some(1500), None, 20 + i / 4, "sf19-n300k-v1"))
        .collect();
    let c = cause_hints(&before, &after);
    assert_eq!(kinds(&c), vec![CauseKind::SessionLength], "{c:?}");
    assert_eq!(
        (c[0].before.clone(), c[0].after.clone()),
        (CauseValue::Number(1.0), CauseValue::Number(4.0))
    );
    assert_valid_causes(&c);

    let after: Vec<CauseGame> = (0..20)
        .map(|i| cg("300+0", Some(1500), None, 20 + i, "sf19-n300k-v2"))
        .collect();
    let c = cause_hints(&before, &after);
    assert_eq!(kinds(&c), vec![CauseKind::ProtocolVersion], "{c:?}");
    assert_eq!(c[0].before, CauseValue::Text("sf19-n300k-v1".to_owned()));
    assert_eq!(c[0].after, CauseValue::Text("sf19-n300k-v2".to_owned()));
    assert_valid_causes(&c);
}

#[test]
fn cause_hints_need_enough_games_and_are_deterministic() {
    // Everything changes, but one side has too few games: nothing is claimed.
    let before = games(MIN_CAUSE_GAMES - 1, "600+0", Some(1200), Some("a"), 0);
    let after = games(20, "60+0", Some(1900), Some("b"), 30);
    assert!(cause_hints(&before, &after).is_empty());
    assert!(cause_hints(&after, &before).is_empty());
    // With enough games every kind fires, in a fixed order.
    let before = games(MIN_CAUSE_GAMES, "600+0", Some(1200), Some("a"), 0);
    let after: Vec<CauseGame> = (0..20)
        .map(|i| cg("60+0", Some(1900), Some("b"), 30 + i / 5, "sf19-n300k-v9"))
        .collect();
    let c = cause_hints(&before, &after);
    assert_eq!(
        kinds(&c),
        vec![
            CauseKind::TimeControlMix,
            CauseKind::NewOpening,
            CauseKind::SessionLength,
            CauseKind::RatingLevel,
            CauseKind::ProtocolVersion
        ]
    );
    assert_valid_causes(&c);
    let mut rev_b = before.clone();
    rev_b.reverse();
    let mut rev_a = after.clone();
    rev_a.reverse();
    assert_eq!(
        cause_hints(&rev_b, &rev_a),
        c,
        "input order must not matter"
    );
}

#[test]
fn cause_descriptions_and_values_respect_limits() {
    let long_tc = "9".repeat(1000);
    let long_line = format!("white:{}", "é".repeat(900));
    let before = games(20, "300+0", Some(1500), None, 0);
    let after: Vec<CauseGame> = (0..20)
        .map(|_| CauseGame {
            tc: long_tc.clone(),
            opponent_rating: Some(i32::MAX),
            line_key: Some(long_line.clone()),
            played_at: base_time() + Duration::days(40),
            protocol: "p".repeat(2000),
        })
        .collect();
    let c = cause_hints(&before, &after);
    assert_eq!(c.len(), 5, "{c:?}");
    assert_valid_causes(&c);
    // Extreme ratings stay finite.
    let lo = games(20, "300+0", Some(i32::MIN), None, 0);
    assert_valid_causes(&cause_hints(&lo, &after));
}
