//! The AI client port: request and result types and the [`AiClient`] trait. The runtime adapter
//! (the Agent SDK sidecar plus the batch client, ADR-R3-1 and ADR-R3-6) and the fake implement it.

use std::collections::BTreeMap;
use std::sync::Arc;
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

impl AiUsage {
    /// The counter-wise sum (saturating).
    pub fn plus(self, o: AiUsage) -> AiUsage {
        AiUsage {
            input_tokens: self.input_tokens.saturating_add(o.input_tokens),
            output_tokens: self.output_tokens.saturating_add(o.output_tokens),
            cache_creation_tokens: self
                .cache_creation_tokens
                .saturating_add(o.cache_creation_tokens),
            cache_read_tokens: self.cache_read_tokens.saturating_add(o.cache_read_tokens),
        }
    }

    /// `self - o` counter-wise, or `None` when any counter would go negative.
    pub fn checked_minus(self, o: AiUsage) -> Option<AiUsage> {
        Some(AiUsage {
            input_tokens: self.input_tokens.checked_sub(o.input_tokens)?,
            output_tokens: self.output_tokens.checked_sub(o.output_tokens)?,
            cache_creation_tokens: self
                .cache_creation_tokens
                .checked_sub(o.cache_creation_tokens)?,
            cache_read_tokens: self.cache_read_tokens.checked_sub(o.cache_read_tokens)?,
        })
    }
}

/// The usage of one model within a call. A call can touch several models (the SDK's internal
/// calls, a refusal fallback); each is priced at its own rates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    pub usage: AiUsage,
}

/// The counter-wise sum over the entries.
pub fn sum_usage(entries: &[ModelUsage]) -> AiUsage {
    entries
        .iter()
        .fold(AiUsage::default(), |acc, m| acc.plus(m.usage))
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
    /// The sum over `model_usage`.
    pub usage: AiUsage,
    /// The requested model id (a batch result carries the response's model).
    pub model: String,
    pub request_id: Option<String>,
    /// Per-model usage; a batch result has exactly one entry, `(message.model, usage)`.
    pub model_usage: Vec<ModelUsage>,
    /// The SDK's client-side estimate, kept for comparison only (never billed). `None` for batches.
    pub sdk_cost_usd: Option<f64>,
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

/// One whole agentic chat turn: the SDK runs up to `max_turns` model rounds and calls the tools
/// through the [`ToolHandler`] passed to [`AiClient::stream_turn`].
#[derive(Clone, Debug, PartialEq)]
pub struct ChatTurnRequest {
    pub model: AiModel,
    pub system: Vec<TextBlock>,
    pub tools: Vec<ToolDef>,
    pub tool_choice: ToolChoice,
    pub messages: Vec<Message>,
    pub max_tokens: u32,
    pub effort: Effort,
    /// The SDK's `maxTurns` (model rounds within this turn; the sidecar caps it at 10).
    pub max_turns: u32,
    /// The Postgres conversation id; it is also the SDK session id (ADR-R3-6 Amendment 1).
    pub conversation_id: uuid::Uuid,
    /// False on the first turn of a conversation, true on every later turn (then `messages`
    /// holds only the new user message).
    pub resume: bool,
    /// Summary plus recent turns, used by the sidecar only when the session cannot be resumed.
    /// Required when `resume` is true.
    pub rebuild_context: Option<String>,
}

/// A tool call of the model. `input` is `None` when the (possibly cut-off) input is unparsable.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: Option<Value>,
}

/// How the sidecar ran a chat turn (`done.session`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionPath {
    New,
    Resumed,
    Rebuilt,
}

