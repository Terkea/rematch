//! What a run records, the scorers (a) to (e), the Markdown scorecard and the baseline check.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::flaws::{Tally, Unit};
use crate::generate::GeneratedGame;
use crate::kpi::{self, KmStep, Kpi, Unit as KpiUnit};

/// A ranked weakness of the CoachingInput.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RankedItem {
    /// Series key (`W4.missed`).
    pub key: String,
    pub rank: u32,
    pub priority: f64,
    pub confidence: f64,
    pub posterior_rate: f64,
    pub interval_low: f64,
    pub interval_high: f64,
    pub band_median: f64,
    pub events: u32,
    pub opportunities: u32,
    /// `ai::output::drills::drills_for` of the weakness.
    pub drills: Vec<String>,
}

/// A `weakness_estimates` row (side `both`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EstimateRow {
    pub key: String,
    pub events: f64,
    pub opportunities: f64,
    pub mean: f64,
    pub low: f64,
    pub high: f64,
    pub priority: f64,
    pub band_median: Option<f64>,
    pub confidence: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub games: usize,
    pub ranked: Vec<RankedItem>,
    pub estimates: Vec<EstimateRow>,
    pub benchmark_players: Option<i32>,
}

impl Checkpoint {
    pub fn rank_of(&self, key: &str) -> Option<u32> {
        self.ranked.iter().find(|r| r.key == key).map(|r| r.rank)
    }
    pub fn estimate(&self, key: &str) -> Option<&EstimateRow> {
        self.estimates.iter().find(|e| e.key == key)
    }
    pub fn top(&self, n: usize) -> Vec<String> {
        self.ranked.iter().take(n).map(|r| r.key.clone()).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlawInfo {
    pub id: String,
    pub rate: f64,
    pub target_key: String,
    pub unit: Unit,
    pub expected_drills: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GenStats {
    pub games: usize,
    pub player_wins: usize,
    pub draws: usize,
    pub player_losses: usize,
    pub mean_plies: f64,
    pub time_forfeits: usize,
    pub injected_moves: usize,
    pub seconds: f64,
}

impl GenStats {
    pub fn of(games: &[GeneratedGame], seconds: f64) -> GenStats {
        let mut s = GenStats {
            games: games.len(),
            seconds,
            ..GenStats::default()
        };
        for g in games {
            let won = match (g.result.as_str(), g.player_color.as_str()) {
                ("1-0", "white") | ("0-1", "black") => Some(true),
                ("1-0", _) | ("0-1", _) => Some(false),
                _ => None,
            };
            match won {
                Some(true) => s.player_wins += 1,
                Some(false) => s.player_losses += 1,
                None => s.draws += 1,
            }
            if g.termination.ends_with("on time") {
                s.time_forfeits += 1;
            }
            s.injected_moves += g.injected_plies.len();
        }
        if !games.is_empty() {
            s.mean_plies =
                games.iter().map(|g| f64::from(g.plies)).sum::<f64>() / games.len() as f64;
        }
        s
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerResult {
    pub name: String,
    pub flaw: Option<FlawInfo>,
    pub tally: Tally,
    pub generation: GenStats,
    pub checkpoints: Vec<Checkpoint>,
    pub perturbed: Option<Checkpoint>,
    pub deterministic: bool,
    /// The side-by-side variant's checkpoints (`ALGO_VARIANT`): the same games in a second
    /// account, every profile built under the variant's prior rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<VariantResult>,
}

/// One player's results under the side-by-side variant.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VariantResult {
    pub name: String,
    pub checkpoints: Vec<Checkpoint>,
    /// The variant's checkpoint of the perturbed account (90% of the games), with `perturb`.
    #[serde(default)]
    pub perturbed: Option<Checkpoint>,
    pub deterministic: bool,
}

impl PlayerResult {
    pub fn last(&self) -> Option<&Checkpoint> {
        self.checkpoints.last()
    }

    /// This player as the variant saw it: its checkpoints, perturbed checkpoint and determinism.
    pub fn as_variant(&self) -> Option<PlayerResult> {
        let v = self.variant.as_ref()?;
        Some(PlayerResult {
            checkpoints: v.checkpoints.clone(),
            deterministic: v.deterministic,
            perturbed: v.perturbed.clone(),
            variant: None,
            ..self.clone()
        })
    }
}

/// One player's line of the scorecard.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Case {
    pub player: String,
    pub flaw: Option<String>,
    pub rate: Option<f64>,
    pub target: Option<String>,
    /// The true rate of the target weakness (`Tally::truth`).
    pub truth: Option<f64>,
    pub target_rank: Option<u32>,
    pub target_estimate: Option<[f64; 3]>,
    pub covered: Option<bool>,
    pub top: Vec<String>,
    pub plan_hit: Option<bool>,
    pub games_to_detection: Option<usize>,
    /// Why the target is not in the top 2, read from its estimate row.
    pub note: Option<String>,
    pub benchmark_players: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    /// (a) Flawed players whose target is in the top 2 at the last checkpoint.
    pub recall_at_2: f64,
    pub recall_at_1: f64,
    /// (a) Players with a top-1 weakness whose top 1 is their flaw (clean controls count as
    /// wrong when they have one).
    pub top1_precision: Option<f64>,
    /// (a) Clean controls with a ranked weakness at rank 1.
    pub clean_top1_false_alarms: usize,
    pub clean_players: usize,
    /// (b) Share of (flawed player, checkpoint) target intervals holding the true rate.
    pub coverage: Option<f64>,
    pub coverage_samples: usize,
    /// (c) Flawed players whose top-1 weakness's drills include one for their flaw. Not a KPI
    /// (ADR 0119): it equals recall at 1 when every rankable key has a drill, which
    /// `every_rankable_key_has_a_drill` holds.
    pub plan_hit_rate: f64,
    /// (d) Mean games to first top-2 detection over detected players. Deprecated (ADR 0119): it
    /// leaves out the players never detected; `kpis["km_median_games"]` replaces it. Kept in
    /// the JSON for one version.
    pub mean_games_to_detection: Option<f64>,
    /// (e) Same rows and clock gave the same weaknesses, for every player.
    pub deterministic: bool,
    /// (e) Mean Jaccard overlap of the top 2 with and without 10% of the games.
    pub top2_agreement: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Thresholds {
    pub recall_at_2_min: f64,
    pub clean_top1_false_alarms_max: usize,
    pub coverage_min: f64,
    pub coverage_max: f64,
}

pub const THRESHOLDS: Thresholds = Thresholds {
    recall_at_2_min: 0.8,
    clean_top1_false_alarms_max: 0,
    coverage_min: 0.65,
    coverage_max: 0.95,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Verdicts {
    pub recall: bool,
    pub clean: bool,
    pub coverage: bool,
    pub pass: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scorecard {
    pub suite: String,
    pub nodes: u64,
    pub metrics: Metrics,
    pub thresholds: Thresholds,
    pub verdicts: Verdicts,
    pub cases: Vec<Case>,
    /// Every Tier 1 KPI as `{value, interval, level, n}` at 80% (ADR 0119, `kpi::kpis`), keyed
    /// by the names of the bench history's `run-record-v1`. Synthetic players only.
    #[serde(default)]
    pub kpis: BTreeMap<String, Kpi>,
    /// The Kaplan–Meier curves of "not yet detected", pooled (`all`) and per flaw type.
    #[serde(default)]
    pub km_curves: BTreeMap<String, Vec<KmStep>>,
}

/// The clean controls' mean raw rate of `key` at their last checkpoint (0 without controls).
fn clean_base(players: &[PlayerResult], key: &str) -> f64 {
    let rates: Vec<f64> = players
        .iter()
        .filter(|p| p.flaw.is_none())
        .filter_map(|p| p.last()?.estimate(key))
        .filter(|e| e.opportunities > 0.0)
        .map(|e| e.events / e.opportunities)
        .collect();
    if rates.is_empty() {
        0.0
    } else {
        rates.iter().sum::<f64>() / rates.len() as f64
    }
}

fn jaccard(a: &[String], b: &[String]) -> f64 {
    let a: BTreeSet<_> = a.iter().collect();
    let b: BTreeSet<_> = b.iter().collect();
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    a.intersection(&b).count() as f64 / a.union(&b).count() as f64
}

fn mean(v: &[f64]) -> Option<f64> {
    (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
}

fn note(c: &Checkpoint, key: &str) -> Option<String> {
    if c.rank_of(key).is_some_and(|r| r <= 2) {
        return None;
    }
    let Some(e) = c.estimate(key) else {
        return Some("no estimate row".to_owned());
    };
    Some(if e.band_median.is_none() {
        "not ranked: no peer benchmark for this key".to_owned()
    } else if e.events == 0.0 {
        "not ranked: no events".to_owned()
    } else if e.priority <= 0.0 {
        // ADR 0101: the interval holds the median, or the excess is below the floor.
        "not ranked: not enough evidence yet".to_owned()
    } else {
        match c.rank_of(key) {
            Some(r) => format!("ranked {r}"),
            None => "not ranked: no exemplar".to_owned(),
        }
    })
}

/// One [`KpiUnit`] per player with a checkpoint: what the KPIs of ADR 0119 read. The hit, the
/// detection, the coverage flags and the plan hit are read exactly as [`score`] reads them.
pub fn units(players: &[PlayerResult]) -> Vec<KpiUnit> {
    let mut out = Vec::new();
    for p in players {
        let Some(last) = p.last() else { continue };
        let jaccard = p
            .perturbed
            .as_ref()
            .map(|q| jaccard(&last.top(2), &q.top(2)));
        let Some(f) = &p.flaw else {
            out.push(KpiUnit {
                player: p.name.clone(),
                flaw: None,
                hit2: false,
                detected_at: None,
                last_games: last.games,
                covered: Vec::new(),
                jaccard,
                flagged: !last.top(3).is_empty(),
                plan_hit: false,
            });
            continue;
        };
        let key = f.target_key.as_str();
        let truth = p.tally.truth(f.rate, clean_base(players, key));
        let covered = p
            .checkpoints
            .iter()
            .filter_map(|c| Some((truth?, c.estimate(key)?)))
            .map(|(t, e)| e.low <= t && t <= e.high)
            .collect();
        out.push(KpiUnit {
            player: p.name.clone(),
            flaw: Some(f.id.clone()),
            hit2: last.rank_of(key).is_some_and(|r| r <= 2),
            detected_at: p
                .checkpoints
                .iter()
                .find(|c| c.rank_of(key).is_some_and(|r| r <= 2))
                .map(|c| c.games),
            last_games: last.games,
            covered,
            jaccard,
            flagged: false,
            plan_hit: last.ranked.first().is_some_and(|r| {
                r.drills
                    .iter()
                    .any(|d| f.expected_drills.iter().any(|x| x == d))
            }),
        });
    }
    out
}

/// Scores a run.
pub fn score(suite: &str, nodes: u64, players: &[PlayerResult]) -> Scorecard {
    let mut cases = Vec::new();
    let mut coverage = Vec::new();
    for p in players {
        let Some(last) = p.last() else { continue };
        let top = last.top(3);
        let Some(f) = &p.flaw else {
            cases.push(Case {
                player: p.name.clone(),
                flaw: None,
                rate: None,
                target: None,
                truth: None,
                target_rank: None,
                target_estimate: None,
                covered: None,
                top,
                plan_hit: None,
                games_to_detection: None,
                note: last
                    .ranked
                    .first()
                    .map(|r| format!("false alarm: {} at rank 1", r.key)),
                benchmark_players: last.benchmark_players,
            });
            continue;
        };
        let key = f.target_key.as_str();
        let truth = p.tally.truth(f.rate, clean_base(players, key));
        for c in &p.checkpoints {
            if let (Some(t), Some(e)) = (truth, c.estimate(key)) {
                coverage.push(e.low <= t && t <= e.high);
            }
        }
        let est = last.estimate(key);
        let plan_hit = last.ranked.first().map(|r| {
            r.drills
                .iter()
                .any(|d| f.expected_drills.iter().any(|x| x == d))
        });
        cases.push(Case {
            player: p.name.clone(),
            flaw: Some(f.id.clone()),
            rate: Some(f.rate),
            target: Some(f.target_key.clone()),
            truth,
            target_rank: last.rank_of(key),
            target_estimate: est.map(|e| [e.mean, e.low, e.high]),
            covered: match (truth, est) {
                (Some(t), Some(e)) => Some(e.low <= t && t <= e.high),
                _ => None,
            },
            top,
            plan_hit: Some(plan_hit.unwrap_or(false)),
            games_to_detection: p
                .checkpoints
                .iter()
                .find(|c| c.rank_of(key).is_some_and(|r| r <= 2))
                .map(|c| c.games),
            note: note(last, key),
            benchmark_players: last.benchmark_players,
        });
    }
    let flawed: Vec<&Case> = cases.iter().filter(|c| c.flaw.is_some()).collect();
    let share = |f: &dyn Fn(&Case) -> bool| {
        if flawed.is_empty() {
            0.0
        } else {
            flawed.iter().filter(|c| f(c)).count() as f64 / flawed.len() as f64
        }
    };
    let recall_at_2 = share(&|c| c.target_rank.is_some_and(|r| r <= 2));
    let recall_at_1 = share(&|c| c.target_rank == Some(1));
    let plan_hit_rate = share(&|c| c.plan_hit == Some(true));
    let clean: Vec<&Case> = cases.iter().filter(|c| c.flaw.is_none()).collect();
    let clean_top1_false_alarms = clean.iter().filter(|c| !c.top.is_empty()).count();
    let with_top1 = cases.iter().filter(|c| !c.top.is_empty()).count();
    let right_top1 = flawed.iter().filter(|c| c.target_rank == Some(1)).count();
    let top1_precision = (with_top1 > 0).then(|| right_top1 as f64 / with_top1 as f64);
    let detected: Vec<f64> = flawed
        .iter()
        .filter_map(|c| c.games_to_detection.map(|g| g as f64))
        .collect();
    let agreement: Vec<f64> = players
        .iter()
        .filter_map(|p| Some(jaccard(&p.last()?.top(2), &p.perturbed.as_ref()?.top(2))))
        .collect();
    let cov = mean(
        &coverage
            .iter()
            .map(|b| if *b { 1.0 } else { 0.0 })
            .collect::<Vec<_>>(),
    );
    let metrics = Metrics {
        recall_at_2,
        recall_at_1,
        top1_precision,
        clean_top1_false_alarms,
        clean_players: clean.len(),
        coverage: cov,
        coverage_samples: coverage.len(),
        plan_hit_rate,
        mean_games_to_detection: mean(&detected),
        deterministic: players.iter().all(|p| p.deterministic),
        top2_agreement: mean(&agreement),
    };
    let t = THRESHOLDS;
    let recall = metrics.recall_at_2 >= t.recall_at_2_min;
    let clean_ok = metrics.clean_top1_false_alarms <= t.clean_top1_false_alarms_max;
    let coverage_ok = cov.is_some_and(|c| (t.coverage_min..=t.coverage_max).contains(&c));
    let u = units(players);
    let kpis = kpi::kpis(&u, metrics.deterministic);
    let km_curves = kpi::km_curves(&u);
    Scorecard {
        suite: suite.to_owned(),
        nodes,
        metrics,
        thresholds: t,
        verdicts: Verdicts {
            recall,
            clean: clean_ok,
            coverage: coverage_ok,
            pass: recall && clean_ok && coverage_ok,
        },
        cases,
        kpis,
        km_curves,
    }
}

fn pct(x: f64) -> String {
    format!("{:.0}%", x * 100.0)
}

fn opt<T: std::fmt::Display>(x: Option<T>) -> String {
    x.map_or("–".to_owned(), |v| v.to_string())
}

fn yes(b: bool) -> &'static str {
    if b { "pass" } else { "fail" }
}

/// The Markdown scorecard.
pub fn markdown(s: &Scorecard) -> String {
    let m = &s.metrics;
    let t = &s.thresholds;
    let mut out = String::new();
    out.push_str(&format!(
        "# Benchmark scorecard: {} suite\n\nStockfish at {} nodes per search. Tier 1: every number here is synthetic, measured on simulated players with injected flaws, not on real ones. Method: `docs/benchmark.md`; what the KPIs mean: ADR 0119.\n\n",
        s.suite, s.nodes
    ));
    out.push_str("| Score | Value | Threshold | Result |\n|---|---|---|---|\n");
    out.push_str(&format!(
        "| (a) Flaw in the top 2 (recall) | {} | at least {} | {} |\n",
        pct(m.recall_at_2),
        pct(t.recall_at_2_min),
        yes(s.verdicts.recall)
    ));
    out.push_str(&format!(
        "| (a) Flaw ranked first | {} | – | – |\n",
        pct(m.recall_at_1)
    ));
    out.push_str(&format!(
        "| (a) Top-1 precision | {} | – | – |\n",
        m.top1_precision.map_or("–".to_owned(), pct)
    ));
    out.push_str(&format!(
        "| (a) Clean controls with a rank-1 weakness | {} of {} | at most {} | {} |\n",
        m.clean_top1_false_alarms,
        m.clean_players,
        t.clean_top1_false_alarms_max,
        yes(s.verdicts.clean)
    ));
    out.push_str(&format!(
        "| (b) 80% intervals holding the true rate | {} ({} intervals) | {} to {} | {} |\n",
        m.coverage.map_or("–".to_owned(), pct),
        m.coverage_samples,
        pct(t.coverage_min),
        pct(t.coverage_max),
        yes(s.verdicts.coverage)
    ));
    out.push_str(&format!(
        "| (c) Top weakness's drills train the flaw (not a KPI) | {} | – | – |\n",
        pct(m.plan_hit_rate)
    ));
    out.push_str(&format!(
        "| (d) Mean games to a top-2 detection (deprecated: the KM median below) | {} | – | – |\n",
        m.mean_games_to_detection
            .map_or("–".to_owned(), |g| format!("{g:.0}"))
    ));
    out.push_str(&format!(
        "| (e) Same data, same weaknesses | {} | – | – |\n",
        if m.deterministic { "yes" } else { "no" }
    ));
    out.push_str(&format!(
        "| (e) Top-2 overlap without 10% of the games | {} | – | – |\n\n",
        m.top2_agreement.map_or("–".to_owned(), pct)
    ));
    out.push_str(&format!(
        "Thresholds: {}.\n\n",
        if s.verdicts.pass {
            "all met"
        } else {
            "not all met"
        }
    ));
    if !s.kpis.is_empty() {
        out.push_str(&kpi_table(&s.kpis));
    }
    out.push_str("| Player | Flaw (rate) | Target | True rate | Estimate [80%] | Rank | Top 3 | Drills hit | Games to detect | Note |\n|---|---|---|---|---|---|---|---|---|---|\n");
    for c in &s.cases {
        let flaw = match (&c.flaw, c.rate) {
            (Some(f), Some(r)) => format!("{f} ({r})"),
            _ => "clean".to_owned(),
        };
        let est = c.target_estimate.map_or("–".to_owned(), |[m, l, h]| {
            format!("{:.3} [{:.3}, {:.3}]", m, l, h)
        });
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            c.player,
            flaw,
            c.target.clone().unwrap_or_else(|| "–".to_owned()),
            c.truth.map_or("–".to_owned(), |t| format!("{t:.3}")),
            est,
            opt(c.target_rank),
            if c.top.is_empty() {
                "none".to_owned()
            } else {
                c.top.join(", ")
            },
            c.plan_hit.map_or("–", |b| if b { "yes" } else { "no" }),
            opt(c.games_to_detection),
            c.note.clone().unwrap_or_default(),
        ));
    }
    out
}

/// The KPI table of the scorecard: each KPI with its 80% interval and n, labelled synthetic.
fn kpi_table(k: &BTreeMap<String, Kpi>) -> String {
    let mut out = String::from(
        "## KPIs (synthetic, ADR 0119)\n\n| KPI | Tier | Value | 80% interval | n | Note |\n|---|---|---|---|---|---|\n",
    );
    for (name, m) in k {
        let digits = if name.starts_with("km_median_games") || name.starts_with("flaw_types") {
            0
        } else {
            3
        };
        let v = m.value.map_or("–".to_owned(), |x| format!("{x:.digits$}"));
        let iv = m.interval.map_or("–".to_owned(), |[l, h]| {
            format!("[{l:.digits$}, {h:.digits$}]")
        });
        out.push_str(&format!(
            "| {name} | synthetic | {v} | {iv} | {} | {} |\n",
            m.n,
            m.note.clone().unwrap_or_default()
        ));
    }
    out.push('\n');
    out
}

/// A committed baseline: the scores of a reference run, the allowed drop, and the known gaps.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    pub suite: String,
    pub metrics: Metrics,
    pub tolerance: Tolerance,
    #[serde(default)]
    pub known_gaps: Vec<KnownGap>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tolerance {
    pub recall_at_2: f64,
    pub top1_precision: f64,
    pub plan_hit_rate: f64,
    /// Allowed growth of the distance between coverage and 0.8.
    pub coverage: f64,
    pub top2_agreement: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KnownGap {
    pub player: String,
    pub issue: String,
    pub note: String,
}

/// Regressions of `now` against the baseline (empty: no regression).
pub fn regressions(b: &Baseline, now: &Metrics) -> Vec<String> {
    let mut out = Vec::new();
    let t = &b.tolerance;
    let base = &b.metrics;
    let fell = |name: &str, was: f64, is: f64, tol: f64, out: &mut Vec<String>| {
        if is < was - tol - 1e-9 {
            out.push(format!(
                "{name} fell from {was:.3} to {is:.3} (tolerance {tol})"
            ));
        }
    };
    fell(
        "recall_at_2",
        base.recall_at_2,
        now.recall_at_2,
        t.recall_at_2,
        &mut out,
    );
    if let (Some(was), Some(is)) = (base.top1_precision, now.top1_precision) {
        fell("top1_precision", was, is, t.top1_precision, &mut out);
    }
    fell(
        "plan_hit_rate",
        base.plan_hit_rate,
        now.plan_hit_rate,
        t.plan_hit_rate,
        &mut out,
    );
    if let (Some(was), Some(is)) = (base.top2_agreement, now.top2_agreement) {
        fell("top2_agreement", was, is, t.top2_agreement, &mut out);
    }
    match (base.coverage, now.coverage) {
        (Some(was), Some(is)) if (is - 0.8).abs() > (was - 0.8).abs() + t.coverage + 1e-9 => {
            out.push(format!(
                "coverage moved from {was:.3} to {is:.3}, further from 0.8 (tolerance {})",
                t.coverage
            ));
        }
        (Some(_), None) => out.push("coverage is no longer computed".to_owned()),
        _ => {}
    }
    if now.clean_top1_false_alarms > base.clean_top1_false_alarms {
        out.push(format!(
            "clean-control false alarms rose from {} to {}",
            base.clean_top1_false_alarms, now.clean_top1_false_alarms
        ));
    }
    if base.deterministic && !now.deterministic {
        out.push("the same data no longer gives the same weaknesses".to_owned());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranked(key: &str, rank: u32, drills: &[&str]) -> RankedItem {
        RankedItem {
            key: key.to_owned(),
            rank,
            priority: 1.0,
            confidence: 0.9,
            posterior_rate: 0.1,
            interval_low: 0.05,
            interval_high: 0.15,
            band_median: 0.01,
            events: 5,
            opportunities: 50,
            drills: drills.iter().map(|d| (*d).to_owned()).collect(),
        }
    }

    fn est(key: &str, events: f64, n: f64, low: f64, high: f64) -> EstimateRow {
        EstimateRow {
            key: key.to_owned(),
            events,
            opportunities: n,
            mean: (low + high) / 2.0,
            low,
            high,
            priority: 0.5,
            band_median: Some(0.01),
            confidence: 0.9,
        }
    }

    fn player(
        name: &str,
        flaw: Option<(&str, f64, &str)>,
        cps: Vec<Checkpoint>,
        tally: Tally,
    ) -> PlayerResult {
        PlayerResult {
            name: name.to_owned(),
            flaw: flaw.map(|(id, rate, key)| FlawInfo {
                id: id.to_owned(),
                rate,
                target_key: key.to_owned(),
                unit: Unit::Move,
                expected_drills: vec!["mate_patterns".to_owned()],
            }),
            tally,
            generation: GenStats::default(),
            perturbed: cps.last().cloned(),
            checkpoints: cps,
            deterministic: true,
            variant: None,
        }
    }

    fn all(n: u64) -> Tally {
        Tally {
            opportunities: n,
            armed: n,
            attempts: n,
            successes: n,
        }
    }

    #[test]
    fn a_detected_flaw_with_a_calibrated_interval_passes() {
        let cp = |games| Checkpoint {
            games,
            ranked: vec![ranked(
                "W4.missed",
                1,
                &["mate_patterns", "own_game_analysis"],
            )],
            estimates: vec![est("W4.missed", 10.0, 20.0, 0.4, 0.6)],
            benchmark_players: Some(12),
        };
        let clean = Checkpoint {
            games: 60,
            ranked: vec![],
            estimates: vec![est("W4.missed", 0.0, 10.0, 0.0, 0.1)],
            benchmark_players: Some(12),
        };
        let players = vec![
            player(
                "m",
                Some(("miss_mate", 0.5, "W4.missed")),
                vec![cp(30), cp(60)],
                all(20),
            ),
            player("c", None, vec![clean], Tally::default()),
        ];
        let s = score("smoke", 1000, &players);
        assert_eq!(s.metrics.recall_at_2, 1.0);
        assert_eq!(s.metrics.top1_precision, Some(1.0));
        assert_eq!(s.metrics.clean_top1_false_alarms, 0);
        assert_eq!(s.metrics.coverage, Some(1.0));
        assert_eq!(s.metrics.coverage_samples, 2);
        assert_eq!(s.metrics.plan_hit_rate, 1.0);
        assert_eq!(s.metrics.mean_games_to_detection, Some(30.0));
        assert_eq!(s.metrics.top2_agreement, Some(1.0));
        // Coverage 100% is outside [0.65, 0.95]: intervals that are too wide also fail.
        assert!(!s.verdicts.coverage);
        assert!(s.verdicts.recall && s.verdicts.clean);
        let md = markdown(&s);
        assert!(
            md.contains("| (a) Flaw in the top 2 (recall) | 100% |"),
            "{md}"
        );
    }

    #[test]
    fn a_flaw_below_the_ranking_floor_says_not_enough_evidence() {
        let mut e = est("W9b", 11.0, 846.0, 0.0075, 0.018);
        e.priority = 0.0;
        let cp = Checkpoint {
            games: 80,
            ranked: vec![],
            estimates: vec![e],
            benchmark_players: Some(16),
        };
        let players = vec![player(
            "l",
            Some(("low_clock_blunder", 0.25, "W9b")),
            vec![cp],
            all(100),
        )];
        let s = score("smoke", 1000, &players);
        assert_eq!(
            s.cases[0].note.as_deref(),
            Some("not ranked: not enough evidence yet")
        );
    }

    #[test]
    fn an_unranked_flaw_says_why_and_a_clean_top1_is_a_false_alarm() {
        let mut e = est("W1", 4.0, 400.0, 0.005, 0.02);
        e.band_median = None;
        e.priority = 0.0;
        let cp = Checkpoint {
            games: 80,
            ranked: vec![ranked("W9b", 1, &["blitz_safety"])],
            estimates: vec![e],
            benchmark_players: Some(14),
        };
        let players = vec![
            player("h", Some(("hang", 0.05, "W1")), vec![cp.clone()], all(100)),
            player("c", None, vec![cp], Tally::default()),
        ];
        let s = score("smoke", 1000, &players);
        assert_eq!(s.metrics.recall_at_2, 0.0);
        assert_eq!(s.metrics.clean_top1_false_alarms, 1);
        assert_eq!(s.metrics.top1_precision, Some(0.0));
        assert_eq!(
            s.cases[0].note.as_deref(),
            Some("not ranked: no peer benchmark for this key")
        );
        assert_eq!(s.cases[0].plan_hit, Some(false));
        assert!(!s.verdicts.pass);
    }

    #[test]
    fn regressions_respect_the_tolerance() {
        let m = Metrics {
            recall_at_2: 0.6,
            recall_at_1: 0.4,
            top1_precision: Some(0.5),
            clean_top1_false_alarms: 0,
            clean_players: 1,
            coverage: Some(0.7),
            coverage_samples: 20,
            plan_hit_rate: 0.4,
            mean_games_to_detection: Some(40.0),
            deterministic: true,
            top2_agreement: Some(0.8),
        };
        let b = Baseline {
            suite: "smoke".to_owned(),
            metrics: m.clone(),
            tolerance: Tolerance {
                recall_at_2: 0.2,
                top1_precision: 0.2,
                plan_hit_rate: 0.2,
                coverage: 0.15,
                top2_agreement: 0.25,
            },
            known_gaps: vec![],
        };
        assert!(regressions(&b, &m).is_empty());
        let mut worse = m.clone();
        worse.recall_at_2 = 0.2;
        worse.clean_top1_false_alarms = 1;
        worse.coverage = Some(0.4);
        worse.deterministic = false;
        let r = regressions(&b, &worse);
        assert_eq!(r.len(), 4, "{r:?}");
        let mut within = m;
        within.recall_at_2 = 0.4;
        within.coverage = Some(0.9);
        assert!(regressions(&b, &within).is_empty());
    }

    #[test]
    fn the_kpis_read_the_pooled_metrics_unchanged() {
        let cp = |games, rank: Option<u32>| Checkpoint {
            games,
            ranked: rank
                .map(|r| vec![ranked("W4.missed", r, &["mate_patterns"])])
                .unwrap_or_default(),
            estimates: vec![est("W4.missed", 10.0, 20.0, 0.4, 0.6)],
            benchmark_players: Some(12),
        };
        let players = vec![
            player(
                "a",
                Some(("miss_mate", 0.5, "W4.missed")),
                vec![cp(30, None), cp(60, Some(1))],
                all(20),
            ),
            player(
                "b",
                Some(("miss_mate", 0.5, "W4.missed")),
                vec![cp(30, None), cp(60, None)],
                all(20),
            ),
            player("c", None, vec![cp(60, Some(1))], Tally::default()),
        ];
        let s = score("smoke", 1000, &players);
        let k = &s.kpis;
        assert_eq!(k["recall_at_2"].value, Some(s.metrics.recall_at_2));
        assert_eq!(k["coverage"].value, s.metrics.coverage);
        assert_eq!(k["coverage"].n, s.metrics.coverage_samples);
        assert_eq!(k["plan_hit_rate"].value, Some(s.metrics.plan_hit_rate));
        assert_eq!(k["top2_jaccard"].value, s.metrics.top2_agreement);
        assert_eq!(
            k["clean_false_weakness"].value.unwrap() * s.metrics.clean_players as f64,
            s.metrics.clean_top1_false_alarms as f64
        );
        // One detected at 60, one censored at 60: the median is 60, the old mean 60 too.
        assert_eq!(k["km_median_games"].value, Some(60.0));
        assert_eq!(s.metrics.mean_games_to_detection, Some(60.0));
        assert_eq!(s.km_curves["all"].len(), 1);
        let md = markdown(&s);
        assert!(md.contains("every number here is synthetic"), "{md}");
        assert!(
            md.contains("| recall_at_2_macro | synthetic | 0.500 |"),
            "{md}"
        );
    }

    /// Plan hit is not a KPI (ADR 0119): with a drill for every rankable key, the top weakness
    /// always has drills, and a flaw ranked first gets its own. This test is what replaces it.
    #[test]
    fn every_rankable_key_has_a_drill() {
        use ai::output::drills::drills_for;
        for k in profile_core::constants::RANKABLE {
            let key = k.as_str();
            let (id, variant) = match key.split_once('.') {
                Some((i, v)) => (i, Some(v)),
                None => (key, None),
            };
            let drills = drills_for(id, variant);
            assert!(
                drills.iter().any(|d| d.as_str() != "own_game_analysis"),
                "{key} has no drill of its own: {drills:?}"
            );
        }
        for (id, rate) in [
            ("hang", 0.05),
            ("miss_mate", 0.5),
            ("opening_blunder", 0.3),
            ("low_clock_blunder", 0.25),
            ("no_convert", 0.5),
        ] {
            let f = crate::flaws::flaw(id, rate).expect("a flaw");
            let key = f.target_key();
            let (tid, variant) = match key.split_once('.') {
                Some((i, v)) => (i, Some(v)),
                None => (key, None),
            };
            let drills: Vec<&str> = drills_for(tid, variant)
                .iter()
                .map(|d| d.as_str())
                .collect();
            assert!(
                f.expected_drills().iter().any(|d| drills.contains(d)),
                "{id}: none of {:?} in {drills:?}",
                f.expected_drills()
            );
        }
    }

    #[test]
    fn jaccard_of_top_lists() {
        let s = |v: &[&str]| v.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
        assert_eq!(jaccard(&s(&["a", "b"]), &s(&["b", "a"])), 1.0);
        assert!((jaccard(&s(&["a", "b"]), &s(&["a", "c"])) - 1.0 / 3.0).abs() < 1e-12);
        assert_eq!(jaccard(&[], &[]), 1.0);
    }
}
