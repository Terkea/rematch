//! KPI snapshot computation (R2 Task 15, TS M2 Task 19, ADR-R2-7).
//!
//! Honesty rules (carried over from Task 12):
//! - no value from zero evidence: a row with no contributing game (or no opportunity) has
//!   `value = None`, no interval and no adjusted value;
//! - NaN and infinity never reach the output: non-finite per-game values, impossible counts
//!   (`events > opportunities`) and stale `PlayerGameMetrics` versions are skipped and counted in
//!   [`SkipCounts`];
//! - the output depends only on the set of games, never on their order, so a recomputation over
//!   the same games gives bit-identical rows and change detection ([`should_append`]) never sees
//!   a spurious change.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kpi::catalogue::{Extracted, KpiDefinition, KpiGame, KpiKind, kpi_catalogue};
use crate::kpi::iso_week::iso_week_key;
use crate::stats::intervals::{
    BOOTSTRAP_RESAMPLES, BOOTSTRAP_SEED, INTERVAL_LEVEL, Z80, bootstrap_mean_interval, wilson,
};
use crate::stats::quantiles::quantile;
use crate::stats::rng::Rng;
use crate::types::{
    BandReference, BaseMetric, MetricId, PLAYER_GAME_METRICS_VERSION, PlayerGameMetrics,
};

/// The games a snapshot covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Window {
    /// The `n` most recent games by `played_at` (the profile uses 50).
    LastN(usize),
    /// Games whose UTC ISO week ([`iso_week_key`]) is this key.
    Week(String),
}

/// `kpi_window` in the database.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowKind {
    LastN,
    Week,
}

impl WindowKind {
    pub fn as_str(self) -> &'static str {
        match self {
            WindowKind::LastN => "last_n",
            WindowKind::Week => "week",
        }
    }
}

impl Window {
    pub fn kind(&self) -> WindowKind {
        match self {
            Window::LastN(_) => WindowKind::LastN,
            Window::Week(_) => WindowKind::Week,
        }
    }

    /// `"last50"` for `LastN(50)`, the ISO week for `Week`.
    pub fn key(&self) -> String {
        match self {
            Window::LastN(n) => format!("last{n}"),
            Window::Week(k) => k.clone(),
        }
    }
}

/// One KPI snapshot row (`kpi_snapshot`). Values are in the row's unit; `ci_low`/`ci_high` are
/// the 80% interval of the raw `value` and always bracket it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KpiValue {
    pub metric: MetricId,
    pub window_kind: WindowKind,
    pub window_key: String,
    pub value: Option<f64>,
    /// `value - opp_slope_per_100 * mean((opp - own) / 100)`; `None` without a reference stat
    /// for the metric, for `Last` rows and when `value` is `None`.
    pub value_adjusted: Option<f64>,
    /// Games (`Mean`, `Last`) or opportunities (`Rate`) behind the value.
    pub n: u32,
    pub ci_low: Option<f64>,
    pub ci_high: Option<f64>,
}

impl KpiValue {
    /// The interval of `value_adjusted`: the raw interval shifted by the adjustment.
    pub fn adjusted_interval(&self) -> Option<(f64, f64)> {
        let shift = self.value_adjusted? - self.value?;
        Some((self.ci_low? + shift, self.ci_high? + shift))
    }
}

/// Inputs left out of a computation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkipCounts {
    /// Games whose `PlayerGameMetrics.version` is not [`PLAYER_GAME_METRICS_VERSION`] (their
    /// defaults would read as clean zero evidence). Skipped before windowing.
    pub stale_version: u32,
    /// Second and later games with an already-seen `game_id`.
    pub duplicate_game: u32,
    /// Per-row contributions that were non-finite or impossible counts (one game can add
    /// several, one per affected row), plus aggregates that overflowed.
    pub invalid_value: u32,
}

/// The rows (one per catalogue entry, in catalogue order) and what was skipped.
#[derive(Clone, Debug, PartialEq)]
pub struct KpiReport {
    pub values: Vec<KpiValue>,
    pub skipped: SkipCounts,
}

