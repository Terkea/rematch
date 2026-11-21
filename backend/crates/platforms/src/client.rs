//! Serial, rate-limited platform HTTP client (port of `apps/worker/src/http/platform-client.ts`).
//!
//! One request at a time per client; the lock is held for a whole NDJSON stream and released when
//! the stream is dropped. HTTP 429 waits `retry_after` and retries up to `max_retries` times.

use std::collections::VecDeque;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures::future::BoxFuture;
use futures::stream::{BoxStream, StreamExt};
use tokio::sync::{Mutex, OwnedMutexGuard};
use tokio::time::timeout;

use crate::types::{Headers, Platform, PlatformClient, PlatformError};

#[derive(Clone, Debug)]
pub struct ClientOptions {
    pub name: Platform,
    pub retry_after: Duration,
    pub max_retries: u32,
    /// Per-request timeout for headers (and the whole body for `get_json`).
    pub request_timeout: Duration,
    /// `get_ndjson`: maximum silence between chunks (not a total cap).
    pub stream_idle_timeout: Duration,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            name: Platform::Lichess,
            retry_after: Duration::from_secs(60),
            max_retries: 3,
            request_timeout: Duration::from_secs(30),
            stream_idle_timeout: Duration::from_secs(60),
        }
    }
}

/// Fixed, anonymous User-Agent: the project URL only, never an email (ADR 0043).
pub fn user_agent() -> String {
    "chess-improver/0.1 (+https://github.com/chess-improver)".to_owned()
}

pub struct HttpPlatformClient {
    opts: ClientOptions,
    http: reqwest::Client,
    lock: Arc<Mutex<()>>,
    user_agent: String,
}

enum Attempt {
    Retry,
    Done(reqwest::Response),
}

impl HttpPlatformClient {
    pub fn new(opts: ClientOptions) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let user_agent = user_agent();
        Self {
            opts,
            http: reqwest::Client::new(),
            lock: Arc::new(Mutex::new(())),
            user_agent,
        }
    }

    fn timeout_err(&self, url: &str, d: Duration) -> PlatformError {
        PlatformError::Timeout {
            url: url.to_owned(),
            ms: d.as_millis() as u64,
        }
    }

    /// One attempt; call only while holding the lock. A 429 is consumed and reported as `Retry`
    /// (or `RateLimited` once retries are exhausted).
    async fn attempt(
        &self,
        url: &str,
        headers: &Headers,
        attempt: u32,
    ) -> Result<Attempt, PlatformError> {
        let mut req = self.http.get(url);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        req = req.header(reqwest::header::USER_AGENT, &self.user_agent);
        let res = req
            .send()
            .await
            .map_err(|e| PlatformError::Transport(e.to_string()))?;
        let status = res.status().as_u16();
        match status {
            429 => {
                let _ = res.bytes().await;
                if attempt >= self.opts.max_retries {
                    return Err(PlatformError::RateLimited {
                        url: url.to_owned(),
                        attempts: attempt + 1,
                    });
                }
                Ok(Attempt::Retry)
            }
            404 => Err(PlatformError::NotFound {
                url: url.to_owned(),
            }),
            s if !(200..300).contains(&s) => Err(PlatformError::Http {
                platform: self.opts.name.as_str().to_owned(),
                status: s,
                url: url.to_owned(),
            }),
            _ => Ok(Attempt::Done(res)),
        }
    }

    /// Runs attempts until one succeeds. `finish` runs inside the request timeout, so for
    /// `get_json` the body read is covered too. Call only while holding the lock.
    async fn request<T, F, Fut>(
        &self,
        url: &str,
        headers: &Headers,
        finish: F,
    ) -> Result<T, PlatformError>
    where
        F: Fn(reqwest::Response) -> Fut,
        Fut: Future<Output = Result<T, PlatformError>>,
    {
        let d = self.opts.request_timeout;
        let mut n = 0;
        loop {
            let step = async {
                match self.attempt(url, headers, n).await? {
                    Attempt::Retry => Ok(None),
                    Attempt::Done(res) => finish(res).await.map(Some),
                }
            };
            match timeout(d, step).await {
                Err(_) => return Err(self.timeout_err(url, d)),
                Ok(Err(e)) => return Err(e),
                Ok(Ok(Some(v))) => return Ok(v),
                Ok(Ok(None)) => {}
            }
            n += 1;
            tokio::time::sleep(self.opts.retry_after).await;
        }
    }
}

