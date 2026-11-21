//! Port of apps/worker/test/recovery/circuit-breaker.test.ts plus the 12b review carry-overs.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use futures::stream::{self, BoxStream};
use futures::{FutureExt, StreamExt};
use platforms::breaker::{
    BreakerOptions, CircuitBreaker, CircuitState, is_outage, with_circuit_breaker,
};
use platforms::{Headers, PlatformClient, PlatformError};
use serde_json::{Value, json};
use tokio::sync::oneshot;

struct Clock {
    base: Instant,
    ms: Arc<AtomicU64>,
}

impl Clock {
    fn new() -> Self {
        Clock {
            base: Instant::now(),
            ms: Arc::new(AtomicU64::new(1_000)),
        }
    }
    fn set(&self, ms: u64) {
        self.ms.store(ms, Ordering::SeqCst);
    }
    fn add(&self, ms: u64) {
        self.ms.fetch_add(ms, Ordering::SeqCst);
    }
    fn now_fn(&self) -> Arc<dyn Fn() -> Instant + Send + Sync> {
        let (base, ms) = (self.base, self.ms.clone());
        Arc::new(move || base + Duration::from_millis(ms.load(Ordering::SeqCst)))
    }
    fn opts(&self) -> BreakerOptions {
        BreakerOptions {
            now: self.now_fn(),
            ..BreakerOptions::default()
        }
    }
}

fn http503() -> PlatformError {
    PlatformError::Http {
        platform: "lichess".into(),
        status: 503,
        url: "u".into(),
    }
}

async fn fail(b: &CircuitBreaker) -> Result<&'static str, PlatformError> {
    b.exec(|| async { Err(http503()) }).await
}

async fn ok(b: &CircuitBreaker) -> Result<&'static str, PlatformError> {
    b.exec(|| async { Ok("ok") }).await
}

#[tokio::test]
async fn circuit_opens_after_5_failures() {
    let clock = Clock::new();
    let b = CircuitBreaker::new(BreakerOptions {
        failure_threshold: 5,
        cooldown: Duration::from_millis(60_000),
        ..clock.opts()
    });
    for _ in 0..5 {
        assert!(matches!(fail(&b).await, Err(PlatformError::Http { .. })));
    }
    assert_eq!(b.state(), CircuitState::Open);

    let called = AtomicUsize::new(0);
    let r = b
        .exec(|| async {
            called.fetch_add(1, Ordering::SeqCst);
            Ok(1)
        })
        .await;
    assert!(matches!(r, Err(PlatformError::CircuitOpen { .. })));
    assert_eq!(called.load(Ordering::SeqCst), 0);

    clock.add(59_999);
    assert_eq!(b.state(), CircuitState::Open);
    clock.add(1);
    assert_eq!(b.state(), CircuitState::HalfOpen);
    assert_eq!(ok(&b).await.unwrap(), "ok");
    assert_eq!(b.state(), CircuitState::Closed);
}

#[tokio::test]
async fn uses_the_briefs_defaults_5_failures_60_s_cooldown() {
    let clock = Clock::new();
    clock.set(0);
    let b = CircuitBreaker::new(clock.opts());
    for _ in 0..4 {
        let _ = fail(&b).await;
    }
    assert_eq!(b.state(), CircuitState::Closed);
    let _ = fail(&b).await;
    assert_eq!(b.state(), CircuitState::Open);
    clock.set(60_000);
    assert_eq!(b.state(), CircuitState::HalfOpen);
}

#[tokio::test]
async fn a_failed_probe_re_opens_for_another_cooldown() {
    let clock = Clock::new();
    clock.set(0);
    let b = CircuitBreaker::new(BreakerOptions {
        failure_threshold: 2,
        cooldown: Duration::from_millis(100),
        ..clock.opts()
    });
    let _ = fail(&b).await;
    let _ = fail(&b).await;
    clock.set(100);
    assert!(matches!(fail(&b).await, Err(PlatformError::Http { .. })));
    assert_eq!(b.state(), CircuitState::Open);
    clock.set(199);
    assert_eq!(b.state(), CircuitState::Open);
    clock.set(200);
    assert_eq!(b.state(), CircuitState::HalfOpen);
}

