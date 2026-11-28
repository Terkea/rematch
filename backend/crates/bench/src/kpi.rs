//! The engine's Tier 1 KPIs (ADR 0119, `algorithm/MATH.md` §KPIs): every metric as
//! `{value, interval, level, n}` at 80%, from one row per synthetic player ([`Unit`]).
//!
//! - recall at 2, pooled and per flaw type: Wilson score intervals;
//! - D1a, the flaw types detectable: the types whose Wilson lower bound is at least 0.8 (and,
//!   separately, by point estimate);
//! - D1b, macro recall at 2: the mean of the per-type recalls, with a stratified bootstrap;
//! - D5, the Kaplan–Meier median games to a top-2 detection, pooled and per type, on the
//!   checkpoint grid, with Greenwood's variance (undetected at the last checkpoint is censored);
//! - stability, coverage, clean false weaknesses and plan hits, each with its interval.
//!
//! [`compare`] differences two runs metric by metric, with a paired bootstrap when both ran the
//! same players on the same seeds. Every bootstrap uses a fixed seed, so a run's numbers are
//! reproducible.

use std::collections::{BTreeMap, BTreeSet};

use profile_core::stats::rng::Rng;
use serde::{Deserialize, Serialize};

/// Every interval here is an 80% one (ADR-WEB-19, the leaderboard's level).
pub const LEVEL: f64 = 0.8;
/// `Phi^-1(0.9)`, the two-sided 80% normal quantile.
pub const Z80: f64 = 1.281_551_565_544_600_4;
/// Bootstrap resamples (EXP-0001's arena uses the same count).
pub const BOOTSTRAP: usize = 2000;
/// The bootstrap's fixed seed: the same players give the same intervals.
pub const SEED: u32 = 119;
/// Recall at 2 a flaw type must reach to count as detectable (D1a, D1b).
pub const RECALL_TARGET: f64 = 0.8;

/// A metric's level: a probability, or `"exact"` for a yes/no property with no sampling error.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Level {
    Prob(f64),
    Text(String),
}

/// One KPI: the leaderboard's `{value, interval, level}` shape, with its count.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Kpi {
    pub value: Option<f64>,
    pub interval: Option<[f64; 2]>,
    pub level: Level,
    pub n: usize,
    /// How to read it, when it needs saying.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl Kpi {
    fn new(value: Option<f64>, interval: Option<[f64; 2]>, n: usize) -> Kpi {
        Kpi {
            value,
            interval,
            level: Level::Prob(LEVEL),
            n,
            note: None,
        }
    }
    fn note(mut self, s: impl Into<String>) -> Kpi {
        self.note = Some(s.into());
        self
    }
    /// A yes/no property: 1 or 0, no interval, level `"exact"`.
    pub fn exact(yes: bool, n: usize) -> Kpi {
        Kpi {
            value: Some(if yes { 1.0 } else { 0.0 }),
            interval: None,
            level: Level::Text("exact".to_owned()),
            n,
            note: None,
        }
    }
}

/// What one synthetic player contributes to the KPIs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Unit {
    pub player: String,
    /// The injected flaw id; `None` for a clean control.
    pub flaw: Option<String>,
    /// The target is in the top 2 at the last checkpoint.
    pub hit2: bool,
    /// The first checkpoint (games) with the target in the top 2.
    pub detected_at: Option<usize>,
    /// The last checkpoint's games: an undetected player is censored here.
    pub last_games: usize,
    /// Per checkpoint, whether the target's 80% interval held the true rate.
    pub covered: Vec<bool>,
    /// Top-2 Jaccard with and without 10% of the games.
    pub jaccard: Option<f64>,
    /// A clean control with a rank-1 weakness at the last checkpoint.
    pub flagged: bool,
    /// The top-1 weakness's drills include one for the flaw.
    pub plan_hit: bool,
}

/// Wilson score interval for `k` of `n` at quantile `z`; `None` for `n = 0`.
pub fn wilson(k: usize, n: usize, z: f64) -> Option<[f64; 2]> {
    if n == 0 {
        return None;
    }
    let (k, n) = (k as f64, n as f64);
    let p = k / n;
    let z2 = z * z;
    let centre = (p + z2 / (2.0 * n)) / (1.0 + z2 / n);
    let half = z * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt() / (1.0 + z2 / n);
    Some([(centre - half).max(0.0), (centre + half).min(1.0)])
}

