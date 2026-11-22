//! The client of the Agent SDK sidecar (ADR 0062): every interactive Claude call goes through it.
//!
//! - `structured`: `POST /v1/query` with the flattened prompt and `outputFormat`. Rust checks the
//!   structured output with its own schema; the SDK's validation never replaces ours.
//! - `stream_turn`: `POST /v1/chat` over SSE, one whole agentic turn. The tools are served by
//!   [`ToolServer`] under a per-turn token.
//! - Retries happen only before the first response byte, only on a connection failure (the
//!   request never reached the sidecar) or a 529 (its queue is full), at most `max_retries`.
//!
//! Logging is `debug` only: method, path, status, attempt, duration. Never a body or a token.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use bytes::Bytes;
use futures::stream::{self, BoxStream, StreamExt};
use serde_json::json;

use super::errors::{classify_status, classify_transport, parse_retry_after, redact};
use super::flatten::flatten_for_sidecar;
use super::port::{
    AiError, ChatTurnRequest, ParseError, StructuredRequest, StructuredResult, ToolChoice,
    ToolHandler, ToolUse, TurnEvent, TurnResult, UnavailableReason, sum_usage,
};
use super::retry::retry_delay;
use super::sidecar_wire::{
    DeltaEvent, ErrorEvent, Health, RetractEvent, SidecarChat, SidecarQuery, SidecarResult,
    SidecarTool, ToolCallback, ToolUseEvent, model_usage,
};
use super::sse::SseDecoder;
use super::tool_server::{ToolRegistration, ToolServer};
use super::wire::{ErrorBody, OutputFormat, USER_AGENT};

/// `maxTurns` of a structured call (the SDK may retry its own structured output).
pub const STRUCTURED_MAX_TURNS: u32 = 3;
/// Backoff base and `retry-after` cap of the sidecar retries (the sidecar's queue-full 529 says
/// `retry-after: 5`).
pub const SIDECAR_BACKOFF_BASE: Duration = Duration::from_millis(250);
pub const SIDECAR_RETRY_AFTER_CAP: Duration = Duration::from_secs(5);
/// Extra time the HTTP client allows past `timeoutMs`, so the sidecar's own 504 arrives first.
pub const TIMEOUT_GRACE: Duration = Duration::from_secs(15);
/// The prefix the SDK gives the sidecar's in-process MCP tools (`mcpServers: {coach}`).
pub const COACH_TOOL_PREFIX: &str = "mcp__coach__";
/// Error bodies are read up to this size.
const MAX_ERROR_BODY: usize = 64 * 1024;

/// The sidecar bearer token (`AI_SIDECAR_TOKEN`). `Debug` and `Display` redact it; there is no
/// `Serialize`.
#[derive(Clone, PartialEq, Eq)]
pub struct SidecarToken(String);

impl SidecarToken {
    pub fn new(token: impl Into<String>) -> Self {
        SidecarToken(token.into())
    }

    /// `AI_SIDECAR_TOKEN`, trimmed; empty means `None`.
    pub fn from_env() -> Option<Self> {
        let v = std::env::var("AI_SIDECAR_TOKEN").ok()?;
        let v = v.trim();
        (!v.is_empty()).then(|| SidecarToken(v.to_owned()))
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SidecarToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SidecarToken([redacted])")
    }
}

impl std::fmt::Display for SidecarToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[redacted]")
    }
}

#[derive(Clone, Debug)]
pub struct SidecarConfig {
    /// `AI_SIDECAR_URL`, e.g. `http://ai-sidecar:7310`.
    pub base_url: String,
    pub token: SidecarToken,
    pub connect_timeout: Duration,
    /// `timeoutMs` of `/v1/query`.
    pub request_timeout: Duration,
    /// `timeoutMs` of `/v1/chat`.
    pub chat_timeout: Duration,
    /// The longest gap between two chat stream chunks (the sidecar pings every 15 s).
    pub stream_idle_timeout: Duration,
    pub max_retries: u32,
}

