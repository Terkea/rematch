//! R2 Task 12: weakness posteriors, P_imp and priority.
//!
//! The first block ports TS M2 Task 16's assertions (`toBeCloseTo(x, d)` becomes
//! `|got - want| < 0.5 * 10^-d`). The second block pins the R2 additions (ADR-R2-5): the shape
//! floor for exactly zero shapes, the mean always inside its 80% interval, zero-event estimates
//! that claim nothing, invalid-point handling, the reason precedence, fatigue, and seeded
//! property tests over zero counts, extreme priors and invalid points. P_imp references come from an independent Simpson integration of
//! `P(X < c Y)` with exact integer-shape incomplete beta values (not from this implementation).

use profile_core::constants::{min_evidence, n0, trainability};
use profile_core::stats::posterior::{
    BetaPosterior, MAX_SHAPE, P_IMP_DELTA_REL, P_IMP_DRAWS, P_IMP_SEED, REGRESSION_FACTOR,
    SHAPE_FLOOR, p_improvement, p_regression, posterior, prob_above,
};
use profile_core::stats::priority::{
    EstimateInput, EstimateReason, MAX_EPL_PER_EVENT, MIN_GAMES, WINDOW_DAYS, WeaknessEstimate,
    estimate_weakness, estimate_weakness_with, rank_weaknesses,
};
use profile_core::stats::rng::Rng;
use profile_core::stats::variant::PriorRule;
use profile_core::stats::weights::HALF_LIFE_DAYS;
use profile_core::types::{Band, BandReferenceStat, MetricId, WeaknessKey};

fn close(got: f64, want: f64, digits: i32) {
    let tol = 0.5 * 10f64.powi(-digits);
    assert!(
        (got - want).abs() < tol,
        "got {got}, want {want} (to {digits} dp)"
    );
}

fn within(got: f64, want: f64, tol: f64) {
    assert!((got - want).abs() <= tol, "got {got}, want {want} +- {tol}");
}

fn reference(key: WeaknessKey, pooled: f64, median: f64, p40: f64) -> BandReferenceStat {
    BandReferenceStat {
        metric: MetricId::Weakness(key),
        median,
        p25: p40 * 0.85,
        p40,
        p75: median * 1.5,
        pooled_rate: Some(pooled),
        epl_per_event: Some(0.25),
        opps_per_game: Some(10.0),
        opp_slope_per_100: Some(0.0),
        n_players: 500,
        n_games: 20_000,
        low_sample: false,
    }
}

fn w1_ref() -> BandReferenceStat {
    BandReferenceStat {
        metric: MetricId::Weakness(WeaknessKey::W1),
        median: 0.02,
        p25: 0.015,
        p40: 0.017,
        p75: 0.03,
        pooled_rate: Some(0.02),
        epl_per_event: Some(0.25),
        opps_per_game: Some(10.0),
        opp_slope_per_100: Some(0.0),
        n_players: 500,
        n_games: 20_000,
        low_sample: false,
    }
}

const W1_POINTS: [(f64, f64, f64, f64); 1] = [(10.0, 400.0, 2.5, 0.0)];
/// Clears the ranking floor (ADR 0101): 40 in 400 against a median of 0.02.
const W1_STRONG: [(f64, f64, f64, f64); 1] = [(40.0, 400.0, 10.0, 0.0)];

fn w1_input<'a>(
    points: &'a [(f64, f64, f64, f64)],
    games: u32,
    r: Option<&'a BandReferenceStat>,
) -> EstimateInput<'a> {
    EstimateInput {
        key: WeaknessKey::W1,
        points,
        games_in_window: games,
        band: Band::B1,
        reference: r,
        fatigue: 0.0,
    }
}

// ---------------------------------------------------------------------------------------------
// TS M2 Task 16
// ---------------------------------------------------------------------------------------------

#[test]
fn ts_w1_estimate() {
    // TS M2 Task 16 pinned the rule of algo-1.x (ADR-R2-5); algo-2's rule is in prior_rule.rs.
    let r = w1_ref();
    let e = estimate_weakness_with(&w1_input(&W1_POINTS, 40, Some(&r)), PriorRule::ALGO_1);
    assert_eq!(e.key, WeaknessKey::W1);
    assert_eq!(e.posterior.alpha, 16.0);
    assert_eq!(e.posterior.beta, 684.0);
    close(e.posterior.mean, 0.0228571, 7);
    close(e.posterior.interval.low, 0.0159747, 7);
    close(e.posterior.interval.high, 0.0303238, 7);
    close(e.confidence, 0.672363, 6);
    close(e.excess, 0.0071429, 6);
    // ADR 0101: the 80% interval [0.016, 0.030] holds the median 0.02 and the excess is 0.007
    // of a point per game, so the estimate is below the ranking floor and ranks nowhere.
    assert!(!e.credible);
    assert_eq!(e.priority, 0.0);
    assert!(!e.precise);
    assert!(!e.min_evidence_met);
    assert_eq!(e.reason, EstimateReason::Ok);
    assert_eq!(e.k, 10.0);
    assert_eq!(e.n, 400.0);
    assert_eq!(e.k_unweighted, 10.0);
    assert_eq!(e.n_unweighted, 400.0);
    assert_eq!(e.band_median, Some(0.02));
    assert_eq!(e.band_p40, Some(0.017));
    assert_eq!(e.trainability, 1.0);
    // Lower is better: a rate above the median is below the 50th percentile.
    let p = e.percentile.expect("percentile with a reference");
    assert!((1.0..50.0).contains(&p), "percentile {p}");
}