/// The percentile interval of bootstrap values at `level`, as EXP-0001's arena takes it:
/// the sorted values at `floor(tail B)` and `floor((1 - tail) B) - 1`.
pub fn percentile_interval(values: &mut [f64], level: f64) -> Option<[f64; 2]> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let b = values.len();
    let tail = (1.0 - level) / 2.0;
    let lo = ((tail * b as f64) as usize).min(b - 1);
    let hi = (((1.0 - tail) * b as f64) as usize).clamp(1, b) - 1;
    Some([values[lo], values[hi]])
}

/// One step of a Kaplan–Meier curve: at `games`, `at_risk` players, `events` first detected,
/// survival (still undetected) and its Greenwood standard error.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KmStep {
    pub games: usize,
    pub at_risk: usize,
    pub events: usize,
    pub survival: f64,
    pub se: f64,
}

/// The Kaplan–Meier estimate of "not yet detected" from `(time, detected)` pairs: a detected
/// player's time is its first detection, an undetected one is censored at its time.
pub fn kaplan_meier(obs: &[(usize, bool)]) -> Vec<KmStep> {
    let times: BTreeSet<usize> = obs.iter().filter(|o| o.1).map(|o| o.0).collect();
    let mut s = 1.0;
    let mut green = 0.0;
    let mut out = Vec::new();
    for t in times {
        let at_risk = obs.iter().filter(|o| o.0 >= t).count();
        let events = obs.iter().filter(|o| o.1 && o.0 == t).count();
        let (n, d) = (at_risk as f64, events as f64);
        s *= 1.0 - d / n;
        if at_risk > events {
            green += d / (n * (n - d));
        }
        let se = if s > 0.0 { s * green.sqrt() } else { 0.0 };
        out.push(KmStep {
            games: t,
            at_risk,
            events,
            survival: s,
            se,
        });
    }
    out
}

/// The KM median (the first time survival is at most 0.5) and its interval: from the first time
/// the lower Greenwood band `S - z se` is at most 0.5 to the first time the upper band
/// `S + z se` is (Brookmeyer–Crowley's inversion with the plain Greenwood band).
pub fn km_median(steps: &[KmStep], z: f64) -> (Option<f64>, Option<[f64; 2]>) {
    let first = |f: &dyn Fn(&KmStep) -> bool| steps.iter().find(|s| f(s)).map(|s| s.games as f64);
    let median = first(&|s| s.survival <= 0.5);
    let lo = first(&|s| s.survival - z * s.se <= 0.5);
    let hi = first(&|s| s.survival + z * s.se <= 0.5);
    let interval = match (lo, hi) {
        (Some(l), Some(h)) => Some([l, h]),
        _ => None,
    };
    (median, interval)
}

fn flawed(units: &[Unit]) -> Vec<&Unit> {
    units.iter().filter(|u| u.flaw.is_some()).collect()
}

/// The flaw types and their players, sorted by flaw id.
fn by_flaw(units: &[Unit]) -> BTreeMap<String, Vec<&Unit>> {
    let mut out: BTreeMap<String, Vec<&Unit>> = BTreeMap::new();
    for u in units {
        if let Some(f) = &u.flaw {
            out.entry(f.clone()).or_default().push(u);
        }
    }
    out
}

fn share(units: &[&Unit], f: impl Fn(&Unit) -> bool) -> Option<f64> {
    (!units.is_empty()).then(|| units.iter().filter(|u| f(u)).count() as f64 / units.len() as f64)
}

fn km_of(units: &[&Unit]) -> Vec<KmStep> {
    let obs: Vec<(usize, bool)> = units
        .iter()
        .map(|u| match u.detected_at {
            Some(g) => (g, true),
            None => (u.last_games, false),
        })
        .collect();
    kaplan_meier(&obs)
}