impl SidecarConfig {
    pub fn new(base_url: impl Into<String>, token: SidecarToken) -> Self {
        SidecarConfig {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            token,
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(600),
            chat_timeout: Duration::from_secs(180),
            stream_idle_timeout: Duration::from_secs(60),
            max_retries: 1,
        }
    }

    /// `None` unless both `AI_SIDECAR_URL` and `AI_SIDECAR_TOKEN` are set.
    pub fn from_env() -> Option<SidecarConfig> {
        let url = std::env::var("AI_SIDECAR_URL").ok()?;
        let url = url.trim();
        if url.is_empty() {
            return None;
        }
        Some(SidecarConfig::new(url, SidecarToken::from_env()?))
    }
}

/// Keeps an API error type only when it looks like one (`[a-z_]{1,64}`): it comes from a body.
pub(crate) fn sanitize_error_type(t: Option<String>) -> Option<String> {
    t.filter(|t| {
        !t.is_empty() && t.len() <= 64 && t.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
    })
}

fn connection() -> AiError {
    AiError::Unavailable {
        reason: UnavailableReason::Connection,
        retry_after: None,
    }
}

fn undecodable(what: &str) -> AiError {
    AiError::Response(redact(&format!("{what} could not be decoded")))
}

fn ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

/// Reads at most [`MAX_ERROR_BODY`] of an error response and classifies it.
pub(crate) async fn error_of(resp: reqwest::Response) -> AiError {
    let status = resp.status().as_u16();
    let retry_after = parse_retry_after(
        resp.headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok()),
        SystemTime::now(),
    );
    let mut body = Vec::new();
    let mut chunks = resp.bytes_stream();
    while let Some(Ok(c)) = chunks.next().await {
        body.extend_from_slice(&c);
        if body.len() > MAX_ERROR_BODY {
            break;
        }
    }
    let ty = sanitize_error_type(ErrorBody::error_type(&body));
    classify_status(status, ty.as_deref(), retry_after)
}

/// Builds a reqwest client with the ring provider installed (reqwest uses
/// `rustls-no-provider`).
pub(crate) fn http_client(connect_timeout: Duration) -> Result<reqwest::Client, AiError> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(connect_timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| classify_transport(&e))
}

pub struct SidecarClient {
    cfg: SidecarConfig,
    http: reqwest::Client,
    tools: Option<Arc<ToolServer>>,
}

impl std::fmt::Debug for SidecarClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SidecarClient")
            .field("cfg", &self.cfg)
            .finish_non_exhaustive()
    }
}

impl SidecarClient {
    /// `tools` serves the chat tool callbacks; without it no chat turn offers tools.
    pub fn new(cfg: SidecarConfig, tools: Option<Arc<ToolServer>>) -> Result<Self, AiError> {
        let http = http_client(cfg.connect_timeout)?;
        Ok(SidecarClient { cfg, http, tools })
    }

    /// `GET /health`.
    pub async fn health(&self) -> Result<Health, AiError> {
        let url = format!("{}/health", self.cfg.base_url);
        let resp = self
            .http
            .get(url)
            .timeout(self.cfg.connect_timeout + Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| classify_transport(&e))?;
        if !resp.status().is_success() {
            return Err(error_of(resp).await);
        }
        let body = resp.bytes().await.map_err(|_| connection())?;
        serde_json::from_slice(&body).map_err(|_| undecodable("sidecar health"))
    }

