//! The report-section output contract (port of `output/report-schema.ts`): what Claude returns
//! for each section call, and the validated shapes the web shows.
//!
//! One envelope serves every section call, so the output format and the cached prefix are
//! identical across the five calls (ADR-R3-4). Every object is strict, and every envelope part is
//! present-but-nullable: the key must be on the wire, `null` when it is not the section asked for.
//! The schema goes through `contract_transform` (`schema::api_schema`); bounds the structured
//! output subset cannot carry (text length, array sizes) are checked by `schema::check_report`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::codes::{Citation, SectionKey};
use super::drills::DrillType;
use crate::input::types::{AreaId, Verdict, nullable};
use crate::segments::Segment;

/// TS `Item`: narrative text (facts only as tokens, ADR-AI-8) and the positions it cites.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Item {
    pub text: String,
    pub citations: Vec<Citation>,
}

/// An overview strength: an [`Item`] plus its area.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StrengthItem {
    pub text: String,
    pub citations: Vec<Citation>,
    pub area: AreaId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverviewSection {
    pub headline: Item,
    pub summary: Item,
    /// At most 4.
    pub strengths: Vec<StrengthItem>,
}

/// A weakness: an [`Item`] plus the computed weakness it narrates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WeaknessItem {
    pub text: String,
    pub citations: Vec<Citation>,
    pub weakness_id: String,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WeaknessesSection {
    pub items: Vec<WeaknessItem>,
}

/// An opening: an [`Item`] plus the line and its computed verdict.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpeningItem {
    pub text: String,
    pub citations: Vec<Citation>,
    pub line_key: String,
    pub verdict: Verdict,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpeningsSection {
    pub items: Vec<OpeningItem>,
}

/// A drill picked from `drills_for` of the focus.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Drill {
    pub drill_type: DrillType,
    pub rationale: String,
}

/// One computed focus of the climb plan (ADR-AI-7).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Focus {
    pub weakness_id: String,
    pub why: Item,
    /// 1 to 3.
    pub drills: Vec<Drill>,
    pub habit: Item,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClimbPlanSection {
    pub intro: Item,
    /// At most 3.
    pub focuses: Vec<Focus>,
    pub weekly_rhythm: Item,
}

/// An area text: an [`Item`] plus its area.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AreaItem {
    pub text: String,
    pub citations: Vec<Citation>,
    pub area: AreaId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AreasSection {
    pub items: Vec<AreaItem>,
}

/// TS `ReportSectionEnvelope`: one schema for every section call.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportSectionEnvelope {
    pub section: SectionKey,
    #[serde(deserialize_with = "nullable")]
    pub overview: Option<OverviewSection>,
    #[serde(deserialize_with = "nullable")]
    pub weaknesses: Option<WeaknessesSection>,
    #[serde(deserialize_with = "nullable")]
    pub openings: Option<OpeningsSection>,
    #[serde(deserialize_with = "nullable")]
    pub climb_plan: Option<ClimbPlanSection>,
    #[serde(deserialize_with = "nullable")]
    pub areas: Option<AreasSection>,
}

// --- Validated shapes (TS `ValidatedSection`): a dropped singleton item becomes `None`, dropped
// array entries are removed. Items carry the server-resolved `segments` (ADR-AI-8). ---

/// An item that passed validation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidatedItem {
    pub text: String,
    pub citations: Vec<Citation>,
    pub segments: Vec<Segment>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidatedStrengthItem {
    pub text: String,
    pub citations: Vec<Citation>,
    pub segments: Vec<Segment>,
    pub area: AreaId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidatedOverviewSection {
    #[serde(deserialize_with = "nullable")]
    pub headline: Option<ValidatedItem>,
    #[serde(deserialize_with = "nullable")]
    pub summary: Option<ValidatedItem>,
    pub strengths: Vec<ValidatedStrengthItem>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidatedWeaknessItem {
    pub text: String,
    pub citations: Vec<Citation>,
    pub segments: Vec<Segment>,
    pub weakness_id: String,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidatedWeaknessesSection {
    pub items: Vec<ValidatedWeaknessItem>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidatedOpeningItem {
    pub text: String,
    pub citations: Vec<Citation>,
    pub segments: Vec<Segment>,
    pub line_key: String,
    pub verdict: Verdict,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidatedOpeningsSection {
    pub items: Vec<ValidatedOpeningItem>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidatedFocus {
    pub weakness_id: String,
    #[serde(deserialize_with = "nullable")]
    pub why: Option<ValidatedItem>,
    pub drills: Vec<Drill>,
    #[serde(deserialize_with = "nullable")]
    pub habit: Option<ValidatedItem>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidatedClimbPlanSection {
    #[serde(deserialize_with = "nullable")]
    pub intro: Option<ValidatedItem>,
    pub focuses: Vec<ValidatedFocus>,
    #[serde(deserialize_with = "nullable")]
    pub weekly_rhythm: Option<ValidatedItem>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidatedAreaItem {
    pub text: String,
    pub citations: Vec<Citation>,
    pub segments: Vec<Segment>,
    pub area: AreaId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidatedAreasSection {
    pub items: Vec<ValidatedAreaItem>,
}

/// One validated section, by [`SectionKey`] (`{"overview": {...}}` on the wire).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValidatedSection {
    Overview(ValidatedOverviewSection),
    Weaknesses(ValidatedWeaknessesSection),
    Openings(ValidatedOpeningsSection),
    ClimbPlan(ValidatedClimbPlanSection),
    Areas(ValidatedAreasSection),
}

impl ValidatedSection {
    pub fn key(&self) -> SectionKey {
        match self {
            ValidatedSection::Overview(_) => SectionKey::Overview,
            ValidatedSection::Weaknesses(_) => SectionKey::Weaknesses,
            ValidatedSection::Openings(_) => SectionKey::Openings,
            ValidatedSection::ClimbPlan(_) => SectionKey::ClimbPlan,
            ValidatedSection::Areas(_) => SectionKey::Areas,
        }
    }
}