/// The point value of every comparable metric, the same function [`kpis`] and [`compare`]'s
/// bootstrap evaluate. A metric with no players is `None`.
pub fn values(units: &[Unit]) -> BTreeMap<String, Option<f64>> {
    let f = flawed(units);
    let types = by_flaw(units);
    let mut out = BTreeMap::new();
    out.insert("recall_at_2".to_owned(), share(&f, |u| u.hit2));
    let per: Vec<f64> = types
        .values()
        .filter_map(|ps| share(ps, |u| u.hit2))
        .collect();
    out.insert(
        "recall_at_2_macro".to_owned(),
        (!per.is_empty()).then(|| per.iter().sum::<f64>() / per.len() as f64),
    );
    for (t, ps) in &types {
        out.insert(format!("recall_at_2.{t}"), share(ps, |u| u.hit2));
    }
    out.insert("km_median_games".to_owned(), km_median(&km_of(&f), Z80).0);
    for (t, ps) in &types {
        out.insert(format!("km_median_games.{t}"), km_median(&km_of(ps), Z80).0);
    }
    let flags: Vec<bool> = f.iter().flat_map(|u| u.covered.iter().copied()).collect();
    out.insert(
        "coverage".to_owned(),
        (!flags.is_empty())
            .then(|| flags.iter().filter(|b| **b).count() as f64 / flags.len() as f64),
    );
    let jac: Vec<f64> = units.iter().filter_map(|u| u.jaccard).collect();
    out.insert(
        "top2_jaccard".to_owned(),
        (!jac.is_empty()).then(|| jac.iter().sum::<f64>() / jac.len() as f64),
    );
    let clean: Vec<&Unit> = units.iter().filter(|u| u.flaw.is_none()).collect();
    out.insert(
        "clean_false_weakness".to_owned(),
        share(&clean, |u| u.flagged),
    );
    out.insert("plan_hit_rate".to_owned(), share(&f, |u| u.plan_hit));
    out
}

/// The strata a bootstrap resamples within: the players of each flaw type (sorted by flaw id),
/// then the clean controls, as indices into `units`.
fn strata(units: &[Unit]) -> Vec<Vec<usize>> {
    let mut by: BTreeMap<Option<&str>, Vec<usize>> = BTreeMap::new();
    for (i, u) in units.iter().enumerate() {
        by.entry(u.flaw.as_deref()).or_default().push(i);
    }
    // `None` sorts first; put the clean controls last so the flaw strata keep their order.
    let mut out: Vec<Vec<usize>> = by
        .iter()
        .filter(|(k, _)| k.is_some())
        .map(|(_, v)| v.clone())
        .collect();
    if let Some(c) = by.get(&None) {
        out.push(c.clone());
    }
    out
}

fn draw(rng: &mut Rng, len: usize) -> usize {
    ((rng.next_f64() * len as f64) as usize).min(len - 1)
}

/// One stratified resample: within each stratum, as many draws with replacement as it has.
fn resample(rng: &mut Rng, strata: &[Vec<usize>]) -> Vec<usize> {
    strata
        .iter()
        .flat_map(|s| {
            (0..s.len())
                .map(|_| s[draw(rng, s.len())])
                .collect::<Vec<_>>()
        })
        .collect()
}

fn pick(units: &[Unit], idx: &[usize]) -> Vec<Unit> {
    idx.iter().map(|&i| units[i].clone()).collect()
}

/// Stratified bootstrap percentile intervals of the metrics `names`, from `B` resamples.
fn bootstrap(
    units: &[Unit],
    names: &[&str],
    b: usize,
    seed: u32,
) -> BTreeMap<String, Option<[f64; 2]>> {
    let st = strata(units);
    let mut rng = Rng::new(seed);
    let mut draws: BTreeMap<&str, Vec<f64>> = names.iter().map(|n| (*n, Vec::new())).collect();
    for _ in 0..b {
        let v = values(&pick(units, &resample(&mut rng, &st)));
        for n in names {
            if let Some(Some(x)) = v.get(*n) {
                draws.get_mut(n).expect("named").push(*x);
            }
        }
    }
    draws
        .into_iter()
        .map(|(n, mut d)| (n.to_owned(), percentile_interval(&mut d, LEVEL)))
        .collect()
}

