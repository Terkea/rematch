//! Priority scoring (R2 Task 12, TS M2 Task 16, ADR-R2-5).
//!
//! `estimate_weakness` turns a weakness series into a shrunk posterior against the band
//! reference and scores it:
//! - only points at most [`WINDOW_DAYS`] old count, recency-weighted with a 60-day half-life;
//! - the prior is `{pooled_rate, N0[key]}`, or `Beta(1, 1)` without a benchmark;
//! - `confidence = P(theta > band median)`;
//! - `excess = max(0, mean - median) * (n_unweighted / games) * epl_per_event`;
//! - `priority = excess * trainability * confidence * (1 - fatigue)`, 0 unless the reason is
//!   `Ok` (benchmark present and at least [`MIN_GAMES`] games).

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

/// Why an estimate can or cannot be ranked (TS `reason`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstimateReason {
    Ok,
    NoBenchmark,
    InsufficientData,
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

/// The band prior of a reference: both `pooled_rate` and `epl_per_event` must be present and
/// finite, with the pooled rate in `[0, 1]`; otherwise there is no benchmark.
fn benchmark(r: Option<&BandReferenceStat>) -> Option<(&BandReferenceStat, f64, f64)> {
    let r = r?;
    let pooled = r.pooled_rate.filter(|p| (0.0..=1.0).contains(p))?;
    let epl = r.epl_per_event.filter(|e| e.is_finite() && *e >= 0.0)?;
    r.median.is_finite().then_some((r, pooled, epl))
}

/// Estimate and score one weakness (TS `estimateWeakness`).
///
/// Reasons: fewer than [`MIN_GAMES`] games is `InsufficientData` (it wins over a missing
/// benchmark, because it is what the player can fix); otherwise no usable reference is
/// `NoBenchmark`. Both force priority 0. Without a benchmark the posterior uses `Beta(1, 1)`,
/// `band_*` and `percentile` are `None`, and confidence and excess are 0.
pub fn estimate_weakness(i: &EstimateInput) -> WeaknessEstimate {
    let in_window: Vec<_> = i
        .points
        .iter()
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
            let excess = (post.mean - r.median).max(0.0) * per_game * epl;
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

    let reason = if i.games_in_window < MIN_GAMES {
        EstimateReason::InsufficientData
    } else if bench.is_none() {
        EstimateReason::NoBenchmark
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
    }
}

/// Rankable estimates (every key but W9d and W12) by priority descending, then by key string
/// (TS `rankWeaknesses`).
pub fn rank_weaknesses(v: Vec<WeaknessEstimate>) -> Vec<WeaknessEstimate> {
    let mut v: Vec<_> = v
        .into_iter()
        .filter(|e| RANKABLE.contains(&e.key))
        .collect();
    v.sort_by(|a, b| {
        b.priority
            .total_cmp(&a.priority)
            .then_with(|| a.key.as_str().cmp(b.key.as_str()))
    });
    v
}
