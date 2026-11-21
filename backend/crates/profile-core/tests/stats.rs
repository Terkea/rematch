//! R2 Task 11: seeded statistics numerics.
//!
//! The first block ports TS M2 Task 15's assertions verbatim (`toBeCloseTo(x, d)` becomes
//! `|got - want| < 0.5 * 10^-d`, `toEqual`/`toBe` become exact comparisons). The second block
//! checks the numerics against independent references with tight tolerances:
//! - mulberry32 goldens produced by the canonical JavaScript algorithm under Node 22;
//! - exact incomplete-beta values for integer parameters from the binomial identity
//!   `I_x(a, b) = P(Bin(a + b - 1, x) >= a)`, summed in 60-digit decimal arithmetic, and Beta
//!   quantiles from bisection on that exact CDF;
//! - closed forms (`I_x(1, b)`, `I_x(a, 1)`, `I_x(1/2, 1/2) = 2/pi asin(sqrt x)`, symmetry);
//! - `ln Gamma` from CPython's `math.lgamma` and the factorial identity;
//! - the Beta-binomial grid argmax from an independent log-sum likelihood;
//! - sampling moments and the sampler's agreement with `beta_quantile`.
//!
//! Fix round 1 (review): typed errors instead of NaN at the distribution layer, explicit point
//! masses for degenerate fits, relative lower-tail quantiles, and artefact-free percentiles
//! (ties within a relative 1e-9 use the mid-rank; no saturation on flat end segments).

use std::f64::consts::PI;

use profile_core::stats::beta::{BetaDist, beta_quantile, sample_beta};
use profile_core::stats::beta_binomial::{BetaBinomialFit, fit_beta_binomial};
use profile_core::stats::correlation::spearman;
use profile_core::stats::error::StatsError;
use profile_core::stats::intervals::{
    BOOTSTRAP_RESAMPLES, BOOTSTRAP_SEED, INTERVAL_LEVEL, Interval, Z80, bootstrap_mean_interval,
    wilson,
};
use profile_core::stats::quantiles::{QuartileRef, percentile_from_quartiles, quantile};
use profile_core::stats::rng::Rng;
use profile_core::stats::special::{lgamma, ln_beta, reg_inc_beta};
use profile_core::stats::weights::{HALF_LIFE_DAYS, recency_weight, weighted_counts};

/// Vitest `toBeCloseTo(want, digits)`: `|got - want| < 10^-digits / 2`.
#[track_caller]
fn assert_close_dp(got: f64, want: f64, digits: i32) {
    let tol = 0.5 * 10f64.powi(-digits);
    assert!(
        (got - want).abs() < tol,
        "got {got}, want {want} to {digits} dp"
    );
}

