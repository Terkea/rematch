//! The minimal Messages Batches client (ADR-R3-1, ADR-R3-6 decision 3): the SDK has no batch
//! support, and there is no official Rust SDK, so this is our own reqwest client at
//! `anthropic-version: 2026-10-21`. It is used only for the monthly and weekly regenerations.
//!
//! - `submit`: `POST {base}/v1/messages/batches` with [`BatchCreate`] of
//!   [`MessagesRequest::structured`] (explicit cache breakpoints are kept).
//! - `status`: `GET {base}/v1/messages/batches/{id}`.
//! - `results`: `GET {base}/v1/messages/batches/{id}/results`, streamed line by line with the
//!   idle timeout. The batch's `results_url` is never followed, so a forged host cannot receive
//!   the key.
//!
//! The non-streaming calls retry per [`super::retry`]. Logging is `debug` only: method, path,
//! status, attempt, duration and `request-id`.

use std::time::{Duration, Instant, SystemTime};

use futures::stream::{self, BoxStream, StreamExt};

use super::errors::{classify_batch_error_type, classify_transport, parse_retry_after, redact};
use super::parse::parse_model_output;
use super::port::{
    AiError, BatchItem, BatchResultItem, BatchStatus, ModelUsage, SchemaResolver, StructuredResult,
};
use super::retry::{retry_delay, should_retry};
use super::sidecar::{error_of, http_client};
use super::wire::{
    BATCHES_PATH, BatchCreate, BatchObject, BatchResult, BatchResultLine, DEFAULT_BASE_URL,
    MessagesRequest, REQUEST_ID_HEADER,
};

/// `anthropic-version` of every batch call (`2026-10-21`).
pub use super::wire::ANTHROPIC_VERSION;
/// The batch client's User-Agent.
pub const BATCHES_USER_AGENT: &str = "chess-improver/0.1";

/// `ANTHROPIC_API_KEY`. `Debug` and `Display` redact it; there is no `Serialize`.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
    pub fn new(key: impl Into<String>) -> Self {
        ApiKey(key.into())
    }

    /// `ANTHROPIC_API_KEY`, trimmed; empty means `None`. A dev OAuth token never reaches Rust.
    pub fn from_env() -> Option<Self> {
        let v = std::env::var("ANTHROPIC_API_KEY").ok()?;
        let v = v.trim();
        (!v.is_empty()).then(|| ApiKey(v.to_owned()))
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey([redacted])")
    }
}

impl std::fmt::Display for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[redacted]")
    }
}

#[derive(Clone, Debug)]
pub struct BatchesConfig {
    pub api_key: ApiKey,
    pub base_url: String,
    pub max_retries: u32,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub stream_idle_timeout: Duration,
    /// A `retry-after` above this is not slept: the call gives up and surfaces it.
    pub retry_after_cap: Duration,
    pub backoff_base: Duration,
}

impl BatchesConfig {
    pub fn new(api_key: ApiKey) -> Self {
        BatchesConfig {
            api_key,
            base_url: DEFAULT_BASE_URL.to_owned(),
            max_retries: 2,
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(120),
            stream_idle_timeout: Duration::from_secs(60),
            retry_after_cap: Duration::from_secs(30),
            backoff_base: Duration::from_millis(500),
        }
    }

    /// `None` without `ANTHROPIC_API_KEY`.
    pub fn from_env() -> Option<Self> {
        ApiKey::from_env().map(BatchesConfig::new)
    }
}

fn invalid_id() -> AiError {
    AiError::Request {
        status: 400,
        api_error_type: Some("invalid_request_error".to_owned()),
    }
}

/// Batch ids go into the path: only `[A-Za-z0-9_-]{1,128}`.
fn check_id(id: &str) -> Result<(), AiError> {
    let ok = !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if ok { Ok(()) } else { Err(invalid_id()) }
}

fn connection() -> AiError {
    AiError::Unavailable {
        reason: super::port::UnavailableReason::Connection,
        retry_after: None,
    }
}

fn ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

pub struct BatchesClient {
    cfg: BatchesConfig,
    http: reqwest::Client,
}

