//! The AI client port: request and result types and the [`AiClient`] trait. The HTTP adapter
//! (our own reqwest client, ADR-R3-1) and the fake implement it in later tasks.

use std::time::Duration;

use futures::future::BoxFuture;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::model::{AiModel, Effort};

/// Token usage of one response (all four counters metered into `ai_usage`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
}

/// A message role. `System` is a mid-conversation system message (after a user turn).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

/// A message. `content` blocks are raw JSON so assistant content is replayed byte-for-byte
/// (append-only conversations keep the cache prefix stable).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<Value>,
}

/// A text block of a system prompt or message; `cache` adds an ephemeral breakpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextBlock {
    pub text: String,
    pub cache: bool,
}

impl TextBlock {
    pub fn new(text: impl Into<String>) -> Self {
        TextBlock {
            text: text.into(),
            cache: false,
        }
    }

    pub fn cached(text: impl Into<String>) -> Self {
        TextBlock {
            text: text.into(),
            cache: true,
        }
    }

    /// `{"type":"text","text":…[,"cache_control":{"type":"ephemeral"}]}`.
    pub fn to_json(&self) -> Value {
        let mut block = json!({ "type": "text", "text": self.text });
        if self.cache {
            block["cache_control"] = json!({ "type": "ephemeral" });
        }
        block
    }
}

impl Serialize for TextBlock {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_json().serialize(serializer)
    }
}

/// A structured output contract: its id and version (used in batch custom ids), the sanitized
/// API schema and the client-side check (deserialize plus bounds).
pub struct OutputSchema {
    pub id: &'static str,
    pub version: u32,
    pub json: fn() -> Value,
    pub check: fn(&Value) -> Result<(), Vec<ParseIssue>>,
}

impl std::fmt::Debug for OutputSchema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutputSchema")
            .field("id", &self.id)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// A Messages call whose JSON comes from `output_config.format`.
#[derive(Clone, Debug)]
pub struct StructuredRequest {
    pub model: AiModel,
    pub system: Vec<TextBlock>,
    pub messages: Vec<Message>,
    pub max_tokens: u32,
    pub effort: Effort,
    pub schema: &'static OutputSchema,
}

/// Why `StructuredResult.parsed` is `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParseError {
    NoText,
    InvalidJson,
    SchemaMismatch,
    Refusal,
    Truncated,
    NoSchema,
}

impl ParseError {
    pub fn as_str(self) -> &'static str {
        match self {
            ParseError::NoText => "no_text",
            ParseError::InvalidJson => "invalid_json",
            ParseError::SchemaMismatch => "schema_mismatch",
            ParseError::Refusal => "refusal",
            ParseError::Truncated => "truncated",
            ParseError::NoSchema => "no_schema",
        }
    }
}

/// A parse issue: location and code only, never model content. Codes: `invalid_type`,
/// `unknown_field`, `missing_field`, `invalid_value`, `too_small`, `too_big`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ParseIssue {
    pub path: String,
    pub code: String,
}

/// The outcome of a structured call. A `None` `parsed` always carries a `parse_error`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StructuredResult {
    pub parsed: Option<Value>,
    pub parse_error: Option<ParseError>,
    pub parse_issues: Option<Vec<ParseIssue>>,
    pub content: Vec<Value>,
    pub stop_reason: Option<String>,
    pub usage: AiUsage,
    pub model: String,
    pub request_id: Option<String>,
}

/// A tool offered to the chat model (tool lists are always sorted by name).
#[derive(Clone, Debug, PartialEq)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

/// Only `auto` and `none`: a forced tool choice is never sent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoice {
    #[default]
    Auto,
    None,
}

/// One streamed chat turn.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatTurnRequest {
    pub model: AiModel,
    pub system: Vec<TextBlock>,
    pub tools: Vec<ToolDef>,
    pub tool_choice: ToolChoice,
    pub messages: Vec<Message>,
    pub max_tokens: u32,
    pub effort: Effort,
}

/// A tool call of the model. `input` is `None` when the (possibly cut-off) input is unparsable.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: Option<Value>,
}

/// The finished turn.
#[derive(Clone, Debug, PartialEq)]
pub struct TurnResult {
    pub content: Vec<Value>,
    pub stop_reason: Option<String>,
    pub tool_uses: Vec<ToolUse>,
    pub usage: AiUsage,
    pub model: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TurnEvent {
    TextDelta(String),
    Done(TurnResult),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchStatus {
    InProgress,
    Canceling,
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchOutcome {
    Succeeded,
    Errored,
    Canceled,
    Expired,
}

/// Error taxonomy by API status and type, never by message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiErrorKind {
    Retryable,
    Fatal,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct BatchItem {
    pub custom_id: String,
    pub req: StructuredRequest,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BatchResultItem {
    pub custom_id: String,
    pub outcome: BatchOutcome,
    pub result: Option<StructuredResult>,
    pub error_type: Option<String>,
    /// Taxonomy of `error_type` for errored items.
    pub error_kind: Option<AiErrorKind>,
    /// Mirrors `result.parse_error` for succeeded items.
    pub parse_error: Option<ParseError>,
}

/// Maps a batch custom id back to its output schema, in any process.
pub trait SchemaResolver: Send + Sync {
    fn resolve(&self, custom_id: &str) -> Option<&'static OutputSchema>;
}

#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnavailableReason {
    #[error("connection")]
    Connection,
    #[error("server")]
    Server,
    #[error("overloaded")]
    Overloaded,
    #[error("rate limited")]
    RateLimited,
    #[error("circuit open")]
    CircuitOpen,
    #[error("no API key")]
    NoApiKey,
}

/// Every error the client returns. Messages never contain the API key or model content.
#[derive(thiserror::Error, Debug, Clone, PartialEq)]
pub enum AiError {
    #[error("AI unavailable: {reason:?}")]
    Unavailable {
        reason: UnavailableReason,
        retry_after: Option<Duration>,
    },
    #[error("AI credentials rejected (HTTP {status})")]
    Auth { status: u16 },
    #[error("AI request rejected (HTTP {status})")]
    Request {
        status: u16,
        api_error_type: Option<String>,
    },
    /// The message is redacted before construction.
    #[error("AI response could not be decoded: {0}")]
    Response(String),
    #[error("AI budget exceeded")]
    Budget,
    #[error("No pricing for model \"{0}\"")]
    UnknownModel(String),
}

impl AiError {
    pub fn is_unavailable(&self) -> bool {
        matches!(self, AiError::Unavailable { .. })
    }

    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            AiError::Unavailable { retry_after, .. } => *retry_after,
            _ => None,
        }
    }
}

pub trait AiClient: Send + Sync {
    fn structured<'a>(
        &'a self,
        req: &'a StructuredRequest,
    ) -> BoxFuture<'a, Result<StructuredResult, AiError>>;

    fn stream_turn<'a>(
        &'a self,
        req: &'a ChatTurnRequest,
    ) -> BoxStream<'a, Result<TurnEvent, AiError>>;

    fn batch_submit<'a>(&'a self, items: &'a [BatchItem])
    -> BoxFuture<'a, Result<String, AiError>>;

    fn batch_status<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<BatchStatus, AiError>>;

    fn batch_results<'a>(
        &'a self,
        id: &'a str,
        resolver: &'a dyn SchemaResolver,
    ) -> BoxStream<'a, Result<BatchResultItem, AiError>>;
}
