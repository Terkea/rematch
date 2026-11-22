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
use super::schema::{
    DRILL_RATIONALE_MAX_CHARS, ITEM_TEXT_MAX_CHARS, MAX_DRILLS, MAX_FOCUSES, MAX_STRENGTHS,
    MIN_DRILLS, WEAKNESS_TITLE_MAX_CHARS, desc,
};
use crate::input::types::{AreaId, Verdict, nullable};
use crate::segments::Segment;

// The output types below carry `//` comments only: a doc comment would become a schema
// description the model reads. Model-facing text is set with `#[schemars(description = ...)]`
// from `schema::desc`, and `tests/output.rs` pins the exact set. Text and array bounds are
// declared with `#[schemars(length(...))]`, so `sanitize` turns them into SDK-style hints
// (`{minLength: 1, maxLength: 2000}`); `schema::report_bounds` enforces them.

// TS `Item`: narrative text (facts only as tokens, ADR-AI-8) and the positions it cites.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Item {
    #[schemars(description = desc::TEXT, length(min = 1, max = ITEM_TEXT_MAX_CHARS))]
    pub text: String,
    #[schemars(description = desc::CITATIONS)]
    pub citations: Vec<Citation>,
}

// An overview strength: an `Item` plus its area.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StrengthItem {
    #[schemars(description = desc::TEXT, length(min = 1, max = ITEM_TEXT_MAX_CHARS))]
    pub text: String,
    #[schemars(description = desc::CITATIONS)]
    pub citations: Vec<Citation>,
    #[schemars(description = desc::AREA)]
    pub area: AreaId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverviewSection {
    pub headline: Item,
    pub summary: Item,
    #[schemars(length(max = MAX_STRENGTHS))]
    pub strengths: Vec<StrengthItem>,
}

// A weakness: an `Item` plus the computed weakness it narrates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WeaknessItem {
    #[schemars(description = desc::TEXT, length(min = 1, max = ITEM_TEXT_MAX_CHARS))]
    pub text: String,
    #[schemars(description = desc::CITATIONS)]
    pub citations: Vec<Citation>,
    #[schemars(description = desc::WEAKNESS_ID)]
    pub weakness_id: String,
    #[schemars(length(min = 1, max = WEAKNESS_TITLE_MAX_CHARS))]
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WeaknessesSection {
    pub items: Vec<WeaknessItem>,
}

// An opening: an `Item` plus the line and its computed verdict.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpeningItem {
    #[schemars(description = desc::TEXT, length(min = 1, max = ITEM_TEXT_MAX_CHARS))]
    pub text: String,
    #[schemars(description = desc::CITATIONS)]
    pub citations: Vec<Citation>,
    #[schemars(description = desc::LINE_KEY)]
    pub line_key: String,
    #[schemars(description = desc::VERDICT)]
    pub verdict: Verdict,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpeningsSection {
    pub items: Vec<OpeningItem>,
}

// A drill picked from `drills_for` of the focus.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Drill {
    #[schemars(description = desc::DRILL_TYPE)]
    pub drill_type: DrillType,
    #[schemars(length(min = 1, max = DRILL_RATIONALE_MAX_CHARS))]
    pub rationale: String,
}

// One computed focus of the climb plan (ADR-AI-7).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Focus {
    #[schemars(description = desc::FOCUS_WEAKNESS_ID)]
    pub weakness_id: String,
    pub why: Item,
    #[schemars(length(min = MIN_DRILLS, max = MAX_DRILLS))]
    pub drills: Vec<Drill>,
    pub habit: Item,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClimbPlanSection {
    pub intro: Item,
    #[schemars(length(max = MAX_FOCUSES))]
    pub focuses: Vec<Focus>,
    pub weekly_rhythm: Item,
}

// An area text: an `Item` plus its area.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AreaItem {
    #[schemars(description = desc::TEXT, length(min = 1, max = ITEM_TEXT_MAX_CHARS))]
    pub text: String,
    #[schemars(description = desc::CITATIONS)]
    pub citations: Vec<Citation>,
    #[schemars(description = desc::AREA)]
    pub area: AreaId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AreasSection {
    pub items: Vec<AreaItem>,
}

// TS `ReportSectionEnvelope`: one schema for every section call.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportSectionEnvelope {
    #[schemars(description = desc::SECTION)]
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

