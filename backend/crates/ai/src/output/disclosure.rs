//! The AI disclosure stored with every report payload, weekly review, change note and chat
//! message (spec §14, port of `output/disclosure.ts`).

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// TS `AI_DISCLOSURE.label`.
pub const AI_DISCLOSURE_LABEL: &str = "Written by AI";

/// TS `AI_DISCLOSURE.note`.
pub const AI_DISCLOSURE_NOTE: &str = "This coaching was written by an AI model (Claude) from your analysed games. It can contain mistakes. The engine data shown alongside it is the source of truth.";

/// TS `AiDisclosure`. Build it with [`make_disclosure`] so `label` and `note` are the fixed copy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiDisclosure {
    pub label: String,
    pub note: String,
    pub model: String,
    pub prompt_version: String,
    /// `Date.toISOString()` form: UTC, milliseconds, `Z`.
    pub generated_at: String,
}

/// TS `makeDisclosure`.
pub fn make_disclosure(model: &str, prompt_version: &str, at: OffsetDateTime) -> AiDisclosure {
    AiDisclosure {
        label: AI_DISCLOSURE_LABEL.to_owned(),
        note: AI_DISCLOSURE_NOTE.to_owned(),
        model: model.to_owned(),
        prompt_version: prompt_version.to_owned(),
        generated_at: common::js_time::to_iso_string(at),
    }
}
