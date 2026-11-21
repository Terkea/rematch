//! Shrunk posteriors and P_imp (R2 Task 12, TS M2 Task 16, ADR-R2-5).
//!
//! A weakness rate gets the conjugate Beta posterior of a band prior `{mu, n0}`:
//! `alpha = n0 mu + k`, `beta = n0 (1 - mu) + n - k`. A shape that is exactly 0 (a zero pooled
//! rate and no events) is replaced by [`SHAPE_FLOOR`], so the posterior stays a proper
//! distribution instead of a point mass; positive shapes are never changed. Because tiny shapes
//! can put the mean outside the equal-tailed 80% interval, the interval is widened to the mean.

use serde::{Deserialize, Serialize};

use crate::stats::beta::BetaDist;
use crate::stats::intervals::{INTERVAL_LEVEL, Interval};
use crate::stats::rng::Rng;

/// Replacement for a posterior shape that is exactly 0 (coordinator ruling, ADR-R2-5 section 7).
/// It is a thousandth of a pseudo-event, so it barely moves the mean; the lightest real
/// in-window event weighs `0.5^(183/60) = 0.121`, so the floor never applies once there is one.
pub const SHAPE_FLOOR: f64 = 1e-3;
/// Largest `alpha + beta`. Larger counts are scaled down together (the mean is kept, the
/// interval becomes wider, i.e. conservative), so `posterior` stays total and the incomplete
/// beta continued fraction stays far below its term cap. Real series stay below 1e6.
pub const MAX_SHAPE: f64 = 1e7;
/// Relative improvement P_imp asks for: `theta_c < theta_b (1 - 0.15)`.
pub const P_IMP_DELTA_REL: f64 = 0.15;
/// Regression factor: `theta_c > theta_b * 1.15`.
pub const REGRESSION_FACTOR: f64 = 1.15;
/// Monte Carlo draws of P_imp and P_reg.
pub const P_IMP_DRAWS: usize = 10_000;
/// Default seed of P_imp and P_reg.
pub const P_IMP_SEED: u32 = 1;

/// A Beta posterior of a rate with its 80% equal-tailed interval (TS `BetaPosterior`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BetaPosterior {
    pub alpha: f64,
    pub beta: f64,
    pub mean: f64,
    pub interval: Interval,
}

impl BetaPosterior {
    /// The distribution, or `None` if the shapes were edited into invalid values.
    fn dist(&self) -> Option<BetaDist> {
        BetaDist::from_shapes(self.alpha, self.beta).ok()
    }
}

