//! Recency weights (R2 Task 11): exponential decay with a half-life in days.

/// Default half-life of a game's weight (improvement-engine section 6.3: 60 days).
pub const HALF_LIFE_DAYS: f64 = 60.0;

/// Weight `0.5^(age / half_life)` of an observation `age_days` old. A negative age (a game
/// dated in the future by clock skew) counts as age 0, so no observation weighs more than 1. A
/// NaN age is unknown and gets weight 0 (excluded). `half_life_days` must be positive: a zero,
/// negative or NaN half-life fails a `debug_assert!`, and in release builds gives weight 0 to
/// every observation (nothing is counted), never NaN or a weight above 1.
pub fn recency_weight(age_days: f64, half_life_days: f64) -> f64 {
    debug_assert!(half_life_days > 0.0, "half-life must be positive");
    if age_days.is_nan() || half_life_days.is_nan() || half_life_days <= 0.0 {
        return 0.0;
    }
    0.5_f64.powf(age_days.max(0.0) / half_life_days)
}

/// Recency-weighted `(k, n)` of `(events, opportunities, age_days)` points (TS `weightedCounts`):
/// `k = sum w events`, `n = sum w opportunities` with `w = recency_weight(age, half_life)`.
/// Points with a NaN age are excluded entirely.
pub fn weighted_counts(points: &[(f64, f64, f64)], half_life_days: f64) -> (f64, f64) {
    points.iter().filter(|&&(_, _, age)| !age.is_nan()).fold(
        (0.0, 0.0),
        |(k, n), &(events, opportunities, age)| {
            let w = recency_weight(age, half_life_days);
            (k + w * events, n + w * opportunities)
        },
    )
}