#[track_caller]
fn assert_close(got: f64, want: f64, tol: f64) {
    assert!(
        (got - want).abs() <= tol,
        "got {got}, want {want} (tol {tol}, diff {})",
        (got - want).abs()
    );
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

fn variance(v: &[f64]) -> f64 {
    let m = mean(v);
    v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / v.len() as f64
}

fn draws(seed: u32, n: usize, a: f64, b: f64) -> Vec<f64> {
    let mut rng = Rng::new(seed);
    (0..n)
        .map(|_| sample_beta(&mut rng, a, b).unwrap())
        .collect()
}

#[track_caller]
fn assert_rel(got: f64, want: f64, rel: f64) {
    assert!(
        ((got - want) / want).abs() <= rel,
        "got {got:e}, want {want:e} (relative tol {rel:e})"
    );
}

fn q(p: f64, a: f64, b: f64) -> f64 {
    beta_quantile(p, a, b).unwrap()
}

fn w(k: f64, n: f64, z: f64) -> Interval {
    wilson(k, n, z).unwrap()
}

fn qr(p25: f64, p40: f64, median: f64, p75: f64) -> QuartileRef {
    QuartileRef {
        p25,
        p40,
        median,
        p75,
    }
}

fn boot(values: &[f64]) -> Option<Interval> {
    bootstrap_mean_interval(values, BOOTSTRAP_RESAMPLES, BOOTSTRAP_SEED, INTERVAL_LEVEL)
}

// ---------------------------------------------------------------------------------------------
// TS M2 Task 15, verbatim.
// ---------------------------------------------------------------------------------------------

#[test]
fn ts_beta_quantile_and_reg_inc_beta() {
    assert_close_dp(q(0.1, 16.0, 684.0), 0.0159747, 6);
    assert_close_dp(q(0.9, 16.0, 684.0), 0.0303238, 6);
    assert_close_dp(1.0 - reg_inc_beta(0.02, 16.0, 684.0), 0.672363, 5);
}

#[test]
fn ts_wilson() {
    let i = w(7.0, 20.0, Z80);
    assert_close_dp(i.low, 0.229497, 5);
    assert_close_dp(i.high, 0.493269, 5);
    let i = w(0.0, 10.0, Z80);
    assert_eq!(i.low, 0.0);
    assert_close_dp(i.high, 0.141069, 5);
    assert_eq!(
        wilson(0.0, 0.0, Z80),
        Ok(Interval {
            low: 0.0,
            high: 1.0
        })
    );
    assert_close_dp(w(2.0, 10.0, Z80).high, 0.398404, 5);
}

#[test]
fn ts_sample_beta_mean() {
    let m = mean(&draws(42, 100_000, 2.0, 5.0));
    assert!((m - 2.0 / 7.0).abs() < 0.005, "mean {m}");
}

#[test]
fn ts_same_seed_same_draws() {
    let take = |seed| {
        let mut r = Rng::new(seed);
        [(); 5].map(|_| r.next_f64())
    };
    assert_eq!(take(1), take(1));
}

#[test]
fn ts_quantile_type_7() {
    assert_eq!(quantile(&[1.0, 2.0, 3.0, 4.0], 0.5), 2.5);
    assert_eq!(quantile(&[1.0, 2.0, 3.0, 4.0], 0.25), 1.75);
}

#[test]
fn ts_percentile_from_quartiles_lower_is_better() {
    let r = QuartileRef {
        p25: 0.01,
        p40: 0.015,
        median: 0.02,
        p75: 0.03,
    };
    let got = [0.02, 0.015, 0.03, 0.0, 0.04].map(|v| percentile_from_quartiles(v, &r, false));
    assert_eq!(got, [50.0, 60.0, 25.0, 99.0, 1.0]);
}

#[test]
fn ts_fit_beta_binomial() {
    assert_eq!(
        fit_beta_binomial(&[(5.0, 100.0); 20]),
        Ok(BetaBinomialFit::Beta { mu: 0.05, phi: 1e5 })
    );
    let mut mixed = vec![(2.0, 100.0); 10];
    mixed.extend([(8.0, 100.0); 10]);
    let Ok(BetaBinomialFit::Beta { mu, phi }) = fit_beta_binomial(&mixed) else {
        panic!("mixed data must give a proper Beta fit");
    };
    assert_close_dp(mu, 0.05, 10);
    assert!(phi < 200.0, "phi {phi}");
}

#[test]
fn ts_spearman() {
    assert_eq!(spearman(&[1.0, 2.0, 3.0], &[3.0, 2.0, 1.0]), Some(-1.0));
    assert_close_dp(
        spearman(&[1.0, 1.0, 2.0], &[1.0, 2.0, 3.0]).unwrap(),
        0.866025,
        5,
    );
    assert_eq!(spearman(&[1.0, 1.0, 1.0], &[1.0, 2.0, 3.0]), None);
}

#[test]
fn ts_weighted_counts() {
    let pts = [(1.0, 10.0, 0.0), (2.0, 10.0, 60.0)];
    assert_eq!(weighted_counts(&pts, HALF_LIFE_DAYS), (2.0, 15.0));
}

#[test]
fn ts_bootstrap_needs_two_values() {
    assert_eq!(boot(&[5.0]), None);
}

// ---------------------------------------------------------------------------------------------
// Reference values.
// ---------------------------------------------------------------------------------------------

#[test]
fn rng_is_bit_identical_to_js_mulberry32() {
    // u32 outputs of the canonical JS mulberry32 (`a = a + 0x6D2B79F5 | 0; ...`) under Node 22.
    let goldens: [(u32, [u32; 5]); 4] = [
        (
            1,
            [2693262067, 11749833, 2265367787, 4213581821, 4159151403],
        ),
        (
            42,
            [2581720956, 1925393290, 3661312704, 2876485805, 750819978],
        ),
        (0, [1144304738, 1416247, 958946056, 627933444, 2007157716]),
        (
            u32::MAX,
            [3850105811, 813802916, 3073704848, 4054706436, 3630262831],
        ),
    ];
    for (seed, want) in goldens {
        let mut r = Rng::new(seed);
        assert_eq!([(); 5].map(|_| r.next_u32()), want, "seed {seed}");
        let mut r = Rng::new(seed);
        let f = [(); 5].map(|_| r.next_f64());
        assert_eq!(
            f,
            want.map(|u| f64::from(u) / 4_294_967_296.0),
            "seed {seed}"
        );
    }
    // JS: createRng(1) -> 0.6270739405881613, 0.002735721180215478, 0.5274470399599522.
    let mut r = Rng::new(1);
    assert_eq!(r.next_f64(), 0.6270739405881613);
    assert_eq!(r.next_f64(), 0.002735721180215478);
    assert_eq!(r.next_f64(), 0.5274470399599522);
    // The state wraps modulo 2^32 exactly as JS `| 0`: the 1 000 000th draw of seed 7.
    let mut r = Rng::new(7);
    let last = (0..1_000_000).map(|_| r.next_f64()).last().unwrap();
    assert_eq!(last, 0.056065975688397884);
    // Every draw is in [0, 1).
    let mut r = Rng::new(3);
    assert!(
        (0..10_000)
            .map(|_| r.next_f64())
            .all(|u| (0.0..1.0).contains(&u))
    );
}

#[test]
fn lgamma_matches_reference_values() {
    // (x, CPython math.lgamma(x))
    let refs = [
        (0.5, 0.5723649429247004),
        (1.0, 0.0),
        (1.5, -0.12078223763524543),
        (2.0, 0.0),
        (0.1, 2.2527126517342055),
        (1e-3, 6.907178885383854),
        (3.7, 1.4280723266653883),
        (10.0, 12.801827480081467),
        (100.5, 361.4355404677776),
        (1e5, 1051287.7089736566),
        (2.5e6, 34329496.777633995),
    ];
    for (x, want) in refs {
        assert_close(lgamma(x), want, 1e-12 * want.abs().max(1.0));
    }
    // ln Gamma(1/2) = ln sqrt(pi); ln Gamma(n) = ln (n - 1)!.
    assert_close(lgamma(0.5), PI.sqrt().ln(), 1e-14);
    let mut fact = 1.0_f64;
    for n in 1..=20 {
        assert_close(lgamma(f64::from(n)), fact.ln(), 1e-13 * fact.ln().max(1.0));
        fact *= f64::from(n);
    }
    // Recurrence ln Gamma(x + 1) = ln Gamma(x) + ln x across the reflection boundary.
    for x in [0.05, 0.3, 0.49, 0.51, 2.2, 37.25] {
        assert_close(lgamma(x + 1.0), lgamma(x) + f64::ln(x), 1e-13);
    }
    assert_eq!(lgamma(0.0), f64::INFINITY);
    assert_eq!(lgamma(-3.0), f64::INFINITY);
    assert!(lgamma(f64::NAN).is_nan());
}

#[test]
fn ln_beta_matches_exact_factorial_values() {
    // ln B(a, b) = ln((a-1)! (b-1)! / (a+b-1)!) from exact big integers in 60-digit decimals. The
    // naive lgamma(a) + lgamma(b) - lgamma(a + b) is off by 1e-10 to 3.5e-10 at the two largest
    // pairs (huge, nearly cancelling terms); since ln B is exponentiated, the absolute error is
    // what matters.
    for (a, b, want) in [
        (400.0, 400.0, -556.247652098107),
        (3000.0, 7000.0, -6111.548896978379),
        (2000.0, 98000.0, -9806.782697632432),
        (50000.0, 50000.0, -69318.86243051325),
    ] {
        assert_close(ln_beta(a, b), want, 2e-11);
    }
    // Mixed branch (one shape below 10): ln B(1, b) = -ln b.
    assert_close(ln_beta(1.0, 1e5), -(1e5_f64.ln()), 1e-13);
    assert_close(ln_beta(1e5, 2.0), -(1e5_f64 * 100001.0).ln(), 1e-12);
    // Small branch: the plain lgamma sum.
    assert_close(
        ln_beta(2.5, 3.5),
        lgamma(2.5) + lgamma(3.5) - lgamma(6.0),
        1e-15,
    );
}

#[test]
fn reg_inc_beta_matches_exact_and_closed_forms() {
    // Exact values for integer parameters (60-digit binomial sums at the binary value of x).
    let exact = [
        (0.3, 2.0, 5.0, 0.5798249999999999),
        (0.05, 3.0, 40.0, 0.35101360934622805),
        (0.999, 30.0, 2.0, 0.9995438962809783),
        (0.02, 16.0, 684.0, 0.32763676563872884),
    ];
    for (x, a, b, want) in exact {
        assert_close(reg_inc_beta(x, a, b), want, 1e-12);
    }
    // Closed forms with non-integer parameters.
    assert_close(reg_inc_beta(0.3, 1.0, 4.5), 1.0 - 0.7_f64.powf(4.5), 1e-14);
    assert_close(reg_inc_beta(0.6, 2.5, 1.0), 0.6_f64.powf(2.5), 1e-14);
    for x in [0.01_f64, 0.3, 0.5, 0.9, 0.999] {
        let want = 2.0 / PI * x.sqrt().asin();
        assert_close(reg_inc_beta(x, 0.5, 0.5), want, 1e-13);
    }
    assert_close(reg_inc_beta(0.5, 3.5, 3.5), 0.5, 1e-13);
    assert_close(reg_inc_beta(0.5, 400.0, 400.0), 0.5, 1e-12);
    // Large shapes: ln B(a, b) comes from Stirling differences, not lgamma(1e5) - lgamma(1e5 + 1).
    assert_rel(
        reg_inc_beta(1e-5, 1.0, 1e5),
        -(1e5 * (-1e-5_f64).ln_1p()).exp_m1(),
        1e-12,
    );
    assert_rel(
        reg_inc_beta(1e-7, 1.0, 1e5),
        -(1e5 * (-1e-7_f64).ln_1p()).exp_m1(),
        1e-12,
    );
    assert_rel(
        reg_inc_beta(0.9999, 5e4, 1.0),
        (5e4 * 0.9999_f64.ln()).exp(),
        1e-12,
    );
    assert_close(reg_inc_beta(0.5, 5e4, 5e4), 0.5, 1e-10);
    // Shapes below 1, deep lower tail: I_x(a, 1) = x^a and I_x(1/2, 1/2) = 2/pi asin(sqrt x).
    for x in [1e-30_f64, 1e-300] {
        assert_rel(reg_inc_beta(x, 0.03, 1.0), x.powf(0.03), 1e-13);
    }
    assert_rel(
        reg_inc_beta(1e-20, 0.5, 0.5),
        2.0 / PI * 1e-10_f64.asin(),
        1e-12,
    );
    // Symmetry I_x(a, b) = 1 - I_{1-x}(b, a) on both sides of the switch point.
    for x in [0.05, 0.2, 0.25, 0.6] {
        let s = reg_inc_beta(x, 2.3, 7.1) + reg_inc_beta(1.0 - x, 7.1, 2.3);
        assert_close(s, 1.0, 1e-13);
    }
    // Edges and invalid parameters.
    assert_eq!(reg_inc_beta(0.0, 2.0, 3.0), 0.0);
    assert_eq!(reg_inc_beta(-0.1, 2.0, 3.0), 0.0);
    assert_eq!(reg_inc_beta(1.0, 2.0, 3.0), 1.0);
    assert_eq!(reg_inc_beta(1.5, 2.0, 3.0), 1.0);
    assert!(reg_inc_beta(0.5, 0.0, 1.0).is_nan());
    assert!(reg_inc_beta(0.5, 1.0, -1.0).is_nan());
    assert!(reg_inc_beta(f64::NAN, 1.0, 1.0).is_nan());
}

#[test]
fn beta_quantile_matches_exact_and_inverts() {
    // Bisection on the exact CDF (60 digits); bisection to 1e-10 leaves at most 5e-11.
    let exact = [
        (0.1, 16.0, 684.0, 0.015974726111944268),
        (0.9, 16.0, 684.0, 0.030323835244047373),
        (0.5, 2.0, 5.0, 0.26444998329565994),
        (0.1, 12.0, 988.0, 0.007849671077955946),
        (0.9, 12.0, 988.0, 0.016568158382463737),
    ];
    for (p, a, b, want) in exact {
        assert_close(q(p, a, b), want, 1e-10);
        assert_rel(q(p, a, b), want, 1e-9);
    }
    // Closed-form inverses.
    assert_close(q(0.37, 1.0, 3.5), 1.0 - 0.63_f64.powf(1.0 / 3.5), 1e-10);
    assert_close(q(0.8, 2.5, 1.0), 0.8_f64.powf(0.4), 1e-10);
    assert_close(q(0.15, 0.5, 0.5), (PI * 0.15 / 2.0).sin().powi(2), 1e-10);
    // The lower tail is relative, not absolute: Beta(0.03, 1) has q(p) = p^(1 / 0.03).
    assert_rel(q(0.1, 0.03, 1.0), 4.641588833612762e-34, 1e-9);
    assert_rel(q(1e-8, 0.5, 0.5), (PI * 1e-8 / 2.0).sin().powi(2), 1e-9);
    assert_rel(reg_inc_beta(q(1e-6, 0.2, 3.0), 0.2, 3.0), 1e-6, 1e-8);
    // Below the smallest normal float the quantile is exactly 0 (0.1^(1/0.001) underflows).
    assert_eq!(q(0.1, 0.001, 1.0), 0.0);
    // Reference-sized posteriors Beta(mu 1e5, (1 - mu) 1e5): exact values (60-digit CDF).
    for (p, want) in [
        (0.1, 0.019434729157651435),
        (0.5, 0.019996800076383155),
        (0.9, 0.020569382057844622),
    ] {
        assert_rel(q(p, 2000.0, 98000.0), want, 1e-9);
    }
    assert_close(q(0.5, 5e4, 5e4), 0.5, 1e-10);
    assert_close(q(0.1, 5e4, 5e4) + q(0.9, 5e4, 5e4), 1.0, 1e-10);
    // Round trip through the CDF.
    for (p, a, b) in [(0.1, 16.0, 684.0), (0.5, 0.7, 3.2), (0.95, 40.0, 9.0)] {
        assert_close(reg_inc_beta(q(p, a, b), a, b), p, 1e-8);
    }
    assert_eq!(q(0.0, 2.0, 3.0), 0.0);
    assert_eq!(q(1.0, 2.0, 3.0), 1.0);
    // Typed errors instead of NaN.
    assert_eq!(
        beta_quantile(0.5, -1.0, 3.0),
        Err(StatsError::InvalidShape { a: -1.0, b: 3.0 })
    );
    assert_eq!(
        beta_quantile(1.5, 2.0, 3.0),
        Err(StatsError::InvalidProbability { p: 1.5 })
    );
    assert!(matches!(
        beta_quantile(f64::NAN, 2.0, 3.0),
        Err(StatsError::InvalidProbability { .. })
    ));
}

#[test]
fn sample_beta_moments_and_cdf_agree_with_beta_quantile() {
    let n = 100_000;
    // Beta(2, 5): mean 2/7, variance ab / ((a+b)^2 (a+b+1)) = 10/392. Tolerances are >= 4 SE.
    let v = draws(42, n, 2.0, 5.0);
    assert!(v.iter().all(|x| (0.0..=1.0).contains(x)));
    assert_close(variance(&v), 10.0 / 392.0, 6e-4);
    // Shapes below 1 take the boost path: Beta(1/2, 1/2) has mean 1/2 and variance 1/8.
    let v = draws(7, n, 0.5, 0.5);
    assert_close(mean(&v), 0.5, 0.005);
    assert_close(variance(&v), 0.125, 0.0015);
    // A posterior-sized Beta(16, 684): mean 16/700, sd 0.0056.
    assert_close(mean(&draws(9, n, 16.0, 684.0)), 16.0 / 700.0, 1e-4);
    // The empirical CDF at beta_quantile(p) is p (binomial SE <= 0.0016).
    for (a, b) in [(2.0, 5.0), (0.3, 0.7), (16.0, 684.0)] {
        let v = draws(11, n, a, b);
        for p in [0.1, 0.5, 0.9] {
            let qp = q(p, a, b);
            let below = v.iter().filter(|&&x| x < qp).count() as f64 / n as f64;
            assert_close(below, p, 0.008);
        }
    }
    // Deterministic per seed; different seeds give different streams.
    assert_eq!(draws(5, 20, 3.0, 4.0), draws(5, 20, 3.0, 4.0));
    assert_ne!(draws(5, 20, 3.0, 4.0), draws(6, 20, 3.0, 4.0));
    // Tiny shapes stay finite (log-space ratio).
    let v = draws(13, 1000, 0.01, 0.01);
    assert!(v.iter().all(|x| (0.0..=1.0).contains(x)));
    assert_eq!(
        sample_beta(&mut Rng::new(1), 0.0, 1.0),
        Err(StatsError::InvalidShape { a: 0.0, b: 1.0 })
    );
    // BetaDist::sample is the same stream as sample_beta.
    let d = BetaDist::new(3.0, 4.0).unwrap();
    let mut rng = Rng::new(5);
    let v: Vec<f64> = (0..20).map(|_| d.sample(&mut rng)).collect();
    assert_eq!(v, draws(5, 20, 3.0, 4.0));
}

#[test]
fn wilson_reference_values_and_edges() {
    assert_eq!(Z80, 1.2815515655446004);
    let i = w(7.0, 20.0, 1.96);
    assert_close(i.low, 0.18118954786794464, 1e-12);
    assert_close(i.high, 0.5671494897805354, 1e-12);
    // Fractional (recency-weighted) counts.
    let i = w(3.5, 12.25, Z80);
    assert_close(i.low, 0.1536671110047927, 1e-12);
    assert_close(i.high, 0.4684277272775095, 1e-12);
    // k = n gives high exactly 1; mirror symmetry.
    assert_eq!(w(10.0, 10.0, Z80).high, 1.0);
    let (a, b) = (w(7.0, 20.0, Z80), w(13.0, 20.0, Z80));
    assert_close(a.low, 1.0 - b.high, 1e-14);
    // Invalid counts or z are typed errors, never {0, 1}.
    assert!(matches!(
        wilson(f64::NAN, 10.0, Z80),
        Err(StatsError::InvalidCounts { .. })
    ));
    assert!(matches!(
        wilson(1.0, f64::NAN, Z80),
        Err(StatsError::InvalidCounts { .. })
    ));
    assert_eq!(
        wilson(-1.0, 10.0, Z80),
        Err(StatsError::InvalidCounts { k: -1.0, n: 10.0 })
    );
    assert_eq!(
        wilson(1.0, -2.0, Z80),
        Err(StatsError::InvalidCounts { k: 1.0, n: -2.0 })
    );
    assert_eq!(
        wilson(1.0, 10.0, -1.0),
        Err(StatsError::InvalidZ { z: -1.0 })
    );
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "k <= n")]
fn wilson_k_above_n_is_a_bug_caught_in_debug() {
    let _ = wilson(12.0, 10.0, Z80);
}

