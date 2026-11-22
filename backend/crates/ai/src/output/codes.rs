//! Section keys, validation codes and the error/citation shapes shared by the validator, the
//! report pipeline and the chat (TS `output/report-schema.ts`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// A report section (TS `SECTION_KEYS`, in this order). A `//` comment, not a doc comment:
// doc comments become model-facing schema descriptions (ADR 0060).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SectionKey {
    Overview,
    Weaknesses,
    Openings,
    ClimbPlan,
    Areas,
}

/// TS `SECTION_KEYS`.
pub const SECTION_KEYS: [SectionKey; 5] = [
    SectionKey::Overview,
    SectionKey::Weaknesses,
    SectionKey::Openings,
    SectionKey::ClimbPlan,
    SectionKey::Areas,
];

impl SectionKey {
    pub fn as_str(self) -> &'static str {
        match self {
            SectionKey::Overview => "overview",
            SectionKey::Weaknesses => "weaknesses",
            SectionKey::Openings => "openings",
            SectionKey::ClimbPlan => "climb_plan",
            SectionKey::Areas => "areas",
        }
    }
}

impl std::fmt::Display for SectionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for SectionKey {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        SECTION_KEYS
            .into_iter()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| format!("unknown section key: {s:?}"))
    }
}

/// TS `VALIDATION_CODES` (same order), plus the four R3 codes of ADR-AI-8 and the two of R3
/// Task 8 fix round 1 (`glued_token`, `unrelated_citation`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValidationCode {
    SchemaInvalid,
    Refusal,
    Truncated,
    WrongSection,
    UnknownCitation,
    MissingCitation,
    UnknownMove,
    UnknownNumber,
    UnknownDate,
    UnknownWeakness,
    UnknownLine,
    VerdictMismatch,
    FocusNotSelected,
    TooManyFocuses,
    DrillNotAllowed,
    AreaMissing,
    BannedPhrase,
    LikelihoodMismatch,
    MissingEvidenceWindow,
    CausalClaim,
    MissingHedge,
    Marketing,
    InsufficientDataNotStated,
    // R3 additions (ADR-AI-8: facts by reference).
    TokenSyntax,
    UnresolvedToken,
    FreeTextFact,
    UncitedReference,
    // R3 Task 8 fix round 1.
    GluedToken,
    UnrelatedCitation,
}

impl ValidationCode {
    /// Every code, in declaration order.
    pub const ALL: [ValidationCode; 29] = [
        ValidationCode::SchemaInvalid,
        ValidationCode::Refusal,
        ValidationCode::Truncated,
        ValidationCode::WrongSection,
        ValidationCode::UnknownCitation,
        ValidationCode::MissingCitation,
        ValidationCode::UnknownMove,
        ValidationCode::UnknownNumber,
        ValidationCode::UnknownDate,
        ValidationCode::UnknownWeakness,
        ValidationCode::UnknownLine,
        ValidationCode::VerdictMismatch,
        ValidationCode::FocusNotSelected,
        ValidationCode::TooManyFocuses,
        ValidationCode::DrillNotAllowed,
        ValidationCode::AreaMissing,
        ValidationCode::BannedPhrase,
        ValidationCode::LikelihoodMismatch,
        ValidationCode::MissingEvidenceWindow,
        ValidationCode::CausalClaim,
        ValidationCode::MissingHedge,
        ValidationCode::Marketing,
        ValidationCode::InsufficientDataNotStated,
        ValidationCode::TokenSyntax,
        ValidationCode::UnresolvedToken,
        ValidationCode::FreeTextFact,
        ValidationCode::UncitedReference,
        ValidationCode::GluedToken,
        ValidationCode::UnrelatedCitation,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ValidationCode::SchemaInvalid => "schema_invalid",
            ValidationCode::Refusal => "refusal",
            ValidationCode::Truncated => "truncated",
            ValidationCode::WrongSection => "wrong_section",
            ValidationCode::UnknownCitation => "unknown_citation",
            ValidationCode::MissingCitation => "missing_citation",
            ValidationCode::UnknownMove => "unknown_move",
            ValidationCode::UnknownNumber => "unknown_number",
            ValidationCode::UnknownDate => "unknown_date",
            ValidationCode::UnknownWeakness => "unknown_weakness",
            ValidationCode::UnknownLine => "unknown_line",
            ValidationCode::VerdictMismatch => "verdict_mismatch",
            ValidationCode::FocusNotSelected => "focus_not_selected",
            ValidationCode::TooManyFocuses => "too_many_focuses",
            ValidationCode::DrillNotAllowed => "drill_not_allowed",
            ValidationCode::AreaMissing => "area_missing",
            ValidationCode::BannedPhrase => "banned_phrase",
            ValidationCode::LikelihoodMismatch => "likelihood_mismatch",
            ValidationCode::MissingEvidenceWindow => "missing_evidence_window",
            ValidationCode::CausalClaim => "causal_claim",
            ValidationCode::MissingHedge => "missing_hedge",
            ValidationCode::Marketing => "marketing",
            ValidationCode::InsufficientDataNotStated => "insufficient_data_not_stated",
            ValidationCode::TokenSyntax => "token_syntax",
            ValidationCode::UnresolvedToken => "unresolved_token",
            ValidationCode::FreeTextFact => "free_text_fact",
            ValidationCode::UncitedReference => "uncited_reference",
            ValidationCode::GluedToken => "glued_token",
            ValidationCode::UnrelatedCitation => "unrelated_citation",
        }
    }
}

impl std::fmt::Display for ValidationCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One validation failure. `detail` never contains raw model text beyond the offending fragment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidationError {
    pub code: ValidationCode,
    pub path: String,
    pub detail: String,
}

/// An item (or a whole section) dropped after its repair attempt still failed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DroppedItem {
    /// A [`SectionKey`] string, `"weekly"`, `"change"` or `"chat"`.
    pub section: String,
    pub path: String,
    pub errors: Vec<ValidationError>,
}

// A position the model cites: a game id and the 1-based ply. (`//`: see `SectionKey`.)
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Citation {
    pub game_id: String,
    pub ply: u32,
}
