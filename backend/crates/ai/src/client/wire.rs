//! The exact JSON the client sends to and reads from the Messages and Batches APIs.
//!
//! Requests are plain structs, so their top-level field order is fixed by declaration order;
//! `None` fields are omitted. Content blocks are raw [`Value`]s, deterministically re-serialised
//! (sorted keys, since `serde_json/preserve_order` is off workspace-wide), which keeps the
//! prompt-cache prefix stable. Responses ignore unknown fields.
//!
//! Never sent (Global Constraints): `temperature`, `top_p`, `top_k`, `thinking`,
//! `budget_tokens`, `fallbacks`, a forced `tool_choice`, or any `anthropic-beta` header.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::model::Effort;
use super::port::{
    AiUsage, BatchOutcome, BatchStatus, ChatTurnRequest, Message, StructuredRequest, TextBlock,
    ToolChoice, ToolDef,
};

/// `anthropic-version` on every call.
pub const ANTHROPIC_VERSION: &str = "2026-10-21";
/// The User-Agent of every request the `ai` crate sends (ADR 0043): the project URL, never an
/// email address or other personal contact data.
pub const USER_AGENT: &str = "chess-improver/0.1 (+https://github.com/chess-improver)";
/// The response header whose value becomes `request_id`.
pub const REQUEST_ID_HEADER: &str = "request-id";
/// Default API base (Task 11 makes it configurable).
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
pub const MESSAGES_PATH: &str = "/v1/messages";
pub const BATCHES_PATH: &str = "/v1/messages/batches";

/// `{"type":"ephemeral"}` (the 5-minute TTL).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub kind: Ephemeral,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ephemeral {
    #[default]
    Ephemeral,
}

/// `{"type":"json_schema","schema":…}`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OutputFormat {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub schema: Value,
}

impl OutputFormat {
    pub fn json_schema(schema: Value) -> Self {
        OutputFormat {
            kind: "json_schema",
            schema,
        }
    }
}

/// `output_config`: effort is always explicit; `format` only on structured calls.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OutputConfig {
    pub effort: Effort,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<OutputFormat>,
}

/// A client tool, always strict and with eager input streaming.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WireTool {
    pub name: &'static str,
    pub description: &'static str,
    pub strict: bool,
    pub eager_input_streaming: bool,
    pub input_schema: Value,
}

impl From<&ToolDef> for WireTool {
    fn from(t: &ToolDef) -> Self {
        WireTool {
            name: t.name,
            description: t.description,
            strict: true,
            eager_input_streaming: true,
            input_schema: t.input_schema.clone(),
        }
    }
}

/// `{"type":"auto"}` or `{"type":"none"}`: [`ToolChoice`] has no forced variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct WireToolChoice {
    #[serde(rename = "type")]
    pub kind: ToolChoice,
}

/// `POST {base}/v1/messages`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MessagesRequest {
    pub model: &'static str,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<Vec<TextBlock>>,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<WireTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<WireToolChoice>,
    pub output_config: OutputConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Top-level automatic caching (chat turns): caches the last cacheable block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

fn system_of(blocks: &[TextBlock]) -> Option<Vec<TextBlock>> {
    (!blocks.is_empty()).then(|| blocks.to_vec())
}

impl MessagesRequest {
    /// A structured call: JSON comes only from `output_config.format`.
    pub fn structured(req: &StructuredRequest) -> Self {
        MessagesRequest {
            model: req.model.as_str(),
            max_tokens: req.max_tokens,
            system: system_of(&req.system),
            messages: req.messages.clone(),
            tools: None,
            tool_choice: None,
            output_config: OutputConfig {
                effort: req.effort,
                format: Some(OutputFormat::json_schema((req.schema.json)())),
            },
            stream: None,
            cache_control: None,
        }
    }