#[test]
fn bootstrap_mean_interval_reference_values() {
    let i = |low, high| Some(Interval { low, high });
    assert_eq!(boot(&[]), None);
    assert_eq!(boot(&[3.0, 3.0, 3.0]), i(3.0, 3.0));
    // Means of two draws from {0, 1}: P(0) = P(1) = 1/4, so the 10% and 90% points are 0 and 1.
    assert_eq!(boot(&[0.0, 1.0]), i(0.0, 1.0));
    // 1..=100: the bootstrap SE is sigma_pop / sqrt(n) = 2.8866, so the 80% interval is about
    // 50.5 -+ 1.2816 * 2.8866 = [46.80, 54.20] (Monte Carlo error with 1000 resamples ~0.2).
    let v: Vec<f64> = (1..=100).map(f64::from).collect();
    let got = boot(&v).unwrap();
    assert_close(got.low, 46.80, 0.4);
    assert_close(got.high, 54.20, 0.4);
    // Golden values from an independent port of the same algorithm (mulberry32 seed 1, index
    // floor(u * n), percentile interval, type-7 quantiles).
    assert_close(got.low, 46.638, 1e-9);
    assert_close(got.high, 54.131, 1e-9);
    let got = boot(&[0.0, 2.0, 1.0]).unwrap();
    assert_close(got.low, 1.0 / 3.0, 1e-12);
    assert_close(got.high, 5.0 / 3.0, 1e-12);
    let acc = [71.5, 80.2, 64.9, 90.1, 77.7, 58.3, 84.0, 69.6];
    let got = boot(&acc).unwrap();
    assert_close(got.low, 69.90375, 1e-9);
    assert_close(got.high, 79.08875, 1e-9);
    assert!(got.low <= mean(&acc) && mean(&acc) <= got.high);
    // Deterministic per seed; the seed matters.
    assert_eq!(boot(&acc), boot(&acc));
    assert_ne!(
        bootstrap_mean_interval(&acc, 1000, 2, 0.8),
        bootstrap_mean_interval(&acc, 1000, 1, 0.8)
    );
    assert_eq!(bootstrap_mean_interval(&acc, 0, 1, 0.8), None);
}

