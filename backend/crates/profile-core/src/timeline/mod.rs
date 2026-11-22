//! Change detectors and weakness timeline states (R2 Tasks 13-14).

pub mod bocpd;
pub mod causes;
pub mod cusum;
pub mod ewma;
pub mod series;
pub mod states;

/// Invalid options of a change detector (BOCPD, CUSUM, EWMA).
#[derive(Clone, Copy, Debug, PartialEq, thiserror::Error)]
pub enum DetectorError {
    /// The hazard must be finite and strictly inside `(0, 1)`.
    #[error("hazard must be in (0, 1) (hazard = {hazard})")]
    InvalidHazard { hazard: f64 },
    /// Beta prior shapes must be finite and positive.
    #[error("prior shapes must be finite and positive (a = {a}, b = {b})")]
    InvalidPrior { a: f64, b: f64 },
    /// `max_run` must be at least 1.
    #[error("max_run must be at least 1")]
    InvalidMaxRun,
    /// CUSUM rates must be finite, inside `(0, 1)` and different.
    #[error("CUSUM rates must be in (0, 1) and differ (p0 = {p0}, p1 = {p1})")]
    InvalidRates { p0: f64, p1: f64 },
    /// The CUSUM threshold must be finite and positive.
    #[error("CUSUM threshold must be finite and positive (h = {h})")]
    InvalidThreshold { h: f64 },
    /// The EWMA weight must be in `(0, 1]`.
    #[error("EWMA lambda must be in (0, 1] (lambda = {lambda})")]
    InvalidLambda { lambda: f64 },
    /// EWMA level, sigma and k must be finite, sigma and k non-negative.
    #[error(
        "EWMA level, sigma and k must be finite, sigma and k >= 0 (level = {level}, sigma = {sigma}, k = {k})"
    )]
    InvalidEwma { level: f64, sigma: f64, k: f64 },
}