impl std::fmt::Debug for BatchesClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BatchesClient")
            .field("cfg", &self.cfg)
            .finish_non_exhaustive()
    }
}

impl BatchesClient {
    pub fn new(cfg: BatchesConfig) -> Result<Self, AiError> {
        let mut cfg = cfg;
        cfg.base_url = cfg.base_url.trim_end_matches('/').to_owned();
        let http = http_client(cfg.connect_timeout)?;
        Ok(BatchesClient { cfg, http })
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, format!("{}{path}", self.cfg.base_url))
            .header("x-api-key", self.cfg.api_key.expose())
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::USER_AGENT, BATCHES_USER_AGENT)
    }

    /// Sends a non-streaming call with retries and returns the 2xx body.
    async fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&[u8]>,
    ) -> Result<bytes::Bytes, AiError> {
        let mut attempt = 0u32;
        loop {
            let started = Instant::now();
            let mut rb = self
                .request(method.clone(), path)
                .timeout(self.cfg.request_timeout);
            if let Some(b) = body {
                rb = rb.body(b.to_vec());
            }
            let (err, retryable, retry_after) = match rb.send().await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let header = |name: &str| {
                        resp.headers()
                            .get(name)
                            .and_then(|v| v.to_str().ok())
                            .map(str::to_owned)
                    };
                    let request_id = header(REQUEST_ID_HEADER);
                    tracing::debug!(
                        method = %method,
                        path,
                        status,
                        attempt,
                        duration_ms = ms(started.elapsed()),
                        request_id = request_id.as_deref().map(redact),
                        "anthropic batch call"
                    );
                    if resp.status().is_success() {
                        return resp.bytes().await.map_err(|_| connection());
                    }
                    let x_should_retry = header("x-should-retry");
                    let retry_after = parse_retry_after(
                        header(reqwest::header::RETRY_AFTER.as_str()).as_deref(),
                        SystemTime::now(),
                    );
                    let retryable = should_retry(Some(status), x_should_retry.as_deref());
                    (error_of(resp).await, retryable, retry_after)
                }
                Err(e) => {
                    tracing::debug!(
                        method = %method,
                        path,
                        attempt,
                        duration_ms = ms(started.elapsed()),
                        "anthropic batch call failed"
                    );
                    let err = classify_transport(&e);
                    let retryable = err.is_unavailable() && should_retry(None, None);
                    (err, retryable, None)
                }
            };
            if !retryable || attempt >= self.cfg.max_retries {
                return Err(err);
            }
            let Some(delay) = retry_delay(
                attempt,
                retry_after,
                self.cfg.retry_after_cap,
                self.cfg.backoff_base,
            ) else {
                return Err(err);
            };
            tokio::time::sleep(delay).await;
            attempt += 1;
        }
    }

    fn batch_object(bytes: &[u8]) -> Result<BatchObject, AiError> {
        serde_json::from_slice(bytes)
            .map_err(|_| AiError::Response(redact("batch object could not be decoded")))
    }

    /// Creates a batch; returns its id.
    pub async fn submit(&self, items: &[BatchItem]) -> Result<String, AiError> {
        let create = BatchCreate::new(
            items
                .iter()
                .map(|i| (i.custom_id.clone(), MessagesRequest::structured(&i.req)))
                .collect(),
        );
        let body = serde_json::to_vec(&create)
            .map_err(|_| AiError::Response(redact("batch request could not be encoded")))?;
        let bytes = self
            .call(reqwest::Method::POST, BATCHES_PATH, Some(&body))
            .await?;
        Ok(Self::batch_object(&bytes)?.id)
    }

    pub async fn status(&self, id: &str) -> Result<BatchStatus, AiError> {
        check_id(id)?;
        let bytes = self
            .call(reqwest::Method::GET, &format!("{BATCHES_PATH}/{id}"), None)
            .await?;
        Ok(Self::batch_object(&bytes)?.processing_status)
    }

    /// Streams the results, one item per JSONL line. A line that does not decode yields
    /// [`AiError::Response`] (with no content) and the stream goes on; a broken or idle body
    /// ends it with `Unavailable(Connection)`.
    pub fn results<'a>(
        &'a self,
        id: &'a str,
        resolver: &'a dyn SchemaResolver,
    ) -> BoxStream<'a, Result<BatchResultItem, AiError>> {
        let start = async move {
            check_id(id)?;
            let path = format!("{BATCHES_PATH}/{id}/results");
            let started = Instant::now();
            let send = self.request(reqwest::Method::GET, &path).send();
            let resp = tokio::time::timeout(self.cfg.stream_idle_timeout, send)
                .await
                .map_err(|_| connection())?
                .map_err(|e| classify_transport(&e))?;
            tracing::debug!(
                method = "GET",
                path = %path,
                status = resp.status().as_u16(),
                attempt = 0u32,
                duration_ms = ms(started.elapsed()),
                "anthropic batch results"
            );
            if !resp.status().is_success() {
                return Err(error_of(resp).await);
            }
            Ok(Lines {
                body: resp.bytes_stream().boxed(),
                buf: Vec::new(),
                idle: self.cfg.stream_idle_timeout,
                done: false,
            })
        };
        stream::once(start)
            .flat_map(move |r| match r {
                Ok(lines) => lines
                    .into_stream()
                    .map(move |line| line.and_then(|l| item_of(&l, resolver)))
                    .boxed(),
                Err(e) => stream::once(async move { Err(e) }).boxed(),
            })
            .boxed()
    }
}

