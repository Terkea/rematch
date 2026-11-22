//! Strength selection (R2 Task 34, TS M2 Task 26 `strengths.ts`, controller ruling, R-1).
//!
//! Candidates are areas scoring at least [`AREA_STRENGTH_SCORE`] and weaknesses the player is
//! very likely better at than the band (`1 - confidence >= `[`WEAKNESS_STRENGTH_CONFIDENCE`]).
//! A strength is only claimed with evidence:
//! - a weakness candidate needs a claimable estimate (`reason = ok`, minimum evidence, at least
//!   one event; never from zero events, T12 ruling) and a band percentile;
//! - every strength needs at least one exemplar of the area's kind ([`area_kinds`]); an area
//!   without a kind (openings, time, mindset) has none, so it is never a strength.
//!
//! At most [`MAX_STRENGTHS`], by band percentile descending (unknown last, then label), each
//! with at most [`MAX_STRENGTH_EXEMPLARS`] exemplars. The contract `Strength` keeps exactly
//! `{area, label, metricKey, exemplars}` (R-1); the rest of [`StrengthCandidate`] is internal.

use coaching_input::{AreaId, ExemplarPosition, Strength};
use profile_core::types::{MetricId, StrengthKind, WeaknessKey};

use super::mapping::truncate_chars;

/// At most this many strengths.
pub const MAX_STRENGTHS: usize = 4;
/// At most this many exemplars per strength.
pub const MAX_STRENGTH_EXEMPLARS: usize = 3;
/// Area score (0..100) at or above which an area is a strength.
pub const AREA_STRENGTH_SCORE: f64 = 60.0;
/// `1 - confidence` at or above which a weakness is a strength.
pub const WEAKNESS_STRENGTH_CONFIDENCE: f64 = 0.8;

/// Where a strength candidate comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrengthSource {
    Area,
    Weakness(WeaknessKey),
}

/// A strength before selection.
#[derive(Clone, Debug, PartialEq)]
pub struct StrengthCandidate {
    pub area: AreaId,
    pub label: String,
    pub kind: StrengthSource,
    /// The supporting metric, only when it is in the input's `metrics`.
    pub metric_key: Option<MetricId>,
    /// 0..100, higher = stronger.
    pub band_percentile: Option<f64>,
    /// Observations behind the candidate (area `n`, or the weakness's opportunities).
    pub n: u32,
    pub exemplars: Vec<ExemplarPosition>,
}

/// The exemplar kinds of an area's strengths, and whether they must be in the endgame phase.
pub fn area_kinds(area: AreaId) -> (&'static [StrengthKind], bool) {
    match area {
        AreaId::Tactics => (&[StrengthKind::TacticFound], false),
        AreaId::Calculation => (&[StrengthKind::OnlyMoveFound], false),
        AreaId::Middlegame => (&[StrengthKind::Conversion, StrengthKind::Defence], false),
        AreaId::Endgame => (&[StrengthKind::Conversion], true),
        AreaId::Openings | AreaId::Time | AreaId::Mindset => (&[], false),
    }
}

/// The exemplar kinds that show a weakness is *not* a problem for the player, and whether they
/// must be in the endgame phase (T34 review m2: a weakness strength cites its own claim, never
/// only its area). Keys without a matching kind (hung material, missed threats, opening,
/// time and vision keys, positional drift) have none, so they are never a strength.
pub fn weakness_kinds(key: WeaknessKey) -> (&'static [StrengthKind], bool) {
    use WeaknessKey as W;
    match key {
        W::W3Shallow | W::W3Deep | W::W4Missed => (&[StrengthKind::TacticFound], false),
        W::W5 => (&[StrengthKind::Conversion], false),
        W::W6 => (&[StrengthKind::Defence], false),
        W::W8Basics | W::W8Advanced => (&[StrengthKind::Conversion], true),
        _ => (&[], false),
    }
}

/// The selected strengths (module docs).
pub fn select_strengths(candidates: Vec<StrengthCandidate>) -> Vec<Strength> {
    let mut v: Vec<StrengthCandidate> = candidates
        .into_iter()
        .filter(|c| !c.exemplars.is_empty() && !c.label.trim().is_empty())
        .collect();
    v.sort_by(|a, b| {
        let key = |c: &StrengthCandidate| c.band_percentile.filter(|p| p.is_finite());
        match (key(a), key(b)) {
            (Some(x), Some(y)) => y.total_cmp(&x),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
        .then_with(|| a.label.cmp(&b.label))
    });
    v.into_iter()
        .take(MAX_STRENGTHS)
        .map(|c| Strength {
            area: c.area,
            label: truncate_chars(&c.label, coaching_input::LIMITS.label_chars),
            metric_key: c.metric_key.map(|m| m.to_string()),
            exemplars: c
                .exemplars
                .into_iter()
                .take(MAX_STRENGTH_EXEMPLARS)
                .collect(),
        })
        .collect()
}
