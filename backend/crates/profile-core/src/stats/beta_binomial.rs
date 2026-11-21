//! Beta-binomial helpers (R2 Task 11): pooled rate and over-dispersion of per-player counts.

use crate::stats::special::ln_beta;

/// Number of grid points for `phi`.
pub const PHI_GRID_POINTS: usize = 60;
/// Smallest `phi` on the grid.
pub const PHI_MIN: f64 = 1.0;
/// Largest `phi` on the grid ("no detectable over-dispersion").
pub const PHI_MAX: f64 = 1e5;

/// Beta-binomial fit: mean `mu` and precision `phi = alpha + beta`
/// (`alpha = mu phi`, `beta = (1 - mu) phi`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BetaBinomialFit {
    pub mu: f64,
    pub phi: f64,
}

/// Fits a Beta-binomial to `(events, opportunities)` pairs (TS `fitBetaBinomial`).
///
/// `mu = sum(k) / sum(n)`; `phi` maximises the log-likelihood over [`PHI_GRID_POINTS`]
/// log-spaced values from [`PHI_MIN`] to [`PHI_MAX`] (the first maximum wins). Pairs with
/// `n <= 0` carry no information. Without opportunities the fit is `{0, PHI_MAX}`; with `mu` at
/// 0 or 1 there is no variation to measure and `phi` is [`PHI_MAX`].
pub fn fit_beta_binomial(data: &[(f64, f64)]) -> BetaBinomialFit {
    let (sum_k, sum_n) = data
        .iter()
        .fold((0.0, 0.0), |(k, n), &(dk, dn)| (k + dk, n + dn));
    if sum_n.is_nan() || sum_n <= 0.0 {
        return BetaBinomialFit {
            mu: 0.0,
            phi: PHI_MAX,
        };
    }
    let mu = sum_k / sum_n;
    if mu.is_nan() || mu <= 0.0 || mu >= 1.0 {
        return BetaBinomialFit { mu, phi: PHI_MAX };
    }
    let mut best = (f64::NEG_INFINITY, PHI_MAX);
    for i in 0..PHI_GRID_POINTS {
        let phi = phi_grid(i);
        let ll = log_likelihood(data, mu, phi);
        if ll > best.0 {
            best = (ll, phi);
        }
    }
    BetaBinomialFit { mu, phi: best.1 }
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
        .filter(|&&(_, n)| n > 0.0)
        .map(|&(k, n)| ln_beta(k + alpha, n - k + beta) - ln_b0)
        .sum()
}