    /// Posts `body`, retrying before the first response byte on a refused connection or a 529.
    /// Returns a 2xx response; any other status is classified.
    async fn post(
        &self,
        route: &'static str,
        body: &impl serde::Serialize,
        timeout: Duration,
    ) -> Result<reqwest::Response, AiError> {
        let url = format!("{}{route}", self.cfg.base_url);
        let body = serde_json::to_vec(body).map_err(|_| undecodable("sidecar request"))?;
        let mut attempt = 0u32;
        loop {
            let started = Instant::now();
            let sent = self
                .http
                .post(&url)
                .bearer_auth(self.cfg.token.expose())
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .timeout(timeout)
                .body(body.clone())
                .send()
                .await;
            let (err, retry_after) = match sent {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    tracing::debug!(
                        method = "POST",
                        path = route,
                        status,
                        attempt,
                        duration_ms = ms(started.elapsed()),
                        "sidecar call"
                    );
                    if resp.status().is_success() {
                        return Ok(resp);
                    }
                    let ra = parse_retry_after(
                        resp.headers()
                            .get(reqwest::header::RETRY_AFTER)
                            .and_then(|v| v.to_str().ok()),
                        SystemTime::now(),
                    );
                    let err = error_of(resp).await;
                    if status != 529 {
                        return Err(err);
                    }
                    (err, ra)
                }
                Err(e) => {
                    tracing::debug!(
                        method = "POST",
                        path = route,
                        attempt,
                        duration_ms = ms(started.elapsed()),
                        "sidecar call failed"
                    );
                    let err = classify_transport(&e);
                    if !e.is_connect() {
                        return Err(err);
                    }
                    (err, None)
                }
            };
            if attempt >= self.cfg.max_retries {
                return Err(err);
            }
            let Some(delay) = retry_delay(
                attempt,
                retry_after,
                SIDECAR_RETRY_AFTER_CAP,
                SIDECAR_BACKOFF_BASE,
            ) else {
                return Err(err);
            };
            tokio::time::sleep(delay).await;
            attempt += 1;
        }
    }

    pub async fn structured(&self, req: &StructuredRequest) -> Result<StructuredResult, AiError> {
        let (system_prompt, prompt) = flatten_for_sidecar(&req.system, &req.messages)?;
        let body = SidecarQuery {
            model: req.model.as_str(),
            effort: req.effort,
            max_tokens: req.max_tokens,
            timeout_ms: ms(self.cfg.request_timeout),
            max_turns: STRUCTURED_MAX_TURNS,
            system_prompt,
            prompt,
            output_format: Some(OutputFormat::json_schema((req.schema.json)())),
        };
        let resp = self
            .post("/v1/query", &body, self.cfg.request_timeout + TIMEOUT_GRACE)
            .await?;
        let bytes = resp.bytes().await.map_err(|_| connection())?;
        let out: SidecarResult =
            serde_json::from_slice(&bytes).map_err(|_| undecodable("sidecar result"))?;
        Ok(map_structured(req, out))
    }

    pub fn stream_turn<'a>(
        &'a self,
        req: &'a ChatTurnRequest,
        tools: Option<Arc<dyn ToolHandler>>,
    ) -> BoxStream<'a, Result<TurnEvent, AiError>> {
        let start = async move {
            let (body, registration) = self.chat_body(req, tools)?;
            // The sidecar sends the headers once a slot is free (its queue waits at most 30 s),
            // so the idle timeout also bounds the wait for them.
            let resp = tokio::time::timeout(
                self.cfg.stream_idle_timeout,
                self.post("/v1/chat", &body, self.cfg.chat_timeout + TIMEOUT_GRACE),
            )
            .await
            .map_err(|_| connection())??;
            Ok::<_, AiError>(TurnStream {
                body: resp.bytes_stream().boxed(),
                decoder: SseDecoder::default(),
                pending: VecDeque::new(),
                finished: false,
                idle: self.cfg.stream_idle_timeout,
                model: req.model.as_str(),
                tool_uses: Vec::new(),
                message: None,
                registration,
            })
        };
        stream::once(start)
            .flat_map(|r| match r {
                Ok(turn) => turn.into_stream().boxed(),
                Err(e) => stream::once(async move { Err(e) }).boxed(),
            })
            .boxed()
    }

    fn chat_body(
        &self,
        req: &ChatTurnRequest,
        tools: Option<Arc<dyn ToolHandler>>,
    ) -> Result<(SidecarChat, Option<ToolRegistration>), AiError> {
        if req.resume && req.rebuild_context.as_deref().is_none_or(str::is_empty) {
            return Err(AiError::Request {
                status: 400,
                api_error_type: Some("invalid_request_error".to_owned()),
            });
        }
        let (system_prompt, prompt) = flatten_for_sidecar(&req.system, &req.messages)?;
        let offer = req.tool_choice != ToolChoice::None;
        let registration = match (offer, tools, &self.tools) {
            // The longest this turn can run on our side: the sidecar's `timeoutMs`, the HTTP
            // grace past it, and one idle wait (headers while queued). The registration is
            // dropped earlier, as soon as the terminal event is queued or the stream is dropped.
            (true, Some(h), Some(server)) => Some(server.register(
                h,
                self.cfg.chat_timeout + TIMEOUT_GRACE + self.cfg.stream_idle_timeout,
            )),
            _ => None,
        };
        let mut wire_tools: Vec<SidecarTool> = if registration.is_some() {
            req.tools
                .iter()
                .map(|t| SidecarTool {
                    name: t.name,
                    description: t.description,
                    input_schema: t.input_schema.clone(),
                })
                .collect()
        } else {
            Vec::new()
        };
        wire_tools.sort_by(|a, b| a.name.cmp(b.name));
        let body = SidecarChat {
            model: req.model.as_str(),
            effort: req.effort,
            max_tokens: req.max_tokens,
            timeout_ms: ms(self.cfg.chat_timeout),
            max_turns: req.max_turns,
            system_prompt,
            prompt,
            tools: wire_tools,
            tool_callback: registration.as_ref().map(|r| ToolCallback {
                url: r.url.clone(),
                token: r.token.clone(),
            }),
            conversation_id: req.conversation_id,
            resume: req.resume,
            rebuild_context: req.rebuild_context.clone(),
        };
        Ok((body, registration))
    }
}

