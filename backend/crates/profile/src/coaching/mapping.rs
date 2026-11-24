//! Internal-to-contract mapping (R2 Task 34, global constraints "Contract mapping to M3",
//! rulings R-2, R-3, R-6, R-7, R-8).
//!
//! Every function here is pure. A value that cannot be stated honestly in the contract (NaN, a
//! missing count, a broken interval) makes the item `None`; it is never patched into range by
//! widening anything. Clamping is only used where a ruling says so (R-2 unit ranges) and only
//! narrows.

use chess_core::{EngineScore, TimeClass};
use coaching_input::limits::{MAX_CP, MAX_MATE, MAX_PLY};
use coaching_input::{
    Alternative, CandidateCause, ExemplarPosition, LIMITS, Metric, MetricUnit, OpeningVerdict,
    Platform, PlayerColor, RankedWeakness, Score,
};
use profile_core::constants::{weakness_area, weakness_label};
use profile_core::kpi::catalogue::{KpiDefinition, kpi_definition};
use profile_core::kpi::snapshots::{KpiValue, clamp_to_unit};
use profile_core::openings::lines::LineSummary;
use profile_core::stats::priority::{EstimateReason, WeaknessEstimate};
use profile_core::stats::quantiles::{QuartileRef, percentile_from_quartiles};
use profile_core::types::{
    BandReference, EndgameType, MetricId, MotifTag, PlatformId, SeriesKey, WeaknessState,
};
use sha2::{Digest, Sha256};

/// Slug budget of a contract line key (R-7), hash suffix included.
pub const LINE_SLUG_MAX_CHARS: usize = 40;
/// Hex digits of `sha256(full slug)` appended to a cut slug (R-7).
pub const LINE_SLUG_HASH_CHARS: usize = 6;

/// `coaching_input::Platform` of a profile platform (T2 hand-off; the orphan rule keeps a
/// `From` impl out of this crate).
pub fn contract_platform(p: PlatformId) -> Platform {
    match p {
        PlatformId::Lichess => Platform::Lichess,
        PlatformId::Chesscom => Platform::Chesscom,
    }
}

/// `TimelineChange.weaknessId` of a series (T2 hand-off): a weakness key gives its contract
/// id, `motif.missed:*` is a missed own tactic (W3), `motif.allowed:*` hung material (W1);
/// opening and endgame series belong to no weakness.
pub fn weakness_id_of_series(key: &SeriesKey) -> Option<String> {
    match key {
        SeriesKey::Weakness(k) => Some(k.contract_id().0.to_owned()),
        SeriesKey::MotifMissed(_) => Some("W3".to_owned()),
        SeriesKey::MotifAllowed(_) => Some("W1".to_owned()),
        SeriesKey::Opening(_) | SeriesKey::Endgame(_) => None,
    }
}

/// The first `max` characters (Unicode code points, as zod counts them).
pub fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// R-7 line key: `"{color}:{slug}"`, with a slug over [`LINE_SLUG_MAX_CHARS`] cut at a `-`
/// boundary and suffixed with `-` and 6 hex digits of `sha256(full slug)` (the same rule as
/// Task 16's `line_key`, which already emits short keys; this guards legacy or foreign keys).
pub fn contract_line_key(key: &str) -> String {
    let (prefix, slug) = match key.split_once(':') {
        Some((p, s)) => (format!("{p}:"), s),
        None => (String::new(), key),
    };
    if slug.chars().count() <= LINE_SLUG_MAX_CHARS && key.chars().count() <= LIMITS.id_chars {
        return key.to_owned();
    }
    let budget = LINE_SLUG_MAX_CHARS - 1 - LINE_SLUG_HASH_CHARS;
    let chars: Vec<char> = slug.chars().collect();
    let head: String = chars.iter().take(budget).collect();
    let cut = if chars.get(budget) == Some(&'-') {
        head.as_str()
    } else {
        head.rsplit_once('-')
            .map_or(head.as_str(), |(kept, _)| kept)
    };
    let digest = hex::encode(Sha256::digest(slug.as_bytes()));
    let suffix = &digest[..LINE_SLUG_HASH_CHARS];
    let prefix = truncate_chars(&prefix, LIMITS.id_chars - LINE_SLUG_MAX_CHARS);
    format!("{prefix}{}-{suffix}", cut.trim_end_matches('-'))
}