#[test]
fn no_reference_gives_priority_0() {
    let e = estimate_weakness(&w1_input(&W1_POINTS, 40, None));
    assert_eq!(e.reason, EstimateReason::NoBenchmark);
    assert_eq!(e.priority, 0.0);
    // Beta(1, 1) prior: alpha = 1 + k, beta = 1 + n - k.
    assert_eq!(e.posterior.alpha, 11.0);
    assert_eq!(e.posterior.beta, 391.0);
    assert_eq!(e.band_median, None);
    assert_eq!(e.band_p40, None);
    assert_eq!(e.percentile, None);
    assert_eq!(e.excess, 0.0);
    assert_eq!(e.confidence, 0.0);
    assert!(!e.precise);
}

#[test]
fn under_30_games_is_insufficient_data() {
    let r = w1_ref();
    let e = estimate_weakness(&w1_input(&W1_POINTS, 29, Some(&r)));
    assert_eq!(e.reason, EstimateReason::InsufficientData);
    assert_eq!(e.priority, 0.0);
    assert_eq!(MIN_GAMES, 30);
    let e = estimate_weakness(&w1_input(&W1_STRONG, 30, Some(&r)));
    assert_eq!(e.reason, EstimateReason::Ok);
    assert!(e.credible);
    assert!(e.priority > 0.0);
}

#[test]
fn point_aged_200_days_is_ignored() {
    let r = w1_ref();
    let base = estimate_weakness(&w1_input(&W1_POINTS, 40, Some(&r)));
    let pts = [W1_POINTS[0], (90.0, 100.0, 9.0, 200.0)];
    let e = estimate_weakness(&w1_input(&pts, 40, Some(&r)));
    assert_eq!(e, base);
    // The weighted sums of algo-1.x's rule (algo-2 counts them at their effective size).
    // The window is 183 days inclusive; a point at 183 counts with weight 0.5^(183/60).
    assert_eq!(WINDOW_DAYS, 183.0);
    let pts = [W1_POINTS[0], (2.0, 100.0, 1.0, 183.0)];
    let e = estimate_weakness_with(&w1_input(&pts, 40, Some(&r)), PriorRule::ALGO_1);
    let w = 0.5_f64.powf(183.0 / 60.0);
    within(e.k, 10.0 + 2.0 * w, 1e-12);
    within(e.n, 400.0 + 100.0 * w, 1e-12);
    assert_eq!(e.k_unweighted, 12.0);
    assert_eq!(e.n_unweighted, 500.0);
}

fn beta(a: f64, b: f64) -> BetaPosterior {
    // posterior(k, n, mu, n0) with n0 = 0 leaves the counts as the shapes.
    let p = posterior(a, a + b, 0.5, 0.0);
    within(p.alpha, a, 1e-12 * a);
    within(p.beta, b, 1e-12 * (a + b));
    p
}

#[test]
fn ts_p_improvement_and_regression() {
    assert_eq!(P_IMP_DELTA_REL, 0.15);
    assert_eq!(REGRESSION_FACTOR, 1.15);
    assert_eq!(P_IMP_DRAWS, 10_000);
    assert_eq!(P_IMP_SEED, 1);
    let imp = |c: &BetaPosterior, b: &BetaPosterior| {
        p_improvement(c, b, P_IMP_DELTA_REL, P_IMP_DRAWS, P_IMP_SEED)
    };
    within(imp(&beta(12.0, 988.0), &beta(30.0, 970.0)), 0.99096, 0.01);
    within(imp(&beta(50.0, 950.0), &beta(50.0, 950.0)), 0.20296, 0.01);
    let reg = p_regression(
        &beta(45.0, 955.0),
        &beta(30.0, 970.0),
        REGRESSION_FACTOR,
        P_IMP_DRAWS,
        P_IMP_SEED,
    );
    within(reg, 0.87883, 0.01);
    // The same seed gives identical results.
    let (c, b) = (beta(12.0, 988.0), beta(30.0, 970.0));
    assert_eq!(imp(&c, &b), imp(&c, &b));
    assert_eq!(
        p_regression(&c, &b, 1.15, 10_000, 7),
        p_regression(&c, &b, 1.15, 10_000, 7)
    );
}

