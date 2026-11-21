//! Priority scoring (R2 Task 12, TS M2 Task 16, ADR-R2-5).
//!
//! `estimate_weakness` turns a weakness series into a shrunk posterior against the band
//! reference and scores it:
//! - only points at most [`WINDOW_DAYS`] old count, recency-weighted with a 60-day half-life;
//! - the prior is `{pooled_rate, N0[key]}`, or `Beta(1, 1)` without a benchmark;
//! - `confidence = P(theta > band median)`;
//! - `excess = max(0, mean - median) * (n_unweighted / games) * epl_per_event`;
//! - `priority = excess * trainability * confidence * (1 - fatigue)`, 0 unless the reason is
//!   `Ok` (benchmark present, at least [`MIN_GAMES`] games and at least one in-window event);
//! - with no in-window event, confidence is 0 and the percentile is `None`: we never express
//!   certainty about a weakness that was never observed;
//! - invalid points are skipped and counted, so no NaN or infinity reaches the output.

use serde::{Deserialize, Serialize};

use crate::constants::{RANKABLE, min_evidence, n0, trainability};
use crate::stats::posterior::{BetaPosterior, posterior, prob_above};
use crate::stats::quantiles::{QuartileRef, percentile_from_quartiles};
use crate::stats::weights::{HALF_LIFE_DAYS, weighted_counts};
use crate::types::{Band, BandReferenceStat, WeaknessKey};

/// Points older than this many days are outside the 6-month window (inclusive bound).
pub const WINDOW_DAYS: f64 = 183.0;
/// Fewer analysed games in the window than this gives `InsufficientData` (section 6.3).
pub const MIN_GAMES: u32 = 30;
/// An estimate is precise when its 80% interval is at most this fraction of the band median.
pub const PRECISE_REL_WIDTH: f64 = 0.25;
/// Largest plausible events or opportunities of one point; larger values are invalid (they keep
/// every sum finite).
pub const MAX_POINT_COUNT: f64 = 1e9;
/// Largest usable band EPL per event (expected points lost, in pawns' worth of win share per
/// event; real values are below 1). Larger values mean a broken reference: no benchmark.
pub const MAX_EPL_PER_EVENT: f64 = 1e3;

/// Why an estimate can or cannot be ranked (TS `reason`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstimateReason {
    Ok,
    NoBenchmark,
    InsufficientData,
    /// Benchmark and games are sufficient, but no event in the window: nothing is claimed.
    NoEvents,
}

/// A scored weakness (TS `WeaknessEstimate`, plus the unweighted event count for the contract).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeaknessEstimate {
    pub key: WeaknessKey,
    /// Recency-weighted events and opportunities inside the window.
    pub k: f64,
    pub n: f64,
    /// Raw opportunities and events inside the window (contract `opportunities`/`events`).
    pub n_unweighted: f64,
    pub k_unweighted: f64,
    pub posterior: BetaPosterior,
    pub band_median: Option<f64>,
    pub band_p40: Option<f64>,
    /// Band percentile of the posterior mean (lower rate is better), `None` without benchmark.
    pub percentile: Option<f64>,
    pub confidence: f64,
    pub excess: f64,
    pub trainability: f64,
    pub priority: f64,
    pub precise: bool,
    pub min_evidence_met: bool,
    pub reason: EstimateReason,
    /// Points skipped as invalid (see [`estimate_weakness`]).
    pub invalid_points: u32,
}

/// Input of [`estimate_weakness`]. `points` are `(events, opportunities, epl, age_days)`; `epl`
/// is carried for TS parity and not used (excess uses the band EPL per event).
#[derive(Clone, Copy, Debug)]
pub struct EstimateInput<'a> {
    pub key: WeaknessKey,
    pub points: &'a [(f64, f64, f64, f64)],
    pub games_in_window: u32,
    pub band: Band,
    pub reference: Option<&'a BandReferenceStat>,
    /// Plan fatigue in `[0, 1]`; 0 until plans exist (M6/M7). Clamped, NaN counts as 0.
    pub fatigue: f64,
}

/// The band prior of a reference: `pooled_rate` in `[0, 1]`, `epl_per_event` in
/// `[0, MAX_EPL_PER_EVENT]`, finite quartiles and a median in `[0, 1]`; otherwise there is no benchmark.
fn benchmark(r: Option<&BandReferenceStat>) -> Option<(&BandReferenceStat, f64, f64)> {
    let r = r?;
    let pooled = r.pooled_rate.filter(|p| (0.0..=1.0).contains(p))?;
    let epl = r
        .epl_per_event
        .filter(|e| (0.0..=MAX_EPL_PER_EVENT).contains(e))?;
    let finite = [r.p25, r.p40, r.median, r.p75]
        .iter()
        .all(|x| x.is_finite());
    (finite && (0.0..=1.0).contains(&r.median)).then_some((r, pooled, epl))
}

/// A usable point: finite, non-negative events, opportunities and EPL with
/// `events <= opportunities <= MAX_POINT_COUNT`, events either 0 or at least 1 (real counts
/// are integers, so a fraction below 1 is corrupt), and a finite age. A negative finite age is
/// clock skew and valid (it weighs 1, ADR-R2-5 section 5).
fn valid_point(&(e, o, epl, age): &(f64, f64, f64, f64)) -> bool {
    let ok = |x: f64| x.is_finite() && x >= 0.0;
    ok(e)
        && (e == 0.0 || e >= 1.0)
        && ok(o)
        && ok(epl)
        && age.is_finite()
        && e <= o
        && o <= MAX_POINT_COUNT
}

