//! R2 Task 12: weakness posteriors, P_imp and priority.
//!
//! The first block ports TS M2 Task 16's assertions (`toBeCloseTo(x, d)` becomes
//! `|got - want| < 0.5 * 10^-d`). The second block pins the R2 additions (ADR-R2-5): the shape
//! floor that keeps zero-pooled-rate priors finite and the posterior mean inside its 80%
//! interval, the reason precedence, fatigue, and seeded property tests over zero counts and
//! extreme priors. P_imp references come from an independent Simpson integration of
//! `P(X < c Y)` with exact integer-shape incomplete beta values (not from this implementation).

use profile_core::constants::{min_evidence, n0, trainability};
use profile_core::stats::posterior::{
    BetaPosterior, P_IMP_DELTA_REL, P_IMP_DRAWS, P_IMP_SEED, REGRESSION_FACTOR, SHAPE_FLOOR,
    p_improvement, p_regression, posterior, prob_above,
};
use profile_core::stats::priority::{
    EstimateInput, EstimateReason, MIN_GAMES, WINDOW_DAYS, WeaknessEstimate, estimate_weakness,
    rank_weaknesses,
};
use profile_core::stats::rng::Rng;
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
        opp_slope_per_100: 0.0,
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
        opp_slope_per_100: 0.0,
        n_players: 500,
        n_games: 20_000,
        low_sample: false,
    }
}

const W1_POINTS: [(f64, f64, f64, f64); 1] = [(10.0, 400.0, 2.5, 0.0)];

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
    let r = w1_ref();
    let e = estimate_weakness(&w1_input(&W1_POINTS, 40, Some(&r)));
    assert_eq!(e.key, WeaknessKey::W1);
    assert_eq!(e.posterior.alpha, 16.0);
    assert_eq!(e.posterior.beta, 684.0);
    close(e.posterior.mean, 0.0228571, 7);
    close(e.posterior.interval.low, 0.0159747, 7);
    close(e.posterior.interval.high, 0.0303238, 7);
    close(e.confidence, 0.672363, 6);
    close(e.excess, 0.0071429, 6);
    close(e.priority, 0.0048026, 7);
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
    let e = estimate_weakness(&w1_input(&W1_POINTS, 30, Some(&r)));
    assert_eq!(e.reason, EstimateReason::Ok);
    assert!(e.priority > 0.0);
}