/// [`compute_kpis_report`] without the skip counts.
pub fn compute_kpis(
    games: &[KpiGame<'_>],
    w: &Window,
    reference: Option<&BandReference>,
) -> Vec<KpiValue> {
    compute_kpis_report(games, w, reference).values
}

/// Computes every catalogue row over the window.
///
/// Games with a stale metrics version are dropped first, then the rest are ordered by
/// `played_at` (newest first, ties by `game_id`), duplicates of a `game_id` are dropped, and the
/// window is applied. Means use the 80% percentile bootstrap (seed 1, 1000 resamples), rates the
/// 80% Wilson interval of the pooled counts; `Last` takes the newest value and has no interval.
pub fn compute_kpis_report(
    games: &[KpiGame<'_>],
    w: &Window,
    reference: Option<&BandReference>,
) -> KpiReport {
    let mut skipped = SkipCounts::default();
    let mut valid: Vec<&KpiGame<'_>> = games
        .iter()
        .filter(|g| {
            let ok = g.metrics.version == PLAYER_GAME_METRICS_VERSION;
            if !ok {
                skipped.stale_version += 1;
            }
            ok
        })
        .collect();
    valid.sort_by(|a, b| {
        b.ctx
            .played_at
            .cmp(&a.ctx.played_at)
            .then_with(|| a.ctx.game_id.cmp(&b.ctx.game_id))
    });
    let mut seen: BTreeSet<Uuid> = BTreeSet::new();
    valid.retain(|g| {
        let fresh = seen.insert(g.ctx.game_id);
        if !fresh {
            skipped.duplicate_game += 1;
        }
        fresh
    });
    let window: Vec<&KpiGame<'_>> = match w {
        Window::LastN(n) => valid.into_iter().take(*n).collect(),
        Window::Week(key) => valid
            .into_iter()
            .filter(|g| iso_week_key(g.ctx.played_at) == *key)
            .collect(),
    };
    let values = kpi_catalogue()
        .iter()
        .map(|def| compute_row(def, &window, w, reference, &mut skipped))
        .collect();
    KpiReport { values, skipped }
}

/// A contribution that passed validation.
struct Contribution<'g, 'a> {
    game: &'g KpiGame<'a>,
    x: Extracted,
}

fn valid_extracted(x: Extracted) -> bool {
    match x {
        Extracted::Value(v) => v.is_finite(),
        Extracted::Counts(k, n) => k.is_finite() && n.is_finite() && 0.0 <= k && k <= n,
    }
}

struct Estimate {
    value: f64,
    n: u32,
    ci: Option<(f64, f64)>,
}

fn compute_row(
    def: &KpiDefinition,
    window: &[&KpiGame<'_>],
    w: &Window,
    reference: Option<&BandReference>,
    skipped: &mut SkipCounts,
) -> KpiValue {
    let mut contributions = Vec::new();
    for &game in window {
        let Some(x) = (def.extract)(game) else {
            continue;
        };
        if valid_extracted(x) {
            contributions.push(Contribution { game, x });
        } else {
            skipped.invalid_value += 1;
        }
    }
    let n_contributing = contributions.len();
    let estimate = match def.kind {
        KpiKind::Last => last(&contributions),
        KpiKind::Rate => rate(&contributions),
        KpiKind::Mean if def.metric == MetricId::Base(BaseMetric::SessionAccuracyDecline) => {
            session_decline(&contributions)
        }
        KpiKind::Mean => mean(&contributions),
    };
    let n_fallback = u32::try_from(n_contributing).unwrap_or(u32::MAX);
    let mut row = KpiValue {
        metric: def.metric,
        window_kind: w.kind(),
        window_key: w.key(),
        value: None,
        value_adjusted: None,
        n: n_fallback,
        ci_low: None,
        ci_high: None,
    };
    let Some(est) = estimate else {
        if def.kind == KpiKind::Rate {
            // No opportunity at all: report the opportunities (0), not the games.
            row.n = pooled_opportunities(&contributions);
        }
        return row;
    };
    row.n = est.n;
    if !est.value.is_finite() {
        skipped.invalid_value += 1;
        return row;
    }
    let value = clean(est.value);
    row.value = Some(value);
    if let Some((lo, hi)) = est.ci
        && lo.is_finite()
        && hi.is_finite()
    {
        // Widen (never narrow) so the stated interval always brackets the value.
        row.ci_low = Some(clean(lo.min(value)));
        row.ci_high = Some(clean(hi.max(value)));
    }
    if def.kind != KpiKind::Last
        && let Some(stat) = reference.and_then(|r| r.get(&def.metric))
        && stat.opp_slope_per_100.is_finite()
    {
        let adjusted = value - stat.opp_slope_per_100 * mean_rating_gap(&contributions);
        if adjusted.is_finite() {
            row.value_adjusted = Some(clean(adjusted));
        }
    }
    row
}

/// Normalises -0.0 to 0.0, so a stored value reads back bit-identical.
fn clean(x: f64) -> f64 {
    if x == 0.0 { 0.0 } else { x }
}

fn as_value(x: Extracted) -> Option<f64> {
    match x {
        Extracted::Value(v) => Some(v),
        Extracted::Counts(..) => None,
    }
}

fn last(c: &[Contribution<'_, '_>]) -> Option<Estimate> {
    let value = c.iter().find_map(|c| as_value(c.x))?;
    Some(Estimate {
        value,
        n: u32::try_from(c.len()).unwrap_or(u32::MAX),
        ci: None,
    })
}

fn mean_of(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    Some(values.iter().sum::<f64>() / values.len() as f64)
}

fn mean(c: &[Contribution<'_, '_>]) -> Option<Estimate> {
    let values: Vec<f64> = c.iter().filter_map(|c| as_value(c.x)).collect();
    let value = mean_of(&values)?;
    let ci = bootstrap_mean_interval(&values, BOOTSTRAP_RESAMPLES, BOOTSTRAP_SEED, INTERVAL_LEVEL)
        .map(|i| (i.low, i.high));
    Some(Estimate {
        value,
        n: u32::try_from(values.len()).unwrap_or(u32::MAX),
        ci,
    })
}

fn pooled(c: &[Contribution<'_, '_>]) -> (f64, f64) {
    c.iter().fold((0.0, 0.0), |(k, n), c| match c.x {
        Extracted::Counts(ck, cn) => (k + ck, n + cn),
        Extracted::Value(_) => (k, n),
    })
}

fn pooled_opportunities(c: &[Contribution<'_, '_>]) -> u32 {
    // `as` saturates; counts are whole numbers.
    pooled(c).1.round() as u32
}

fn rate(c: &[Contribution<'_, '_>]) -> Option<Estimate> {
    let (k, n) = pooled(c);
    if n <= 0.0 {
        return None;
    }
    let ci = wilson(k, n, Z80).ok().map(|i| (i.low, i.high));
    Some(Estimate {
        value: k / n,
        n: n.round() as u32,
        ci,
    })
}

/// `mean(accuracy at index >= 4) - mean(accuracy at index 1)`, with a two-sample percentile
/// bootstrap (each group resampled on its own) when both groups have at least two games.
fn session_decline(c: &[Contribution<'_, '_>]) -> Option<Estimate> {
    let mut first = Vec::new();
    let mut late = Vec::new();
    for x in c {
        let (Some(v), Some(s)) = (as_value(x.x), x.game.session) else {
            continue;
        };
        if s.index_in_session == 1 {
            first.push(v);
        } else {
            late.push(v);
        }
    }
    let value = mean_of(&late)? - mean_of(&first)?;
    let ci = (first.len() >= 2 && late.len() >= 2).then(|| diff_interval(&first, &late));
    Some(Estimate {
        value,
        n: u32::try_from(first.len() + late.len()).unwrap_or(u32::MAX),
        ci,
    })
}

fn resample_mean(rng: &mut Rng, values: &[f64]) -> f64 {
    let n = values.len();
    let mut sum = 0.0;
    for _ in 0..n {
        let i = ((rng.next_f64() * n as f64) as usize).min(n - 1);
        sum += values[i];
    }
    sum / n as f64
}

fn diff_interval(first: &[f64], late: &[f64]) -> (f64, f64) {
    let mut rng = Rng::new(BOOTSTRAP_SEED);
    let mut diffs: Vec<f64> = (0..BOOTSTRAP_RESAMPLES)
        .map(|_| {
            let l = resample_mean(&mut rng, late);
            l - resample_mean(&mut rng, first)
        })
        .collect();
    diffs.sort_by(f64::total_cmp);
    (
        quantile(&diffs, (1.0 - INTERVAL_LEVEL) / 2.0),
        quantile(&diffs, (1.0 + INTERVAL_LEVEL) / 2.0),
    )
}

fn ratings(m: &PlayerGameMetrics, g: &KpiGame<'_>) -> Option<(i32, i32)> {
    Some((
        m.player_rating.or(g.ctx.player_rating)?,
        m.opponent_rating.or(g.ctx.opponent_rating)?,
    ))
}

/// Mean `(opp - own) / 100` over the contributing games that know both ratings; 0 (no
/// adjustment) when none does.
fn mean_rating_gap(c: &[Contribution<'_, '_>]) -> f64 {
    let gaps: Vec<f64> = c
        .iter()
        .filter_map(|c| ratings(c.game.metrics, c.game))
        .map(|(own, opp)| f64::from(opp - own) / 100.0)
        .collect();
    mean_of(&gaps).unwrap_or(0.0)
}

/// Outcome of comparing two snapshots of one metric.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    Improved,
    Worse,
    NoClearChange,
}

/// Critical |z| of the two-proportion test.
pub const Z_CHANGE: f64 = 1.96;

/// Compares `b` (later) against `a` (earlier) on the raw values. A change needs the 80%
/// intervals to separate or, for rates, a two-proportion |z| >= 1.96; its direction comes from
/// `higher_is_better`. Missing values, a missing direction or different metrics give
/// `NoClearChange`.
pub fn compare_kpi(a: &KpiValue, b: &KpiValue, def: &KpiDefinition) -> Comparison {
    if a.metric != def.metric || b.metric != def.metric {
        return Comparison::NoClearChange;
    }
    let (Some(better_up), Some(va), Some(vb)) = (def.higher_is_better, a.value, b.value) else {
        return Comparison::NoClearChange;
    };
    let mut went_up = match (a.ci_low, a.ci_high, b.ci_low, b.ci_high) {
        (Some(_), Some(ah), Some(bl), Some(_)) if bl > ah => Some(true),
        (Some(al), Some(_), Some(_), Some(bh)) if bh < al => Some(false),
        _ => None,
    };
    if went_up.is_none() && def.kind == KpiKind::Rate {
        let (na, nb) = (f64::from(a.n), f64::from(b.n));
        if na > 0.0 && nb > 0.0 {
            let p = (va * na + vb * nb) / (na + nb);
            let se = (p * (1.0 - p) * (1.0 / na + 1.0 / nb)).sqrt();
            if se > 0.0 && se.is_finite() {
                let z = (vb - va) / se;
                if z.abs() >= Z_CHANGE {
                    went_up = Some(z > 0.0);
                }
            }
        }
    }
    match went_up {
        None => Comparison::NoClearChange,
        Some(up) if up == better_up => Comparison::Improved,
        Some(_) => Comparison::Worse,
    }
}

/// Snapshots are append-only and a new row is written only when `value` or `n` changed against
/// the latest stored row of the same metric and window (an interval or adjustment that moved on
/// its own, for example after a new reference build, is not a new snapshot).
pub fn should_append(previous: Option<&KpiValue>, current: &KpiValue) -> bool {
    match previous {
        None => true,
        Some(p) => p.value != current.value || p.n != current.n,
    }
}
