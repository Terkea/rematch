//! Job kinds, payloads and dedupe keys (final in Task 1).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const KIND_IMPORT: &str = "import-account";
pub const KIND_ANALYSE: &str = "analyse-game";
pub const KIND_RECONCILE: &str = "reconcile";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyseGamePayload {
    pub game_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportAccountPayload {
    pub account_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<u32>,
}

pub fn analyse_key(game_id: Uuid) -> String {
    format!("analyse:{game_id}")
}

pub fn import_key(account_id: Uuid) -> String {
    format!("import:{account_id}")
}

pub const RECONCILE_KEY: &str = "reconcile";
pub const RECONCILE_EVERY: Duration = Duration::from_secs(300);

// R2 job kinds (R2 Task 1, final; ADR-R2-1). Behaviour lands in the scheduler and profile crates.

pub const KIND_SCHEDULE: &str = "schedule-analysis";
pub const KIND_PREVIEW: &str = "preview-game";
pub const KIND_BUILD_PROFILE: &str = "build-profile";
pub const KIND_REFERENCE_GAME: &str = "analyse-reference-game";

/// Preview passes go before every full analysis tier.
pub const PREVIEW_PRIORITY: i16 = 0;
/// Reference-game analysis runs below every user tier (R-11).
pub const REFERENCE_PRIORITY: i16 = 200;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulePayload {
    pub account_id: Uuid,
    /// `"beta"`, `"free"` or `"pro"`; `None` = beta (no plan limits until M7, ADR-R2-16).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPayload {
    pub game_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildProfilePayload {
    pub account_id: Uuid,
    pub time_class: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceGamePayload {
    pub reference_game_id: Uuid,
}

pub fn schedule_key(account_id: Uuid) -> String {
    format!("schedule:{account_id}")
}

pub fn preview_key(game_id: Uuid) -> String {
    format!("preview:{game_id}")
}

pub fn profile_key(account_id: Uuid, time_class: &str) -> String {
    format!("profile:{account_id}:{time_class}")
}

pub fn refgame_key(reference_game_id: Uuid) -> String {
    format!("refgame:{reference_game_id}")
}
