//! Special functions (R2 Task 11): `ln Gamma`, `ln B` and the regularised incomplete beta
//! function.
//!
//! Our own implementations of the textbook formulas (no statistics crate, ADR-R2-1), so the
//! TS M2 test values are pinned exactly. These keep the libm convention for invalid input
//! (NaN in, NaN out); the typed-error layer is `BetaDist` and friends (ADR-R2-5).

use std::f64::consts::PI;

/// Lanczos approximation with g = 7 and nine coefficients (Godfrey). Relative error of Gamma
/// about 1e-15 for x >= 1/2.
const LANCZOS_G: f64 = 7.0;
const LANCZOS: [f64; 9] = [
    0.999_999_999_999_809_9,
    676.520_368_121_885_1,
    -1_259.139_216_722_402_8,
    771.323_428_777_653_1,
    -176.615_029_162_140_6,
    12.507_343_278_686_905,
    -0.138_571_095_265_720_12,
    9.984_369_578_019_572e-6,
    1.505_632_735_149_311_6e-7,
];

/// `ln(2 pi) / 2`.
const HALF_LN_2PI: f64 = 0.918_938_533_204_672_8;
/// From this argument on, the Stirling series below is accurate to about 1e-16.
const STIRLING_MIN: f64 = 10.0;

/// Modified Lentz: floor for vanishing denominators, convergence threshold, term cap.
const LENTZ_TINY: f64 = 1e-300;
const LENTZ_EPS: f64 = 1e-15;
const LENTZ_MAX_TERMS: usize = 20_000;

/// `ln |Gamma(x)|`. `+inf` at the poles `x = 0, -1, -2, ...` and at `+-inf`; NaN for NaN.
/// Below 1/2 it uses the reflection `Gamma(x) Gamma(1 - x) = pi / sin(pi x)`.
pub fn lgamma(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x.is_infinite() || (x <= 0.0 && x == x.floor()) {
        return f64::INFINITY;
    }
    if x < 0.5 {
        return (PI / (PI * x).sin().abs()).ln() - lgamma(1.0 - x);
    }
    let z = x - 1.0;
    let mut series = LANCZOS[0];
    for (i, c) in LANCZOS.iter().enumerate().skip(1) {
        series += c / (z + i as f64);
    }
    let t = z + LANCZOS_G + 0.5;
    0.5 * (2.0 * PI).ln() + (z + 0.5) * t.ln() - t + series.ln()
}

/// Stirling remainder `ln Gamma(x) - [(x - 1/2) ln x - x + ln(2 pi) / 2]` for
/// `x >= STIRLING_MIN`: the Bernoulli series `1/(12x) - 1/(360x^3) + 1/(1260x^5) - ...`.
fn stirling_remainder(x: f64) -> f64 {
    let r = 1.0 / x;
    let r2 = r * r;
    r * (1.0 / 12.0
        - r2 * (1.0 / 360.0
            - r2 * (1.0 / 1260.0
                - r2 * (1.0 / 1680.0
                    - r2 * (1.0 / 1188.0 - r2 * (691.0 / 360_360.0 - r2 / 156.0))))))
}

/// `ln B(a, b) = ln Gamma(a) + ln Gamma(b) - ln Gamma(a + b)` for `a, b > 0`.
///
/// For large shapes the three `ln Gamma` values are huge and nearly cancel (`lgamma(1e5)` has
/// an ulp of 1e-10), so large arguments use Stirling differences written with `ln_1p`, which
/// keep the result accurate to a few ulps of itself.
pub fn ln_beta(a: f64, b: f64) -> f64 {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let s = lo + hi;
    if lo >= STIRLING_MIN {
        // (lo - 1/2) ln(lo/s) + (hi - 1/2) ln(hi/s) - ln(s)/2 + ln(2 pi)/2 + remainders.
        HALF_LN_2PI - 0.5 * s.ln() - (lo - 0.5) * (hi / lo).ln_1p() - (hi - 0.5) * (lo / hi).ln_1p()
            + stirling_remainder(lo)
            + stirling_remainder(hi)
            - stirling_remainder(s)
    } else if hi >= STIRLING_MIN {
        // ln Gamma(lo) + [ln Gamma(hi) - ln Gamma(hi + lo)].
        lgamma(lo) - (hi - 0.5) * (lo / hi).ln_1p() - lo * s.ln() + lo + stirling_remainder(hi)
            - stirling_remainder(s)
    } else {
        lgamma(lo) + lgamma(hi) - lgamma(s)
    }
}