#[test]
fn quantile_type_7_reference_values() {
    let x = [1.0, 3.0, 7.0, 15.0];
    assert_eq!(quantile(&x, 0.0), 1.0);
    assert_eq!(quantile(&x, 1.0), 15.0);
    // R: quantile(c(1, 3, 7, 15), 0.9, type = 7) = 12.6
    assert_close(quantile(&x, 0.9), 12.6, 1e-12);
    assert_eq!(quantile(&[4.0], 0.3), 4.0);
    assert_eq!(quantile(&x, -0.5), 1.0);
    assert_eq!(quantile(&x, 1.5), 15.0);
    assert!(quantile(&[], 0.5).is_nan());
}

#[test]
fn percentile_from_quartiles_reference_values() {
    let r = QuartileRef {
        p25: 10.0,
        p40: 16.0,
        median: 20.0,
        p75: 30.0,
    };
    let hi = |v| percentile_from_quartiles(v, &r, true);
    assert_eq!(hi(20.0), 50.0);
    assert_eq!(hi(30.0), 75.0);
    assert_close(hi(18.0), 45.0, 1e-12); // halfway p40 -> median
    assert_close(hi(13.0), 32.5, 1e-12); // halfway p25 -> p40
    assert_close(hi(4.0), 10.0, 1e-12); // extends the p25-p40 slope (2.5 per unit)
    assert_close(hi(34.0), 85.0, 1e-12); // extends the median-p75 slope (2.5 per unit)
    assert_eq!(hi(40.0), 99.0);
    assert_eq!(hi(0.0), 1.0);
    assert_close(percentile_from_quartiles(18.0, &r, false), 55.0, 1e-12);
    // Degenerate references (rare events: p25 = p40 = 0). Tied knots merge into one point at
    // their mid-rank (25 and 40 -> 32.5), and the polyline runs through the merged points:
    // (0, 32.5), (0.001, 50), (0.003, 75).
    let rare = qr(0.0, 0.0, 0.001, 0.003);
    assert_eq!(percentile_from_quartiles(0.0, &rare, false), 67.5);
    assert_close(percentile_from_quartiles(0.0005, &rare, false), 58.75, 1e-9);
    // Below the merged point the nearest non-flat slope (17.5 per 0.001) continues: 15 raw.
    assert_close(percentile_from_quartiles(-0.001, &rare, false), 85.0, 1e-9);
    // Flat end segments use the nearest non-flat slope, never +-inf.
    let flat_top = qr(0.01, 0.015, 0.02, 0.02); // (0.01, 25), (0.015, 40), (0.02, 62.5)
    assert_close(
        percentile_from_quartiles(0.021, &flat_top, true),
        67.0,
        1e-9,
    );
    let flat_bottom = qr(0.01, 0.01, 0.015, 0.02); // (0.01, 32.5), (0.015, 50), (0.02, 75)
    assert_close(
        percentile_from_quartiles(0.009, &flat_bottom, true),
        29.0,
        1e-9,
    );
    // All four knots equal: the mid-rank 50 on the point, the knot bounds 25 / 75 beyond it.
    let flat = qr(0.02, 0.02, 0.02, 0.02);
    assert_eq!(percentile_from_quartiles(0.02, &flat, true), 50.0);
    assert_eq!(percentile_from_quartiles(0.03, &flat, true), 75.0);
    assert_eq!(percentile_from_quartiles(0.01, &flat, true), 25.0);
    // A reference with no scale at all (every quartile exactly 0) steps at 0.
    let zeros = qr(0.0, 0.0, 0.0, 0.0);
    assert_eq!(percentile_from_quartiles(0.0, &zeros, false), 50.0);
    assert_eq!(percentile_from_quartiles(0.001, &zeros, false), 25.0);
}

