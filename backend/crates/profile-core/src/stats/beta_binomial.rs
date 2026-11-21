//! Beta-binomial helpers (R2 Task 11): pooled rate and over-dispersion of per-player counts.

use crate::stats::beta::BetaDist;
use crate::stats::error::StatsError;
use crate::stats::special::ln_beta;

/// Number of grid points for `phi`.
pub const PHI_GRID_POINTS: usize = 60;
/// Smallest `phi` on the grid.
pub const PHI_MIN: f64 = 1.0;
/// Largest `phi` on the grid ("no detectable over-dispersion").
pub const PHI_MAX: f64 = 1e5;

/// A Beta-binomial fit of per-player rates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BetaBinomialFit {
    /// Rates ~ Beta(mu phi, (1 - mu) phi) with `0 < mu < 1` and `phi` on the grid.
    Beta { mu: f64, phi: f64 },
    /// Every player has the same rate `at`: 0 when no observation has an event (including no
    /// observations at all: zero opportunities), 1 when every opportunity was an event. Its
    /// quantiles are exactly `at`.
    PointMass { at: f64 },
}

impl BetaBinomialFit {
    /// The pooled rate `sum(k) / sum(n)` (0 without observations).
    pub fn mu(&self) -> f64 {
        match *self {
            Self::Beta { mu, .. } => mu,
            Self::PointMass { at } => at,
        }
    }

    /// The precision `phi = alpha + beta`; `None` for a point mass.
    pub fn phi(&self) -> Option<f64> {
        match *self {
            Self::Beta { phi, .. } => Some(phi),
            Self::PointMass { .. } => None,
        }
    }

    /// The fitted distribution of per-player rates: Beta(mu phi, (1 - mu) phi), or the point
    /// mass. Fails only for a hand-built value with invalid fields.
    pub fn rates(&self) -> Result<BetaDist, StatsError> {
        match *self {
            Self::Beta { mu, phi } => BetaDist::new(mu * phi, (1.0 - mu) * phi),
            Self::PointMass { at } => BetaDist::point_mass(at),
        }
    }
}

/// Fits a Beta-binomial to `(events, opportunities)` pairs (TS `fitBetaBinomial`).
///
/// Every pair must have a finite `n > 0` and `0 <= k <= n` (`StatsError::InvalidCounts`
/// otherwise; callers drop players without opportunities first). `mu = sum(k) / sum(n)`. With no
/// events, or no pairs at all, the fit is `PointMass { at: 0 }`; with every opportunity an event
/// it is `PointMass { at: 1 }`. Otherwise `phi` maximises the log-likelihood over
/// [`PHI_GRID_POINTS`] log-spaced values from [`PHI_MIN`] to [`PHI_MAX`] (the first maximum wins).
pub fn fit_beta_binomial(data: &[(f64, f64)]) -> Result<BetaBinomialFit, StatsError> {
    for &(k, n) in data {
        let valid = n.is_finite() && n > 0.0 && k >= 0.0 && k <= n;
        if !valid {
            return Err(StatsError::InvalidCounts { k, n });
        }
    }
    let (sum_k, sum_n) = data
        .iter()
        .fold((0.0, 0.0), |(k, n), &(dk, dn)| (k + dk, n + dn));
    if sum_k <= 0.0 {
        return Ok(BetaBinomialFit::PointMass { at: 0.0 });
    }
    let mu = sum_k / sum_n;
    if mu >= 1.0 {
        return Ok(BetaBinomialFit::PointMass { at: 1.0 });
    }
    let mut best = (f64::NEG_INFINITY, PHI_MAX);
    for i in 0..PHI_GRID_POINTS {
        let phi = phi_grid(i);
        let ll = log_likelihood(data, mu, phi);
        if ll > best.0 {
            best = (ll, phi);
        }
    }
    Ok(BetaBinomialFit::Beta { mu, phi: best.1 })
}

/// `PHI_MIN * (PHI_MAX / PHI_MIN)^(i / (PHI_GRID_POINTS - 1))`, with exact end points.
fn phi_grid(i: usize) -> f64 {
    if i + 1 >= PHI_GRID_POINTS {
        return PHI_MAX;
    }
    PHI_MIN * (PHI_MAX / PHI_MIN).powf(i as f64 / (PHI_GRID_POINTS - 1) as f64)
}

/// Beta-binomial log-likelihood without the binomial coefficients (they do not depend on
/// `phi`): `sum ln B(k + alpha, n - k + beta) - ln B(alpha, beta)`.
fn log_likelihood(data: &[(f64, f64)], mu: f64, phi: f64) -> f64 {
    let (alpha, beta) = (mu * phi, (1.0 - mu) * phi);
    let ln_b0 = ln_beta(alpha, beta);
    data.iter()
        .map(|&(k, n)| ln_beta(k + alpha, n - k + beta) - ln_b0)
        .sum()
}
