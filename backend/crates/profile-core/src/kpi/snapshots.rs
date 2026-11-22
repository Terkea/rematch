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

use std::collections::BTreeMap;

use coaching_input::MetricUnit;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kpi::catalogue::{
    Extracted, KpiDefinition, KpiGame, KpiKind, is_count_mean, kpi_catalogue, kpi_definition,
};
use crate::kpi::iso_week::iso_week_key;
use crate::stats::intervals::{
    BOOTSTRAP_RESAMPLES, BOOTSTRAP_SEED, INTERVAL_LEVEL, Z80, bootstrap_mean_interval, wilson,
};
use crate::stats::quantiles::quantile;
use crate::stats::rng::Rng;
use crate::stats::special::lgamma;
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
    /// `value - opp_slope_per_100 * mean((opp - own) / 100)`, clamped to the unit's domain
    /// ([`clamp_to_unit`]). `None` without a reference stat for the metric, for `Last` rows,
    /// when `value` is `None`, and when no contributing game knows both ratings.
    pub value_adjusted: Option<f64>,
    /// Diagnostics only: the adjusted value before the unit clamp. Never shown, compared or
    /// used for percentiles.
    pub value_adjusted_raw: Option<f64>,
    /// Games (`Mean`, `Last`) or **opportunities** (`Rate`) behind the value. A rate row's `n`
    /// is not a game count.
    pub n: u32,
    pub ci_low: Option<f64>,
    pub ci_high: Option<f64>,
}

impl KpiValue {
    /// The interval of `value_adjusted`: the raw interval shifted by the unclamped adjustment,
    /// then both ends clamped like the value, so it still brackets `value_adjusted` (the clamp
    /// is monotone).
    pub fn adjusted_interval(&self) -> Option<(f64, f64)> {
        let shift = self.value_adjusted_raw? - self.value?;
        let unit = kpi_definition(self.metric).and_then(|d| d.contract_unit);
        Some((
            clean(clamp_to_unit(self.ci_low? + shift, unit)),
            clean(clamp_to_unit(self.ci_high? + shift, unit)),
        ))
    }
}

/// Clamps a value to its unit's domain: rate [0, 1], pct [0, 100], per_game, seconds and elo
/// at least 0. Rows without a unit are signed and stay unclamped.
pub fn clamp_to_unit(x: f64, unit: Option<MetricUnit>) -> f64 {
    match unit {
        None => x,
        Some(MetricUnit::Rate) => x.clamp(0.0, 1.0),
        Some(MetricUnit::Pct) => x.clamp(0.0, 100.0),
        Some(MetricUnit::PerGame | MetricUnit::Seconds | MetricUnit::Elo) => x.max(0.0),
    }
}