/// A White-POV engine score in the contract's ranges (R-8).
pub fn contract_score(s: EngineScore) -> Score {
    match s {
        EngineScore::Cp(cp) => Score::Cp(cp.clamp(-MAX_CP, MAX_CP)),
        EngineScore::Mate(m) => Score::Mate(m.clamp(-MAX_MATE, MAX_MATE)),
    }
}

fn finite(x: f64) -> Option<f64> {
    x.is_finite().then_some(x)
}

fn in_unit(x: f64) -> Option<f64> {
    finite(x).filter(|v| (0.0..=1.0).contains(v))
}

fn unit_max(unit: MetricUnit) -> f64 {
    unit.max()
}

fn clamp_unit(x: f64, unit: MetricUnit) -> f64 {
    clamp_to_unit(x, Some(unit)).min(unit_max(unit))
}

/// R-2: a catalogue row with an area and a contract unit, and a value, becomes a `Metric`. The
/// value and interval are the raw ones clamped to the unit range (the interval only when both
/// ends are finite); `bandPercentile` comes from `value_adjusted` (else `value`) against the band
/// quartiles, `bandMedian` is the band median (both `None` without a usable band statistic or a
/// direction). `sufficient = n >= min_n`.
pub fn metric(
    def: &KpiDefinition,
    row: &KpiValue,
    reference: Option<&BandReference>,
) -> Option<Metric> {
    let area = def.area?;
    let unit = def.contract_unit?;
    let value = clamp_unit(finite(row.value?)?, unit);
    let (low, high) = match (row.ci_low.and_then(finite), row.ci_high.and_then(finite)) {
        (Some(lo), Some(hi)) if lo <= hi => {
            (Some(clamp_unit(lo, unit)), Some(clamp_unit(hi, unit)))
        }
        _ => (None, None),
    };
    let stat = reference.and_then(|r| r.get(&def.metric));
    let band_median = stat
        .and_then(|s| finite(s.median))
        .map(|m| clamp_unit(m, unit));
    let band_percentile = stat.and_then(|s| {
        let v = finite(row.value_adjusted.or(row.value)?)?;
        let q = QuartileRef {
            p25: s.p25,
            p40: s.p40,
            median: s.median,
            p75: s.p75,
        };
        finite(percentile_from_quartiles(v, &q, def.higher_is_better?)).map(|p| p.clamp(0.0, 100.0))
    });
    let sufficient = row.n >= def.min_n;
    Some(Metric {
        key: truncate_chars(&def.metric.to_string(), LIMITS.id_chars),
        area,
        label: truncate_chars(def.label, LIMITS.label_chars),
        value,
        unit,
        n: row.n,
        low,
        high,
        band_median,
        band_percentile,
        sufficient,
        need_more: (!sufficient).then(|| def.min_n - row.n),
    })
}

/// Whether an estimate may be claimed as a weakness (R-3 and the T12 ruling): a benchmark and
/// enough games (`reason = ok`, which also needs an in-window event), the minimum evidence, a
/// real event count, a measured excess over the band (`priority > 0`), and every contract
/// number finite.
pub fn claimable_weakness(e: &WeaknessEstimate) -> bool {
    e.reason == EstimateReason::Ok
        && e.min_evidence_met
        && e.k_unweighted > 0.0
        && e.priority > 0.0
        && e.priority.is_finite()
        && e.band_median.is_some()
        && e.percentile.is_some()
}

fn count(x: f64) -> Option<u32> {
    finite(x)
        .filter(|v| *v >= 0.0 && *v <= f64::from(u32::MAX))
        .map(|v| v.round() as u32)
}