/// Splits a streamed body into non-empty lines.
struct Lines {
    body: BoxStream<'static, reqwest::Result<bytes::Bytes>>,
    buf: Vec<u8>,
    idle: Duration,
    done: bool,
}

impl Lines {
    fn take_line(&mut self) -> Option<Vec<u8>> {
        loop {
            let at = self.buf.iter().position(|&b| b == b'\n')?;
            let mut line: Vec<u8> = self.buf.drain(..=at).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if !line.iter().all(u8::is_ascii_whitespace) {
                return Some(line);
            }
        }
    }

    fn into_stream(self) -> BoxStream<'static, Result<Vec<u8>, AiError>> {
        stream::unfold(self, |mut s| async move {
            loop {
                if let Some(line) = s.take_line() {
                    return Some((Ok(line), s));
                }
                if s.done {
                    let rest = std::mem::take(&mut s.buf);
                    if rest.iter().all(u8::is_ascii_whitespace) {
                        return None;
                    }
                    return Some((Ok(rest), s));
                }
                match tokio::time::timeout(s.idle, s.body.next()).await {
                    Ok(Some(Ok(chunk))) => s.buf.extend_from_slice(&chunk),
                    Ok(None) => s.done = true,
                    _ => {
                        s.done = true;
                        s.buf.clear();
                        return Some((Err(connection()), s));
                    }
                }
            }
        })
        .boxed()
    }
}

fn item_of(line: &[u8], resolver: &dyn SchemaResolver) -> Result<BatchResultItem, AiError> {
    let parsed: BatchResultLine = serde_json::from_slice(line)
        .map_err(|_| AiError::Response(redact("batch result line could not be decoded")))?;
    let outcome = parsed.result.outcome();
    let mut item = BatchResultItem {
        custom_id: parsed.custom_id,
        outcome,
        result: None,
        error_type: None,
        error_kind: None,
        parse_error: None,
    };
    match parsed.result {
        BatchResult::Succeeded { message } => {
            let schema = resolver.resolve(&item.custom_id);
            let (value, parse_error, parse_issues) =
                parse_model_output(&message.content, message.stop_reason.as_deref(), schema);
            let usage = message.usage.to_ai_usage();
            item.parse_error = parse_error;
            item.result = Some(StructuredResult {
                parsed: value,
                parse_error,
                parse_issues,
                content: message.content,
                stop_reason: message.stop_reason,
                usage,
                model: message.model.clone(),
                request_id: None,
                model_usage: vec![ModelUsage {
                    model: message.model,
                    usage,
                }],
                sdk_cost_usd: None,
            });
        }
        BatchResult::Errored { error } => {
            let ty = super::sidecar::sanitize_error_type(error.error.kind);
            item.error_kind = Some(classify_batch_error_type(ty.as_deref()));
            item.error_type = ty;
        }
        BatchResult::Canceled | BatchResult::Expired => {}
    }
    Ok(item)
}
