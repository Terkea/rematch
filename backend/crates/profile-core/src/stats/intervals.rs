//! Wilson and bootstrap intervals (R2 Task 11). Every interval in the profile engine is 80%.

use serde::{Deserialize, Serialize};

use crate::stats::quantiles::quantile;
use crate::stats::rng::Rng;

/// `Phi^-1(0.9)`: the normal quantile of a two-sided 80% interval.
pub const Z80: f64 = 1.2815515655446004;
/// Default interval level (TS `level: 0.8`).
pub const INTERVAL_LEVEL: f64 = 0.8;
/// Default number of bootstrap resamples (TS `resamples: 1000`).
pub const BOOTSTRAP_RESAMPLES: usize = 1000;
/// Default bootstrap seed (TS `seed: 1`).
pub const BOOTSTRAP_SEED: u32 = 1;

/// A closed interval `[low, high]`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Interval {
    pub low: f64,
    pub high: f64,
}

/// Wilson score interval for `k` events in `n` trials at normal quantile `z` (use [`Z80`]).
///
/// Counts may be fractional (recency-weighted). `n <= 0` gives `{0, 1}` (no information); `k` is
/// clamped to `[0, n]`. The bounds are exactly 0 for `k = 0` and exactly 1 for `k = n`.
pub fn wilson(k: f64, n: f64, z: f64) -> Interval {
    if n.is_nan() || n <= 0.0 {
        return Interval {
            low: 0.0,
            high: 1.0,
        };
    }
    let k = k.clamp(0.0, n);
    let p = k / n;
    let z2 = z * z;
    let denom = 1.0 + z2 / n;
    let center = (p + z2 / (2.0 * n)) / denom;
    let half = z * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt() / denom;
    Interval {
        low: if k <= 0.0 {
            0.0
        } else {
            (center - half).max(0.0)
        },
        high: if k >= n {
            1.0
        } else {
            (center + half).min(1.0)
        },
    }
}

/// Percentile-bootstrap interval of the mean (TS `bootstrapMeanInterval`; defaults
/// [`BOOTSTRAP_RESAMPLES`], [`BOOTSTRAP_SEED`], [`INTERVAL_LEVEL`]).
///
/// Each resample draws `n` indices `floor(u * n)` from `Rng::new(seed)`; the bounds are the
/// type-7 quantiles `(1 - level) / 2` and `(1 + level) / 2` of the sorted resample means.
/// Deterministic for a given seed. `None` with fewer than two values or zero resamples.
pub fn bootstrap_mean_interval(
    values: &[f64],
    resamples: usize,
    seed: u32,
    level: f64,
) -> Option<Interval> {
    let n = values.len();
    if n < 2 || resamples == 0 {
        return None;
    }
    let nf = n as f64;
    let mut rng = Rng::new(seed);
    let mut means: Vec<f64> = (0..resamples)
        .map(|_| {
            let mut sum = 0.0;
            for _ in 0..n {
                let i = ((rng.next_f64() * nf) as usize).min(n - 1);
                sum += values[i];
            }
            sum / nf
        })
        .collect();
    means.sort_by(f64::total_cmp);
    Some(Interval {
        low: quantile(&means, (1.0 - level) / 2.0),
        high: quantile(&means, (1.0 + level) / 2.0),
    })
}
