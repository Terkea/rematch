//! Shrunk posteriors and P_imp (R2 Task 12, TS M2 Task 16, ADR-R2-5).
//!
//! A weakness rate gets the conjugate Beta posterior of a band prior `{mu, n0}`:
//! `alpha = n0 mu + k`, `beta = n0 (1 - mu) + n - k`, with both shapes floored at
//! [`SHAPE_FLOOR`]. The floor keeps a zero pooled rate (prior `Beta(0, n0)`) a proper
//! distribution instead of a point mass that would claim certainty from no data, and it keeps
//! the posterior mean inside the equal-tailed 80% interval (below a shape of about 0.04 the
//! 90% quantile falls under the mean).

use serde::{Deserialize, Serialize};

use crate::stats::beta::BetaDist;
use crate::stats::intervals::{INTERVAL_LEVEL, Interval};
use crate::stats::rng::Rng;

/// Smallest posterior shape. At 0.1 the worst case (the Gamma limit, `b -> inf`) has
/// `P(X <= mean) = P(Gamma(0.1) <= 0.1) = 0.83 < 0.9`, so the mean is always inside the 80%
/// interval, and the floor adds at most a tenth of a pseudo-event.
pub const SHAPE_FLOOR: f64 = 0.1;
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
/// Shapes are floored at [`SHAPE_FLOOR`]. The interval is the 10% and 90% quantiles, widened
/// to the mean if the bisection tolerance (5e-11 relative) ever put it on the wrong side.
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
    let n = nonneg(n);
    let k = nonneg(k).min(n);
    let n0 = nonneg(n0);
    let mu = if mu.is_nan() { 0.5 } else { mu.clamp(0.0, 1.0) };

    let alpha = (n0 * mu + k).max(SHAPE_FLOOR);
    let beta = (n0 * (1.0 - mu) + (n - k)).max(SHAPE_FLOOR);
    let Ok(dist) = BetaDist::new(alpha, beta) else {
        // Only reachable if the shapes overflow to infinity (counts near f64::MAX).
        debug_assert!(false, "posterior shapes overflowed ({alpha}, {beta})");
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
