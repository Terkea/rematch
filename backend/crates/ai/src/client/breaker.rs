//! The circuit breaker around the AI client (ADR-R3-6: Rust owns it, the sidecar holds no rules).
//!
//! - A failure is `Unavailable` with reason `Connection`, `Server` or `Overloaded`; that includes
//!   the sidecar's own 529 when its queue is full. Every other outcome (a success, `RateLimited`,
//!   `NoApiKey`, `Budget`, a 4xx, a 409 `turn_in_progress`) records a success and is returned
//!   unchanged.
//! - `failure_threshold` consecutive failures open the circuit. While open, calls get
//!   `Unavailable(CircuitOpen)` without reaching the inner client. After `cooldown` it is
//!   half-open and admits one probe: its success closes the circuit, its failure reopens it for a
//!   full cooldown. A probe dropped before its outcome records nothing and frees the probe slot.
//! - Time comes from the injected `now`, so tests drive it without sleeping.

use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use futures::stream::{self, BoxStream, StreamExt};

use super::port::{
    AiClient, AiError, BatchItem, BatchResultItem, BatchStatus, ChatTurnRequest, SchemaResolver,
    StructuredRequest, StructuredResult, ToolHandler, TurnEvent, UnavailableReason,
};

pub struct BreakerConfig {
    /// Consecutive failures that open the circuit (5).
    pub failure_threshold: u32,
    /// How long the circuit stays open before it admits a probe (60 s).
    pub cooldown: Duration,
    pub now: Arc<dyn Fn() -> Instant + Send + Sync>,
}

impl Default for BreakerConfig {
    fn default() -> Self {
        BreakerConfig {
            failure_threshold: 5,
            cooldown: Duration::from_secs(60),
            now: Arc::new(Instant::now),
        }
    }
}

impl fmt::Debug for BreakerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BreakerConfig")
            .field("failure_threshold", &self.failure_threshold)
            .field("cooldown", &self.cooldown)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BreakerState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Default)]
struct Inner {
    /// Consecutive failures while closed.
    failures: u32,
    /// When the circuit last opened; `None` while closed.
    opened_at: Option<Instant>,
    probe_in_flight: bool,
}

#[derive(Debug)]
pub struct Breaker {
    cfg: BreakerConfig,
    inner: Mutex<Inner>,
}

/// Whether an outcome counts as an outage.
pub fn is_failure(e: &AiError) -> bool {
    matches!(
        e,
        AiError::Unavailable {
            reason: UnavailableReason::Connection
                | UnavailableReason::Server
                | UnavailableReason::Overloaded,
            ..
        }
    )
}

fn circuit_open() -> AiError {
    AiError::Unavailable {
        reason: UnavailableReason::CircuitOpen,
        retry_after: None,
    }
}

impl Breaker {
    pub fn new(cfg: BreakerConfig) -> Self {
        Breaker {
            cfg,
            inner: Mutex::new(Inner::default()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn state_of(&self, i: &Inner) -> BreakerState {
        match i.opened_at {
            None => BreakerState::Closed,
            Some(at) if (self.cfg.now)().saturating_duration_since(at) < self.cfg.cooldown => {
                BreakerState::Open
            }
            Some(_) => BreakerState::HalfOpen,
        }
    }

    pub fn state(&self) -> BreakerState {
        self.state_of(&self.lock())
    }

    /// Admits a call, or `None` while open (or half-open with its probe in flight).
    pub fn try_acquire(&self) -> Option<BreakerPermit<'_>> {
        let mut i = self.lock();
        let probe = match self.state_of(&i) {
            BreakerState::Closed => false,
            BreakerState::Open => return None,
            BreakerState::HalfOpen if i.probe_in_flight => return None,
            BreakerState::HalfOpen => {
                i.probe_in_flight = true;
                true
            }
        };
        Some(BreakerPermit {
            breaker: self,
            probe,
            settled: false,
        })
    }

    fn record(&self, probe: bool, failure: bool) {
        let mut i = self.lock();
        if probe {
            i.probe_in_flight = false;
            if failure {
                i.opened_at = Some((self.cfg.now)());
                tracing::warn!("ai circuit breaker probe failed; circuit reopened");
            } else {
                i.failures = 0;
                i.opened_at = None;
                tracing::info!("ai circuit breaker closed");
            }
            return;
        }
        // A call admitted while closed that ends after the circuit opened changes nothing.
        if i.opened_at.is_some() {
            return;
        }
        if failure {
            i.failures = i.failures.saturating_add(1);
            if i.failures >= self.cfg.failure_threshold {
                i.opened_at = Some((self.cfg.now)());
                tracing::warn!(failures = i.failures, "ai circuit breaker opened");
            }
        } else {
            i.failures = 0;
        }
    }
}

/// One admitted call. Settle it with [`BreakerPermit::record`]; dropping it unsettled records
/// nothing (and frees the half-open probe slot).
pub struct BreakerPermit<'a> {
    breaker: &'a Breaker,
    probe: bool,
    settled: bool,
}

impl BreakerPermit<'_> {
    /// Records the call's outcome by [`is_failure`].
    pub fn record<T>(mut self, outcome: &Result<T, AiError>) {
        self.settled = true;
        let failure = outcome.as_ref().err().is_some_and(is_failure);
        self.breaker.record(self.probe, failure);
    }
}