/// Maps a `/v1/query` reply, in order: refusal, `max_tokens`, the SDK's structured-output retry
/// limit, a missing output, then Rust's own schema check.
fn map_structured(req: &StructuredRequest, out: SidecarResult) -> StructuredResult {
    let stop_reason = out.stop_reason();
    let entries = model_usage(&out.model_usage);
    let text = match &out.structured_output {
        Some(v) => v.to_string(),
        None => out.text.clone(),
    };
    let content = if text.is_empty() {
        Vec::new()
    } else {
        vec![json!({"type": "text", "text": text})]
    };
    let (parsed, parse_error, parse_issues) = match (stop_reason.as_deref(), &out.structured_output)
    {
        (Some("refusal"), _) => (None, Some(ParseError::Refusal), None),
        (Some("max_tokens"), _) => (None, Some(ParseError::Truncated), None),
        _ if out.subtype == "error_max_structured_output_retries" => {
            (None, Some(ParseError::SchemaMismatch), None)
        }
        (_, None) => (None, Some(ParseError::NoText), None),
        (_, Some(v)) => match (req.schema.check)(v) {
            Ok(()) => (Some(v.clone()), None, None),
            Err(issues) => (None, Some(ParseError::SchemaMismatch), Some(issues)),
        },
    };
    StructuredResult {
        parsed,
        parse_error,
        parse_issues,
        content,
        stop_reason,
        usage: sum_usage(&entries),
        model: req.model.as_str().to_owned(),
        request_id: None,
        model_usage: entries,
        sdk_cost_usd: out.sdk_cost_usd,
    }
}

/// The state of one chat stream. The registration keeps the turn's tools reachable until the
/// terminal event is queued (or the stream is dropped, or its TTL runs out).
struct TurnStream {
    body: BoxStream<'static, reqwest::Result<Bytes>>,
    decoder: SseDecoder,
    pending: VecDeque<Result<TurnEvent, AiError>>,
    finished: bool,
    idle: Duration,
    model: &'static str,
    /// Each tool use with the API message it belongs to, so a retract can remove it.
    tool_uses: Vec<(Option<String>, ToolUse)>,
    message: Option<String>,
    registration: Option<ToolRegistration>,
}

