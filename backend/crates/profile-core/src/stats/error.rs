//! Typed errors of the statistics distribution layer (R2 Task 11, ADR-R2-5).
//!
//! Distribution-level functions (`BetaDist`, `beta_quantile`, `sample_beta`, `wilson`,
//! `fit_beta_binomial`) reject invalid input with a `StatsError` instead of returning NaN, so
//! a bad prior or count can never leak a NaN into a posterior, interval or percentile. The
//! special functions `lgamma` and `reg_inc_beta` keep the libm convention (NaN in, NaN out).

/// Invalid input to a statistics function.
#[derive(Clone, Copy, Debug, PartialEq, thiserror::Error)]
pub enum StatsError {
    /// Beta shapes must be finite and positive.
    #[error("Beta shapes must be finite and positive (a = {a}, b = {b})")]
    InvalidShape { a: f64, b: f64 },
    /// A probability (or a point-mass location on the rate scale) outside `[0, 1]`, or NaN.
    #[error("probability must be in [0, 1] (p = {p})")]
    InvalidProbability { p: f64 },
    /// Event and opportunity counts must be finite with `0 <= k <= n` (and `n > 0` where an
    /// observation is required).
    #[error("invalid counts (k = {k}, n = {n})")]
    InvalidCounts { k: f64, n: f64 },
    /// A normal quantile must be finite and non-negative.
    #[error("normal quantile must be finite and non-negative (z = {z})")]
    InvalidZ { z: f64 },
}