#[tokio::test]
async fn allows_only_one_probe_at_a_time_while_half_open() {
    let clock = Clock::new();
    clock.set(0);
    let b = CircuitBreaker::new(BreakerOptions {
        failure_threshold: 1,
        cooldown: Duration::from_millis(10),
        ..clock.opts()
    });
    let _ = fail(&b).await;
    clock.set(10);
    let (tx, rx) = oneshot::channel::<()>();
    let mut probe = Box::pin(b.exec(|| async {
        rx.await.unwrap();
        Ok::<_, PlatformError>(())
    }));
    // Drive the probe until it is parked on the channel.
    assert!(probe.as_mut().now_or_never().is_none());
    assert!(matches!(
        ok(&b).await,
        Err(PlatformError::CircuitOpen { .. })
    ));
    tx.send(()).unwrap();
    probe.await.unwrap();
    assert_eq!(b.state(), CircuitState::Closed);
}

#[tokio::test]
async fn counts_consecutive_failures_only() {
    let clock = Clock::new();
    let b = CircuitBreaker::new(BreakerOptions {
        failure_threshold: 3,
        cooldown: Duration::from_millis(10),
        ..clock.opts()
    });
    let _ = fail(&b).await;
    let _ = fail(&b).await;
    ok(&b).await.unwrap();
    let _ = fail(&b).await;
    let _ = fail(&b).await;
    assert_eq!(b.state(), CircuitState::Closed);
}

#[tokio::test]
async fn ignores_errors_that_are_not_outages_404() {
    let clock = Clock::new();
    let b = CircuitBreaker::new(BreakerOptions {
        failure_threshold: 1,
        cooldown: Duration::from_millis(10),
        is_failure: Arc::new(is_outage),
        ..clock.opts()
    });
    let r: Result<(), _> = b
        .exec(|| async { Err(PlatformError::NotFound { url: "u".into() }) })
        .await;
    assert!(matches!(r, Err(PlatformError::NotFound { .. })));
    assert_eq!(b.state(), CircuitState::Closed);
    assert!(!is_outage(&PlatformError::NotFound { url: "u".into() }));
}

struct FakeClient {
    calls: Arc<AtomicUsize>,
}

impl PlatformClient for FakeClient {
    fn get_json<'a>(
        &'a self,
        _url: &'a str,
        _h: Headers,
    ) -> futures::future::BoxFuture<'a, Result<Value, PlatformError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        async { Err(PlatformError::Transport("down".into())) }.boxed()
    }
    fn get_ndjson<'a>(
        &'a self,
        _url: &'a str,
        _h: Headers,
    ) -> BoxStream<'a, Result<Value, PlatformError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        stream::iter(vec![Ok(json!({"n": 1}))]).boxed()
    }
}

#[tokio::test]
async fn wraps_a_platform_clients_get_json_and_get_ndjson() {
    let clock = Clock::new();
    let breaker = Arc::new(CircuitBreaker::new(BreakerOptions {
        failure_threshold: 1,
        cooldown: Duration::from_millis(60_000),
        ..clock.opts()
    }));
    let calls = Arc::new(AtomicUsize::new(0));
    let client = with_circuit_breaker(
        Arc::new(FakeClient {
            calls: calls.clone(),
        }),
        breaker.clone(),
    );
    let r = client.get_json("u", vec![]).await;
    assert!(matches!(r, Err(PlatformError::Transport(_))));
    assert_eq!(breaker.state(), CircuitState::Open);
    let mut s = client.get_ndjson("u", vec![]);
    let first = s.next().await.unwrap();
    assert!(matches!(first, Err(PlatformError::CircuitOpen { .. })));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn wrapped_ndjson_streams_items_then_ends_through_the_breaker() {
    let clock = Clock::new();
    let breaker = Arc::new(CircuitBreaker::new(clock.opts()));
    let calls = Arc::new(AtomicUsize::new(0));
    let client = with_circuit_breaker(
        Arc::new(FakeClient {
            calls: calls.clone(),
        }),
        breaker.clone(),
    );
    let items: Vec<_> = client.get_ndjson("u", vec![]).collect().await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].as_ref().unwrap(), &json!({"n": 1}));
    assert_eq!(breaker.state(), CircuitState::Closed);
}

fn half_open_with_probe(
    clock: &Clock,
    cooldown_ms: u64,
    probe_timeout: Duration,
    jitter: Duration,
) -> CircuitBreaker {
    clock.set(0);
    CircuitBreaker::new(BreakerOptions {
        failure_threshold: 1,
        cooldown: Duration::from_millis(cooldown_ms),
        probe_timeout,
        jitter: Arc::new(move || jitter),
        ..clock.opts()
    })
}

async fn retry_in_during_probe(b: &CircuitBreaker, clock: &Clock) -> Duration {
    let _ = fail(b).await;
    clock.add(10);
    let (tx, rx) = oneshot::channel::<()>();
    let mut probe = Box::pin(b.exec(|| async {
        rx.await.unwrap();
        Ok::<_, PlatformError>(())
    }));
    assert!(probe.as_mut().now_or_never().is_none());
    let got = match ok(b).await {
        Err(PlatformError::CircuitOpen { retry_in }) => retry_in,
        other => panic!("expected CircuitOpen, got {other:?}"),
    };
    tx.send(()).unwrap();
    probe.await.unwrap();
    got
}