impl Drop for BreakerPermit<'_> {
    fn drop(&mut self) {
        if self.probe && !self.settled {
            self.breaker.lock().probe_in_flight = false;
        }
    }
}

struct BreakerClient {
    inner: Arc<dyn AiClient>,
    breaker: Arc<Breaker>,
}

/// Wraps a client (normally `AiRuntime`) so `structured`, `stream_turn`, `batch_submit` and
/// `batch_status` run through the breaker. `batch_results` and `batches_enabled` delegate.
pub fn with_breaker(inner: Arc<dyn AiClient>, b: Arc<Breaker>) -> Arc<dyn AiClient> {
    Arc::new(BreakerClient { inner, breaker: b })
}

impl BreakerClient {
    async fn guard<'a, T>(
        &'a self,
        call: impl Future<Output = Result<T, AiError>> + 'a,
    ) -> Result<T, AiError> {
        let permit = self.breaker.try_acquire().ok_or_else(circuit_open)?;
        let out = call.await;
        permit.record(&out);
        out
    }
}

impl AiClient for BreakerClient {
    fn structured<'a>(
        &'a self,
        req: &'a StructuredRequest,
    ) -> BoxFuture<'a, Result<StructuredResult, AiError>> {
        // The inner call is made inside the async block, so an open circuit never makes it.
        Box::pin(self.guard(async move { self.inner.structured(req).await }))
    }

    /// The outcome is the first `Err` or the final `Done`; the stream is admitted at its first
    /// poll, and a stream dropped before its outcome records nothing.
    fn stream_turn<'a>(
        &'a self,
        req: &'a ChatTurnRequest,
        tools: Option<Arc<dyn ToolHandler>>,
    ) -> BoxStream<'a, Result<TurnEvent, AiError>> {
        stream::once(async move { self.breaker.try_acquire().map(|p| (p, tools)) })
            .flat_map(move |admitted| match admitted {
                None => stream::once(async { Err(circuit_open()) }).boxed(),
                Some((permit, tools)) => {
                    let mut permit = Some(permit);
                    self.inner
                        .stream_turn(req, tools)
                        .map(move |item| {
                            let terminal = matches!(item, Err(_) | Ok(TurnEvent::Done(_)));
                            if terminal && let Some(p) = permit.take() {
                                p.record(&item);
                            }
                            item
                        })
                        .boxed()
                }
            })
            .boxed()
    }

    fn batches_enabled(&self) -> bool {
        self.inner.batches_enabled()
    }

    fn batch_submit<'a>(
        &'a self,
        items: &'a [BatchItem],
    ) -> BoxFuture<'a, Result<String, AiError>> {
        Box::pin(self.guard(async move { self.inner.batch_submit(items).await }))
    }

    fn batch_status<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<BatchStatus, AiError>> {
        Box::pin(self.guard(async move { self.inner.batch_status(id).await }))
    }

    fn batch_results<'a>(
        &'a self,
        id: &'a str,
        resolver: &'a dyn SchemaResolver,
    ) -> BoxStream<'a, Result<BatchResultItem, AiError>> {
        self.inner.batch_results(id, resolver)
    }
}
