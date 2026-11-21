//! Beta distribution (R2 Task 11): a validated distribution type with quantiles, CDF and seeded
//! sampling, including the point-mass limits of degenerate fits and priors (ADR-R2-5).

use std::f64::consts::PI;

use crate::stats::error::StatsError;
use crate::stats::rng::Rng;
use crate::stats::special::{ln_beta, reg_inc_beta_with_ln_beta, valid_shapes};

/// Relative bisection tolerance of quantiles: the bracket on `ln x` is at most this wide, so the
/// result is within a relative `QUANTILE_TOL / 2` (and an absolute 5e-11) of the true quantile.
pub const QUANTILE_TOL: f64 = 1e-10;

/// A Beta(a, b) distribution with validated shapes, or a point mass on `[0, 1]` (the limit of
/// Beta(a, b) as one shape goes to 0: no events, no opportunities, or a zero pooled rate).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BetaDist {
    kind: Kind,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Beta { a: f64, b: f64, ln_b: f64 },
    PointMass(f64),
}

impl BetaDist {
    /// Beta(a, b). `StatsError::InvalidShape` unless `a > 0` and `b > 0` are finite.
    pub fn new(a: f64, b: f64) -> Result<Self, StatsError> {
        if !valid_shapes(a, b) {
            return Err(StatsError::InvalidShape { a, b });
        }
        Ok(Self {
            kind: Kind::Beta {
                a,
                b,
                ln_b: ln_beta(a, b),
            },
        })
    }

    /// The point mass at `at` (a rate in `[0, 1]`; `StatsError::InvalidProbability` otherwise).
    pub fn point_mass(at: f64) -> Result<Self, StatsError> {
        if !(0.0..=1.0).contains(&at) {
            return Err(StatsError::InvalidProbability { p: at });
        }
        Ok(Self {
            kind: Kind::PointMass(at),
        })
    }

    /// Beta(a, b) extended to its limits: `a = 0 < b` is the point mass at 0 and `a > 0 = b`
    /// the point mass at 1 (e.g. a `Beta(n0 * 0, n0)` prior with no events). Any other invalid
    /// pair, including `(0, 0)`, is `StatsError::InvalidShape`.
    pub fn from_shapes(a: f64, b: f64) -> Result<Self, StatsError> {
        if a == 0.0 && b > 0.0 && b.is_finite() {
            return Self::point_mass(0.0);
        }
        if b == 0.0 && a > 0.0 && a.is_finite() {
            return Self::point_mass(1.0);
        }
        Self::new(a, b)
    }

    /// `(a, b)` of a proper Beta; `None` for a point mass.
    pub fn shapes(&self) -> Option<(f64, f64)> {
        match self.kind {
            Kind::Beta { a, b, .. } => Some((a, b)),
            Kind::PointMass(_) => None,
        }
    }

    /// The location of a point mass; `None` for a proper Beta.
    pub fn point_mass_at(&self) -> Option<f64> {
        match self.kind {
            Kind::Beta { .. } => None,
            Kind::PointMass(at) => Some(at),
        }
    }

    /// The mean `a / (a + b)` (the location for a point mass).
    pub fn mean(&self) -> f64 {
        match self.kind {
            Kind::Beta { a, b, .. } => a / (a + b),
            Kind::PointMass(at) => at,
        }
    }

    /// The CDF `P(X <= x)` (`reg_inc_beta`; a step at the location for a point mass).
    /// NaN for a NaN `x`, like the special functions.
    pub fn cdf(&self, x: f64) -> f64 {
        match self.kind {
            _ if x.is_nan() => f64::NAN,
            Kind::Beta { a, b, ln_b } => reg_inc_beta_with_ln_beta(x, a, b, ln_b),
            Kind::PointMass(at) => {
                if x >= at {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }

    /// The `p` quantile. `StatsError::InvalidProbability` unless `0 <= p <= 1`.
    ///
    /// A point mass returns its location exactly. A proper Beta returns 0 for `p = 0`, 1 for
    /// `p = 1`, and otherwise bisects on `ln x` (so the lower tail is relative: Beta(0.03, 1)
    /// gives `q(0.1) = 4.64e-34`, not an absolute-tolerance artefact). A quantile below the
    /// smallest normal float (`CDF(f64::MIN_POSITIVE) >= p`) is returned as exactly 0.
    pub fn quantile(&self, p: f64) -> Result<f64, StatsError> {
        if !(0.0..=1.0).contains(&p) {
            return Err(StatsError::InvalidProbability { p });
        }
        Ok(match self.kind {
            Kind::PointMass(at) => at,
            Kind::Beta { .. } if p == 0.0 => 0.0,
            Kind::Beta { .. } if p == 1.0 => 1.0,
            Kind::Beta { .. } => {
                if self.cdf(f64::MIN_POSITIVE) >= p {
                    return Ok(0.0);
                }
                let (mut lo, mut hi) = (f64::MIN_POSITIVE.ln(), 0.0_f64);
                while hi - lo > QUANTILE_TOL {
                    let mid = 0.5 * (lo + hi);
                    if self.cdf(mid.exp()) < p {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                (0.5 * (lo + hi)).exp()
            }
        })
    }

    /// One seeded draw (Marsaglia-Tsang gamma ratio, see [`sample_beta`]). A point mass returns
    /// its location and consumes no draws.
    pub fn sample(&self, rng: &mut Rng) -> f64 {
        match self.kind {
            Kind::Beta { a, b, .. } => sample_valid(rng, a, b),
            Kind::PointMass(at) => at,
        }
    }
}

/// The `p` quantile of Beta(a, b) (TS `betaQuantile`): `BetaDist::new(a, b)?.quantile(p)`.
/// For repeated use, build the [`BetaDist`] once.
pub fn beta_quantile(p: f64, a: f64, b: f64) -> Result<f64, StatsError> {
    BetaDist::new(a, b)?.quantile(p)
}

/// One draw from Beta(a, b) (TS `sampleBeta`) as the gamma ratio `X / (X + Y)` with
/// `X ~ Gamma(a)`, `Y ~ Gamma(b)` (Marsaglia-Tsang). The ratio is formed in log space, so tiny
/// shapes neither underflow nor give 0/0. `StatsError::InvalidShape` unless `a > 0` and `b > 0`
/// are finite. Same stream as [`BetaDist::sample`].
pub fn sample_beta(rng: &mut Rng, a: f64, b: f64) -> Result<f64, StatsError> {
    if !valid_shapes(a, b) {
        return Err(StatsError::InvalidShape { a, b });
    }
    Ok(sample_valid(rng, a, b))
}

fn sample_valid(rng: &mut Rng, a: f64, b: f64) -> f64 {
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
