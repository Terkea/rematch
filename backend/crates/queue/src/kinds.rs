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
