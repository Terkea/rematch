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
//! - **Evaluation baselines** (ADR 0101): a key with no peer comparison by design (W5) is
//!   compared with [`model_baseline`] instead, built from the side's own score lost per failure
//!   ([`model_baselines`]). It applies only when the side has a benchmark at all, only to keys
//!   the benchmark lacks, and only to the all-games side (the colour pages and the coach name
//!   every colour comparison as the peers of that colour).

use std::collections::BTreeMap;

use chess_core::TimeClass;
use db::profile_rows::WeaknessEstimateRow;
use profile_core::stats::priority::{
    EstimateInput, EstimateReason, WINDOW_DAYS, WeaknessEstimate, estimate_weakness,
    estimate_weakness_against_model, model_baseline,
};
use profile_core::timeline::series::SeriesPoint;
use profile_core::types::{
    Band, BandReference, BandReferenceStat, GameContext, MetricId, PlayerGameMetrics, SeriesKey,
    Side, WeaknessKey, WeaknessState,
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

/// Evaluation baselines (ADR 0101) of the keys that have one, from `games` (a side's loaded
/// games): each key's own score lost per event, `Σ epl / Σ events` over the games at most
/// [`WINDOW_DAYS`] old (`None` without an event).
pub fn model_baselines<'a>(
    games: impl IntoIterator<Item = &'a (GameContext, PlayerGameMetrics)>,
    now: OffsetDateTime,
) -> BTreeMap<WeaknessKey, BandReferenceStat> {
    let mut sums: BTreeMap<WeaknessKey, (f64, f64)> = BTreeMap::new();
    for (ctx, m) in games {
        if age_days(now, ctx.played_at) > WINDOW_DAYS {
            continue;
        }
        for (key, obs) in &m.weakness {
            if obs.epl.is_finite() && obs.epl >= 0.0 {
                let s = sums.entry(*key).or_default();
                s.0 += obs.epl;
                s.1 += f64::from(obs.events);
            }
        }
    }
    binomial_keys()
        .filter_map(|key| {
            let epl = sums.get(&key).filter(|(_, k)| *k > 0.0).map(|(e, k)| e / k);
            model_baseline(key, epl).map(|r| (key, r))
        })
        .collect()
}

/// `reference` with the `model` rows of the keys it lacks; `None` without a reference (no
/// benchmark: nothing is compared). The timeline reads this, so a key's state and its estimate
/// compare with the same row.
pub fn with_model_baselines(
    reference: Option<&BandReference>,
    model: &BTreeMap<WeaknessKey, BandReferenceStat>,
) -> Option<BandReference> {
    let mut r = reference?.clone();
    for (key, stat) in model {
        r.entry(MetricId::Weakness(*key))
            .or_insert_with(|| stat.clone());
    }
    Some(r)
}

/// One estimate per binomial key, in key order. `model` holds the evaluation baselines
/// ([`model_baselines`]); a key uses its row only when `reference` exists and lacks the key.
pub fn compute_estimates(
    series: &BTreeMap<SeriesKey, Vec<SeriesPoint>>,
    games_in_window: u32,
    band: Band,
    reference: Option<&BandReference>,
    model: &BTreeMap<WeaknessKey, BandReferenceStat>,
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
            let peer = reference.and_then(|r| r.get(&MetricId::Weakness(key)));
            let modelled = peer
                .is_none()
                .then(|| reference.and(model.get(&key)))
                .flatten();
            let input = EstimateInput {
                key,
                points: &points,
                games_in_window,
                band,
                reference: peer.or(modelled),
                fatigue: 0.0,
            };
            if modelled.is_some() {
                estimate_weakness_against_model(&input)
            } else {
                estimate_weakness(&input)
            }
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
