//! The chat tool-callback endpoint (ADR-R3-1, ADR-R3-6). The sidecar's SDK runs the tool rounds
//! of a turn and posts each call here: `POST /v1/tools` with `authorization: Bearer <token>` and
//! `{name, input}`; we answer `{content, isError}`.
//!
//! Each turn registers its [`ToolHandler`] and gets a fresh token (32 random bytes, hex), so two
//! turns never see each other's calls. Dropping the [`ToolRegistration`] unregisters it, and the
//! token also expires after its TTL (the longest the turn can run). An
//! unknown or expired token is 404, a missing one 401. The body limit is 64 KiB. A call that runs
//! longer than the per-call timeout (50 s: under the sidecar's 55 s callback fetch, over T23's
//! 45 s engine wait) answers `isError` with `{"available":false,"reason":"engine busy"}`.
//!
//! Tokens are looked up by their SHA-256 and then compared in constant time; they are never
//! logged and `Debug` redacts them.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::port::{AiError, ToolHandler, UnavailableReason};

/// `AI_TOOL_BIND` default.
pub const DEFAULT_TOOL_BIND: &str = "0.0.0.0:7311";
/// The route the sidecar posts to.
pub const TOOLS_PATH: &str = "/v1/tools";
/// Request body limit.
pub const TOOL_BODY_LIMIT: usize = 64 * 1024;
/// Per-call timeout (T23 engine wait 45 s < this < the sidecar's callback fetch 55 s).
pub const TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(50);

type Key = [u8; 32];

struct Entry {
    token: String,
    handler: Arc<dyn ToolHandler>,
    /// After this the token is expired (404) even if the registration was never dropped.
    expires_at: Instant,
}

struct Inner {
    registry: Mutex<HashMap<Key, Entry>>,
    call_timeout: Duration,
}

fn key_of(token: &str) -> Key {
    Sha256::digest(token.as_bytes()).into()
}

fn purge_expired(reg: &mut HashMap<Key, Entry>) {
    let now = Instant::now();
    reg.retain(|_, e| e.expires_at > now);
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl Inner {
    fn lookup(&self, token: &str) -> Option<Arc<dyn ToolHandler>> {
        let mut reg = self.registry.lock().unwrap_or_else(|e| e.into_inner());
        purge_expired(&mut reg);
        let entry = reg.get(&key_of(token))?;
        ct_eq(entry.token.as_bytes(), token.as_bytes()).then(|| entry.handler.clone())
    }
}

/// The running endpoint. Dropping the last `Arc` stops the server.
pub struct ToolServer {
    inner: Arc<Inner>,
    public_url: String,
    local_addr: SocketAddr,
    task: tokio::task::JoinHandle<()>,
}

impl std::fmt::Debug for ToolServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolServer")
            .field("public_url", &self.public_url)
            .field("local_addr", &self.local_addr)
            .finish_non_exhaustive()
    }
}

impl Drop for ToolServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// One turn's registration: the callback `url` and `token` to send to the sidecar.
pub struct ToolRegistration {
    pub url: String,
    pub token: String,
    key: Key,
    inner: Weak<Inner>,
}

impl std::fmt::Debug for ToolRegistration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistration")
            .field("url", &self.url)
            .field("token", &"[redacted]")
            .finish()
    }
}

impl Drop for ToolRegistration {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            inner
                .registry
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&self.key);
        }
    }
}

#[derive(Deserialize)]
struct CallBody {
    name: String,
    #[serde(default)]
    input: Value,
}

fn status(code: StatusCode) -> Response {
    code.into_response()
}