/// Both shape parameters finite and positive.
pub(crate) fn valid_shapes(a: f64, b: f64) -> bool {
    a > 0.0 && b > 0.0 && a.is_finite() && b.is_finite()
}

/// Regularised incomplete beta function `I_x(a, b) = B(x; a, b) / B(a, b)`, the Beta(a, b) CDF.
///
/// 0 for `x <= 0` and 1 for `x >= 1`; NaN for a NaN `x` or unless `a > 0` and `b > 0` are finite.
/// Evaluates the continued fraction of DLMF 8.17.22 with the modified Lentz method on the side
/// of `x = (a + 1) / (a + b + 2)` where it converges quickly, and uses
/// `I_x(a, b) = 1 - I_{1-x}(b, a)` on the other side.
pub fn reg_inc_beta(x: f64, a: f64, b: f64) -> f64 {
    if x.is_nan() || !valid_shapes(a, b) {
        return f64::NAN;
    }
    reg_inc_beta_with_ln_beta(x, a, b, ln_beta(a, b))
}

/// [`reg_inc_beta`] for valid shapes with `ln B(a, b)` supplied, so repeated evaluations (the
/// quantile bisection) compute it once.
pub(crate) fn reg_inc_beta_with_ln_beta(x: f64, a: f64, b: f64, ln_b: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    // ln x and ln(1 - x) are taken from x itself on both branches, for accuracy near 0 and 1.
    let (ln_x, ln_1mx) = (x.ln(), (-x).ln_1p());
    let i = if x < (a + 1.0) / (a + b + 2.0) {
        cf_branch(x, ln_x, ln_1mx, a, b, ln_b)
    } else {
        1.0 - cf_branch(1.0 - x, ln_1mx, ln_x, b, a, ln_b)
    };
    i.clamp(0.0, 1.0)
}

/// `x^a (1 - x)^b / (a B(a, b)) / K(x)`, where `K` is the continued fraction
/// `1 + d1 / (1 + d2 / (1 + ...))`. `ln_x` and `ln_1mx` are `ln x` and `ln(1 - x)`; `ln_b` is
/// `ln B(a, b)` (symmetric in its arguments).
fn cf_branch(x: f64, ln_x: f64, ln_1mx: f64, a: f64, b: f64, ln_b: f64) -> f64 {
    let ln_front = a * ln_x + b * ln_1mx - ln_b - a.ln();
    ln_front.exp() / continued_fraction(x, a, b)
}

/// Modified Lentz evaluation of `K = 1 + d1 / (1 + d2 / (1 + ...))` with
/// `d_{2m+1} = -(a + m)(a + b + m) x / ((a + 2m)(a + 2m + 1))` and
/// `d_{2m} = m (b - m) x / ((a + 2m - 1)(a + 2m))`.
///
/// It needs about `sqrt(max(a, b))` terms near the switch point, far below the cap for any
/// realistic shapes (1e6 opportunities: ~1000 terms). Hitting the cap is signalled by a
/// `debug_assert!` (tests and debug builds); release builds return the partial value.
fn continued_fraction(x: f64, a: f64, b: f64) -> f64 {
    let mut f = 1.0;
    let mut c = 1.0;
    let mut d = 0.0;
    let mut converged = false;
    for j in 1..=LENTZ_MAX_TERMS {
        let m = (j / 2) as f64;
        let dj = if j % 2 == 1 {
            -(a + m) * (a + b + m) * x / ((a + 2.0 * m) * (a + 2.0 * m + 1.0))
        } else {
            m * (b - m) * x / ((a + 2.0 * m - 1.0) * (a + 2.0 * m))
        };
        d = 1.0 + dj * d;
        if d.abs() < LENTZ_TINY {
            d = LENTZ_TINY;
        }
        d = 1.0 / d;
        c = 1.0 + dj / c;
        if c.abs() < LENTZ_TINY {
            c = LENTZ_TINY;
        }
        let delta = c * d;
        f *= delta;
        if (delta - 1.0).abs() < LENTZ_EPS {
            converged = true;
            break;
        }
    }
    debug_assert!(
        converged,
        "incomplete beta continued fraction did not converge in {LENTZ_MAX_TERMS} terms \
         (x = {x}, a = {a}, b = {b})"
    );
    f
}