/// Every Tier 1 KPI of a run, keyed by the names the bench history uses (`run-record-v1`).
pub fn kpis(units: &[Unit], deterministic: bool) -> BTreeMap<String, Kpi> {
    let v = values(units);
    let f = flawed(units);
    let types = by_flaw(units);
    let val = |k: &str| v.get(k).copied().flatten();
    let mut out = BTreeMap::new();
    let hits = f.iter().filter(|u| u.hit2).count();
    out.insert(
        "recall_at_2".to_owned(),
        Kpi::new(val("recall_at_2"), wilson(hits, f.len(), Z80), f.len())
            .note("pooled over the flawed players (Wilson); EXP-0001's primary"),
    );
    let mut clear = 0;
    let mut by_point = 0;
    for (t, ps) in &types {
        let k = ps.iter().filter(|u| u.hit2).count();
        let iv = wilson(k, ps.len(), Z80);
        if iv.is_some_and(|[lo, _]| lo >= RECALL_TARGET) {
            clear += 1;
        }
        if k as f64 / ps.len() as f64 >= RECALL_TARGET {
            by_point += 1;
        }
        out.insert(
            format!("recall_at_2.{t}"),
            Kpi::new(val(&format!("recall_at_2.{t}")), iv, ps.len()),
        );
    }
    out.insert(
        "flaw_types_detectable".to_owned(),
        Kpi::new(Some(clear as f64), None, types.len()).note(
            "D1a: flaw types whose recall at 2 has its 80% Wilson lower bound at or above 0.8",
        ),
    );
    out.insert(
        "flaw_types_detectable_point".to_owned(),
        Kpi::new(Some(by_point as f64), None, types.len())
            .note("flaw types whose recall at 2 is at or above 0.8 by point estimate alone"),
    );
    let boot = bootstrap(
        units,
        &["recall_at_2_macro", "coverage", "top2_jaccard"],
        BOOTSTRAP,
        SEED,
    );
    let biv = |k: &str| boot.get(k).copied().flatten();
    out.insert(
        "recall_at_2_macro".to_owned(),
        Kpi::new(val("recall_at_2_macro"), biv("recall_at_2_macro"), f.len()).note(format!(
            "D1b: the mean over {} flaw types of recall at 2 (stratified bootstrap, {BOOTSTRAP} resamples, seed {SEED})",
            types.len()
        )),
    );
    let km = |ps: &[&Unit]| -> Kpi {
        let (m, iv) = km_median(&km_of(ps), Z80);
        let last = ps.iter().map(|u| u.last_games).max().unwrap_or(0);
        let k = Kpi::new(m, iv, ps.len());
        match (m, iv) {
            (None, _) => k.note(format!(
                "median not reached: fewer than half detected by {last} games"
            )),
            (Some(_), None) => k.note(format!(
                "the interval's upper end lies beyond the last checkpoint ({last} games)"
            )),
            _ => k,
        }
    };
    let pooled = km(&f);
    let lead = "D5: Kaplan–Meier median games to a top-2 detection on the checkpoint grid (Greenwood); undetected players are censored at the last checkpoint";
    let note = match &pooled.note {
        Some(n) => format!("{lead}; {n}"),
        None => lead.to_owned(),
    };
    out.insert("km_median_games".to_owned(), pooled.note(note));
    for (t, ps) in &types {
        out.insert(format!("km_median_games.{t}"), km(ps));
    }
    let flags: Vec<bool> = f.iter().flat_map(|u| u.covered.iter().copied()).collect();
    out.insert(
        "coverage".to_owned(),
        Kpi::new(val("coverage"), biv("coverage"), flags.len()).note(
            "share of the targets' 80% intervals holding the true rate, every checkpoint (players resampled by flaw)",
        ),
    );
    let jac = units.iter().filter(|u| u.jaccard.is_some()).count();
    out.insert(
        "top2_jaccard".to_owned(),
        Kpi::new(val("top2_jaccard"), biv("top2_jaccard"), jac).note(
            "top2_agreement: mean top-2 Jaccard with and without 10% of the games (bootstrap over players)",
        ),
    );
    let clean: Vec<&Unit> = units.iter().filter(|u| u.flaw.is_none()).collect();
    let flagged = clean.iter().filter(|u| u.flagged).count();
    out.insert(
        "clean_false_weakness".to_owned(),
        Kpi::new(
            val("clean_false_weakness"),
            wilson(flagged, clean.len(), Z80),
            clean.len(),
        )
        .note("D3: clean controls with a rank-1 weakness at the last checkpoint (Wilson)"),
    );
    let plan = f.iter().filter(|u| u.plan_hit).count();
    out.insert(
        "plan_hit_rate".to_owned(),
        Kpi::new(val("plan_hit_rate"), wilson(plan, f.len(), Z80), f.len()).note(
            "not a KPI: equals recall at 1 when every key has a drill (a unit test holds that)",
        ),
    );
    out.insert(
        "deterministic".to_owned(),
        Kpi::exact(deterministic, units.len()),
    );
    out
}

