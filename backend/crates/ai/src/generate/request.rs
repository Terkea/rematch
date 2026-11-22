//! The cache-friendly request of one report section (Task 13, ADR-R3-4). The five sections of a
//! report share a byte-identical prefix with three cache breakpoints:
//!
//! 1. `system[0]`: `core/system` + `core/tokens` (shared by every band and every call kind);
//! 2. `system[1]`: `report/report` + the band's `tone/b<n>` (shared within a band);
//! 3. user block 1: `<coaching_input>` + the canonical prompt context (shared within a report);
//!
//! and then the uncached section task, the only part that differs. The Messages API (batches)
//! uses the breakpoints; the sidecar drops them (ADR-R3-6) and benefits from the stable prefix.

use crate::client::model::{Effort, Models};
use crate::client::port::{Message, Role, StructuredRequest, StructuredResult, TextBlock};
use crate::input::types::CoachingInput;
use crate::output::codes::{SectionKey, ValidationError};
use crate::output::schema::REPORT_SECTION_SCHEMA;
use crate::prompts::{load_prompt, render_template};

use super::context::{build_prompt_context, canonical_json_escaped};

/// `max_tokens` of a section call.
pub const SECTION_MAX_TOKENS: u32 = 16000;

/// The text of a registered version-1 prompt. Every id used here is in `REPORT_PROMPT_SET`, which
/// `tests/prompts.rs` loads, so a miss is a build defect.
fn text(id: &str) -> &'static str {
    load_prompt(id, 1).unwrap_or_else(|e| panic!("{e}")).text
}

fn joined(a: &str, b: &str) -> String {
    format!("{}\n\n{}", text(a), text(b))
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
            TextBlock::cached(joined("core/system", "core/tokens")),
            TextBlock::cached(joined("report/report", &tone)),
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
/// exactly as returned, then the rendered repair prompt listing each error as `- code: detail`.
/// The last message is the user message (no prefill).
pub fn build_repair_messages(prev: &StructuredResult, errors: &[ValidationError]) -> Vec<Message> {
    let list = errors
        .iter()
        .map(|e| format!("- {}: {}", e.code.as_str(), e.detail))
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
