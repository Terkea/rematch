//! EWMA drift detector (R2 Task 13, research §6.6 "Drift monitoring").
//!
//! `z_0 = level`, `z_t = lambda x_t + (1 - lambda) z_{t-1}`. It alarms at the first game with
//! `z_t > level + k sigma sqrt(lambda / (2 - lambda))` and stops there.
//!
//! Non-finite values are skipped and counted (`z` is unchanged and the game index still
//! advances), so with no valid value `z` stays at `level` and nothing alarms.

use super::bocpd::DetectorError;

/// Default EWMA weight.
pub const EWMA_DEFAULT_LAMBDA: f64 = 0.1;
/// Default alarm width in EWMA standard deviations.
pub const EWMA_DEFAULT_K: f64 = 2.0;

/// EWMA outcome with the number of skipped values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EwmaResult {
    /// 1-based position of the first alarm.
    pub alarm_game: Option<usize>,
    /// `z` at the alarm, or after the last value without one.
    pub z: f64,
    /// Non-finite values seen (up to the alarm).
    pub skipped: usize,
}

/// Runs the EWMA monitor; invalid parameters are a typed error.
pub fn ewma_drift_counted(
    x: &[f64],
    lambda: f64,
    level: f64,
    sigma: f64,
    k: f64,
) -> Result<EwmaResult, DetectorError> {
    if !(lambda > 0.0 && lambda <= 1.0) {
        return Err(DetectorError::InvalidLambda { lambda });
    }
    if !(level.is_finite() && sigma.is_finite() && sigma >= 0.0 && k.is_finite() && k >= 0.0) {
        return Err(DetectorError::InvalidEwma { level, sigma, k });
    }
    let limit = level + k * sigma * (lambda / (2.0 - lambda)).sqrt();
    let mut z = level;
    let mut skipped = 0;
    for (i, &xi) in x.iter().enumerate() {
        if !xi.is_finite() {
            skipped += 1;
            continue;
        }
        let next = lambda * xi + (1.0 - lambda) * z;
        if !next.is_finite() {
            // Overflow from huge finite values: skip rather than let inf reach the output.
            skipped += 1;
            continue;
        }
        z = next;
        if z > limit {
            return Ok(EwmaResult {
                alarm_game: Some(i + 1),
                z,
                skipped,
            });
        }
    }
    Ok(EwmaResult {
        alarm_game: None,
        z,
        skipped,
    })
}

/// `(alarm_game, z)`. Invalid parameters give `(None, level)` when `level` is finite and
/// `(None, 0.0)` otherwise: no alarm is claimed; use [`ewma_drift_counted`] for the error.
pub fn ewma_drift(x: &[f64], lambda: f64, level: f64, sigma: f64, k: f64) -> (Option<usize>, f64) {
    match ewma_drift_counted(x, lambda, level, sigma, k) {
        Ok(r) => (r.alarm_game, r.z),
        Err(_) => (None, if level.is_finite() { level } else { 0.0 }),
    }
}