impl TurnStream {
    fn into_stream(self) -> BoxStream<'static, Result<TurnEvent, AiError>> {
        stream::unfold(self, |mut s| async move {
            loop {
                if let Some(item) = s.pending.pop_front() {
                    return Some((item, s));
                }
                if s.finished {
                    return None;
                }
                match tokio::time::timeout(s.idle, s.body.next()).await {
                    Ok(Some(Ok(chunk))) => {
                        for ev in s.decoder.push(&chunk) {
                            s.on_event(ev.event.as_deref(), &ev.data);
                            if s.finished {
                                break;
                            }
                        }
                        if !s.finished && s.decoder.overflowed() {
                            s.fail(AiError::Response(redact("sidecar event too large")));
                        }
                    }
                    // Idle too long, a broken body, or an end before the terminal event.
                    _ => s.fail(connection()),
                }
            }
        })
        .boxed()
    }

    /// Queues the terminal item. The turn is over, so its tools are unregistered at once.
    fn finish(&mut self, item: Result<TurnEvent, AiError>) {
        self.pending.push_back(item);
        self.finished = true;
        self.registration = None;
    }

    fn fail(&mut self, e: AiError) {
        self.finish(Err(e));
    }

    fn parse<T: serde::de::DeserializeOwned>(&mut self, data: &str) -> Option<T> {
        match serde_json::from_str(data) {
            Ok(v) => Some(v),
            Err(_) => {
                self.fail(undecodable("sidecar event"));
                None
            }
        }
    }

    fn on_event(&mut self, name: Option<&str>, data: &str) {
        match name {
            Some("delta") => {
                let Some(d) = self.parse::<DeltaEvent>(data) else {
                    return;
                };
                if let Some(id) = d.message_id
                    && self.message.as_deref() != Some(id.as_str())
                {
                    self.message = Some(id.clone());
                    self.pending.push_back(Ok(TurnEvent::Message(id)));
                }
                self.pending.push_back(Ok(TurnEvent::TextDelta(d.text)));
            }
            Some("retract") => {
                let Some(r) = self.parse::<RetractEvent>(data) else {
                    return;
                };
                self.tool_uses
                    .retain(|(m, _)| m.as_ref().is_none_or(|m| !r.message_ids.contains(m)));
                self.pending.push_back(Ok(TurnEvent::Retract {
                    message_ids: r.message_ids,
                    reason: r.reason,
                }));
            }
            Some("tool_use") => {
                let Some(t) = self.parse::<ToolUseEvent>(data) else {
                    return;
                };
                // The SDK names MCP tools `mcp__coach__<name>`; Rust's tools are bare names.
                let name = match t.name.strip_prefix(COACH_TOOL_PREFIX) {
                    Some(bare) => bare.to_owned(),
                    None => t.name,
                };
                self.tool_uses.push((
                    t.message_id,
                    ToolUse {
                        id: t.id,
                        name,
                        input: t.input,
                    },
                ));
            }
            Some("done") => {
                let Some(out) = self.parse::<SidecarResult>(data) else {
                    return;
                };
                let turn = self.turn_result(out);
                self.finish(Ok(TurnEvent::Done(turn)));
            }
            Some("error") => {
                let Some(e) = self.parse::<ErrorEvent>(data) else {
                    return;
                };
                let ty = sanitize_error_type(e.error.kind);
                self.fail(classify_status(e.status, ty.as_deref(), None));
            }
            _ => {}
        }
    }

    fn turn_result(&mut self, out: SidecarResult) -> TurnResult {
        let entries = model_usage(&out.model_usage);
        let content = if out.text.is_empty() {
            Vec::new()
        } else {
            vec![json!({"type": "text", "text": out.text})]
        };
        TurnResult {
            content,
            stop_reason: out.stop_reason(),
            tool_uses: std::mem::take(&mut self.tool_uses)
                .into_iter()
                .map(|(_, t)| t)
                .collect(),
            usage: sum_usage(&entries),
            model: self.model.to_owned(),
            model_usage: entries,
            sdk_cost_usd: out.sdk_cost_usd,
            session_model_usage: out
                .session_model_usage
                .as_ref()
                .map(model_usage)
                .unwrap_or_default(),
            session: out.session,
        }
    }
}