#[test]
fn ts_rank_weaknesses_drops_continuous_and_sorts() {
    let r = w1_ref();
    let mk = |key: WeaknessKey, priority: f64| WeaknessEstimate {
        key,
        priority,
        ..estimate_weakness(&w1_input(&W1_POINTS, 40, Some(&r)))
    };
    let ranked = rank_weaknesses(vec![
        mk(WeaknessKey::W12, 9.0),
        mk(WeaknessKey::W2, 0.002),
        mk(WeaknessKey::W9d, 8.0),
        mk(WeaknessKey::W5, 0.01),
        mk(WeaknessKey::W10, 0.002),
        mk(WeaknessKey::W1, 0.0),
    ]);
    let keys: Vec<_> = ranked.iter().map(|e| e.key).collect();
    // Priority desc, then key: "W10" < "W2" as strings (TS key order).
    assert_eq!(
        keys,
        [
            WeaknessKey::W5,
            WeaknessKey::W10,
            WeaknessKey::W2,
            WeaknessKey::W1
        ]
    );
}

// ---------------------------------------------------------------------------------------------
// R2 additions
// ---------------------------------------------------------------------------------------------

#[test]
fn p_improvement_matches_exact_integration() {
    // Simpson integration of P(Xc < c Xb) with exact incomplete beta (scratchpad pimp.py).
    let cases = [
        (12.0, 988.0, 30.0, 970.0, 0.85, 0.9909591724515829, true),
        (50.0, 950.0, 50.0, 950.0, 0.85, 0.20295638035821587, true),
        (45.0, 955.0, 30.0, 970.0, 1.15, 0.8788281481965678, false),
    ];
    for (ac, bc, ab, bb, c, want, imp) in cases {
        let (cur, base) = (beta(ac, bc), beta(ab, bb));
        let got = if imp {
            p_improvement(&cur, &base, 1.0 - c, 200_000, 3)
        } else {
            p_regression(&cur, &base, c, 200_000, 3)
        };
        // 200 000 draws: SE <= 0.0012; 4 SE.
        within(got, want, 0.0045);
    }
}

#[test]
fn p_improvement_edges() {
    let p = beta(20.0, 980.0);
    // Identical posteriors, no margin: P(Xc < Xb) = 1/2 by symmetry.
    within(p_improvement(&p, &p, 0.0, 100_000, 5), 0.5, 0.01);
    within(p_regression(&p, &p, 1.0, 100_000, 5), 0.5, 0.01);
    // No draws: no evidence either way.
    assert_eq!(p_improvement(&p, &p, 0.15, 0, 1), 0.0);
    assert_eq!(p_regression(&p, &p, 1.15, 0, 1), 0.0);
    // A different seed is a different, equally valid estimate.
    let a = p_improvement(&p, &p, 0.15, 10_000, 1);
    let b = p_improvement(&p, &p, 0.15, 10_000, 2);
    assert_ne!(a, b);
    within(a, b, 0.03);
}

// Fix round 1: the shape floor applies only to an exactly zero shape, and an estimate with no
// in-window events never claims confidence or a percentile.

#[test]
fn floor_applies_only_to_an_exactly_zero_shape() {
    assert_eq!(SHAPE_FLOOR, 1e-3);
    // Band where nobody showed the event: prior Beta(0, n0) floors alpha.
    let p = posterior(0.0, 0.0, 0.0, 300.0);
    assert_eq!(p.alpha, SHAPE_FLOOR);
    assert_eq!(p.beta, 300.0);
    assert!(p.mean > 0.0 && p.mean.is_finite());
    assert!(p.interval.low <= p.mean && p.mean <= p.interval.high);
    // Pooled rate 1 with every opportunity an event floors beta symmetrically.
    let p = posterior(5.0, 5.0, 1.0, 300.0);
    assert_eq!((p.alpha, p.beta), (305.0, SHAPE_FLOOR));
    assert!(p.interval.low <= p.mean && p.mean <= p.interval.high);
    // A tiny but positive prior shape is not floored: the interval widens to the mean instead.
    let p = posterior(0.0, 0.0, 1e-9, 300.0);
    within(p.alpha, 3e-7, 1e-20);
    assert!(
        p.interval.low <= p.mean && p.mean <= p.interval.high,
        "{p:?}"
    );
}

#[test]
fn review_case_tiny_alpha_keeps_mean_inside_interval() {
    // T11 review: equal-tailed Beta(0.02, 40) has q90 = 7.5e-5 < mean 5e-4. Not floored (the
    // shape is positive); the interval is widened to the mean.
    let p = posterior(0.0, 0.0, 0.0005, 40.0);
    within(p.alpha, 0.02, 1e-15);
    assert_eq!(p.beta, 40.0 * (1.0 - 0.0005));
    within(p.mean, 0.0005, 1e-15);
    assert!(
        p.interval.low <= p.mean && p.mean <= p.interval.high,
        "{p:?}"
    );
    assert_eq!(p.interval.high, p.mean);
}