    /// A streamed chat turn: tools sorted by name, top-level cache breakpoint, no `format`.
    /// With no tools, neither `tools` nor `tool_choice` is sent.
    pub fn chat_turn(req: &ChatTurnRequest) -> Self {
        let mut tools: Vec<WireTool> = req.tools.iter().map(WireTool::from).collect();
        tools.sort_by(|a, b| a.name.cmp(b.name));
        let has_tools = !tools.is_empty();
        MessagesRequest {
            model: req.model.as_str(),
            max_tokens: req.max_tokens,
            system: system_of(&req.system),
            messages: req.messages.clone(),
            tools: has_tools.then_some(tools),
            tool_choice: has_tools.then_some(WireToolChoice {
                kind: req.tool_choice,
            }),
            output_config: OutputConfig {
                effort: req.effort,
                format: None,
            },
            stream: Some(true),
            cache_control: Some(CacheControl::default()),
        }
    }
}

/// `usage` of a response. The cache counters may be absent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct WireUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: Option<u64>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<u64>,
}

impl WireUsage {
    pub fn to_ai_usage(self) -> AiUsage {
        AiUsage {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cache_creation_tokens: self.cache_creation_input_tokens.unwrap_or(0),
            cache_read_tokens: self.cache_read_input_tokens.unwrap_or(0),
        }
    }
}

/// A Messages response. `content` stays raw [`Value`]s; when replayed they are
/// deterministically re-serialised (sorted keys).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct MessageResponse {
    pub id: String,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    pub model: String,
    pub content: Vec<Value>,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub stop_sequence: Option<String>,
    #[serde(default)]
    pub stop_details: Option<Value>,
    #[serde(default)]
    pub usage: WireUsage,
}

/// `{"type":"error","error":{"type":…,"message":…},"request_id":…}`. Only `error.type` is
/// kept; the message (which may echo request content) is never read.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct ErrorBody {
    #[serde(default)]
    pub error: ErrorDetail,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct ErrorDetail {
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
}

impl ErrorBody {
    /// `error.type` of an error response body, if it parses.
    pub fn error_type(body: &[u8]) -> Option<String> {
        serde_json::from_slice::<ErrorBody>(body)
            .ok()
            .and_then(|b| b.error.kind)
    }
}

/// One request of a batch. `params` never carries `stream`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BatchRequest {
    pub custom_id: String,
    pub params: MessagesRequest,
}

/// `POST {base}/v1/messages/batches`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BatchCreate {
    pub requests: Vec<BatchRequest>,
}

impl BatchCreate {
    pub fn new(items: Vec<(String, MessagesRequest)>) -> Self {
        BatchCreate {
            requests: items
                .into_iter()
                .map(|(custom_id, mut params)| {
                    params.stream = None;
                    BatchRequest { custom_id, params }
                })
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct RequestCounts {
    #[serde(default)]
    pub processing: u64,
    #[serde(default)]
    pub succeeded: u64,
    #[serde(default)]
    pub errored: u64,
    #[serde(default)]
    pub canceled: u64,
    #[serde(default)]
    pub expired: u64,
}

/// The response of batch create and of `GET {base}/v1/messages/batches/{id}`. Timestamps stay
/// strings (they are only logged); missing ones read as `None`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct BatchObject {
    pub id: String,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    pub processing_status: BatchStatus,
    #[serde(default)]
    pub request_counts: RequestCounts,
    #[serde(default)]
    pub results_url: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub ended_at: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
}

/// The `result` of one results JSONL line.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BatchResult {
    Succeeded { message: Box<MessageResponse> },
    Errored { error: ErrorBody },
    Canceled,
    Expired,
}

impl BatchResult {
    pub fn outcome(&self) -> BatchOutcome {
        match self {
            BatchResult::Succeeded { .. } => BatchOutcome::Succeeded,
            BatchResult::Errored { .. } => BatchOutcome::Errored,
            BatchResult::Canceled => BatchOutcome::Canceled,
            BatchResult::Expired => BatchOutcome::Expired,
        }
    }

    /// `error.error.type` of an errored item.
    pub fn error_type(&self) -> Option<&str> {
        match self {
            BatchResult::Errored { error } => error.error.kind.as_deref(),
            _ => None,
        }
    }
}

/// One line of the batch results JSONL.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct BatchResultLine {
    pub custom_id: String,
    pub result: BatchResult,
}
