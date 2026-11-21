//! Per-platform circuit breaker (port of `apps/worker/src/http/circuit-breaker.ts`).
//!
//! Closed: calls pass. After `failure_threshold` consecutive failures it opens and rejects with
//! `CircuitOpen` without calling the closure. After `cooldown` it is half-open: a single probe
//! goes through; success closes it, a failure re-opens it for another cooldown.
//!
//! While a half-open probe is in flight, other callers get `retry_in` floored at
//! `max(5 s, probe_timeout)` plus jitter, so they neither tight-loop nor stampede.
//! Rate limiting (429) is the platform answering, so it is not counted as a failure.

use std::collections::hash_map::RandomState;
use std::future::Future;
use std::hash::{BuildHasher, Hasher};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use futures::stream::{self, BoxStream};
use futures::{FutureExt, StreamExt};

use crate::{Headers, PlatformClient, PlatformError};

/// Minimum `retry_in` reported to callers that hit a half-open circuit with a probe in flight.
pub const PROBE_RETRY_FLOOR: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

/// True when the error means the platform is unhealthy. A 404 is the caller's problem and a 429
/// is the platform being alive and rate limiting us; neither is an outage.
pub fn is_outage(e: &PlatformError) -> bool {
    !matches!(
        e,
        PlatformError::NotFound { .. }
            | PlatformError::RateLimited { .. }
            | PlatformError::CircuitOpen { .. }
    )
}

#[derive(Clone)]
pub struct BreakerOptions {
    /// Consecutive failures that open the circuit.
    pub failure_threshold: u32,
    /// How long the circuit stays open before one probe is allowed.
    pub cooldown: Duration,
    /// Which errors count as a failure. Default: [`is_outage`].
    pub is_failure: Arc<dyn Fn(&PlatformError) -> bool + Send + Sync>,
    pub now: Arc<dyn Fn() -> Instant + Send + Sync>,
    /// Longest a probe can run (the request timeout); floors `retry_in` for blocked callers.
    pub probe_timeout: Duration,
    /// Random extra delay added to the probe-in-flight `retry_in`.
    pub jitter: Arc<dyn Fn() -> Duration + Send + Sync>,
}

impl Default for BreakerOptions {
    fn default() -> Self {
        BreakerOptions {
            failure_threshold: 5,
            cooldown: Duration::from_secs(60),
            is_failure: Arc::new(is_outage),
            now: Arc::new(Instant::now),
            probe_timeout: Duration::from_secs(30),
            jitter: Arc::new(default_jitter),
        }
    }
}

/// 0..2000 ms from std's randomly keyed hasher (no extra dependency).
fn default_jitter() -> Duration {
    let n = RandomState::new().build_hasher().finish();
    Duration::from_millis(n % 2_000)
}

#[derive(Default)]
struct Inner {
    failures: u32,
    opened_at: Option<Instant>,
    probing: bool,
}

pub struct CircuitBreaker {
    opts: BreakerOptions,
    inner: Mutex<Inner>,
}

/// Releases the probe slot even if the probing future is dropped mid-flight.
struct ProbeGuard<'a>(&'a CircuitBreaker);

impl Drop for ProbeGuard<'_> {
    fn drop(&mut self) {
        self.0.lock().probing = false;
    }
}

impl CircuitBreaker {
    pub fn new(opts: BreakerOptions) -> Self {
        CircuitBreaker {
            opts,
            inner: Mutex::new(Inner::default()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn state_of(&self, inner: &Inner, now: Instant) -> CircuitState {
        match inner.opened_at {
            None => CircuitState::Closed,
            Some(at) if now.saturating_duration_since(at) >= self.opts.cooldown => {
                CircuitState::HalfOpen
            }
            Some(_) => CircuitState::Open,
        }
    }

    pub fn state(&self) -> CircuitState {
        let now = (self.opts.now)();
        self.state_of(&self.lock(), now)
    }

    pub async fn exec<T, F>(&self, f: impl FnOnce() -> F) -> Result<T, PlatformError>
    where
        F: Future<Output = Result<T, PlatformError>>,
    {
        let probe = {
            let now = (self.opts.now)();
            let mut inner = self.lock();
            match self.state_of(&inner, now) {
                CircuitState::Closed => false,
                CircuitState::Open => {
                    let at = inner.opened_at.unwrap_or(now);
                    let retry_in = (at + self.opts.cooldown).saturating_duration_since(now);
                    return Err(PlatformError::CircuitOpen { retry_in });
                }
                CircuitState::HalfOpen if inner.probing => {
                    let floor = PROBE_RETRY_FLOOR.max(self.opts.probe_timeout);
                    return Err(PlatformError::CircuitOpen {
                        retry_in: floor + (self.opts.jitter)(),
                    });
                }
                CircuitState::HalfOpen => {
                    inner.probing = true;
                    true
                }
            }
        };
        let _guard = probe.then(|| ProbeGuard(self));

        // The lock is never held across this await.
        let result = f().await;

        let now = (self.opts.now)();
        let mut inner = self.lock();
        match &result {
            Ok(_) => {
                inner.failures = 0;
                inner.opened_at = None;
            }
            Err(e) if (self.opts.is_failure)(e) => {
                inner.failures += 1;
                if probe || inner.failures >= self.opts.failure_threshold {
                    inner.opened_at = Some(now);
                }
            }
            Err(_) if probe => {
                // The platform answered: it is up.
                inner.failures = 0;
                inner.opened_at = None;
            }
            Err(_) => {}
        }
        result
    }
}

/// A `PlatformClient` whose every request (and every streamed chunk) goes through `breaker`.
pub fn with_circuit_breaker(
    client: Arc<dyn PlatformClient>,
    breaker: Arc<CircuitBreaker>,
) -> Arc<dyn PlatformClient> {
    Arc::new(Breaking { client, breaker })
}

struct Breaking {
    client: Arc<dyn PlatformClient>,
    breaker: Arc<CircuitBreaker>,
}

impl PlatformClient for Breaking {
    fn get_json<'a>(
        &'a self,
        url: &'a str,
        headers: Headers,
    ) -> BoxFuture<'a, Result<serde_json::Value, PlatformError>> {
        async move {
            self.breaker
                .exec(|| self.client.get_json(url, headers))
                .await
        }
        .boxed()
    }

    fn get_ndjson<'a>(
        &'a self,
        url: &'a str,
        headers: Headers,
    ) -> BoxStream<'a, Result<serde_json::Value, PlatformError>> {
        type Inner<'a> = BoxStream<'a, Result<serde_json::Value, PlatformError>>;
        struct St<'a> {
            inner: Option<Inner<'a>>,
            headers: Option<Headers>,
            done: bool,
        }
        let st = St {
            inner: None,
            headers: Some(headers),
            done: false,
        };
        stream::unfold(st, move |mut st| async move {
            if st.done {
                return None;
            }
            let res = self
                .breaker
                .exec(|| async {
                    if st.inner.is_none() {
                        let h = st.headers.take().unwrap_or_default();
                        st.inner = Some(self.client.get_ndjson(url, h));
                    }
                    let inner = st.inner.as_mut().expect("initialised above");
                    match inner.next().await {
                        None => Ok::<_, PlatformError>(None),
                        Some(Ok(v)) => Ok(Some(v)),
                        Some(Err(e)) => Err(e),
                    }
                })
                .await;
            match res {
                Ok(Some(v)) => Some((Ok(v), st)),
                Ok(None) => None,
                Err(e) => {
                    st.done = true;
                    Some((Err(e), st))
                }
            }
        })
        .boxed()
    }
}