#[test]
fn point_aged_200_days_is_ignored() {
    let r = w1_ref();
    let base = estimate_weakness(&w1_input(&W1_POINTS, 40, Some(&r)));
    let pts = [W1_POINTS[0], (90.0, 100.0, 9.0, 200.0)];
    let e = estimate_weakness(&w1_input(&pts, 40, Some(&r)));
    assert_eq!(e, base);
    // The window is 183 days inclusive; a point at 183 counts with weight 0.5^(183/60).
    assert_eq!(WINDOW_DAYS, 183.0);
    let pts = [W1_POINTS[0], (2.0, 100.0, 1.0, 183.0)];
    let e = estimate_weakness(&w1_input(&pts, 40, Some(&r)));
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

#[test]
fn zero_pooled_rate_prior_is_floored() {
    // Band where nobody showed the event: prior Beta(0, n0) would be a point mass at 0.
    let p = posterior(0.0, 0.0, 0.0, 300.0);
    assert_eq!(p.alpha, SHAPE_FLOOR);
    assert_eq!(p.beta, 300.0);
    assert!(p.mean > 0.0 && p.mean.is_finite());
    assert!(p.interval.low <= p.mean && p.mean <= p.interval.high);
    assert!(p.interval.high > p.interval.low);
    // Pooled rate 1 floors beta symmetrically.
    let p = posterior(5.0, 5.0, 1.0, 300.0);
    assert_eq!(p.alpha, 305.0);
    assert_eq!(p.beta, SHAPE_FLOOR);
    assert!(p.interval.low <= p.mean && p.mean <= p.interval.high);

    // estimate_weakness with such a band stays finite and contract-safe.
    let r = reference(WeaknessKey::W4Allowed, 0.0, 0.0, 0.0);
    let pts = [(0.0, 50.0, 0.0, 3.0)];
    let e = estimate_weakness(&EstimateInput {
        key: WeaknessKey::W4Allowed,
        points: &pts,
        games_in_window: 60,
        band: Band::B3,
        reference: Some(&r),
        fatigue: 0.0,
    });
    assert_eq!(e.reason, EstimateReason::Ok);
    for v in [e.posterior.mean, e.confidence, e.excess, e.priority] {
        assert!(v.is_finite() && v >= 0.0, "{e:?}");
    }
    assert!(e.posterior.interval.low <= e.posterior.mean);
    assert!(e.posterior.mean <= e.posterior.interval.high);
}

#[test]
fn review_case_tiny_alpha_keeps_mean_inside_interval() {
    // T11 review: equal-tailed Beta(0.02, 40) has q90 = 7.5e-5 < mean 5e-4.
    let p = posterior(0.0, 0.0, 0.0005, 40.0);
    assert_eq!(p.alpha, SHAPE_FLOOR);
    assert_eq!(p.beta, 40.0 * (1.0 - 0.0005));
    assert!(
        p.interval.low <= p.mean && p.mean <= p.interval.high,
        "{p:?}"
    );
}

#[test]
fn shape_floor_keeps_mean_inside_interval_on_a_grid() {
    // At the floor the worst case is the Gamma limit: P(Gamma(0.1) <= 0.1) = 0.83 < 0.9.
    let shapes = [
        SHAPE_FLOOR,
        0.2,
        0.5,
        1.0,
        3.0,
        30.0,
        300.0,
        3000.0,
        30_000.0,
    ];
    for &a in &shapes {
        for &b in &shapes {
            let p = beta(a, b);
            let c = 1.0 - prob_above(&p, p.mean);
            assert!(
                (0.1..=0.9).contains(&c),
                "CDF at mean {c} for Beta({a}, {b})"
            );
            assert!(p.interval.low <= p.mean && p.mean <= p.interval.high);
        }
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
    let base = estimate_weakness(&w1_input(&W1_POINTS, 40, Some(&r)));
    let at = |fatigue: f64| {
        estimate_weakness(&EstimateInput {
            fatigue,
            ..w1_input(&W1_POINTS, 40, Some(&r))
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
    let n0s = [0.0, 1e-9, 1.0, 40.0, 300.0, 600.0, 1e4, 1e6];
    let ns = [0.0, 1e-6, 0.3, 1.0, 7.0, 400.0, 2e4, 2e5];
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
        assert!(p.alpha >= SHAPE_FLOOR && p.beta >= SHAPE_FLOOR, "{ctx}");
        assert!(p.alpha.is_finite() && p.beta.is_finite(), "{ctx}");
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
        assert!(s2 <= s1, "{ctx}: S({t1}) = {s1} < S({t2}) = {s2}");
        assert_eq!(prob_above(&p, 1.0), 0.0, "{ctx}");
        assert_eq!(prob_above(&p, -0.5), 1.0, "{ctx}");
    }
}

#[test]
fn property_p_improvement_is_a_probability_and_monotone_in_margin() {
    let mut rng = Rng::new(99);
    let shapes = [SHAPE_FLOOR, 0.5, 1.0, 5.0, 50.0, 1000.0, 1e5];
    for _ in 0..60 {
        let c = beta(pick(&mut rng, &shapes), pick(&mut rng, &shapes));
        let b = beta(pick(&mut rng, &shapes), pick(&mut rng, &shapes));
        let seed = rng.next_u32();
        let mut last = f64::INFINITY;
        // Same seed, larger required improvement: never more likely.
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

#[test]
fn property_estimate_is_contract_safe() {
    let mut rng = Rng::new(2026);
    let rates = [0.0, 1e-9, 0.0005, 0.02, 0.3, 0.999, 1.0];
    let ages = [-3.0, 0.0, 10.0, 60.0, 182.9, 183.0, 183.1, 400.0, f64::NAN];
    let counts = [0.0, 1.0, 3.0, 40.0, 1000.0];
    let games = [0, 1, 29, 30, 31, 200, 5000];
    for _ in 0..2000 {
        let key = WeaknessKey::ALL[(rng.next_f64() * WeaknessKey::ALL.len() as f64) as usize];
        let band = Band::ALL[(rng.next_f64() * Band::ALL.len() as f64) as usize];
        let npts = (rng.next_f64() * 5.0) as usize;
        let pts: Vec<_> = (0..npts)
            .map(|_| {
                let opps = pick(&mut rng, &counts);
                (opps * rng.next_f64(), opps, 1.0, pick(&mut rng, &ages))
            })
            .collect();
        let pooled = pick(&mut rng, &rates);
        let median = pick(&mut rng, &rates);
        let p40 = median * rng.next_f64();
        let r = reference(key, pooled, median, p40);
        let has_ref = rng.next_f64() < 0.8;
        let g = games[(rng.next_f64() * games.len() as f64) as usize];
        let e = estimate_weakness(&EstimateInput {
            key,
            points: &pts,
            games_in_window: g,
            band,
            reference: has_ref.then_some(&r),
            fatigue: rng.next_f64(),
        });
        let ctx = format!("{key} {band:?} {pts:?} games {g} ref {has_ref} {r:?} -> {e:?}");
        assert!(
            e.k_unweighted <= e.n_unweighted && e.k <= e.n + 1e-9,
            "{ctx}"
        );
        assert!(e.k >= 0.0 && e.n <= e.n_unweighted + 1e-9, "{ctx}");
        let post = &e.posterior;
        assert!(
            post.interval.low <= post.mean && post.mean <= post.interval.high,
            "{ctx}"
        );
        assert!((0.0..=1.0).contains(&e.confidence), "{ctx}");
        assert!(e.excess >= 0.0 && e.excess.is_finite(), "{ctx}");
        assert!(e.priority >= 0.0 && e.priority.is_finite(), "{ctx}");
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
            EstimateReason::Ok => {
                assert!(has_ref && g >= MIN_GAMES, "{ctx}");
                let pct = e.percentile.expect("percentile");
                assert!((1.0..=99.0).contains(&pct), "{ctx}");
                assert_eq!(e.band_median, Some(median), "{ctx}");
                if post.mean <= median {
                    assert_eq!(e.excess, 0.0, "{ctx}");
                }
            }
        }
        // Prior strength is N0 of the key whenever there is a benchmark.
        if e.reason != EstimateReason::NoBenchmark && has_ref {
            let want_a = (n0(key) * pooled + e.k).max(SHAPE_FLOOR);
            within(post.alpha, want_a, 1e-9 * want_a.max(1.0));
        }
    }
}