#[test]
fn interval_contains_mean_on_a_shape_grid() {
    let shapes = [
        SHAPE_FLOOR,
        0.02,
        0.04,
        0.1,
        0.5,
        1.0,
        30.0,
        3000.0,
        30_000.0,
    ];
    for &a in &shapes {
        for &b in &shapes {
            let p = beta(a, b);
            assert!(
                p.interval.low <= p.mean && p.mean <= p.interval.high,
                "Beta({a}, {b}) = {p:?}"
            );
        }
    }
}

#[test]
fn floor_never_applies_with_an_in_window_event() {
    // The lightest in-window event weighs 0.5^(183/60) = 0.121, far above the floor.
    let lightest = 0.5_f64.powf(WINDOW_DAYS / HALF_LIFE_DAYS);
    assert!(lightest > 100.0 * SHAPE_FLOOR, "{lightest}");
    for age in [0.0, 60.0, WINDOW_DAYS] {
        let r = reference(WeaknessKey::W4Allowed, 0.0, 0.0, 0.0);
        let pts = [(1.0, 50.0, 0.0, age)];
        let input = EstimateInput {
            key: WeaknessKey::W4Allowed,
            points: &pts,
            games_in_window: 60,
            band: Band::B3,
            reference: Some(&r),
            fatigue: 0.0,
        };
        let w = 0.5_f64.powf(age / HALF_LIFE_DAYS);
        // Pooled rate 0: alpha is exactly the weighted event (algo-1.x), never the floor.
        let e = estimate_weakness_with(&input, PriorRule::ALGO_1);
        assert_eq!(e.posterior.alpha, w, "age {age}");
        assert!(e.posterior.alpha >= lightest);
        assert_eq!(e.reason, EstimateReason::Ok);
        // algo-2 counts one game at its effective size, its own: one event, never the floor.
        let e = estimate_weakness(&input);
        assert!((e.posterior.alpha - 1.0).abs() < 1e-12, "age {age}");
        assert_eq!(e.reason, EstimateReason::Ok);
    }
}

/// Realistic zero-event cases: (key, band, pooled, median, p40, opportunities, games).
const ZERO_EVENT_TABLE: [(WeaknessKey, Band, f64, f64, f64, f64, u32); 7] = [
    (WeaknessKey::W4Allowed, Band::B3, 0.0, 0.0, 0.0, 50.0, 60),
    (WeaknessKey::W4Allowed, Band::B3, 0.002, 0.0, 0.0, 50.0, 60),
    (WeaknessKey::W8Basics, Band::B2, 0.01, 0.0, 0.0, 30.0, 100),
    (WeaknessKey::W3Deep, Band::B5, 0.001, 0.0, 0.0, 500.0, 300),
    (WeaknessKey::W5, Band::B4, 0.05, 0.03, 0.02, 15.0, 200),
    (WeaknessKey::W1, Band::B1, 0.02, 0.015, 0.012, 400.0, 40),
    (WeaknessKey::W9a, Band::B2, 0.0005, 0.0, 0.0, 120.0, 120),
];

#[test]
fn zero_event_estimates_claim_nothing() {
    for (key, band, pooled, median, p40, opps, games) in ZERO_EVENT_TABLE {
        let r = reference(key, pooled, median, p40);
        // Zero in-window events, plus events that are outside the window.
        let pts = [(0.0, opps, 0.0, 5.0), (7.0, 20.0, 1.0, WINDOW_DAYS + 1.0)];
        let e = estimate_weakness(&EstimateInput {
            key,
            points: &pts,
            games_in_window: games,
            band,
            reference: Some(&r),
            fatigue: 0.0,
        });
        let ctx = format!("{key} {band:?} pooled {pooled} median {median}: {e:?}");
        assert_eq!(e.k_unweighted, 0.0, "{ctx}");
        assert_eq!(e.reason, EstimateReason::NoEvents, "{ctx}");
        assert_eq!(e.confidence, 0.0, "{ctx}");
        assert_eq!(e.percentile, None, "{ctx}");
        assert_eq!(e.priority, 0.0, "{ctx}");
        assert!(e.posterior.interval.low <= e.posterior.mean, "{ctx}");
        assert!(e.posterior.mean <= e.posterior.interval.high, "{ctx}");
        assert_eq!(e.band_median, Some(median), "{ctx}");
    }
}