/// The Kaplan–Meier curves, pooled (`all`) and per flaw type, for the charts.
pub fn km_curves(units: &[Unit]) -> BTreeMap<String, Vec<KmStep>> {
    let mut out = BTreeMap::new();
    out.insert("all".to_owned(), km_of(&flawed(units)));
    for (t, ps) in by_flaw(units) {
        out.insert(t, km_of(&ps));
    }
    out
}

/// Which way is better for a metric, for the comparison's wording.
pub fn better(name: &str) -> &'static str {
    match name.split('.').next().unwrap_or(name) {
        "km_median_games" | "clean_false_weakness" => "lower",
        "coverage" => "closer to 0.8",
        _ => "higher",
    }
}

/// One metric of a comparison of two runs: B minus A.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Difference {
    pub name: String,
    pub a: Option<f64>,
    pub b: Option<f64>,
    pub difference: Option<f64>,
    pub interval: Option<[f64; 2]>,
    pub level: f64,
    /// Bootstrap resamples where both sides had a value.
    pub resamples: usize,
    /// `higher`, `lower` or `no clear difference`, read from the interval alone.
    pub verdict: String,
    pub better: String,
}

/// The verdict of a difference interval: `higher` when it lies above 0, `lower` when below,
/// otherwise `no clear difference`; `not comparable` without an interval.
pub fn verdict(interval: Option<[f64; 2]>) -> &'static str {
    match interval {
        Some([lo, _]) if lo > 0.0 => "higher",
        Some([_, hi]) if hi < 0.0 => "lower",
        Some(_) => "no clear difference",
        None => "not comparable",
    }
}

/// A comparison of two runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    pub schema: String,
    /// The runs share their players and seeds, so each resample draws the same players on both
    /// sides (paired); otherwise the sides are resampled independently.
    pub paired: bool,
    pub level: f64,
    pub bootstrap: usize,
    pub seed: u32,
    pub metrics: Vec<Difference>,
}

/// B minus A for every metric of [`values`]. `paired`: the two unit lists hold the same players
/// (by name) on the same seeds, so one stratified resample of the players serves both sides;
/// otherwise each side is resampled on its own. A resample where a side has no value is skipped.
pub fn compare(a: &[Unit], b: &[Unit], paired: bool, resamples: usize, seed: u32) -> Comparison {
    let va = values(a);
    let vb = values(b);
    let names: BTreeSet<String> = va.keys().chain(vb.keys()).cloned().collect();
    let mut draws: BTreeMap<String, Vec<f64>> =
        names.iter().map(|n| (n.clone(), Vec::new())).collect();
    let mut rng = Rng::new(seed);
    // Pair by player name: B's units in A's order.
    let b_by_name: BTreeMap<&str, &Unit> = b.iter().map(|u| (u.player.as_str(), u)).collect();
    let b_paired: Vec<Unit> = if paired {
        a.iter()
            .filter_map(|u| b_by_name.get(u.player.as_str()).map(|x| (*x).clone()))
            .collect()
    } else {
        Vec::new()
    };
    let paired = paired && b_paired.len() == a.len() && a.len() == b.len();
    let (sa, sb) = (strata(a), strata(b));
    for _ in 0..resamples {
        let (ra, rb) = if paired {
            let idx = resample(&mut rng, &sa);
            (pick(a, &idx), pick(&b_paired, &idx))
        } else {
            let ia = resample(&mut rng, &sa);
            let ib = resample(&mut rng, &sb);
            (pick(a, &ia), pick(b, &ib))
        };
        let (xa, xb) = (values(&ra), values(&rb));
        for n in &names {
            if let (Some(Some(x)), Some(Some(y))) = (xa.get(n), xb.get(n)) {
                draws.get_mut(n).expect("named").push(y - x);
            }
        }
    }
    let metrics = names
        .iter()
        .map(|n| {
            let (x, y) = (va.get(n).copied().flatten(), vb.get(n).copied().flatten());
            let mut d = draws.remove(n).unwrap_or_default();
            let count = d.len();
            // Fewer than half the resamples with a value on both sides: no interval.
            let interval = if count * 2 >= resamples {
                percentile_interval(&mut d, LEVEL)
            } else {
                None
            };
            Difference {
                name: n.clone(),
                a: x,
                b: y,
                difference: x.zip(y).map(|(x, y)| y - x),
                interval,
                level: LEVEL,
                resamples: count,
                verdict: verdict(interval).to_owned(),
                better: better(n).to_owned(),
            }
        })
        .collect();
    Comparison {
        schema: "bench-compare-v1".to_owned(),
        paired,
        level: LEVEL,
        bootstrap: resamples,
        seed,
        metrics,
    }
}