#[tokio::test]
async fn half_open_probe_in_flight_floors_retry_in_at_5_s_plus_jitter() {
    let clock = Clock::new();
    let b = half_open_with_probe(
        &clock,
        10,
        Duration::from_secs(1),
        Duration::from_millis(700),
    );
    assert_eq!(
        retry_in_during_probe(&b, &clock).await,
        Duration::from_millis(5_700)
    );
}

#[tokio::test]
async fn half_open_probe_in_flight_floors_retry_in_at_probe_timeout_when_longer() {
    let clock = Clock::new();
    let b = half_open_with_probe(
        &clock,
        10,
        Duration::from_secs(30),
        Duration::from_millis(250),
    );
    assert_eq!(
        retry_in_during_probe(&b, &clock).await,
        Duration::from_millis(30_250)
    );
}

#[tokio::test]
async fn default_jitter_stays_within_bounds() {
    let clock = Clock::new();
    clock.set(0);
    let b = CircuitBreaker::new(BreakerOptions {
        failure_threshold: 1,
        cooldown: Duration::from_millis(10),
        probe_timeout: Duration::from_secs(1),
        ..clock.opts()
    });
    let d = retry_in_during_probe(&b, &clock).await;
    assert!(
        d >= Duration::from_secs(5) && d <= Duration::from_secs(8),
        "{d:?}"
    );
}

#[tokio::test]
async fn open_state_retry_in_is_the_remaining_cooldown() {
    let clock = Clock::new();
    let b = CircuitBreaker::new(BreakerOptions {
        failure_threshold: 1,
        cooldown: Duration::from_millis(60_000),
        ..clock.opts()
    });
    let _ = fail(&b).await;
    clock.add(10_000);
    match ok(&b).await {
        Err(PlatformError::CircuitOpen { retry_in }) => {
            assert_eq!(retry_in, Duration::from_millis(50_000))
        }
        other => panic!("expected CircuitOpen, got {other:?}"),
    }
}

#[tokio::test]
async fn rate_limited_429_is_not_counted_as_a_failure() {
    let clock = Clock::new();
    let b = CircuitBreaker::new(BreakerOptions {
        failure_threshold: 2,
        ..clock.opts()
    });
    for _ in 0..10 {
        let r: Result<(), _> = b
            .exec(|| async {
                Err(PlatformError::RateLimited {
                    url: "u".into(),
                    attempts: 3,
                })
            })
            .await;
        assert!(matches!(r, Err(PlatformError::RateLimited { .. })));
    }
    assert_eq!(b.state(), CircuitState::Closed);
}

#[tokio::test]
async fn overloaded_5xx_and_timeouts_count_as_failures() {
    for err in [
        http503(),
        PlatformError::Timeout {
            url: "u".into(),
            ms: 30_000,
        },
        PlatformError::Transport("reset".into()),
    ] {
        assert!(is_outage(&err), "{err:?}");
        let clock = Clock::new();
        let b = CircuitBreaker::new(BreakerOptions {
            failure_threshold: 1,
            ..clock.opts()
        });
        let e = err.clone();
        let _: Result<(), _> = b.exec(|| async { Err(e) }).await;
        assert_eq!(b.state(), CircuitState::Open);
    }
    assert!(!is_outage(&PlatformError::RateLimited {
        url: "u".into(),
        attempts: 3
    }));
}

#[tokio::test]
async fn non_outage_error_on_a_probe_closes_the_circuit() {
    let clock = Clock::new();
    clock.set(0);
    let b = CircuitBreaker::new(BreakerOptions {
        failure_threshold: 1,
        cooldown: Duration::from_millis(10),
        ..clock.opts()
    });
    let _ = fail(&b).await;
    clock.set(10);
    let _: Result<(), _> = b
        .exec(|| async { Err(PlatformError::NotFound { url: "u".into() }) })
        .await;
    assert_eq!(b.state(), CircuitState::Closed);
}

#[tokio::test]
async fn a_cancelled_probe_releases_the_probe_slot() {
    let clock = Clock::new();
    clock.set(0);
    let b = CircuitBreaker::new(BreakerOptions {
        failure_threshold: 1,
        cooldown: Duration::from_millis(10),
        ..clock.opts()
    });
    let _ = fail(&b).await;
    clock.set(10);
    {
        let mut probe = Box::pin(b.exec(futures::future::pending::<Result<(), PlatformError>>));
        assert!(probe.as_mut().now_or_never().is_none());
    } // dropped mid-flight
    assert_eq!(ok(&b).await.unwrap(), "ok");
    assert_eq!(b.state(), CircuitState::Closed);
}