#[test]
fn large_counts_are_scaled_not_panicking() {
    assert_eq!(MAX_SHAPE, 1e7);
    let p = posterior(3e14, 1e15, 0.25, 1e14);
    assert!(p.alpha + p.beta <= MAX_SHAPE * (1.0 + 1e-12), "{p:?}");
    // The mean is preserved: (0.25e14 + 3e14) / 1.1e15.
    within(p.mean, 3.25e14 / 1.1e15, 1e-12);
    assert!(p.interval.low <= p.mean && p.mean <= p.interval.high);
    let p = posterior(1e300, 1e300, 0.5, 1e300);
    assert!(
        p.alpha.is_finite() && p.beta.is_finite() && p.mean.is_finite(),
        "{p:?}"
    );
}

#[test]
fn invalid_points_are_skipped_and_counted() {
    let r = w1_ref();
    let base = estimate_weakness(&w1_input(&W1_POINTS, 40, Some(&r)));
    assert_eq!(base.invalid_points, 0);
    let bad = [
        W1_POINTS[0],
        (f64::NAN, 10.0, 0.0, 0.0),
        (1.0, f64::INFINITY, 0.0, 0.0),
        (-1.0, 10.0, 0.0, 0.0),
        (1.0, -10.0, 0.0, 0.0),
        (11.0, 10.0, 0.0, 0.0),
        (1.0, 10.0, f64::NAN, 0.0),
        (1.0, 10.0, -0.5, 0.0),
        (1.0, 10.0, 0.0, f64::NAN),
        (1.0, 10.0, 0.0, f64::NEG_INFINITY),
        (1.0, 2e9, 0.0, 0.0),
    ];
    let e = estimate_weakness(&w1_input(&bad, 40, Some(&r)));
    assert_eq!(e.invalid_points, 10);
    assert_eq!(
        WeaknessEstimate {
            invalid_points: 0,
            ..e
        },
        base
    );
    // An old point is valid but outside the window: ignored, not invalid. A negative age is
    // clock skew (weight 1), not invalid.
    let pts = [
        W1_POINTS[0],
        (1.0, 10.0, 0.0, 400.0),
        (0.0, 10.0, 0.0, -2.0),
    ];
    let e = estimate_weakness(&w1_input(&pts, 40, Some(&r)));
    assert_eq!(e.invalid_points, 0);
    assert_eq!(e.n_unweighted, 410.0);
}

#[test]
fn fractional_events_below_one_are_invalid() {
    let r = w1_ref();
    let base = estimate_weakness(&w1_input(&W1_POINTS, 40, Some(&r)));
    for events in [1e-300, 0.5, 0.999] {
        let pts = [W1_POINTS[0], (events, 10.0, 0.0, 0.0)];
        let e = estimate_weakness(&w1_input(&pts, 40, Some(&r)));
        assert_eq!(e.invalid_points, 1, "events {events}");
        assert_eq!(e.k_unweighted, 10.0);
    }
    // 0 and whole counts (and counts of at least 1) stay valid.
    let pts = [W1_POINTS[0], (0.0, 10.0, 0.0, 0.0), (1.0, 10.0, 0.0, 0.0)];
    let e = estimate_weakness(&w1_input(&pts, 40, Some(&r)));
    assert_eq!(e.invalid_points, 0);
    assert_eq!(e.k_unweighted, base.k_unweighted + 1.0);
}

#[test]
fn huge_epl_per_event_is_no_benchmark() {
    assert_eq!(MAX_EPL_PER_EVENT, 1e3);
    for epl in [1e308, f64::INFINITY, f64::NAN, -0.1, 1000.5] {
        let mut r = w1_ref();
        r.epl_per_event = Some(epl);
        let e = estimate_weakness(&w1_input(&W1_POINTS, 40, Some(&r)));
        assert_eq!(e.reason, EstimateReason::NoBenchmark, "epl {epl}");
        assert!(e.excess.is_finite() && e.priority.is_finite(), "epl {epl}");
        assert_eq!(e.priority, 0.0);
    }
    // The bound itself is usable, and excess stays finite with the largest counts.
    let mut r = w1_ref();
    r.epl_per_event = Some(MAX_EPL_PER_EVENT);
    let pts = [(1e9, 1e9, 0.0, 0.0); 4];
    let e = estimate_weakness(&w1_input(&pts, 30, Some(&r)));
    assert_eq!(e.reason, EstimateReason::Ok);
    assert!(
        e.excess.is_finite() && e.excess > 0.0 && e.priority.is_finite(),
        "{e:?}"
    );
}

#[test]
fn non_finite_priorities_rank_last() {
    let r = w1_ref();
    let mk = |key: WeaknessKey, priority: f64| WeaknessEstimate {
        key,
        priority,
        ..estimate_weakness(&w1_input(&W1_POINTS, 40, Some(&r)))
    };
    let ranked = rank_weaknesses(vec![
        mk(WeaknessKey::W2, f64::NAN),
        mk(WeaknessKey::W5, f64::INFINITY),
        mk(WeaknessKey::W1, 0.001),
        mk(WeaknessKey::W6, 0.0),
        mk(WeaknessKey::W3Deep, f64::NEG_INFINITY),
    ]);
    let keys: Vec<_> = ranked.iter().map(|e| e.key).collect();
    // Finite first (priority desc), then the non-finite ones by key.
    assert_eq!(
        keys,
        [
            WeaknessKey::W1,
            WeaknessKey::W6,
            WeaknessKey::W2,
            WeaknessKey::W3Deep,
            WeaknessKey::W5
        ]
    );
}

