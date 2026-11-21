//! The context a conversation or a message is opened from (a web contract). Anchors are bound
//! to the account server-side; a foreign anchor is ignored (Task 25).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::output::codes::SectionKey;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ChatAnchor {
    ReportSection {
        report_id: Uuid,
        section: SectionKey,
    },
    Weakness {
        weakness_id: String,
    },
    Position {
        game_id: String,
        ply: u32,
    },
    Move {
        game_id: String,
        ply: u32,
    },
    OpeningLine {
        line_key: String,
    },
    Game {
        game_id: String,
    },
}