async fn handle(State(inner): State<Arc<Inner>>, headers: HeaderMap, body: Bytes) -> Response {
    let Some(token) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|t| !t.is_empty())
    else {
        return status(StatusCode::UNAUTHORIZED);
    };
    let Some(handler) = inner.lookup(token) else {
        return status(StatusCode::NOT_FOUND);
    };
    let Ok(call) = serde_json::from_slice::<CallBody>(&body) else {
        return status(StatusCode::BAD_REQUEST);
    };
    let started = std::time::Instant::now();
    let reply =
        tokio::time::timeout(inner.call_timeout, handler.call(&call.name, call.input)).await;
    let (content, is_error) = match reply {
        Ok(r) => (r.content, r.is_error),
        Err(_) => (
            json!({"available": false, "reason": "engine busy"}).to_string(),
            true,
        ),
    };
    tracing::debug!(
        method = "POST",
        path = TOOLS_PATH,
        status = 200u16,
        is_error,
        duration_ms = started.elapsed().as_millis() as u64,
        "tool call"
    );
    axum::Json(json!({"content": content, "isError": is_error})).into_response()
}

impl ToolServer {
    /// Binds `bind` and serves `/v1/tools` with the 50 s per-call timeout. `public_url` is how
    /// the sidecar reaches this process (e.g. `http://rust-worker:7311`).
    pub async fn start(bind: SocketAddr, public_url: String) -> Result<Arc<ToolServer>, AiError> {
        Self::start_with_timeout(bind, public_url, TOOL_CALL_TIMEOUT).await
    }

    /// [`ToolServer::start`] with another per-call timeout (tests).
    pub async fn start_with_timeout(
        bind: SocketAddr,
        public_url: String,
        call_timeout: Duration,
    ) -> Result<Arc<ToolServer>, AiError> {
        let unavailable = || AiError::Unavailable {
            reason: UnavailableReason::Connection,
            retry_after: None,
        };
        let listener = tokio::net::TcpListener::bind(bind)
            .await
            .map_err(|_| unavailable())?;
        let local_addr = listener.local_addr().map_err(|_| unavailable())?;
        let inner = Arc::new(Inner {
            registry: Mutex::new(HashMap::new()),
            call_timeout,
        });
        let app = Router::new()
            .route(TOOLS_PATH, post(handle))
            .layer(DefaultBodyLimit::max(TOOL_BODY_LIMIT))
            .with_state(inner.clone());
        let task = tokio::spawn(async move {
            if axum::serve(listener, app).await.is_err() {
                tracing::warn!(path = TOOLS_PATH, "tool endpoint stopped");
            }
        });
        Ok(Arc::new(ToolServer {
            inner,
            public_url: public_url.trim_end_matches('/').to_owned(),
            local_addr,
            task,
        }))
    }

    /// From `AI_TOOL_BIND` (default `0.0.0.0:7311`) and `AI_TOOL_PUBLIC_URL`. `Ok(None)` when no
    /// public URL is configured (chat tools are then unavailable).
    pub async fn start_from_env() -> Result<Option<Arc<ToolServer>>, AiError> {
        let Some(public_url) = std::env::var("AI_TOOL_PUBLIC_URL")
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
        else {
            return Ok(None);
        };
        let bind = std::env::var("AI_TOOL_BIND")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_TOOL_BIND.to_owned());
        let bind: SocketAddr = bind.trim().parse().map_err(|_| AiError::Request {
            status: 0,
            api_error_type: Some("invalid_tool_bind".to_owned()),
        })?;
        Self::start(bind, public_url).await.map(Some)
    }

    /// The bound address (useful with port 0).
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Registers one turn's handler under a fresh token that expires after `ttl` (the longest
    /// the turn can run), even if the registration is never dropped.
    pub fn register(&self, h: Arc<dyn ToolHandler>, ttl: Duration) -> ToolRegistration {
        let token = hex::encode(rand::random::<[u8; 32]>());
        let key = key_of(&token);
        let mut reg = self
            .inner
            .registry
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        purge_expired(&mut reg);
        reg.insert(
            key,
            Entry {
                token: token.clone(),
                handler: h,
                expires_at: Instant::now() + ttl,
            },
        );
        drop(reg);
        ToolRegistration {
            url: format!("{}{TOOLS_PATH}", self.public_url),
            token,
            key,
            inner: Arc::downgrade(&self.inner),
        }
    }

    /// The number of live (registered and unexpired) registrations.
    pub fn registrations(&self) -> usize {
        let mut reg = self
            .inner
            .registry
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        purge_expired(&mut reg);
        reg.len()
    }
}