/// Inputs left out of a computation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkipCounts {
    /// Games whose `PlayerGameMetrics.version` is not [`PLAYER_GAME_METRICS_VERSION`] (their
    /// defaults would read as clean zero evidence). Skipped before windowing.
    pub stale_version: u32,
    /// Extra identical copies of a game (same `game_id` and equal content): one copy is kept.
    pub duplicate_game: u32,
    /// `game_id`s whose copies disagree: every copy is refused, since keeping any one of them
    /// would depend on input order.
    pub conflicting_duplicates: u32,
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
/// Games with a stale metrics version are dropped first. Copies of a `game_id` are then merged
/// when identical and refused when they conflict, the rest are ordered by `played_at` (newest
/// first, ties by `game_id`), and the window is applied. The result depends only on the
/// multiset of games, never on their order. Means use the 80% percentile bootstrap (seed 1, 1000 resamples), rates the
/// 80% Wilson interval of the pooled counts; `Last` takes the newest value and has no interval.
pub fn compute_kpis_report(
    games: &[KpiGame<'_>],
    w: &Window,
    reference: Option<&BandReference>,
) -> KpiReport {
    let mut skipped = SkipCounts::default();
    let valid: Vec<&KpiGame<'_>> = games
        .iter()
        .filter(|g| {
            let ok = g.metrics.version == PLAYER_GAME_METRICS_VERSION;
            if !ok {
                skipped.stale_version += 1;
            }
            ok
        })
        .collect();
    let mut by_id: BTreeMap<Uuid, Vec<&KpiGame<'_>>> = BTreeMap::new();
    for g in valid {
        by_id.entry(g.ctx.game_id).or_default().push(g);
    }
    let mut valid: Vec<&KpiGame<'_>> = Vec::with_capacity(by_id.len());
    for copies in by_id.into_values() {
        let Some((&first, rest)) = copies.split_first() else {
            continue;
        };
        if rest.iter().all(|c| same_game(c, first)) {
            skipped.duplicate_game += u32::try_from(rest.len()).unwrap_or(u32::MAX);
            valid.push(first);
        } else {
            skipped.conflicting_duplicates += 1;
        }
    }
    valid.sort_by(|a, b| {
        b.ctx
            .played_at
            .cmp(&a.ctx.played_at)
            .then_with(|| a.ctx.game_id.cmp(&b.ctx.game_id))
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

/// Equal content (a NaN anywhere makes copies unequal, so they are refused as conflicting).
fn same_game(a: &KpiGame<'_>, b: &KpiGame<'_>) -> bool {
    a.ctx == b.ctx && a.metrics == b.metrics && a.session == b.session
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
    /// Sum of the per-game values (`Mean` rows), for the Garwood fallback.
    sum: f64,
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
        value_adjusted_raw: None,
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
    // Widen (never narrow) so the stated interval always brackets the value.
    let mut ci = est
        .ci
        .filter(|(lo, hi)| lo.is_finite() && hi.is_finite())
        .map(|(lo, hi)| (lo.min(value), hi.max(value)));
    if let Some((lo, hi)) = ci
        && hi <= lo
    {
        // A zero-width interval claims certainty the data cannot give.
        ci = if is_count_mean(def.metric) {
            garwood_rate_interval(est.sum, f64::from(est.n))
                .map(|(lo, hi)| (lo.min(value), hi.max(value)))
        } else {
            None
        };
    }
    if let Some((lo, hi)) = ci {
        row.ci_low = Some(clean(lo));
        row.ci_high = Some(clean(hi));
    }
    if def.kind != KpiKind::Last
        && let Some(stat) = reference.and_then(|r| r.get(&def.metric))
        && let Some(slope) = stat.opp_slope_per_100.filter(|s| s.is_finite())
        && let Some(gap) = mean_rating_gap(&contributions)
    {
        // No slope (unknown) means no adjustment, never a guessed one.
        let raw = value - slope * gap;
        if raw.is_finite() {
            row.value_adjusted_raw = Some(clean(raw));
            row.value_adjusted = Some(clean(clamp_to_unit(raw, def.contract_unit)));
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
        sum: value,
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
        sum: values.iter().sum(),
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
        sum: k,
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
        sum: value,
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

/// Mean `(opp - own) / 100` over the contributing games that know both ratings, weighted by
/// opportunities for count contributions (rate rows) and 1 per game otherwise. `None` when no
/// game (or no opportunity) knows both ratings: there is nothing to adjust from.
fn mean_rating_gap(c: &[Contribution<'_, '_>]) -> Option<f64> {
    let (mut sum, mut weight) = (0.0, 0.0);
    for x in c {
        let Some((own, opp)) = ratings(x.game.metrics, x.game) else {
            continue;
        };
        let w = match x.x {
            Extracted::Counts(_, n) => n,
            Extracted::Value(_) => 1.0,
        };
        sum += w * f64::from(opp - own) / 100.0;
        weight += w;
    }
    (weight > 0.0).then(|| sum / weight)
}

/// Lower tail of each side of the 80% interval.
const TAIL: f64 = (1.0 - INTERVAL_LEVEL) / 2.0;

/// `P(X <= k)` for `X ~ Poisson(mu)`, summed in log space (`k` is a whole number).
fn poisson_cdf(k: f64, mu: f64) -> f64 {
    if mu <= 0.0 {
        return 1.0;
    }
    let ln_mu = mu.ln();
    let ln_terms: Vec<f64> = (0..=(k as u64))
        .map(|j| {
            let j = j as f64;
            -mu + j * ln_mu - lgamma(j + 1.0)
        })
        .collect();
    let m = ln_terms.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let s: f64 = ln_terms.iter().map(|t| (t - m).exp()).sum();
    (m + s.ln()).exp().min(1.0)
}

/// The `mu` with `poisson_cdf(k, mu) = p` (the CDF decreases in `mu`), by bisection.
fn poisson_mu(k: f64, p: f64) -> f64 {
    let mut lo = 0.0;
    let mut hi = k + 20.0 + 20.0 * (k + 1.0).sqrt();
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if poisson_cdf(k, mid) > p {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo <= 1e-12 * hi.max(1.0) {
            break;
        }
    }
    0.5 * (lo + hi)
}

/// Exact Poisson (Garwood) 80% interval of a per-game rate: `total` events over `games` games.
/// The lower `mu_L` solves `P(X >= total) = 0.1` (0 when `total = 0`), the upper `mu_U` solves
/// `P(X <= total) = 0.1`; both are divided by `games`. `None` without games or for a negative,
/// non-finite or non-integer total.
pub fn garwood_rate_interval(total: f64, games: f64) -> Option<(f64, f64)> {
    let valid = games > 0.0 && total >= 0.0 && total.is_finite() && total.fract() == 0.0;
    if !valid {
        return None;
    }
    let upper = poisson_mu(total, TAIL);
    let lower = if total == 0.0 {
        0.0
    } else {
        poisson_mu(total - 1.0, 1.0 - TAIL)
    };
    Some((lower / games, upper / games))
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
/// intervals to separate or, for rates, a two-proportion |z| >= 1.96. Separation of two 80%
/// intervals is looser than |z| >= 1.96 (about |z| >= 1.81 for equal widths), so the interval
/// rule decides most changes (ADR-R2-7); its direction comes from
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
