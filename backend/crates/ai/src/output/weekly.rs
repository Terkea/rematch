//! The weekly review (TS plan B4): its strict input, which Milestone 7 builds, and the model's
//! output. Bounds the structured output subset cannot carry are checked by
//! `schema::check_weekly`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use time::Date;

use super::report::{Item, ValidatedItem};
use crate::input::types::{
    AreaId, BandId, Basis, ExemplarPosition, TimeClass, TimelineChange, WeaknessState, nullable,
};

/// The weekly output: at most 3 outcomes and 2 exemplars.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WeeklyReviewOutput {
    pub headline: Item,
    /// Process celebration, kept apart from outcomes (improvement-engine §6.7).
    pub process: Item,
    /// At most 3.
    pub outcomes: Vec<Item>,
    /// At most 2.
    pub exemplars: Vec<Item>,
    pub next_week: Item,
}

/// The validated weekly review: a dropped singleton becomes `None`, dropped entries are removed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidatedWeeklyReview {
    #[serde(deserialize_with = "nullable")]
    pub headline: Option<ValidatedItem>,
    #[serde(deserialize_with = "nullable")]
    pub process: Option<ValidatedItem>,
    pub outcomes: Vec<ValidatedItem>,
    pub exemplars: Vec<ValidatedItem>,
    #[serde(deserialize_with = "nullable")]
    pub next_week: Option<ValidatedItem>,
}

/// `inputVersion`: the literal `"weekly-input-v1"`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum WeeklyInputVersion {
    #[default]
    #[serde(rename = "weekly-input-v1")]
    V1,
}

/// `YYYY-MM-DD`, as the coaching input writes dates.
mod iso_date {
    use serde::{Deserialize, Deserializer, Serializer, de, ser};
    use time::Date;
    use time::format_description::FormatItem;
    use time::macros::format_description;

    const FORMAT: &[FormatItem<'static>] = format_description!("[year]-[month]-[day]");

    pub fn serialize<S: Serializer>(date: &Date, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&date.format(FORMAT).map_err(ser::Error::custom)?)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Date, D::Error> {
        let text = String::deserialize(deserializer)?;
        Date::parse(&text, FORMAT).map_err(de::Error::custom)
    }
}

/// Practice done against plan this week; `null` in the input when nothing was planned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Adherence {
    pub minutes_done: u32,
    pub minutes_planned: u32,
}

/// A metric estimate at one end of the week.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MetricPoint {
    pub value: f64,
    pub n: u32,
    /// Credible interval ends; `null` when there is no interval (as `Metric.low`/`high`).
    #[serde(deserialize_with = "nullable")]
    pub low: Option<f64>,
    #[serde(deserialize_with = "nullable")]
    pub high: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeltaVerdict {
    Improved,
    Worse,
    NoClearChange,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MetricDelta {
    pub key: String,
    pub area: AreaId,
    pub label: String,
    pub before: MetricPoint,
    pub after: MetricPoint,
    pub verdict: DeltaVerdict,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FocusUpdate {
    pub weakness_id: String,
    pub label: String,
    /// Probability of improvement; drives the likelihood words of outcome items.
    #[serde(deserialize_with = "nullable")]
    pub p_imp: Option<f64>,
    pub state: WeaknessState,
}

/// TS `WeeklyReviewInput` (plan B4), field for field. `exemplars` has at most 2 entries
/// ([`WeeklyReviewInput::check_bounds`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WeeklyReviewInput {
    pub input_version: WeeklyInputVersion,
    pub time_class: TimeClass,
    pub band: BandId,
    #[serde(with = "iso_date")]
    #[schemars(with = "String")]
    pub week_start: Date,
    #[serde(with = "iso_date")]
    #[schemars(with = "String")]
    pub week_end: Date,
    pub games_this_week: u32,
    pub basis: Basis,
    #[serde(deserialize_with = "nullable")]
    pub adherence: Option<Adherence>,
    pub metric_deltas: Vec<MetricDelta>,
    pub focus_updates: Vec<FocusUpdate>,
    pub exemplars: Vec<ExemplarPosition>,
    pub changes: Vec<TimelineChange>,
}

/// TS `exemplars: ExemplarPosition[] (max 2)`.
pub const WEEKLY_INPUT_MAX_EXEMPLARS: usize = 2;

impl WeeklyReviewInput {
    /// The bound serde cannot express: at most [`WEEKLY_INPUT_MAX_EXEMPLARS`] exemplars.
    pub fn check_bounds(&self) -> Result<(), String> {
        if self.exemplars.len() > WEEKLY_INPUT_MAX_EXEMPLARS {
            return Err(format!(
                "exemplars: at most {WEEKLY_INPUT_MAX_EXEMPLARS}, got {}",
                self.exemplars.len()
            ));
        }
        Ok(())
    }
}