/// The posterior of `k` events in `n` opportunities under the prior `Beta(n0 mu, n0 (1 - mu))`
/// (TS `posterior(k, n, { mu, n0 })`). `k` and `n` may be recency-weighted (fractional).
///
/// A shape that is exactly 0 becomes [`SHAPE_FLOOR`]; `n0 + n` above [`MAX_SHAPE`] is scaled
/// down to it, keeping the mean. The interval is the 10% and 90% quantiles, widened to include
/// the mean (equal-tailed intervals of shapes below about 0.04 exclude it: Beta(0.02, 40) has
/// q90 = 7.5e-5 < mean 5e-4).
///
/// Valid input is finite with `0 <= k <= n`, `0 <= mu <= 1` and `n0 >= 0`; anything else fails
/// a `debug_assert!`. Release builds clamp it: NaN or negative counts and `n0` become 0, `k` is
/// clamped to `[0, n]` and `mu` to `[0, 1]` (NaN `mu` becomes 1/2), so the result is always a
/// finite, proper posterior.
pub fn posterior(k: f64, n: f64, mu: f64, n0: f64) -> BetaPosterior {
    debug_assert!(
        n.is_finite() && n >= 0.0 && k.is_finite() && (0.0..=n).contains(&k),
        "posterior counts must satisfy 0 <= k <= n (k = {k}, n = {n})"
    );
    debug_assert!(
        (0.0..=1.0).contains(&mu),
        "prior mean must be in [0, 1] ({mu})"
    );
    debug_assert!(
        n0.is_finite() && n0 >= 0.0,
        "prior strength must be >= 0 ({n0})"
    );
    let nonneg = |x: f64| if x.is_finite() && x > 0.0 { x } else { 0.0 };
    let mut n = nonneg(n);
    let mut k = nonneg(k).min(n);
    let mut n0 = nonneg(n0);
    let mu = if mu.is_nan() { 0.5 } else { mu.clamp(0.0, 1.0) };

    // Scale `n0 + n` down to MAX_SHAPE without forming a sum that could overflow.
    let big = n0.max(n);
    if big > 0.0 {
        let rel_total = n0 / big + n / big;
        if rel_total > MAX_SHAPE / big {
            let f = (MAX_SHAPE / big) / rel_total;
            n0 *= f;
            n *= f;
            k = (k * f).min(n);
        }
    }

    let floor_zero = |x: f64| if x > 0.0 { x } else { SHAPE_FLOOR };
    let alpha = floor_zero(n0 * mu + k);
    let beta = floor_zero(n0 * (1.0 - mu) + (n - k));
    let Ok(dist) = BetaDist::new(alpha, beta) else {
        // Unreachable: both shapes are in (0, MAX_SHAPE].
        return BetaPosterior {
            alpha: 1.0,
            beta: 1.0,
            mean: 0.5,
            interval: Interval {
                low: 0.1,
                high: 0.9,
            },
        };
    };
    let mean = dist.mean();
    let tail = (1.0 - INTERVAL_LEVEL) / 2.0;
    let low = dist.quantile(tail).unwrap_or(mean).min(mean);
    let high = dist.quantile(1.0 - tail).unwrap_or(mean).max(mean);
    BetaPosterior {
        alpha,
        beta,
        mean,
        interval: Interval { low, high },
    }
}

/// `P(theta > threshold)` under the posterior (TS `probAbove`), in `[0, 1]`. A NaN threshold or
/// invalid shapes (only possible by editing the public fields) give 0: no confidence.
pub fn prob_above(p: &BetaPosterior, threshold: f64) -> f64 {
    match p.dist() {
        Some(d) if !threshold.is_nan() => (1.0 - d.cdf(threshold)).clamp(0.0, 1.0),
        _ => 0.0,
    }
}

/// `P(theta_c < theta_b (1 - delta_rel))` by `draws` seeded Monte Carlo pairs (TS
/// `pImprovement`; defaults [`P_IMP_DELTA_REL`], [`P_IMP_DRAWS`], [`P_IMP_SEED`]).
/// Each pair draws `current` first, then `baseline`, from one mulberry32 stream. Zero draws or
/// invalid shapes give 0 (no evidence of improvement).
pub fn p_improvement(
    current: &BetaPosterior,
    baseline: &BetaPosterior,
    delta_rel: f64,
    draws: usize,
    seed: u32,
) -> f64 {
    let factor = 1.0 - delta_rel;
    monte_carlo(current, baseline, draws, seed, |c, b| c < b * factor)
}

/// `P(theta_c > theta_b factor)` by `draws` seeded Monte Carlo pairs (TS `pRegression`;
/// defaults [`REGRESSION_FACTOR`], [`P_IMP_DRAWS`], [`P_IMP_SEED`]). Same stream and edge
/// conventions as [`p_improvement`].
pub fn p_regression(
    current: &BetaPosterior,
    baseline: &BetaPosterior,
    factor: f64,
    draws: usize,
    seed: u32,
) -> f64 {
    monte_carlo(current, baseline, draws, seed, |c, b| c > b * factor)
}

fn monte_carlo(
    current: &BetaPosterior,
    baseline: &BetaPosterior,
    draws: usize,
    seed: u32,
    hit: impl Fn(f64, f64) -> bool,
) -> f64 {
    let (Some(c), Some(b)) = (current.dist(), baseline.dist()) else {
        return 0.0;
    };
    if draws == 0 {
        return 0.0;
    }
    let mut rng = Rng::new(seed);
    let hits = (0..draws)
        .filter(|_| {
            let xc = c.sample(&mut rng);
            let xb = b.sample(&mut rng);
            hit(xc, xb)
        })
        .count();
    hits as f64 / draws as f64
}