#[test]
fn benchmark_needs_finite_quartiles_and_a_rate_median() {
    for edit in 0..4 {
        let mut r = w1_ref();
        match edit {
            0 => r.p25 = f64::NAN,
            1 => r.p75 = f64::INFINITY,
            2 => r.median = 1.5,
            _ => r.median = -0.01,
        }
        let e = estimate_weakness(&w1_input(&W1_POINTS, 40, Some(&r)));
        assert_eq!(e.reason, EstimateReason::NoBenchmark, "edit {edit}");
        assert_eq!(e.priority, 0.0);
    }
}

#[test]
fn reason_precedence_and_missing_benchmark_fields() {
    // Fewer than 30 games and no reference: insufficient data wins (it is what the user can fix).
    let e = estimate_weakness(&w1_input(&W1_POINTS, 10, None));
    assert_eq!(e.reason, EstimateReason::InsufficientData);
    assert_eq!(e.priority, 0.0);
    // A reference without a pooled rate or EPL per event is no benchmark.
    for strip_pooled in [true, false] {
        let mut r = w1_ref();
        if strip_pooled {
            r.pooled_rate = None;
        } else {
            r.epl_per_event = None;
        }
        let e = estimate_weakness(&w1_input(&W1_POINTS, 40, Some(&r)));
        assert_eq!(e.reason, EstimateReason::NoBenchmark);
        assert_eq!(e.priority, 0.0);
        assert_eq!(e.band_median, None);
    }
}

#[test]
fn fatigue_scales_priority() {
    let r = w1_ref();
    let base = estimate_weakness(&w1_input(&W1_STRONG, 40, Some(&r)));
    assert!(base.priority > 0.0);
    let at = |fatigue: f64| {
        estimate_weakness(&EstimateInput {
            fatigue,
            ..w1_input(&W1_STRONG, 40, Some(&r))
        })
        .priority
    };
    within(at(0.5), base.priority * 0.5, 1e-15);
    assert_eq!(at(1.0), 0.0);
    // Out-of-range or NaN fatigue is clamped to [0, 1] (NaN counts as no fatigue).
    assert_eq!(at(2.0), 0.0);
    assert_eq!(at(-1.0), base.priority);
    assert_eq!(at(f64::NAN), base.priority);
}

#[test]
fn precise_and_min_evidence() {
    // 20 000 opportunities at the band rate: a narrow interval and enough evidence.
    let r = w1_ref();
    let pts = [(400.0, 20_000.0, 0.0, 0.0)];
    let e = estimate_weakness(&w1_input(&pts, 400, Some(&r)));
    let width = e.posterior.interval.high - e.posterior.interval.low;
    assert!(width <= 0.25 * 0.02, "width {width}");
    assert!(e.precise);
    assert!(e.min_evidence_met);
    assert_eq!(min_evidence(WeaknessKey::W1), 1000.0);
    // Exactly at the minimum counts.
    let pts = [(20.0, 1000.0, 0.0, 0.0)];
    assert!(estimate_weakness(&w1_input(&pts, 100, Some(&r))).min_evidence_met);
    let pts = [(20.0, 999.0, 0.0, 0.0)];
    assert!(!estimate_weakness(&w1_input(&pts, 100, Some(&r))).min_evidence_met);
}

// ---------------------------------------------------------------------------------------------
// Seeded property tests (no proptest crate: inputs drawn from mulberry32)
// ---------------------------------------------------------------------------------------------

/// A value from a heavy-tailed menu that hits the edges often.
fn pick(rng: &mut Rng, menu: &[f64]) -> f64 {
    let i = (rng.next_f64() * menu.len() as f64) as usize;
    menu[i.min(menu.len() - 1)]
}

