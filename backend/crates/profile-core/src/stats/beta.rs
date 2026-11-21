//! Beta distribution (R2 Task 11): quantiles by bisection and seeded sampling.

use std::f64::consts::PI;

use crate::stats::rng::Rng;
use crate::stats::special::{reg_inc_beta, valid_shapes};

/// Bisection stops once the bracket is at most this wide (so the error is at most half of it).
pub const QUANTILE_TOL: f64 = 1e-10;

/// The `p` quantile of Beta(a, b): bisection on [`reg_inc_beta`] over `[0, 1]` until the bracket
/// is at most [`QUANTILE_TOL`] wide. 0 for `p <= 0`, 1 for `p >= 1`; NaN for a NaN `p` or
/// invalid shapes.
pub fn beta_quantile(p: f64, a: f64, b: f64) -> f64 {
    if p.is_nan() || !valid_shapes(a, b) {
        return f64::NAN;
    }
    if p <= 0.0 {
        return 0.0;
    }
    if p >= 1.0 {
        return 1.0;
    }
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    while hi - lo > QUANTILE_TOL {
        let mid = 0.5 * (lo + hi);
        if reg_inc_beta(mid, a, b) < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// One draw from Beta(a, b) as the gamma ratio `X / (X + Y)` with `X ~ Gamma(a)`,
/// `Y ~ Gamma(b)` (Marsaglia-Tsang). The ratio is formed in log space, so tiny shapes neither
/// underflow nor give 0/0. NaN unless `a > 0` and `b > 0` are finite.
pub fn sample_beta(rng: &mut Rng, a: f64, b: f64) -> f64 {
    if !valid_shapes(a, b) {
        return f64::NAN;
    }
    let ln_x = ln_gamma_variate(rng, a);
    let ln_y = ln_gamma_variate(rng, b);
    1.0 / (1.0 + (ln_y - ln_x).exp())
}

/// `ln` of one Gamma(shape, 1) draw by Marsaglia and Tsang (2000). A shape below 1 uses
/// `G(shape) = G(shape + 1) * U^(1 / shape)`.
fn ln_gamma_variate(rng: &mut Rng, shape: f64) -> f64 {
    if shape < 1.0 {
        let u = open_unit(rng);
        return ln_gamma_variate(rng, shape + 1.0) + u.ln() / shape;
    }
    let d = shape - 1.0 / 3.0;
    let c = 1.0 / (9.0 * d).sqrt();
    loop {
        let x = standard_normal(rng);
        let v = 1.0 + c * x;
        if v <= 0.0 {
            continue;
        }
        let v = v * v * v;
        let u = open_unit(rng);
        let x2 = x * x;
        if u < 1.0 - 0.0331 * x2 * x2 || u.ln() < 0.5 * x2 + d * (1.0 - v + v.ln()) {
            return d.ln() + v.ln();
        }
    }
}

/// Uniform in `(0, 1]`: never 0, so its logarithm is finite.
fn open_unit(rng: &mut Rng) -> f64 {
    1.0 - rng.next_f64()
}

/// Standard normal by Box-Muller (cosine branch only, so each call uses exactly two draws).
fn standard_normal(rng: &mut Rng) -> f64 {
    let u1 = open_unit(rng);
    let u2 = rng.next_f64();
    (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos()
}