type Item = Result<serde_json::Value, PlatformError>;

struct NdjsonState<'a> {
    client: &'a HttpPlatformClient,
    url: String,
    headers: Headers,
    phase: Phase,
}

enum Phase {
    Start,
    Streaming {
        /// Held until the stream is dropped or finishes.
        _guard: OwnedMutexGuard<()>,
        body: BoxStream<'static, Result<Bytes, reqwest::Error>>,
        buf: Vec<u8>,
        ready: VecDeque<Item>,
    },
    Done,
}

fn parse_line(line: &[u8]) -> Option<Item> {
    let text = String::from_utf8_lossy(line);
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    Some(serde_json::from_str(t).map_err(|e| PlatformError::Decode(e.to_string())))
}

impl PlatformClient for HttpPlatformClient {
    fn get_json<'a>(
        &'a self,
        url: &'a str,
        headers: Headers,
    ) -> BoxFuture<'a, Result<serde_json::Value, PlatformError>> {
        Box::pin(async move {
            let _guard = self.lock.lock().await;
            self.request(url, &headers, |res| async move {
                let bytes = res
                    .bytes()
                    .await
                    .map_err(|e| PlatformError::Transport(e.to_string()))?;
                serde_json::from_slice(&bytes).map_err(|e| PlatformError::Decode(e.to_string()))
            })
            .await
        })
    }

    fn get_ndjson<'a>(&'a self, url: &'a str, headers: Headers) -> BoxStream<'a, Item> {
        let state = NdjsonState {
            client: self,
            url: url.to_owned(),
            headers,
            phase: Phase::Start,
        };
        futures::stream::unfold(state, |mut st| async move {
            loop {
                match &mut st.phase {
                    Phase::Done => return None,
                    Phase::Start => {
                        let guard = st.client.lock.clone().lock_owned().await;
                        let res = st
                            .client
                            .request(&st.url, &st.headers, |r| async move { Ok(r) })
                            .await;
                        match res {
                            Ok(r) => {
                                st.phase = Phase::Streaming {
                                    _guard: guard,
                                    body: r.bytes_stream().boxed(),
                                    buf: Vec::new(),
                                    ready: VecDeque::new(),
                                };
                            }
                            Err(e) => {
                                st.phase = Phase::Done;
                                return Some((Err(e), st));
                            }
                        }
                    }
                    Phase::Streaming {
                        body, buf, ready, ..
                    } => {
                        if let Some(item) = ready.pop_front() {
                            if item.is_err() {
                                st.phase = Phase::Done;
                            }
                            return Some((item, st));
                        }
                        let idle = st.client.opts.stream_idle_timeout;
                        match timeout(idle, body.next()).await {
                            Err(_) => {
                                let e = st.client.timeout_err(&st.url, idle);
                                st.phase = Phase::Done;
                                return Some((Err(e), st));
                            }
                            Ok(Some(Ok(chunk))) => {
                                buf.extend_from_slice(&chunk);
                                while let Some(nl) = buf.iter().position(|b| *b == b'\n') {
                                    let line: Vec<u8> = buf.drain(..=nl).collect();
                                    if let Some(item) = parse_line(&line) {
                                        ready.push_back(item);
                                    }
                                }
                            }
                            Ok(Some(Err(e))) => {
                                st.phase = Phase::Done;
                                return Some((Err(PlatformError::Transport(e.to_string())), st));
                            }
                            Ok(None) => {
                                if let Some(item) = parse_line(buf) {
                                    ready.push_back(item);
                                }
                                buf.clear();
                                if ready.is_empty() {
                                    return None;
                                }
                                // Drain `ready`, then the empty body ends the stream.
                                *body = futures::stream::empty().boxed();
                            }
                        }
                    }
                }
            }
        })
        .boxed()
    }
}