#[test]
fn property_posterior_is_finite_and_contains_its_mean() {
    let mut rng = Rng::new(12);
    let mus = [0.0, 1e-12, 1e-6, 0.0005, 0.02, 0.5, 0.98, 1.0 - 1e-9, 1.0];
    let n0s = [0.0, 1e-9, 1.0, 40.0, 300.0, 600.0, 1e4, 1e6, 1e12];
    let ns = [0.0, 1e-6, 0.3, 1.0, 7.0, 400.0, 2e4, 2e5, 1e15];
    for _ in 0..1500 {
        let mu = pick(&mut rng, &mus);
        let n0v = pick(&mut rng, &n0s);
        let n = pick(&mut rng, &ns);
        // k at 0, at n, or anywhere in between.
        let k = match (rng.next_f64() * 3.0) as u32 {
            0 => 0.0,
            1 => n,
            _ => n * rng.next_f64(),
        };
        let p = posterior(k, n, mu, n0v);
        let ctx = format!("posterior({k}, {n}, {mu}, {n0v}) = {p:?}");
        assert!(p.alpha > 0.0 && p.beta > 0.0, "{ctx}");
        assert!(p.alpha.is_finite() && p.beta.is_finite(), "{ctx}");
        assert!(
            p.alpha + p.beta <= MAX_SHAPE * (1.0 + 1e-9) + 2.0 * SHAPE_FLOOR,
            "{ctx}"
        );
        within(p.mean, p.alpha / (p.alpha + p.beta), 1e-15);
        assert!(0.0 <= p.interval.low, "{ctx}");
        assert!(p.interval.low <= p.mean, "{ctx}");
        assert!(p.mean <= p.interval.high, "{ctx}");
        assert!(p.interval.high <= 1.0, "{ctx}");
        // prob_above is a survival function: in [0, 1], non-increasing, 1 at 0- and 0 at 1.
        let t1 = rng.next_f64();
        let t2 = t1 + (1.0 - t1) * rng.next_f64();
        let (s1, s2) = (prob_above(&p, t1), prob_above(&p, t2));
        assert!(
            (0.0..=1.0).contains(&s1) && (0.0..=1.0).contains(&s2),
            "{ctx}"
        );
        // The incomplete beta near CDF = 0 or 1 carries ~1e-15 of rounding at extreme shapes
        // (1e-21 or 1e-18, reachable now that only exact zeros are floored).
        assert!(s2 <= s1 + 1e-12, "{ctx}: S({t1}) = {s1} < S({t2}) = {s2}");
        assert_eq!(prob_above(&p, 1.0), 0.0, "{ctx}");
        assert_eq!(prob_above(&p, -0.5), 1.0, "{ctx}");
    }
}

/// Regression guard, not a statistical property: with one seed every margin sees the same
/// draws, so the hit sets nest and the estimate is monotone by construction. This pins that
/// each call keeps one stream (current, then baseline) and a strict comparison.
#[test]
fn regression_guard_p_improvement_is_a_probability_and_monotone_in_margin() {
    let mut rng = Rng::new(99);
    let shapes = [SHAPE_FLOOR, 0.1, 0.5, 1.0, 5.0, 50.0, 1000.0, 1e5];
    for _ in 0..60 {
        let c = beta(pick(&mut rng, &shapes), pick(&mut rng, &shapes));
        let b = beta(pick(&mut rng, &shapes), pick(&mut rng, &shapes));
        let seed = rng.next_u32();
        let mut last = f64::INFINITY;
        for d in [0.0, 0.15, 0.5, 0.9] {
            let p = p_improvement(&c, &b, d, 2000, seed);
            assert!((0.0..=1.0).contains(&p), "{p}");
            assert!(p <= last, "delta {d}: {p} > {last}");
            last = p;
        }
        let mut last = f64::INFINITY;
        for f in [1.0, 1.15, 2.0, 10.0] {
            let p = p_regression(&c, &b, f, 2000, seed);
            assert!((0.0..=1.0).contains(&p), "{p}");
            assert!(p <= last, "factor {f}: {p} > {last}");
            last = p;
        }
    }
}

/// A point `estimate_weakness` must skip as invalid (written independently of the code).
fn is_invalid(&(e, o, epl, age): &(f64, f64, f64, f64)) -> bool {
    let bad = |x: f64| !x.is_finite() || x < 0.0;
    bad(e) || (e > 0.0 && e < 1.0) || bad(o) || bad(epl) || !age.is_finite() || e > o || o > 1e9
}