#[test]
fn percentile_of_a_zero_rate_has_one_answer_however_the_quartiles_were_produced() {
    // A zero rate against a band where most players have zero events. The same band's quartiles
    // can come out exactly 0, with float noise, or unsorted; each must give ONE answer: the
    // mid-rank of {p25, p40, median} = 37.5, so 62.5 for a lower-is-better rate.
    let refs = [
        qr(0.0, 0.0, 0.0, 0.002),
        qr(0.0, 0.0, 1e-15, 0.002),
        qr(1e-15, 0.0, 0.0, 0.002),
        qr(-1e-16, 1e-16, 0.0, 0.002),
        qr(0.0, 2e-18, 1e-17, 0.002),
    ];
    for r in &refs {
        for v in [0.0, 1e-13, -1e-13] {
            assert_eq!(percentile_from_quartiles(v, r, false), 62.5, "{r:?} at {v}");
        }
    }
}

#[test]
fn percentile_is_continuous_monotone_and_bounded() {
    let refs = [
        qr(0.01, 0.015, 0.02, 0.03),
        qr(0.015, 0.02, 0.02, 0.03),
        qr(0.02, 0.02, 0.02, 0.02),
        qr(0.0, 0.0, 0.001, 0.003),
        qr(0.0, 0.0, 0.0, 0.002),
        qr(0.01, 0.015, 0.02, 0.02),
        qr(0.01, 0.01, 0.015, 0.02),
    ];
    for r in &refs {
        for hib in [true, false] {
            let f = |v: f64| percentile_from_quartiles(v, r, hib);
            // No jump under a 1e-12 perturbation at the value the review used.
            for dv in [1e-12, -1e-12] {
                assert!(
                    (f(0.02 + dv) - f(0.02)).abs() <= 1e-6,
                    "{r:?} at 0.02{dv:+e}"
                );
            }
            // Monotone (in the metric's direction), bounded, and free of 1e-12 jumps on a grid.
            let mut prev = if hib {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            };
            for i in 0..=1200 {
                let v = -0.03 + f64::from(i) * 1e-4;
                let y = f(v);
                assert!((1.0..=99.0).contains(&y), "{r:?} at {v}: {y}");
                assert!(if hib { y >= prev } else { y <= prev }, "{r:?} at {v}");
                assert!((f(v + 1e-12) - y).abs() <= 1e-3, "{r:?} jumps at {v}");
                prev = y;
            }
        }
    }
}

