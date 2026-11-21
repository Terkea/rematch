//! Special functions (R2 Task 11): `ln Gamma` and the regularised incomplete beta function.
//!
//! Our own implementations of the textbook formulas (no statistics crate, ADR-R2-1), so the
//! TS M2 test values are pinned exactly.

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

/// `ln B(a, b) = ln Gamma(a) + ln Gamma(b) - ln Gamma(a + b)`.
pub(crate) fn ln_beta(a: f64, b: f64) -> f64 {
    lgamma(a) + lgamma(b) - lgamma(a + b)
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
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    // ln x and ln(1 - x) are taken from x itself on both branches, for accuracy near 0 and 1.
    let (ln_x, ln_1mx) = (x.ln(), (-x).ln_1p());
    let i = if x < (a + 1.0) / (a + b + 2.0) {
        cf_branch(x, ln_x, ln_1mx, a, b)
    } else {
        1.0 - cf_branch(1.0 - x, ln_1mx, ln_x, b, a)
    };
    i.clamp(0.0, 1.0)
}

/// `x^a (1 - x)^b / (a B(a, b)) / K(x)`, where `K` is the continued fraction
/// `1 + d1 / (1 + d2 / (1 + ...))`. `ln_x` and `ln_1mx` are `ln x` and `ln(1 - x)`.
fn cf_branch(x: f64, ln_x: f64, ln_1mx: f64, a: f64, b: f64) -> f64 {
    let ln_front = a * ln_x + b * ln_1mx - ln_beta(a, b) - a.ln();
    ln_front.exp() / continued_fraction(x, a, b)
}

/// Modified Lentz evaluation of `K = 1 + d1 / (1 + d2 / (1 + ...))` with
/// `d_{2m+1} = -(a + m)(a + b + m) x / ((a + 2m)(a + 2m + 1))` and
/// `d_{2m} = m (b - m) x / ((a + 2m - 1)(a + 2m))`.
fn continued_fraction(x: f64, a: f64, b: f64) -> f64 {
    let mut f = 1.0;
    let mut c = 1.0;
    let mut d = 0.0;
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
            break;
        }
    }
    f
}
