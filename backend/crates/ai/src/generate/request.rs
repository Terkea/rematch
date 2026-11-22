//! The cache-friendly request of one report section (Task 13, ADR 0060). The five sections of a
//! report share a byte-identical prefix with three cache breakpoints:
//!
//! 1. `system[0]`: [`rules_block`], `core/rules` + `core/tokens`: the format-neutral rules and
//!    token grammar, byte-identical for reports and the chat, so it caches for both;
//! 2. `system[1]`: `report/report` + `report/output` (the JSON output instructions, report only)
//!    + the band's `tone/b<n>` (shared within a band) + `core/glossary` (ADR 0085);
//! 3. user block 1: `<coaching_input>` + the canonical prompt context (shared within a report);
//!
//! and then the uncached section task, the only part that differs. The Messages API (batches)
//! uses the breakpoints; the sidecar drops them (ADR 0062) and benefits from the stable prefix.

use crate::client::model::{Effort, Models};
use crate::client::port::{Message, Role, StructuredRequest, StructuredResult, TextBlock};
use crate::input::types::CoachingInput;
use crate::output::codes::{SectionKey, ValidationError};
use crate::output::schema::REPORT_SECTION_SCHEMA;
use crate::prompts::{REPORT_PROMPT_SET, load_prompt, render_template};

use super::context::{build_prompt_context, canonical_json_escaped};

/// `max_tokens` of a section call.
pub const SECTION_MAX_TOKENS: u32 = 16000;

/// The text of a prompt at its version in `REPORT_PROMPT_SET` (version 1 for an id outside it).
/// Every id used here is in the set, which `tests/prompts.rs` loads, so a miss is a build defect.
/// (Before ADR 0085 this always read version 1, so `core/tokens` v2 recorded in the set was
/// never sent.)
pub(crate) fn text(id: &str) -> &'static str {
    let v = REPORT_PROMPT_SET
        .iter()
        .find(|(i, _)| *i == id)
        .map_or(1, |(_, v)| *v);
    load_prompt(id, v).unwrap_or_else(|e| panic!("{e}")).text
}

fn joined(ids: &[&str]) -> String {
    ids.iter()
        .map(|id| text(id))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The shared, format-neutral first system block: `core/rules` + `core/tokens`, cached. Every
/// call kind (report sections, the chat) starts with exactly these bytes, so the block is one
/// cache entry for all of them. It says nothing about the output format; each call kind adds its
/// own output section after it (`report/output` for reports).
pub fn rules_block() -> TextBlock {
    TextBlock::cached(joined(&["core/rules", "core/tokens"]))
}

/// The request for one section of the report on `input`.
pub fn build_section_request(
    input: &CoachingInput,
    section: SectionKey,
    is_first_report: bool,
) -> StructuredRequest {
    let tone = format!("tone/{}", input.band.as_str().to_ascii_lowercase());
    let context = build_prompt_context(input, is_first_report);
    let task_id = format!("report/section-{}", section.as_str());
    let task = render_template(text(&task_id), &[]).unwrap_or_else(|e| panic!("{task_id}: {e}"));
    StructuredRequest {
        model: Models::REPORT,
        system: vec![
            rules_block(),
            TextBlock::cached(joined(&[
                "report/report",
                "report/output",
                &tone,
                "core/glossary",
            ])),
        ],
        messages: vec![Message {
            role: Role::User,
            content: vec![
                TextBlock::cached(format!(
                    "<coaching_input>{}</coaching_input>",
                    canonical_json_escaped(&context)
                ))
                .to_json(),
                TextBlock::new(task).to_json(),
            ],
        }],
        max_tokens: SECTION_MAX_TOKENS,
        effort: Effort::Medium,
        schema: &REPORT_SECTION_SCHEMA,
    }
}

/// The two messages a repair appends to the section's messages: the previous assistant content
/// exactly as returned, then the rendered repair prompt listing each error as
/// `- path: code: detail`. The path is our own JSON path of the failing part (no user data); the
/// model needs it to repair the right item (controller ruling, T13 fix round 1, overriding the
/// brief's `- code: detail`). The last message is the user message (no prefill).
pub fn build_repair_messages(prev: &StructuredResult, errors: &[ValidationError]) -> Vec<Message> {
    let list = errors
        .iter()
        .map(|e| format!("- {}: {}: {}", e.path, e.code.as_str(), e.detail))
        .collect::<Vec<_>>()
        .join("\n");
    let repair = render_template(text("repair/repair"), &[("errors", &list)])
        .unwrap_or_else(|e| panic!("repair/repair: {e}"));
    vec![
        Message {
            role: Role::Assistant,
            content: prev.content.clone(),
        },
        Message {
            role: Role::User,
            content: vec![TextBlock::new(repair).to_json()],
        },
    ]
}