/// The comparison as Markdown, one row per metric.
pub fn compare_markdown(c: &Comparison, a: &str, b: &str) -> String {
    let f = |x: Option<f64>| x.map_or("–".to_owned(), |v| format!("{v:.3}"));
    let mut out = format!(
        "# Benchmark comparison\n\nA: {a}\nB: {b}\n\n{} bootstrap, {} resamples, seed {}, {:.0}% intervals of B minus A. Tier 1 numbers are synthetic.\n\n| Metric | A | B | B − A | {:.0}% interval | Verdict | Better |\n|---|---|---|---|---|---|---|\n",
        if c.paired {
            "Paired (same players and seeds)"
        } else {
            "Unpaired"
        },
        c.bootstrap,
        c.seed,
        c.level * 100.0,
        c.level * 100.0,
    );
    for d in &c.metrics {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            d.name,
            f(d.a),
            f(d.b),
            d.difference.map_or("–".to_owned(), |v| format!("{v:+.3}")),
            d.interval
                .map_or("–".to_owned(), |[l, h]| format!("[{l:+.3}, {h:+.3}]")),
            d.verdict,
            d.better,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(name: &str, flaw: Option<&str>, detected: Option<usize>) -> Unit {
        Unit {
            player: name.to_owned(),
            flaw: flaw.map(str::to_owned),
            hit2: detected.is_some(),
            detected_at: detected,
            last_games: 80,
            covered: vec![true, false],
            jaccard: Some(1.0),
            flagged: false,
            plan_hit: detected.is_some(),
        }
    }

    #[test]
    fn wilson_matches_the_site_and_the_closed_form() {
        // 9 of 9 at 80%: the lower end is n / (n + z^2).
        let [lo, hi] = wilson(9, 9, Z80).unwrap();
        assert!((lo - 9.0 / (9.0 + Z80 * Z80)).abs() < 1e-12);
        assert_eq!(hi, 1.0);
        let [lo, hi] = wilson(0, 3, Z80).unwrap();
        assert_eq!(lo, 0.0);
        assert!((hi - Z80 * Z80 / (3.0 + Z80 * Z80)).abs() < 1e-12);
        assert_eq!(wilson(0, 0, Z80), None);
    }

    #[test]
    fn percentiles_take_exp_0001s_indices() {
        let mut v: Vec<f64> = (0..10).rev().map(f64::from).collect();
        // (1 - 0.8) / 2 is 0.09999999999999998 in floating point, as in EXP-0001's Python, so
        // the lower index of 10 values is 0 (and of 2,000 it is 199), the upper 8 (1,799).
        assert_eq!(percentile_interval(&mut v, 0.8), Some([0.0, 8.0]));
        let mut w: Vec<f64> = (0..2000).map(f64::from).collect();
        assert_eq!(percentile_interval(&mut w, 0.8), Some([199.0, 1799.0]));
    }

    #[test]
    fn kaplan_meier_with_censoring_matches_a_hand_computation() {
        // 5 players: detected at 30, 30, 45; censored at 80 twice.
        let obs = [(30, true), (30, true), (45, true), (80, false), (80, false)];
        let km = kaplan_meier(&obs);
        assert_eq!(km.len(), 2);
        assert!((km[0].survival - 0.6).abs() < 1e-12);
        // Greenwood: 0.6^2 * 2 / (5 * 3).
        assert!((km[0].se - (0.36_f64 * 2.0 / 15.0).sqrt()).abs() < 1e-12);
        assert!((km[1].survival - 0.6 * 2.0 / 3.0).abs() < 1e-12);
        let g: f64 = 2.0 / 15.0 + 1.0 / (3.0 * 2.0);
        assert!((km[1].se - 0.4 * g.sqrt()).abs() < 1e-12);
        let (m, iv) = km_median(&km, Z80);
        assert_eq!(m, Some(45.0));
        // Lower band at 30: 0.6 - 1.28 * 0.219 = 0.319 <= 0.5; upper at 45: 0.4 + 0.29 > 0.5.
        assert_eq!(iv, None);
        let (m, _) = km_median(&kaplan_meier(&[(80, false), (80, false), (30, true)]), Z80);
        assert_eq!(m, None);
    }

    #[test]
    fn km_median_interval_closes_when_everyone_is_detected() {
        let obs: Vec<(usize, bool)> = (0..20)
            .map(|i| (if i < 10 { 30 } else { 45 }, true))
            .collect();
        let (m, iv) = km_median(&kaplan_meier(&obs), Z80);
        assert_eq!(m, Some(30.0));
        assert_eq!(iv, Some([30.0, 45.0]));
    }

    fn suite() -> Vec<Unit> {
        let mut u = Vec::new();
        for i in 0..6 {
            u.push(unit(&format!("h{i}"), Some("hang"), Some(30)));
            u.push(unit(&format!("m{i}"), Some("miss_mate"), None));
            u.push(unit(
                &format!("o{i}"),
                Some("opening_blunder"),
                (i < 3).then_some(60),
            ));
        }
        u.push(unit("c", None, None));
        u
    }

    #[test]
    fn macro_recall_is_the_mean_of_the_types_and_d1a_counts_lower_bounds() {
        let u = suite();
        let k = kpis(&u, true);
        assert_eq!(k["recall_at_2.hang"].value, Some(1.0));
        assert_eq!(k["recall_at_2.miss_mate"].value, Some(0.0));
        assert_eq!(k["recall_at_2.opening_blunder"].value, Some(0.5));
        let mac = k["recall_at_2_macro"].value.unwrap();
        assert!((mac - 0.5).abs() < 1e-12);
        let [lo, hi] = k["recall_at_2_macro"].interval.unwrap();
        assert!(lo <= mac && mac <= hi && lo < hi);
        // 6 of 6: Wilson lower 6 / (6 + z^2) = 0.785 < 0.8, so no type clears 0.8 by its interval.
        assert_eq!(k["flaw_types_detectable"].value, Some(0.0));
        assert_eq!(k["flaw_types_detectable_point"].value, Some(1.0));
        assert_eq!(k["flaw_types_detectable"].n, 3);
        assert_eq!(k["recall_at_2"].n, 18);
        assert_eq!(k["deterministic"].level, Level::Text("exact".to_owned()));
        assert_eq!(k["km_median_games.hang"].value, Some(30.0));
        assert_eq!(k["km_median_games.miss_mate"].value, None);
        // Pooled: 6 at 30 (12 at risk... 18), 3 at 60: S(30) = 12/18, S(60) = 9/18 = 0.5.
        assert_eq!(k["km_median_games"].value, Some(60.0));
        // Same input, same seed: the same intervals.
        assert_eq!(kpis(&u, true), k);
    }

    #[test]
    fn compare_a_run_with_itself_is_no_clear_difference_and_a_gain_reads_higher() {
        let a = suite();
        let c = compare(&a, &a, true, 400, SEED);
        assert!(c.paired);
        let r = c.metrics.iter().find(|d| d.name == "recall_at_2").unwrap();
        assert_eq!(r.difference, Some(0.0));
        assert_eq!(r.interval, Some([0.0, 0.0]));
        assert_eq!(r.verdict, "no clear difference");
        // B detects every miss_mate player.
        let b: Vec<Unit> = a
            .iter()
            .map(|u| {
                if u.flaw.as_deref() == Some("miss_mate") {
                    unit(&u.player, Some("miss_mate"), Some(45))
                } else {
                    u.clone()
                }
            })
            .collect();
        let c = compare(&a, &b, true, 400, SEED);
        let r = c
            .metrics
            .iter()
            .find(|d| d.name == "recall_at_2_macro")
            .unwrap();
        assert!((r.difference.unwrap() - 1.0 / 3.0).abs() < 1e-12);
        assert_eq!(r.verdict, "higher");
        let k = c
            .metrics
            .iter()
            .find(|d| d.name == "km_median_games")
            .unwrap();
        assert_eq!(k.better, "lower");
        // Unpaired when the players differ.
        let mut other = b.clone();
        other.pop();
        assert!(!compare(&a, &other, true, 100, SEED).paired);
        let md = compare_markdown(&c, "a.json", "b.json");
        assert!(
            md.contains("| recall_at_2_macro | 0.500 | 0.833 | +0.333 |"),
            "{md}"
        );
    }
}
