//! Weakness estimates (R2 Task 33, TS M2 Task 25 step 3): one [`estimate_weakness`] per
//! binomial weakness key, and its `weakness_estimates` row.
//!
//! - The points are the key's series points of the games the build loaded (current-version
//!   metrics only), aged against the build's clock, so the same inputs and clock give the same
//!   estimate.
//! - A key without series points still gets an estimate (no events, no opportunities): every
//!   binomial key has exactly one row per account × time class.
//! - **Sides** (the colour split): the build computes the estimates for `Both` and for each
//!   colour, from that side's series points and games (so [`MIN_GAMES`](profile_core::stats::priority::MIN_GAMES)
//!   is counted per side) against that side's benchmark only.
//! - Without a usable benchmark the estimate is `NoBenchmark` (Task 12): `band_*`, `percentile`
//!   and `excess` are `NULL`, confidence and priority 0. Fatigue is 0 until plans exist.

use std::collections::BTreeMap;

use chess_core::TimeClass;
use db::profile_rows::WeaknessEstimateRow;
use profile_core::stats::priority::{
    EstimateInput, EstimateReason, WINDOW_DAYS, WeaknessEstimate, estimate_weakness,
};
use profile_core::timeline::series::SeriesPoint;
use profile_core::types::{
    Band, BandReference, MetricId, SeriesKey, Side, WeaknessKey, WeaknessState,
};
use time::OffsetDateTime;
use uuid::Uuid;

/// The binomial weakness keys, in key order (every key except the continuous W9d and W12).
pub fn binomial_keys() -> impl Iterator<Item = WeaknessKey> {
    WeaknessKey::ALL.into_iter().filter(|k| k.is_binomial())
}

/// Age of `at` in days at `now` (negative for a future date, which weighs 1).
pub fn age_days(now: OffsetDateTime, at: OffsetDateTime) -> f64 {
    (now - at).as_seconds_f64() / 86_400.0
}

/// Analysed games played at most [`WINDOW_DAYS`] before `now`.
pub fn games_in_window(
    played: impl IntoIterator<Item = OffsetDateTime>,
    now: OffsetDateTime,
) -> u32 {
    let n = played
        .into_iter()
        .filter(|&at| age_days(now, at) <= WINDOW_DAYS)
        .count();
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// One estimate per binomial key, in key order.
pub fn compute_estimates(
    series: &BTreeMap<SeriesKey, Vec<SeriesPoint>>,
    games_in_window: u32,
    band: Band,
    reference: Option<&BandReference>,
    now: OffsetDateTime,
) -> Vec<WeaknessEstimate> {
    binomial_keys()
        .map(|key| {
            let points: Vec<(f64, f64, f64, f64)> = series
                .get(&SeriesKey::Weakness(key))
                .map(|pts| {
                    pts.iter()
                        .map(|p| {
                            (
                                f64::from(p.events),
                                f64::from(p.opportunities),
                                0.0,
                                age_days(now, p.played_at),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            estimate_weakness(&EstimateInput {
                key,
                points: &points,
                games_in_window,
                band,
                reference: reference.and_then(|r| r.get(&MetricId::Weakness(key))),
                fatigue: 0.0,
            })
        })
        .collect()
}

/// The `weakness_estimates` row of an estimate of `side`, with the timeline's state and `P_imp`
/// (the timeline is `Both` only, so a colour's row carries the account's state).
/// `excess` is stored only with a benchmark (it is 0 by construction otherwise, which would
/// read as "measured, no excess").
pub fn estimate_row(
    account_id: Uuid,
    tc: TimeClass,
    side: Side,
    e: &WeaknessEstimate,
    state: WeaknessState,
    p_imp: Option<f64>,
) -> WeaknessEstimateRow {
    let benchmarked = e.band_median.is_some() && e.reason != EstimateReason::NoBenchmark;
    WeaknessEstimateRow {
        account_id,
        time_class: tc.as_str().to_owned(),
        side: side.as_str().to_owned(),
        series_key: e.key.as_str().to_owned(),
        k: e.k,
        n: e.n,
        n_unweighted: i32::try_from(e.n_unweighted.round() as i64).unwrap_or(i32::MAX),
        k_unweighted: e.k_unweighted,
        alpha: e.posterior.alpha,
        beta: e.posterior.beta,
        mean: e.posterior.mean,
        ci_low: e.posterior.interval.low,
        ci_high: e.posterior.interval.high,
        band_median: e.band_median,
        band_p40: e.band_p40,
        confidence: e.confidence,
        excess: benchmarked.then_some(e.excess),
        priority: e.priority,
        percentile: e.percentile,
        state: state.as_str().to_owned(),
        p_imp: p_imp.filter(|p| p.is_finite()),
    }
}