/// One validated section in the TS `ValidatedSection` shape: the five camelCase keys
/// `overview`, `weaknesses`, `openings`, `climbPlan` and `areas` are always serialised, and
/// exactly one is non-null, the one of [`ValidatedSection::key`]. Build it with
/// [`ValidatedSection::new`]; deserialising rejects zero or several non-null parts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    rename_all = "camelCase",
    deny_unknown_fields,
    try_from = "RawValidatedSection"
)]
pub struct ValidatedSection {
    overview: Option<ValidatedOverviewSection>,
    weaknesses: Option<ValidatedWeaknessesSection>,
    openings: Option<ValidatedOpeningsSection>,
    climb_plan: Option<ValidatedClimbPlanSection>,
    areas: Option<ValidatedAreasSection>,
}

/// The content of one validated section.
#[derive(Clone, Debug, PartialEq)]
pub enum ValidatedPart {
    Overview(ValidatedOverviewSection),
    Weaknesses(ValidatedWeaknessesSection),
    Openings(ValidatedOpeningsSection),
    ClimbPlan(ValidatedClimbPlanSection),
    Areas(ValidatedAreasSection),
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawValidatedSection {
    #[serde(deserialize_with = "nullable")]
    overview: Option<ValidatedOverviewSection>,
    #[serde(deserialize_with = "nullable")]
    weaknesses: Option<ValidatedWeaknessesSection>,
    #[serde(deserialize_with = "nullable")]
    openings: Option<ValidatedOpeningsSection>,
    #[serde(deserialize_with = "nullable")]
    climb_plan: Option<ValidatedClimbPlanSection>,
    #[serde(deserialize_with = "nullable")]
    areas: Option<ValidatedAreasSection>,
}

impl TryFrom<RawValidatedSection> for ValidatedSection {
    type Error = String;

    fn try_from(r: RawValidatedSection) -> Result<Self, String> {
        let v = ValidatedSection {
            overview: r.overview,
            weaknesses: r.weaknesses,
            openings: r.openings,
            climb_plan: r.climb_plan,
            areas: r.areas,
        };
        let non_null = [
            v.overview.is_some(),
            v.weaknesses.is_some(),
            v.openings.is_some(),
            v.climb_plan.is_some(),
            v.areas.is_some(),
        ]
        .into_iter()
        .filter(|x| *x)
        .count();
        if non_null == 1 {
            Ok(v)
        } else {
            Err(format!(
                "exactly one section part must be non-null, got {non_null}"
            ))
        }
    }
}

impl ValidatedSection {
    pub fn new(part: ValidatedPart) -> Self {
        let mut v = ValidatedSection {
            overview: None,
            weaknesses: None,
            openings: None,
            climb_plan: None,
            areas: None,
        };
        match part {
            ValidatedPart::Overview(p) => v.overview = Some(p),
            ValidatedPart::Weaknesses(p) => v.weaknesses = Some(p),
            ValidatedPart::Openings(p) => v.openings = Some(p),
            ValidatedPart::ClimbPlan(p) => v.climb_plan = Some(p),
            ValidatedPart::Areas(p) => v.areas = Some(p),
        }
        v
    }

    /// The section of the non-null part.
    pub fn key(&self) -> SectionKey {
        if self.overview.is_some() {
            SectionKey::Overview
        } else if self.weaknesses.is_some() {
            SectionKey::Weaknesses
        } else if self.openings.is_some() {
            SectionKey::Openings
        } else if self.climb_plan.is_some() {
            SectionKey::ClimbPlan
        } else {
            SectionKey::Areas
        }
    }

    pub fn overview(&self) -> Option<&ValidatedOverviewSection> {
        self.overview.as_ref()
    }

    pub fn weaknesses(&self) -> Option<&ValidatedWeaknessesSection> {
        self.weaknesses.as_ref()
    }

    pub fn openings(&self) -> Option<&ValidatedOpeningsSection> {
        self.openings.as_ref()
    }

    pub fn climb_plan(&self) -> Option<&ValidatedClimbPlanSection> {
        self.climb_plan.as_ref()
    }

    pub fn areas(&self) -> Option<&ValidatedAreasSection> {
        self.areas.as_ref()
    }

    /// Takes the non-null part.
    pub fn into_part(self) -> ValidatedPart {
        if let Some(p) = self.overview {
            ValidatedPart::Overview(p)
        } else if let Some(p) = self.weaknesses {
            ValidatedPart::Weaknesses(p)
        } else if let Some(p) = self.openings {
            ValidatedPart::Openings(p)
        } else if let Some(p) = self.climb_plan {
            ValidatedPart::ClimbPlan(p)
        } else {
            ValidatedPart::Areas(
                self.areas
                    .expect("a ValidatedSection always has exactly one part"),
            )
        }
    }
}
