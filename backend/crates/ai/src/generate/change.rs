//! Weakness-change notes (Task 15, TS plan B4 `explainWeaknessChange`), on the lighter model: a
//! short, hedged paragraph on one change of the R2 timeline.
//!
//! The note can only say what the change's own evidence holds ([`build_change_evidence`]: its
//! examples, numbers and dates, nothing else from the input), must cite an example, hedges every
//! candidate cause (`no_causal_claims`: correlated changes are possible causes, never proven
//! ones), and may not describe a direction the timeline did not detect ([`change_direction`]).
//! It goes through Task 14's repair-once flow ([`super::attempt`]).

use serde_json::{Value, json};

use super::attempt::{Attempted, Checked, attempt_and_repair, parsed};
use super::context::canonical_json_escaped;
use super::request::rules_block;
use super::section::{BeforeCall, CallRecord, SectionError, SectionStatus};
use crate::client::model::{Effort, Models};
use crate::client::port::{
    AiClient, Message, Role, StructuredRequest, StructuredResult, TextBlock,
};
use crate::output::change::WeaknessChangeInput;
use crate::output::codes::{Citation, DroppedItem, ValidationCode, ValidationError};
use crate::output::schema::{WEAKNESS_CHANGE_SCHEMA, parse_weakness_change};
use crate::prompts::load_prompt;
use crate::segments::Segment;
use crate::validator::evidence::{
    Evidence, build_evidence_from, list_date_paths, list_number_paths,
};
use crate::validator::language::{
    banned_phrases, change_direction, engine_claims, no_causal_claims, no_marketing,
    no_quantity_words,
};
use crate::validator::text::{TextRules, validate_text};

/// `max_tokens` of a change-note call.
pub const CHANGE_MAX_TOKENS: u32 = 2000;

/// `DroppedItem::section` of a change note.
pub const CHANGE_SECTION: &str = "change";

fn text(id: &str) -> &'static str {
    load_prompt(id, 1).unwrap_or_else(|e| panic!("{e}")).text
}

/// The change as the evidence and the prompt see it: `{"changes": [change]}`, so its token paths
/// are `changes.<seriesKey>.…`, as in a report.
fn change_json(input: &WeaknessChangeInput) -> Value {
    json!({ "changes": [serde_json::to_value(&input.change).expect("TimelineChange serialises")] })
}

/// The index of a change note: the change's examples, numbers and dates only (with its ratio
/// and rate paths). The basis and the rest of the input cannot be cited.
pub fn build_change_evidence(input: &WeaknessChangeInput) -> Evidence {
    build_evidence_from(&change_json(input), &[], &[&input.change], &[])
}

/// What the change prompts read: `band`, `timeClass`, `changes` (the change) and `refs` (its
/// `num` and `date` token paths, sorted).
pub fn build_change_context(input: &WeaknessChangeInput) -> Value {
    let ev = build_change_evidence(input);
    let mut ctx = change_json(input);
    ctx["band"] = json!(input.band);
    ctx["timeClass"] = json!(input.time_class);
    ctx["refs"] = json!({
        "num": list_number_paths(&ev),
        "date": list_date_paths(&ev),
    });
    ctx
}

/// The change-note request: `Models::EXPLAIN`, low effort, [`CHANGE_MAX_TOKENS`].
/// - `system[0]`: [`rules_block`] (cached, shared with reports and the chat);
/// - `system[1]`: `weakness-change/system` + `weakness-change/output` (cached);
/// - user: `<change_input>` + the canonical context (cached), then `weakness-change/task`.
pub fn build_change_request(input: &WeaknessChangeInput) -> StructuredRequest {
    StructuredRequest {
        model: Models::EXPLAIN,
        system: vec![
            rules_block(),
            TextBlock::cached(format!(
                "{}\n\n{}",
                text("weakness-change/system"),
                text("weakness-change/output")
            )),
        ],
        messages: vec![Message {
            role: Role::User,
            content: vec![
                TextBlock::cached(format!(
                    "<change_input>{}</change_input>",
                    canonical_json_escaped(&build_change_context(input))
                ))
                .to_json(),
                TextBlock::new(text("weakness-change/task")).to_json(),
            ],
        }],
        max_tokens: CHANGE_MAX_TOKENS,
        effort: Effort::Low,
        schema: &WEAKNESS_CHANGE_SCHEMA,
    }
}

/// The rules of a change note: a citation, `no_causal_claims` (no causal phrase, and a hedge),
/// `banned_phrases`, `no_marketing` (TS B4), plus `engine_claims`, `no_quantity_words` and
/// [`change_direction`] for the change's rates.
pub fn change_text_rules(input: &WeaknessChangeInput) -> TextRules {
    TextRules {
        require_citation: true,
        extra: vec![
            no_causal_claims(),
            banned_phrases(),
            no_marketing(),
            engine_claims(),
            no_quantity_words(),
            change_direction(input.change.rate_before, input.change.rate_after),
        ],
        ..TextRules::default()
    }
}

/// The result of a change note. `text`, `segments` and a non-empty `citations` are there exactly
/// when `status` is `Ready`; `dropped` (the audit copy, details included) when it is `Dropped`.
#[derive(Clone, Debug, PartialEq)]
pub struct ChangeOutcome {
    pub status: SectionStatus,
    pub text: Option<String>,
    pub segments: Option<Vec<Segment>>,
    pub citations: Vec<Citation>,
    pub dropped: Vec<DroppedItem>,
    /// Every model call made: 1 or 2. All of them are metered.
    pub calls: Vec<CallRecord>,
}

struct Note {
    text: String,
    citations: Vec<Citation>,
    segments: Vec<Segment>,
}

fn check(r: &StructuredResult, rules: &TextRules, ev: &Evidence) -> Checked<Note> {
    let value = match parsed(r) {
        Ok(v) => v,
        Err(errors) => return Checked::Unusable(errors),
    };
    let out = match parse_weakness_change(value) {
        Ok(out) => out,
        Err(issues) => {
            return Checked::Unusable(
                issues
                    .into_iter()
                    .map(|i| ValidationError {
                        code: ValidationCode::SchemaInvalid,
                        path: i.path,
                        detail: i.code,
                    })
                    .collect(),
            );
        }
    };
    match validate_text(&out.text, &out.citations, ev, rules, "text") {
        Ok(segments) => Checked::Clean(Note {
            text: out.text,
            citations: out.citations,
            segments,
        }),
        Err(errors) => Checked::Unusable(errors),
    }
}

/// Writes the note for one change: attempt, validate, repair once, else `Dropped` with no text.
/// Client errors come back as a [`SectionError`] with the records of the calls already paid for.
pub async fn explain_weakness_change(
    ai: &dyn AiClient,
    before_call: BeforeCall<'_>,
    input: &WeaknessChangeInput,
) -> Result<ChangeOutcome, SectionError> {
    let ev = build_change_evidence(input);
    let rules = change_text_rules(input);
    let req = build_change_request(input);
    let Attempted {
        status,
        content,
        dropped,
        calls,
    } = attempt_and_repair(ai, before_call, &req, CHANGE_SECTION, |r| {
        check(r, &rules, &ev)
    })
    .await?;
    Ok(match content {
        Some(n) => ChangeOutcome {
            status,
            text: Some(n.text),
            segments: Some(n.segments),
            citations: n.citations,
            dropped,
            calls,
        },
        None => ChangeOutcome {
            status,
            text: None,
            segments: None,
            citations: Vec::new(),
            dropped,
            calls,
        },
    })
}
