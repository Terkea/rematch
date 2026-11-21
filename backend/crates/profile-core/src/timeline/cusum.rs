//! CUSUM detector (R2 Task 13).
//!
//! Page's one-sided CUSUM on the binomial log-likelihood ratio of rate `p1` against `p0`:
//! `S_0 = 0`, `S_t = max(0, S_{t-1} + k ln(p1/p0) + (n - k) ln((1 - p1)/(1 - p0)))`. It alarms
//! at the first game with `S_t > h` and stops there. `p1 > p0` watches for a rise (relapse),
//! `p1 < p0` for a fall (improvement).
//!
//! Observations with `k > n` are skipped and counted (the game index still advances); a game
//! with no opportunities adds nothing, so zero evidence never alarms.

use super::DetectorError;

/// Default alarm threshold.
pub const CUSUM_DEFAULT_H: f64 = 4.0;

/// CUSUM outcome with the number of skipped observations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CusumResult {
    /// 1-based game of the first alarm.
    pub alarm_game: Option<usize>,
    /// `S` at the alarm, or after the last game without one.
    pub statistic: f64,
    /// Observations seen (up to the alarm) with `k > n`.
    pub skipped: usize,
}

/// Runs CUSUM; invalid rates or threshold are a typed error.
pub fn cusum_counted(
    obs: &[(u32, u32)],
    p0: f64,
    p1: f64,
    h: f64,
) -> Result<CusumResult, DetectorError> {
    let rate_ok = |p: f64| p > 0.0 && p < 1.0;
    if !(rate_ok(p0) && rate_ok(p1)) || p0 == p1 {
        return Err(DetectorError::InvalidRates { p0, p1 });
    }
    if !(h > 0.0 && h.is_finite()) {
        return Err(DetectorError::InvalidThreshold { h });
    }
    let w_event = (p1 / p0).ln();
    let w_miss = ((1.0 - p1) / (1.0 - p0)).ln();
    let mut s: f64 = 0.0;
    let mut skipped = 0;
    for (i, &(k, n)) in obs.iter().enumerate() {
        if k > n {
            skipped += 1;
            continue;
        }
        s = (s + f64::from(k) * w_event + f64::from(n - k) * w_miss).max(0.0);
        if s > h {
            return Ok(CusumResult {
                alarm_game: Some(i + 1),
                statistic: s,
                skipped,
            });
        }
    }
    Ok(CusumResult {
        alarm_game: None,
        statistic: s,
        skipped,
    })
}

/// `(alarm_game, statistic)`. Invalid parameters give `(None, 0.0)`: no alarm is claimed;
/// use [`cusum_counted`] for the error.
pub fn cusum(obs: &[(u32, u32)], p0: f64, p1: f64, h: f64) -> (Option<usize>, f64) {
    cusum_counted(obs, p0, p1, h).map_or((None, 0.0), |r| (r.alarm_game, r.statistic))
}
