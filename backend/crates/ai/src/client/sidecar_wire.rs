//! The JSON of the Agent SDK sidecar contract (Task 10, ADR-R3-6 "Contract and isolation"). Keys
//! are camelCase. Responses ignore unknown fields, so the sidecar can add fields first.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::model::Effort;
use super::port::{AiUsage, ModelUsage, SessionPath};
use super::wire::OutputFormat;

/// `POST /v1/query`: one stateless structured call.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarQuery {
    pub model: &'static str,
    pub effort: Effort,
    pub max_tokens: u32,
    pub timeout_ms: u64,
    pub max_turns: u32,
    pub system_prompt: String,
    /// Text blocks only, without `cache_control` (caching on the sidecar path is automatic).
    pub prompt: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<OutputFormat>,
}

/// A chat tool as the sidecar converts it to an SDK MCP tool.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarTool {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

/// Where the sidecar posts tool calls. The token is a per-turn secret: `Debug` redacts it.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct ToolCallback {
    pub url: String,
    pub token: String,
}

impl std::fmt::Debug for ToolCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolCallback")
            .field("url", &self.url)
            .field("token", &"[redacted]")
            .finish()
    }
}

/// `POST /v1/chat`: one chat turn over SSE.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarChat {
    pub model: &'static str,
    pub effort: Effort,
    pub max_tokens: u32,
    pub timeout_ms: u64,
    pub max_turns: u32,
    pub system_prompt: String,
    pub prompt: Vec<Value>,
    pub tools: Vec<SidecarTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_callback: Option<ToolCallback>,
    pub conversation_id: uuid::Uuid,
    pub resume: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rebuild_context: Option<String>,
}

/// One model's token counts in `modelUsage` / `sessionModelUsage`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarModelUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
}

impl SidecarModelUsage {
    pub fn to_ai_usage(self) -> AiUsage {
        AiUsage {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cache_creation_tokens: self.cache_creation_input_tokens,
            cache_read_tokens: self.cache_read_input_tokens,
        }
    }
}

/// A usage map as [`ModelUsage`] entries, sorted by model id.
pub fn model_usage(map: &BTreeMap<String, SidecarModelUsage>) -> Vec<ModelUsage> {
    map.iter()
        .map(|(model, u)| ModelUsage {
            model: model.clone(),
            usage: u.to_ai_usage(),
        })
        .collect()
}

/// The reply of `/v1/query`, and the data of the chat `done` event (plus `sessionModelUsage`
/// and `session`).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarResult {
    #[serde(default)]
    pub subtype: String,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub structured_output: Option<Value>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub num_turns: u32,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub model_usage: BTreeMap<String, SidecarModelUsage>,
    #[serde(default)]
    pub session_model_usage: Option<BTreeMap<String, SidecarModelUsage>>,
    #[serde(default)]
    pub sdk_cost_usd: Option<f64>,
    #[serde(default)]
    pub session: Option<SessionPath>,
}

impl SidecarResult {
    /// `error_max_turns` becomes `"max_turns"`; otherwise the API stop reason as is.
    pub fn stop_reason(&self) -> Option<String> {
        if self.subtype == "error_max_turns" {
            return Some("max_turns".to_owned());
        }
        self.stop_reason.clone()
    }
}

/// `delta {text, messageId?}`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeltaEvent {
    pub text: String,
    #[serde(default)]
    pub message_id: Option<String>,
}

/// `retract {messageIds, reason}`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetractEvent {
    pub message_ids: Vec<String>,
    pub reason: String,
}

/// `tool_use {id, name, input, messageId?}`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ToolUseEvent {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub input: Option<Value>,
    /// The API message of the tool call; a later `retract` naming it removes the call.
    #[serde(default, rename = "messageId")]
    pub message_id: Option<String>,
}

/// `error`: the Anthropic error body plus `status`. Only `error.type` is kept.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ErrorEvent {
    #[serde(default)]
    pub error: super::wire::ErrorDetail,
    pub status: u16,
}

/// The credential the sidecar runs with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    ApiKey,
    Cloud,
    Oauth,
    None,
}

/// `GET /health` (no token, no model call, no secret).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub ok: bool,
    pub auth: AuthKind,
    #[serde(default)]
    pub in_flight: u32,
    #[serde(default)]
    pub queued: u32,
    #[serde(default)]
    pub max_concurrency: u32,
    #[serde(default)]
    pub sdk_version: String,
    #[serde(default)]
    pub mirror_errors: Option<u64>,
    #[serde(default)]
    pub nul_stripped_entries: Option<u64>,
    #[serde(default)]
    pub lock_lost: Option<u64>,
}