/// A claimable estimate as a `RankedWeakness`; `None` when a contract number is unknown or its
/// interval does not hold the posterior (never widened to make it fit).
pub fn ranked_weakness(
    e: &WeaknessEstimate,
    state: WeaknessState,
    p_imp: Option<f64>,
    rank: u32,
    exemplars: Vec<ExemplarPosition>,
) -> Option<RankedWeakness> {
    if !claimable_weakness(e) {
        return None;
    }
    let (id, variant) = e.key.contract_id();
    let posterior = in_unit(e.posterior.mean)?;
    let low = in_unit(e.posterior.interval.low)?;
    let high = in_unit(e.posterior.interval.high)?;
    if !(low <= posterior && posterior <= high) {
        return None;
    }
    let events = count(e.k_unweighted)?;
    let opportunities = count(e.n_unweighted)?;
    if events > opportunities {
        return None;
    }
    Some(RankedWeakness {
        id: id.to_owned(),
        variant: variant.map(str::to_owned),
        label: weakness_label(e.key).to_owned(),
        area: weakness_area(e.key),
        rank,
        posterior_rate: posterior,
        interval_low: low,
        interval_high: high,
        band_median: in_unit(e.band_median?)?,
        band_percentile: finite(e.percentile?)?.clamp(0.0, 100.0),
        excess_epl_per_game: finite(e.excess)?.clamp(0.0, 100.0),
        confidence: in_unit(e.confidence)?,
        priority: finite(e.priority)?.clamp(0.0, 1000.0),
        state,
        p_imp: p_imp.and_then(in_unit),
        events,
        opportunities,
        exemplars: exemplars
            .into_iter()
            .take(LIMITS.exemplars_per_item)
            .collect(),
    })
}

fn valid_san(s: &str) -> bool {
    (1..=LIMITS.san_chars).contains(&s.chars().count())
}

/// SAN moves within the contract: at most 40, cut at the first invalid token.
fn moves_san(moves: &[String]) -> Vec<String> {
    moves
        .iter()
        .take_while(|m| valid_san(m))
        .take(LIMITS.moves_san)
        .cloned()
        .collect()
}

/// An opening line with a verdict as an `OpeningVerdict` (R-7, R-8). `None` without a verdict
/// (n < 8) or without an example (R3 T8: an item without an exemplar cannot be cited).
pub fn opening_verdict(s: &LineSummary, examples: Vec<ExemplarPosition>) -> Option<OpeningVerdict> {
    let verdict = s.verdict?;
    if examples.is_empty() {
        return None;
    }
    let name = truncate_chars(s.name.trim(), LIMITS.label_chars);
    if name.is_empty() {
        return None;
    }
    Some(OpeningVerdict {
        line_key: contract_line_key(&s.line_key),
        name,
        eco: s
            .eco
            .clone()
            .filter(|e| (1..=coaching_input::limits::ECO_MAX_CHARS).contains(&e.chars().count())),
        color: PlayerColor::from(s.color),
        side: s.kind,
        moves_san: moves_san(&s.moves_san),
        games: s.games,
        score: in_unit(s.score)?,
        performance_vs_expected: s
            .perf_vs_expected
            .and_then(finite)
            .map(|p| p.clamp(-1.0, 1.0)),
        avg_eval_at_opening_end: s
            .avg_eval_white_cp
            .map(|cp| contract_score(EngineScore::Cp(cp))),
        verdict,
        deviation_ply: s.deviation_ply.filter(|p| (1..=MAX_PLY).contains(p)),
        deviation_move_san: s.deviation_move_san.clone().filter(|m| valid_san(m)),
        alternatives: s
            .alternatives
            .iter()
            .filter(|a| valid_san(&a.san))
            .filter_map(|a| {
                // Coordinator ruling: the single alternative move. A stored PV line for an
                // alternative would be used instead, cut to 16 plies, but `AlternativeMove`
                // (Task 18) stores none, so there is nothing to cut.
                Some(Alternative {
                    name: truncate_chars(a.name.as_deref().unwrap_or(&a.san), LIMITS.label_chars),
                    moves_san: vec![a.san.clone()],
                    band_score: in_unit(a.band_score)?,
                    games: a.games,
                })
            })
            .take(LIMITS.alternatives)
            .collect(),
        examples: examples
            .into_iter()
            .take(LIMITS.examples_per_opening)
            .collect(),
    })
}