#[test]
fn fit_beta_binomial_matches_independent_grid_argmax() {
    // Independent log-sum likelihood (ln Gamma(a + k) - ln Gamma(a) = sum ln(a + j)) over the
    // 60-point grid 10^(5 i / 59): the argmax is i = 20 (gap to the runner-up 0.011 nats).
    let data = [
        (0.0, 40.0),
        (1.0, 60.0),
        (3.0, 50.0),
        (7.0, 80.0),
        (2.0, 30.0),
        (12.0, 90.0),
        (0.0, 20.0),
        (5.0, 70.0),
    ];
    let Ok(BetaBinomialFit::Beta { mu, phi }) = fit_beta_binomial(&data) else {
        panic!("proper fit expected");
    };
    assert_close(mu, 30.0 / 440.0, 1e-15);
    assert_close(phi, 49.5353520895917, 1e-9);
    // Mixed case of the TS test: argmax i = 23 (phi = 88.95, gap 0.034 nats).
    let mut mixed = vec![(2.0, 100.0); 10];
    mixed.extend([(8.0, 100.0); 10]);
    let fit = fit_beta_binomial(&mixed).unwrap();
    let Some(phi) = fit.phi() else {
        panic!("proper fit expected");
    };
    assert_close(phi, 88.95134973108232, 1e-9);
    // The fitted rate distribution is Beta(mu phi, (1 - mu) phi).
    let want = BetaDist::new(0.05 * phi, (1.0 - 0.05) * phi).unwrap();
    assert_eq!(fit.rates().unwrap().quantile(0.9), want.quantile(0.9));
    // Invalid observations are typed errors.
    for (k, n) in [(-1.0, 10.0), (11.0, 10.0), (1.0, 0.0), (1.0, f64::INFINITY)] {
        assert_eq!(
            fit_beta_binomial(&[(1.0, 10.0), (k, n)]),
            Err(StatsError::InvalidCounts { k, n })
        );
    }
    assert!(matches!(
        fit_beta_binomial(&[(f64::NAN, 10.0)]),
        Err(StatsError::InvalidCounts { .. })
    ));
}

