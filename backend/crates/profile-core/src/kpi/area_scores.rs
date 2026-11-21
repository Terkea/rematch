//! Area scores (R2 Task 15, TS M2 Task 19, ADR-R2-7).
//!
//! An area's score is the mean band percentile of its components. A component counts when its
//! snapshot has a value, `n >= min_n`, and the band reference has a stat for it. The area is
//! sufficient when at least half of its components count. Mindset has no benchmark yet, so it is
//! never scored.

use coaching_input::AreaScore;

use crate::kpi::catalogue::kpi_definition;
use crate::kpi::snapshots::KpiValue;
use crate::stats::quantiles::{QuartileRef, percentile_from_quartiles};
use crate::types::{AREAS, Area, BandReference, BaseMetric, MetricId, WeaknessKey};

use BaseMetric as B;
use MetricId::{Base, Weakness};
use WeaknessKey as W;

const OPENINGS: [MetricId; 4] = [
    Base(B::AccuracyOpening),
    Base(B::EvalAfterOpening),
    Weakness(W::W7Traps),
    Weakness(W::W7Prep),
];
const MIDDLEGAME: [MetricId; 4] = [
    Base(B::AccuracyMiddlegame),
    Weakness(W::W10),
    Weakness(W::W5),
    Weakness(W::W6),
];
const ENDGAME: [MetricId; 3] = [
    Base(B::AccuracyEndgame),
    Weakness(W::W8Basics),
    Weakness(W::W8Advanced),
];
const TACTICS: [MetricId; 4] = [
    Weakness(W::W3Shallow),
    Weakness(W::W3Deep),
    Weakness(W::W4Missed),
    Base(B::MissedPunishRate),
];
const CALCULATION: [MetricId; 5] = [
    Weakness(W::W1),
    Weakness(W::W2),
    Weakness(W::W11),
    Weakness(W::W4Allowed),
    Base(B::BlundersPerGame),
];
const TIME: [MetricId; 4] = [
    Weakness(W::W9a),
    Weakness(W::W9b),
    Weakness(W::W9c),
    Base(B::CriticalTimeRatio),
];
const MINDSET: [MetricId; 2] = [Base(B::ScoreAfterLoss), Base(B::SessionAccuracyDecline)];

/// The components of an area (TS M2 `AREA_COMPONENTS`), in order; the first is the one whose
/// shortfall `need_more` reports.
pub fn area_components(area: Area) -> &'static [MetricId] {
    match area {
        Area::Openings => &OPENINGS,
        Area::Middlegame => &MIDDLEGAME,
        Area::Endgame => &ENDGAME,
        Area::Tactics => &TACTICS,
        Area::Calculation => &CALCULATION,
        Area::Time => &TIME,
        Area::Mindset => &MINDSET,
    }
}

/// Areas without a band benchmark in this milestone.
fn has_benchmark(area: Area) -> bool {
    area != Area::Mindset
}

/// One score per area, in contract order ([`AREAS`]), from one window's snapshots (the first
/// row per metric is used). The percentile uses `value_adjusted` when present, else `value`.
///
/// - Sufficient: `score` = mean percentile of the counting components (each in `[1, 99]`),
///   `n` = the smallest `n` among them, `need_more = None`.
/// - Otherwise `score = None`; `n` and `need_more = min_n - n` come from the first component
///   short of its minimum (a missing row has `n = 0`). When no component is short (the gap is a
///   missing benchmark, which more games cannot fix), `n` is the first component's and
///   `need_more = None`.
/// - Mindset: `score None, sufficient false, need_more None`, `n` of its first component.
pub fn compute_area_scores(kpis: &[KpiValue], reference: Option<&BandReference>) -> Vec<AreaScore> {
    AREAS
        .iter()
        .map(|&area| area_score(area, kpis, reference))
        .collect()
}

fn area_score(area: Area, kpis: &[KpiValue], reference: Option<&BandReference>) -> AreaScore {
    let comps = area_components(area);
    let row_of = |m: MetricId| kpis.iter().find(|k| k.metric == m);
    let n_of = |m: MetricId| row_of(m).map_or(0, |r| r.n);
    let first_n = comps.first().map_or(0, |&m| n_of(m));
    if !has_benchmark(area) {
        return AreaScore {
            area,
            score: None,
            n: first_n,
            sufficient: false,
            need_more: None,
        };
    }
    let mut percentiles = Vec::new();
    let mut min_counted_n = u32::MAX;
    for &m in comps {
        if let Some((p, n)) = component_percentile(m, row_of(m), reference) {
            percentiles.push(p);
            min_counted_n = min_counted_n.min(n);
        }
    }
    let sufficient = !percentiles.is_empty() && percentiles.len() * 2 >= comps.len();
    if sufficient {
        let score = percentiles.iter().sum::<f64>() / percentiles.len() as f64;
        if score.is_finite() {
            return AreaScore {
                area,
                score: Some(score),
                n: min_counted_n,
                sufficient: true,
                need_more: None,
            };
        }
    }
    let short = comps.iter().find_map(|&m| {
        let min_n = kpi_definition(m)?.min_n;
        let n = n_of(m);
        (n < min_n).then(|| (n, min_n - n))
    });
    let (n, need_more) = match short {
        Some((n, need)) => (n, Some(need)),
        None => (first_n, None),
    };
    AreaScore {
        area,
        score: None,
        n,
        sufficient: false,
        need_more,
    }
}

/// The band percentile of a component and its `n`, when it counts.
fn component_percentile(
    m: MetricId,
    row: Option<&KpiValue>,
    reference: Option<&BandReference>,
) -> Option<(f64, u32)> {
    let def = kpi_definition(m)?;
    let row = row?;
    if row.n < def.min_n {
        return None;
    }
    let value = row.value_adjusted.or(row.value)?;
    let stat = reference?.get(&m)?;
    let q = QuartileRef {
        p25: stat.p25,
        p40: stat.p40,
        median: stat.median,
        p75: stat.p75,
    };
    let finite = [value, q.p25, q.p40, q.median, q.p75]
        .iter()
        .all(|x| x.is_finite());
    if !finite {
        return None;
    }
    let p = percentile_from_quartiles(value, &q, def.higher_is_better?);
    p.is_finite().then_some((p, row.n))
}
