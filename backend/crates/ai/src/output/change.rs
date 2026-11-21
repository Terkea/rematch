//! Weakness-change notes (TS plan B4): the model's output, its input and the stable change key.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::codes::Citation;
use super::schema::{ITEM_TEXT_MAX_CHARS, desc};
use crate::input::types::{BandId, Basis, TimeClass, TimelineChange};

// The `weakness-change` output (`{ text, citations }`). Text bounds are checked client-side
// (`schema::change_bounds`). `//` comments only: see `report.rs`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WeaknessChangeOutput {
    #[schemars(description = desc::CHANGE_TEXT, length(min = 1, max = ITEM_TEXT_MAX_CHARS))]
    pub text: String,
    #[schemars(description = desc::CITATIONS)]
    pub citations: Vec<Citation>,
}

/// TS `WeaknessChangeInput`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WeaknessChangeInput {
    pub change: TimelineChange,
    pub band: BandId,
    pub time_class: TimeClass,
    pub basis: Basis,
}

/// TS `changeKey`: `"{seriesKey}@{onsetGameId}"`.
pub fn change_key(c: &TimelineChange) -> String {
    format!("{}@{}", c.series_key, c.onset_game_id)
}