#[test]
fn degenerate_fits_are_point_masses_whose_quantiles_are_exact() {
    let zero = Ok(BetaBinomialFit::PointMass { at: 0.0 });
    // Zero events in every observation.
    let no_events = fit_beta_binomial(&[(0.0, 50.0); 5]);
    assert_eq!(no_events, zero);
    // Zero opportunities: no observations at all.
    let no_data = fit_beta_binomial(&[]);
    assert_eq!(no_data, zero);
    for fit in [no_events, no_data] {
        let fit = fit.unwrap();
        assert_eq!(fit.mu(), 0.0);
        assert_eq!(fit.phi(), None);
        let rates = fit.rates().unwrap();
        for p in [0.0, 0.1, 0.5, 0.9, 1.0] {
            assert_eq!(rates.quantile(p), Ok(0.0));
        }
        assert_eq!(rates.mean(), 0.0);
        assert_eq!(rates.cdf(0.0), 1.0);
        assert_eq!(rates.cdf(-1e-9), 0.0);
        assert_eq!(rates.sample(&mut Rng::new(1)), 0.0);
    }
    // Every opportunity an event: a point mass at 1.
    let all = fit_beta_binomial(&[(10.0, 10.0); 3]).unwrap();
    assert_eq!(all, BetaBinomialFit::PointMass { at: 1.0 });
    assert_eq!(all.rates().unwrap().quantile(0.1), Ok(1.0));
}

