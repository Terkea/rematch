//! Thread titles and summaries (for the conversation list and the history rollover).
//!
//! The summary is model text the player may see (the title) and that later turns read (the
//! summary), so both are checked like chat text without tokens: no leaks (moves, numbers,
//! dates), no banned phrases, no marketing, and bounded. A text that fails becomes `None`; the
//! caller then uses [`fallback_title`] or no summary.

use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::anchor::ChatAnchor;
use super::engine::{BeforeCall, CallRecord, escape_tag_text};
use crate::client::model::{Effort, Models};
use crate::client::port::{
    AiClient, AiError, Message, OutputSchema, ParseIssue, Role, StructuredRequest, TextBlock,
};
use crate::output::schema::{api_schema, deserialize_checked};
use crate::validator::evidence::Evidence;
use crate::validator::language::{banned_phrases, no_marketing};
use crate::validator::text::{TextRules, validate_text};

pub const CHAT_SUMMARY_SYSTEM_PROMPT: &str =
    include_str!("../../prompts/chat-summary/system.v1.md");
pub const CHAT_SUMMARY_TASK_PROMPT: &str = include_str!("../../prompts/chat-summary/task.v1.md");

pub const MAX_TITLE_CHARS: usize = 80;
pub const MAX_SUMMARY_CHARS: usize = 2000;
const MAX_TOKENS: u32 = 1000;

// The `chat-summary` output. `//` comments only: doc comments would reach the model schema.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChatSummaryOutput {
    pub title: String,
    pub summary: String,
}

fn check(v: &Value) -> Result<(), Vec<ParseIssue>> {
    deserialize_checked::<ChatSummaryOutput>(v).map(|_| ())
}

pub static CHAT_SUMMARY_SCHEMA: OutputSchema = OutputSchema {
    id: "chat-summary",
    version: 1,
    json: api_schema::<ChatSummaryOutput>,
    check,
};

#[derive(Clone, Debug, PartialEq)]
pub struct ThreadSummary {
    pub title: Option<String>,
    pub summary: Option<String>,
    pub calls: Vec<CallRecord>,
}

/// The text if it is non-blank, at most `max` characters, and passes the leak, banned-phrase
/// and marketing checks.
fn checked(text: &str, max: usize, path: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > max {
        return None;
    }
    let rules = TextRules {
        extra: vec![banned_phrases(), no_marketing()],
        ..TextRules::default()
    };
    validate_text(text, &[], &Evidence::default(), &rules, path)
        .ok()
        .map(|_| text.to_owned())
}

/// `(question, answer token text)` pairs as plain text.
fn transcript_text(transcript: &[(String, String)]) -> String {
    transcript
        .iter()
        .map(|(q, a)| format!("Player: {q}\nCoach: {a}"))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// One `Models::EXPLAIN` call (low effort, 1000 tokens). A refusal, a parse failure or a
/// failing text gives `None` for that field; client errors propagate.
pub fn summarise_thread<'a>(
    ai: &'a dyn AiClient,
    before_call: BeforeCall<'a>,
    transcript: &'a [(String, String)],
) -> BoxFuture<'a, Result<ThreadSummary, AiError>> {
    Box::pin(async move {
        let req = StructuredRequest {
            model: Models::EXPLAIN,
            system: vec![TextBlock::cached(CHAT_SUMMARY_SYSTEM_PROMPT)],
            messages: vec![Message {
                role: Role::User,
                content: vec![
                    TextBlock::new(format!(
                        "<transcript>{}</transcript>",
                        escape_tag_text(&transcript_text(transcript))
                    ))
                    .to_json(),
                    TextBlock::new(CHAT_SUMMARY_TASK_PROMPT).to_json(),
                ],
            }],
            max_tokens: MAX_TOKENS,
            effort: Effort::Low,
            schema: &CHAT_SUMMARY_SCHEMA,
        };
        before_call().await?;
        let r = ai.structured(&req).await?;
        let calls = r
            .model_usage
            .iter()
            .enumerate()
            .map(|(i, m)| CallRecord {
                model: m.model.clone(),
                usage: m.usage,
                batch: false,
                sdk_cost_usd: if i == 0 { r.sdk_cost_usd } else { None },
            })
            .collect();
        let out = r
            .parsed
            .as_ref()
            .and_then(|v| serde_json::from_value::<ChatSummaryOutput>(v.clone()).ok());
        Ok(ThreadSummary {
            title: out
                .as_ref()
                .and_then(|o| checked(&o.title, MAX_TITLE_CHARS, "title")),
            summary: out
                .as_ref()
                .and_then(|o| checked(&o.summary, MAX_SUMMARY_CHARS, "summary")),
            calls,
        })
    })
}

/// The title when the model gave none (or an unsafe one), by the conversation's anchor.
pub fn fallback_title(anchor: Option<&ChatAnchor>) -> &'static str {
    match anchor {
        Some(ChatAnchor::ReportSection { .. }) => "Chat about your report",
        Some(ChatAnchor::Weakness { .. }) => "Chat about a weakness",
        Some(ChatAnchor::Position { .. }) => "Chat about a position",
        Some(ChatAnchor::Move { .. }) => "Chat about a move",
        Some(ChatAnchor::OpeningLine { .. }) => "Chat about an opening",
        Some(ChatAnchor::Game { .. }) => "Chat about a game",
        None => "Chat with your coach",
    }
}