/// The finished turn.
#[derive(Clone, Debug, PartialEq)]
pub struct TurnResult {
    /// The authoritative final text as one text block (empty when the turn produced none).
    pub content: Vec<Value>,
    /// `"max_turns"` when the round cap was hit; otherwise the API stop reason.
    pub stop_reason: Option<String>,
    pub tool_uses: Vec<ToolUse>,
    /// The sum over `model_usage`.
    pub usage: AiUsage,
    /// The requested model id.
    pub model: String,
    /// This turn's own usage per model (the sidecar's `modelUsage`).
    pub model_usage: Vec<ModelUsage>,
    pub sdk_cost_usd: Option<f64>,
    /// The SDK's running session total (`sessionModelUsage`). Store it per turn; never meter it
    /// raw (see [`metered_turn_usage`]).
    pub session_model_usage: Vec<ModelUsage>,
    pub session: Option<SessionPath>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TurnEvent {
    /// The following text deltas belong to this API message (the sidecar's `delta.messageId`),
    /// so a later [`TurnEvent::Retract`] can name them.
    Message(String),
    TextDelta(String),
    /// The client must drop the text of these streamed messages (a refusal or a superseded
    /// message). `Done`'s content is authoritative.
    Retract {
        message_ids: Vec<String>,
        reason: String,
    },
    Done(TurnResult),
}

/// What to meter for a finished chat turn (controller ruling, ADR-R3-6):
/// - a resumed turn with the previous turn's stored `session_model_usage`: per model,
///   `session_model_usage(now) - previous`, falling back to `model_usage` when any difference is
///   negative (a model missing from `now` counts as negative);
/// - otherwise (a new or rebuilt turn, or no stored previous total, or an empty one):
///   `model_usage`.
pub fn metered_turn_usage(turn: &TurnResult, previous: Option<&[ModelUsage]>) -> Vec<ModelUsage> {
    let (Some(SessionPath::Resumed), Some(prev)) = (turn.session, previous) else {
        return turn.model_usage.clone();
    };
    // An empty stored total is no baseline (it would meter the whole session again).
    if turn.session_model_usage.is_empty() || prev.is_empty() {
        return turn.model_usage.clone();
    }
    let mut now: BTreeMap<&str, AiUsage> = BTreeMap::new();
    for m in &turn.session_model_usage {
        let e = now.entry(m.model.as_str()).or_default();
        *e = e.plus(m.usage);
    }
    let mut before: BTreeMap<&str, AiUsage> = BTreeMap::new();
    for m in prev {
        let e = before.entry(m.model.as_str()).or_default();
        *e = e.plus(m.usage);
    }
    if before.keys().any(|k| !now.contains_key(k)) {
        return turn.model_usage.clone();
    }
    let mut out = Vec::new();
    for (model, usage) in now {
        let base = before.get(model).copied().unwrap_or_default();
        match usage.checked_minus(base) {
            Some(d) if d == AiUsage::default() => {}
            Some(d) => out.push(ModelUsage {
                model: model.to_owned(),
                usage: d,
            }),
            None => return turn.model_usage.clone(),
        }
    }
    out
}

/// A tool's answer to the model. `content` is what the model sees (usually JSON text).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolReply {
    pub content: String,
    pub is_error: bool,
}

/// Runs the chat tools of one turn. The sidecar's SDK calls back through the tool endpoint
/// (`ToolServer`), which routes each call to the handler registered for that turn.
pub trait ToolHandler: Send + Sync {
    fn call<'a>(&'a self, name: &'a str, input: Value) -> BoxFuture<'a, ToolReply>;
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

    /// One whole agentic turn: the SDK runs the tool rounds and calls `tools` for each tool use.
    /// With `None` (or [`ToolChoice::None`]) no tool is offered.
    fn stream_turn<'a>(
        &'a self,
        req: &'a ChatTurnRequest,
        tools: Option<Arc<dyn ToolHandler>>,
    ) -> BoxStream<'a, Result<TurnEvent, AiError>>;

    /// Whether the batch calls can work (they need `ANTHROPIC_API_KEY`).
    fn batches_enabled(&self) -> bool {
        true
    }

    fn batch_submit<'a>(&'a self, items: &'a [BatchItem])
    -> BoxFuture<'a, Result<String, AiError>>;

    fn batch_status<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<BatchStatus, AiError>>;

    fn batch_results<'a>(
        &'a self,
        id: &'a str,
        resolver: &'a dyn SchemaResolver,
    ) -> BoxStream<'a, Result<BatchResultItem, AiError>>;
}