#[test]
fn property_estimate_is_contract_safe() {
    let mut rng = Rng::new(2026);
    let rates = [0.0, 1e-9, 0.0005, 0.02, 0.3, 0.999, 1.0];
    let ages = [-3.0, 0.0, 10.0, 60.0, 182.9, 183.0, 183.1, 400.0];
    let counts = [0.0, 1.0, 3.0, 40.0, 1000.0];
    let junk = [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -1.0,
        1e300,
        5e9,
        1e-300,
        0.5,
    ];
    let games = [0, 1, 29, 30, 31, 200, 5000];
    for _ in 0..3000 {
        let key = WeaknessKey::ALL[(rng.next_f64() * WeaknessKey::ALL.len() as f64) as usize];
        let band = Band::ALL[(rng.next_f64() * Band::ALL.len() as f64) as usize];
        let npts = (rng.next_f64() * 5.0) as usize;
        let pts: Vec<_> = (0..npts)
            .map(|_| {
                let opps = pick(&mut rng, &counts);
                let mut p = (
                    (opps * rng.next_f64()).floor(),
                    opps,
                    1.0,
                    pick(&mut rng, &ages),
                );
                // One point in four gets a junk field (or events above opportunities).
                if rng.next_f64() < 0.25 {
                    let j = pick(&mut rng, &junk);
                    match (rng.next_f64() * 5.0) as u32 {
                        0 => p.0 = j,
                        1 => p.1 = j,
                        2 => p.2 = j,
                        3 => p.3 = j,
                        _ => p.0 = p.1 + 1.0,
                    }
                }
                p
            })
            .collect();
        let want_invalid = pts.iter().filter(|p| is_invalid(p)).count() as u32;
        let pooled = pick(&mut rng, &rates);
        let median = pick(&mut rng, &rates);
        let p40 = median * rng.next_f64();
        let r = reference(key, pooled, median, p40);
        let has_ref = rng.next_f64() < 0.8;
        let g = games[(rng.next_f64() * games.len() as f64) as usize];
        let input = EstimateInput {
            key,
            points: &pts,
            games_in_window: g,
            band,
            reference: has_ref.then_some(&r),
            fatigue: rng.next_f64(),
        };
        for rule in [PriorRule::ALGO_1, PriorRule::CHAMPION] {
            let e = estimate_weakness_with(&input, rule);
            let ctx = format!("{key} {band:?} {pts:?} games {g} ref {has_ref} {r:?} -> {e:?}");
            assert_eq!(e.invalid_points, want_invalid, "{ctx}");
            let post = &e.posterior;
            let floats = [
                e.k,
                e.n,
                e.n_unweighted,
                e.k_unweighted,
                post.alpha,
                post.beta,
                post.mean,
                post.interval.low,
                post.interval.high,
                e.confidence,
                e.excess,
                e.trainability,
                e.priority,
            ];
            assert!(floats.iter().all(|x| x.is_finite()), "{ctx}");
            for x in [e.band_median, e.band_p40, e.percentile]
                .into_iter()
                .flatten()
            {
                assert!(x.is_finite(), "{ctx}");
            }
            assert!(e.k_unweighted <= e.n_unweighted && e.k <= e.n, "{ctx}");
            assert!(e.k >= 0.0 && e.n <= e.n_unweighted + 1e-9, "{ctx}");
            assert!(
                post.interval.low <= post.mean && post.mean <= post.interval.high,
                "{ctx}"
            );
            assert!((0.0..=1.0).contains(&e.confidence), "{ctx}");
            assert!(e.excess >= 0.0, "{ctx}");
            assert!(e.priority >= 0.0, "{ctx}");
            assert_eq!(e.trainability, trainability(key, band), "{ctx}");
            match e.reason {
                EstimateReason::InsufficientData => {
                    assert!(g < MIN_GAMES, "{ctx}");
                    assert_eq!(e.priority, 0.0, "{ctx}");
                }
                EstimateReason::NoBenchmark => {
                    assert!(!has_ref && g >= MIN_GAMES, "{ctx}");
                    assert_eq!(e.priority, 0.0, "{ctx}");
                    assert_eq!(e.percentile, None, "{ctx}");
                }
                EstimateReason::NoEvents => {
                    assert!(has_ref && g >= MIN_GAMES && e.k_unweighted == 0.0, "{ctx}");
                    assert_eq!(e.confidence, 0.0, "{ctx}");
                    assert_eq!(e.percentile, None, "{ctx}");
                    assert_eq!(e.priority, 0.0, "{ctx}");
                }
                EstimateReason::Ok => {
                    assert!(has_ref && g >= MIN_GAMES && e.k_unweighted > 0.0, "{ctx}");
                    let pct = e.percentile.expect("percentile");
                    assert!((1.0..=99.0).contains(&pct), "{ctx}");
                    assert_eq!(e.band_median, Some(median), "{ctx}");
                    // ADR 0101: only a credible estimate has a priority.
                    if !e.credible {
                        assert_eq!(e.priority, 0.0, "{ctx}");
                    }
                    if post.mean <= median {
                        assert_eq!(e.excess, 0.0, "{ctx}");
                    }
                }
            }
            // Prior strength is N0 of the key whenever there is a benchmark (algo-1.x), or N0 capped
            // at two of the player's games counted as the data are (algo-2); the floor only replaces
            // an exactly zero shape.
            if has_ref && e.reason != EstimateReason::NoBenchmark {
                let strength = match rule.prior_games {
                    Some(cap) if g > 0 => n0(key).min(cap * e.n / f64::from(g)),
                    _ => n0(key),
                };
                let raw = strength * pooled + e.k;
                let want_a = if raw == 0.0 { SHAPE_FLOOR } else { raw };
                within(post.alpha, want_a, 1e-9 * want_a.max(1.0));
            }
        }
    }
}
