//! The plan context's wire shape (ADR 0094). Every fraction is a share in 0..1 (the schema says
//! so, and the evidence prints it as a percentage); every count is a plain number the text names
//! ("chances", "days"); dates are `YYYY-MM-DD` (UTC).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::input::types::nullable;

/// The colour a focus counts (ADR 0082): `both` counts every game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanSide {
    Both,
    White,
    Black,
}

/// Where a running focus stands (ADR 0091 §4): below 10 chances it is too early to tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FocusStatus {
    OnTrack,
    NotYet,
    TooEarly,
}

/// How a focus that has reached its N games ended (ADR 0091 §5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FocusOutcome {
    Met,
    NotMet,
    Inconclusive,
}

/// "Is it working?" (ADR 0090 §8): a change is claimed only when both sides have 30 chances
/// and the 80% ranges separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EfficacyVerdict {
    TooEarly,
    Lower,
    Higher,
    NoClearChange,
}

/// A rate per chance with its n and its 80% Wilson range (null without a chance).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanRate {
    pub events: u32,
    pub chances: u32,
    #[serde(deserialize_with = "nullable")]
    #[schemars(range(min = 0, max = 1))]
    pub rate: Option<f64>,
    #[serde(deserialize_with = "nullable")]
    #[schemars(range(min = 0, max = 1))]
    pub low: Option<f64>,
    #[serde(deserialize_with = "nullable")]
    #[schemars(range(min = 0, max = 1))]
    pub high: Option<f64>,
}

/// The user's running focus on this account and time class (ADR 0091), with its live progress.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanFocus {
    pub series_key: String,
    /// The weakness's plain name.
    pub label: String,
    pub side: PlanSide,
    pub started_on: String,
    /// The goal: at most this share of chances over the focus's games.
    #[schemars(range(min = 0, max = 1))]
    pub target_rate: f64,
    /// N: the focus's length in analysed games.
    pub games_n: u32,
    /// Analysed games played since the focus started (at most N).
    pub games_played: u32,
    /// The weakness's events and chances in those games, the rate and its 80% range.
    pub progress: PlanRate,
    pub status: FocusStatus,
    /// Chances still needed before a verdict (only while `too_early`).
    #[serde(deserialize_with = "nullable")]
    pub more_chances: Option<u32>,
    /// Set once the focus has its N games.
    #[serde(deserialize_with = "nullable")]
    pub outcome: Option<FocusOutcome>,
    /// What the focus started from: the estimate's rate and its unweighted counts.
    #[schemars(range(min = 0, max = 1))]
    pub baseline_rate: f64,
    pub baseline_events: u32,
    pub baseline_chances: u32,
}

/// One trainer source's attempts in a window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceTraining {
    pub source: String,
    pub label: String,
    pub attempts: u32,
    /// Attempts answered right (each is the item's first try that day, ADR 0090 §5).
    pub right: u32,
    #[serde(deserialize_with = "nullable")]
    #[schemars(range(min = 0, max = 1))]
    pub first_try_rate: Option<f64>,
}

/// The trainer over the last `days` UTC days, today included.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrainingWindow {
    pub days: u32,
    /// Distinct days with at least one attempt.
    pub days_practised: u32,
    pub attempts: u32,
    pub right: u32,
    #[serde(deserialize_with = "nullable")]
    #[schemars(range(min = 0, max = 1))]
    pub first_try_rate: Option<f64>,
    /// Positions retired in the window (three clean passes).
    pub items_retired: u32,
    /// Per source, only sources with an attempt in the window.
    pub by_source: Vec<SourceTraining>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanTraining {
    pub last_week: TrainingWindow,
    pub last_month: TrainingWindow,
    /// Positions waiting in the trainer (not held out, not retired).
    pub active_items: u32,
}

/// "Is it working?" for one source: the weakness rate per chance in games before its first
/// drill and from it on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceEfficacy {
    pub source: String,
    pub label: String,
    /// Items of the source drilled at least once.
    pub drilled: u32,
    #[serde(deserialize_with = "nullable")]
    pub first_drill: Option<String>,
    pub before: PlanRate,
    pub after: PlanRate,
    pub verdict: EfficacyVerdict,
}

/// Drilled against held back: how often the motifs of drilled positions were found in later
/// games, against motifs only held-out positions have. `events` counts the times found.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PatternEfficacy {
    #[serde(deserialize_with = "nullable")]
    pub since: Option<String>,
    pub drilled: PlanRate,
    pub held_out: PlanRate,
    pub verdict: EfficacyVerdict,
}

/// What one user is working on (ADR 0094): their focus, their training and whether it is
/// working, for one of their accounts and a time class. Never shared across users.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanContext {
    /// The UTC day the context was built on.
    pub as_of: String,
    #[serde(deserialize_with = "nullable")]
    pub focus: Option<PlanFocus>,
    pub training: PlanTraining,
    pub is_it_working: Vec<SourceEfficacy>,
    #[serde(deserialize_with = "nullable")]
    pub patterns: Option<PatternEfficacy>,
}

/// The JSON Schema of [`PlanContext`], with the contract's rules (every property required, an
/// `Option` keeps its `null`).
pub fn plan_json_schema() -> serde_json::Value {
    schemars::generate::SchemaSettings::draft2020_12()
        .with_transform(schemars::transform::RecursiveTransform(
            coaching_input::schema::contract_transform,
        ))
        .into_generator()
        .into_root_schema_for::<PlanContext>()
        .to_value()
}