fn http(status: u16) -> PlatformError {
    PlatformError::Http {
        platform: "lichess".into(),
        status,
        url: "u".into(),
    }
}

#[test]
fn is_outage_counts_http_only_from_500() {
    for s in [400, 401, 403, 404, 410, 422, 429] {
        assert!(!is_outage(&http(s)), "{s}");
    }
    for s in [500, 502, 503, 504] {
        assert!(is_outage(&http(s)), "{s}");
    }
    assert!(is_outage(&PlatformError::Decode("x".into())));
}

struct StatusClient(u16);

impl PlatformClient for StatusClient {
    fn get_json<'a>(
        &'a self,
        _url: &'a str,
        _h: Headers,
    ) -> futures::future::BoxFuture<'a, Result<Value, PlatformError>> {
        let s = self.0;
        async move { Err(http(s)) }.boxed()
    }
    fn get_ndjson<'a>(
        &'a self,
        _url: &'a str,
        _h: Headers,
    ) -> BoxStream<'a, Result<Value, PlatformError>> {
        stream::iter(vec![Err(http(self.0))]).boxed()
    }
}

async fn five_streams(status: u16) -> CircuitState {
    let clock = Clock::new();
    let breaker = Arc::new(CircuitBreaker::new(clock.opts()));
    let client = with_circuit_breaker(Arc::new(StatusClient(status)), breaker.clone());
    for _ in 0..5 {
        let items: Vec<_> = client.get_ndjson("u", vec![]).collect().await;
        assert!(matches!(items[0], Err(PlatformError::Http { .. })));
    }
    breaker.state()
}

#[tokio::test]
async fn five_403_streams_leave_the_breaker_closed() {
    assert_eq!(five_streams(403).await, CircuitState::Closed);
}

#[tokio::test]
async fn five_503_streams_open_the_breaker() {
    assert_eq!(five_streams(503).await, CircuitState::Open);
}

#[tokio::test]
async fn circuit_open_error_from_a_probe_leaves_state_unchanged() {
    let clock = Clock::new();
    clock.set(0);
    let b = CircuitBreaker::new(BreakerOptions {
        failure_threshold: 1,
        cooldown: Duration::from_millis(10),
        ..clock.opts()
    });
    let _ = fail(&b).await;
    clock.set(10);
    let _: Result<(), _> = b
        .exec(|| async {
            Err(PlatformError::CircuitOpen {
                retry_in: Duration::ZERO,
            })
        })
        .await;
    assert_eq!(b.state(), CircuitState::HalfOpen);
}

#[tokio::test]
async fn a_panicking_probe_releases_the_probe_slot() {
    let clock = Clock::new();
    clock.set(0);
    let b = Arc::new(CircuitBreaker::new(BreakerOptions {
        failure_threshold: 1,
        cooldown: Duration::from_millis(10),
        ..clock.opts()
    }));
    let _ = fail(&b).await;
    clock.set(10);
    let b2 = b.clone();
    let h = tokio::spawn(async move {
        b2.exec(|| async {
            if true {
                panic!("probe panics");
            }
            Ok::<(), PlatformError>(())
        })
        .await
    });
    assert!(h.await.unwrap_err().is_panic());
    assert_eq!(b.state(), CircuitState::HalfOpen);
    assert_eq!(ok(&b).await.unwrap(), "ok");
    assert_eq!(b.state(), CircuitState::Closed);
}

#[tokio::test]
async fn concurrent_callers_past_the_cooldown_run_exactly_one_probe() {
    let clock = Clock::new();
    clock.set(0);
    let b = CircuitBreaker::new(BreakerOptions {
        failure_threshold: 1,
        cooldown: Duration::from_millis(10),
        ..clock.opts()
    });
    let _ = fail(&b).await;
    clock.set(10);
    let invoked = AtomicUsize::new(0);
    let call = || {
        b.exec(|| async {
            invoked.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
            Ok::<_, PlatformError>(())
        })
    };
    let results = futures::future::join_all((0..8).map(|_| call())).await;
    assert_eq!(invoked.load(Ordering::SeqCst), 1);
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(
        results
            .iter()
            .filter(|r| r.is_err())
            .all(|r| matches!(r, Err(PlatformError::CircuitOpen { .. })))
    );
    assert_eq!(b.state(), CircuitState::Closed);
}