#[test]
fn beta_dist_validates_and_handles_its_limits() {
    assert_eq!(
        BetaDist::new(0.0, 1.0),
        Err(StatsError::InvalidShape { a: 0.0, b: 1.0 })
    );
    assert_eq!(
        BetaDist::new(f64::INFINITY, 1.0),
        Err(StatsError::InvalidShape {
            a: f64::INFINITY,
            b: 1.0
        })
    );
    assert!(matches!(
        BetaDist::new(1.0, f64::NAN),
        Err(StatsError::InvalidShape { .. })
    ));
    // The limits of Beta(a, b) as one shape goes to 0 are point masses (Beta(0, n0) priors).
    let at0 = BetaDist::from_shapes(0.0, 300.0).unwrap();
    assert_eq!(at0.point_mass_at(), Some(0.0));
    assert_eq!(at0.quantile(0.9), Ok(0.0));
    let at1 = BetaDist::from_shapes(5.0, 0.0).unwrap();
    assert_eq!(at1.quantile(0.1), Ok(1.0));
    assert_eq!(
        BetaDist::from_shapes(0.0, 0.0),
        Err(StatsError::InvalidShape { a: 0.0, b: 0.0 })
    );
    assert_eq!(
        BetaDist::point_mass(1.5),
        Err(StatsError::InvalidProbability { p: 1.5 })
    );
    let d = BetaDist::from_shapes(16.0, 684.0).unwrap();
    assert_eq!(d.shapes(), Some((16.0, 684.0)));
    assert_close(d.mean(), 16.0 / 700.0, 1e-15);
    assert_eq!(d.quantile(0.1), beta_quantile(0.1, 16.0, 684.0));
    assert_eq!(d.cdf(0.02), reg_inc_beta(0.02, 16.0, 684.0));
    assert!(matches!(
        d.quantile(f64::NAN),
        Err(StatsError::InvalidProbability { .. })
    ));
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "did not converge")]
fn continued_fraction_cap_is_signalled_in_debug() {
    // About sqrt(1e12) terms would be needed at the switch point; the cap is 20 000.
    let _ = reg_inc_beta(0.5, 1e12, 1e12);
}

#[test]
fn spearman_reference_values() {
    // Average ranks with ties, then Pearson on the ranks (computed independently).
    let x = [10.0, 20.0, 20.0, 40.0, 50.0, 50.0, 50.0];
    let y = [1.0, 3.0, 2.0, 5.0, 4.0, 7.0, 6.0];
    assert_close(spearman(&x, &y).unwrap(), 0.8794653965640157, 1e-12);
    // Rank-based: any increasing transform gives 1.
    let x = [0.1, 0.5, 2.0, 9.0];
    let y = x.map(|v: f64| v.exp());
    assert_close(spearman(&x, &y).unwrap(), 1.0, 1e-15);
    assert_eq!(spearman(&[1.0, 2.0], &[1.0]), None);
    assert_eq!(spearman(&[1.0], &[1.0]), None);
    assert_eq!(spearman(&[1.0, 2.0, 3.0], &[2.0, 2.0, 2.0]), None);
}

#[test]
fn weighted_counts_reference_values() {
    assert_eq!(recency_weight(0.0, 60.0), 1.0);
    assert_eq!(recency_weight(60.0, 60.0), 0.5);
    assert_eq!(recency_weight(120.0, 60.0), 0.25);
    assert_eq!(recency_weight(60.0, 30.0), 0.25);
    assert_close(
        recency_weight(30.0, 60.0),
        std::f64::consts::FRAC_1_SQRT_2,
        1e-15,
    );
    // A game dated in the future (clock skew) counts as today, never more than full weight.
    assert_eq!(recency_weight(-5.0, 60.0), 1.0);
    assert_eq!(weighted_counts(&[], 60.0), (0.0, 0.0));
    let pts = [(4.0, 40.0, 120.0), (1.0, 8.0, 0.0)];
    assert_eq!(weighted_counts(&pts, 60.0), (2.0, 18.0));
    // A NaN age is excluded (weight 0), not counted as today.
    assert_eq!(recency_weight(f64::NAN, 60.0), 0.0);
    let pts = [
        (1.0, 10.0, f64::NAN),
        (1.0, 10.0, 0.0),
        (f64::NAN, f64::NAN, f64::NAN),
    ];
    assert_eq!(weighted_counts(&pts, 60.0), (1.0, 10.0));
}