/// The KPI catalogue's name of a missed motif ("Missed forks"), as the pages show it.
fn motif_missed_label(t: MotifTag) -> &'static str {
    kpi_definition(MetricId::MotifMissRate(t)).map_or("Missed tactics", |d| d.label)
}

fn endgame_label(e: EndgameType) -> &'static str {
    match e {
        EndgameType::Pawn => "Pawn endgames",
        EndgameType::Rook => "Rook endgames",
        EndgameType::Minor => "Minor-piece endgames",
        EndgameType::Queen => "Queen endgames",
        EndgameType::Mixed => "Mixed endgames",
    }
}

/// `TimelineChange.label` of a series.
pub fn series_label(key: &SeriesKey, opening_name: Option<&str>) -> String {
    let label = match key {
        SeriesKey::Weakness(k) => weakness_label(*k).to_owned(),
        SeriesKey::MotifMissed(t) => motif_missed_label(*t).to_owned(),
        // The web's `seriesLabel`: the missed name with "Allowed" for "Missed".
        SeriesKey::MotifAllowed(t) => {
            let missed = motif_missed_label(*t);
            format!(
                "Allowed {}",
                missed.strip_prefix("Missed ").unwrap_or(missed)
            )
        }
        SeriesKey::Opening(line) => format!("Opening: {}", opening_name.unwrap_or(line)),
        SeriesKey::Endgame(e) => endgame_label(*e).to_owned(),
    };
    truncate_chars(&label, LIMITS.label_chars)
}

/// Stored cause hints as contract causes, each within its bounds, at most 6.
pub fn candidate_causes(stored: &serde_json::Value) -> Vec<CandidateCause> {
    let causes: Vec<CandidateCause> = serde_json::from_value(stored.clone()).unwrap_or_default();
    causes
        .into_iter()
        .filter(|c| {
            let ok = |v: &coaching_input::CauseValue| match v {
                coaching_input::CauseValue::Text(t) => t.chars().count() <= LIMITS.label_chars,
                coaching_input::CauseValue::Number(n) => n.is_finite(),
            };
            (1..=LIMITS.description_chars).contains(&c.description.chars().count())
                && ok(&c.before)
                && ok(&c.after)
        })
        .take(LIMITS.candidate_causes)
        .collect()
}

/// `timeControl`'s daily rule: no typical clock for daily games.
pub fn has_time_control(tc: TimeClass) -> bool {
    tc != TimeClass::Daily
}

#[cfg(test)]
mod tests {
    use super::*;
    use profile_core::types::WeaknessKey;

    /// Audit A2: the coach names a series the way the pages do (the web's `seriesLabel`).
    #[test]
    fn series_labels_match_the_pages() {
        assert_eq!(
            series_label(&SeriesKey::Weakness(WeaknessKey::W5), None),
            "Not winning won games"
        );
        assert_eq!(
            series_label(&SeriesKey::MotifMissed(MotifTag::Fork), None),
            "Missed forks"
        );
        assert_eq!(
            series_label(&SeriesKey::MotifAllowed(MotifTag::DiscoveredAttack), None),
            "Allowed discovered attacks"
        );
        // A hidden motif (EXP-0007, #122) has no KPI row: the generic name, as on the pages.
        assert_eq!(
            series_label(&SeriesKey::MotifAllowed(MotifTag::HangingPiece), None),
            "Allowed tactics"
        );
        assert_eq!(
            series_label(&SeriesKey::MotifMissed(MotifTag::Pin), None),
            "Missed tactics"
        );
        assert_eq!(
            series_label(&SeriesKey::MotifAllowed(MotifTag::MateIn1), None),
            "Allowed mates in one"
        );
    }
}