/// Estimate and score one weakness (TS `estimateWeakness`).
///
/// Reasons: fewer than [`MIN_GAMES`] games is `InsufficientData` (it wins over a missing
/// benchmark, because it is what the player can fix); otherwise no usable reference is
/// `NoBenchmark`; otherwise no in-window event is `NoEvents`. All three force priority 0.
/// Without a benchmark the posterior uses `Beta(1, 1)`, `band_*` and `percentile` are `None`,
/// and confidence and excess are 0. Whenever `k_unweighted == 0` (whatever the reason),
/// confidence is 0 and `percentile` is `None`.
///
/// Invalid points (see `valid_point`: non-finite or negative values, events above
/// opportunities, implausibly large counts, a non-finite age) are skipped and counted in
/// `invalid_points`. Valid points older than [`WINDOW_DAYS`] are ignored, not invalid.
pub fn estimate_weakness(i: &EstimateInput) -> WeaknessEstimate {
    let valid: Vec<_> = i.points.iter().filter(|p| valid_point(p)).collect();
    let invalid_points = u32::try_from(i.points.len() - valid.len()).unwrap_or(u32::MAX);
    let in_window: Vec<_> = valid
        .into_iter()
        .filter(|p| p.3 <= WINDOW_DAYS)
        .copied()
        .collect();
    let weighted: Vec<_> = in_window.iter().map(|&(e, o, _, a)| (e, o, a)).collect();
    let (k, n) = weighted_counts(&weighted, HALF_LIFE_DAYS);
    let (k_unweighted, n_unweighted) = in_window
        .iter()
        .fold((0.0, 0.0), |(ks, ns), &(e, o, _, _)| (ks + e, ns + o));

    let bench = benchmark(i.reference);
    let post = match bench {
        Some((_, pooled, _)) => posterior(k, n, pooled, n0(i.key)),
        None => posterior(k, n, 0.5, 2.0),
    };
    let train = trainability(i.key, i.band);
    let fatigue = if i.fatigue.is_nan() {
        0.0
    } else {
        i.fatigue.clamp(0.0, 1.0)
    };

    let (band_median, band_p40, percentile, confidence, excess, precise) = match bench {
        Some((r, _, epl)) => {
            let q = QuartileRef {
                p25: r.p25,
                p40: r.p40,
                median: r.median,
                p75: r.p75,
            };
            let pct = percentile_from_quartiles(post.mean, &q, false);
            let per_game = if i.games_in_window == 0 {
                0.0
            } else {
                n_unweighted / f64::from(i.games_in_window)
            };
            // Bounded factors: rate gap <= 1, per_game <= points * MAX_POINT_COUNT,
            // epl <= MAX_EPL_PER_EVENT, so the product is finite; the guard is defensive.
            let excess = (post.mean - r.median).max(0.0) * per_game * epl;
            let excess = if excess.is_finite() { excess } else { 0.0 };
            let width = post.interval.high - post.interval.low;
            (
                Some(r.median),
                Some(r.p40),
                pct.is_finite().then_some(pct),
                prob_above(&post, r.median),
                excess,
                width <= PRECISE_REL_WIDTH * r.median,
            )
        }
        None => (None, None, None, 0.0, 0.0, false),
    };

    let no_events = k_unweighted == 0.0;
    let (confidence, percentile) = if no_events {
        (0.0, None)
    } else {
        (confidence, percentile)
    };

    let reason = if i.games_in_window < MIN_GAMES {
        EstimateReason::InsufficientData
    } else if bench.is_none() {
        EstimateReason::NoBenchmark
    } else if no_events {
        EstimateReason::NoEvents
    } else {
        EstimateReason::Ok
    };
    let priority = match reason {
        EstimateReason::Ok => excess * train * confidence * (1.0 - fatigue),
        _ => 0.0,
    };

    WeaknessEstimate {
        key: i.key,
        k,
        n,
        n_unweighted,
        k_unweighted,
        posterior: post,
        band_median,
        band_p40,
        percentile,
        confidence,
        excess,
        trainability: train,
        priority,
        precise,
        min_evidence_met: n_unweighted >= min_evidence(i.key),
        reason,
        invalid_points,
    }
}

/// Rankable estimates (every key but W9d and W12) by priority descending, then by key string
/// (TS `rankWeaknesses`). Non-finite priorities (only possible in hand-built estimates) sort
/// after every finite one, by key.
pub fn rank_weaknesses(v: Vec<WeaknessEstimate>) -> Vec<WeaknessEstimate> {
    let mut v: Vec<_> = v
        .into_iter()
        .filter(|e| RANKABLE.contains(&e.key))
        .collect();
    v.sort_by(|a, b| {
        let (fa, fb) = (a.priority.is_finite(), b.priority.is_finite());
        fb.cmp(&fa)
            .then_with(|| {
                if fa && fb {
                    b.priority.total_cmp(&a.priority)
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .then_with(|| a.key.as_str().cmp(b.key.as_str()))
    });
    v
}
