//! The circuit breaker. Ports the `withBreaker` block of
//! `packages/ai/test/client/fake-breaker.test.ts` and the `breaker and rate limits` block of
//! `fix-round-1.test.ts`, against the real [`Breaker`] with an injected clock.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ai::client::breaker::{Breaker, BreakerConfig, BreakerState, with_breaker};
use ai::client::errors::classify_status;
use ai::client::fake::{FakeAiClient, FakeBatch, FakeScript, FakeStructured, FakeTurn};
use ai::client::model::{AiModel, Effort};
use ai::client::port::{
    AiClient, AiError, BatchItem, BatchStatus, ChatTurnRequest, OutputSchema, SchemaResolver,
    StructuredRequest, ToolChoice, TurnEvent, UnavailableReason,
};
use futures::StreamExt;
use serde_json::json;

static ANY: OutputSchema = OutputSchema {
    id: "any",
    version: 1,
    json: || json!({}),
    check: |_| Ok(()),
};

fn sreq() -> StructuredRequest {
    StructuredRequest {
        model: AiModel::Haiku55,
        system: vec![],
        messages: vec![],
        max_tokens: 10,
        effort: Effort::Low,
        schema: &ANY,
    }
}

fn treq() -> ChatTurnRequest {
    ChatTurnRequest {
        model: AiModel::Sonnet55,
        system: vec![],
        tools: vec![],
        tool_choice: ToolChoice::Auto,
        messages: vec![],
        max_tokens: 10,
        effort: Effort::Low,
        max_turns: 3,
        conversation_id: uuid::Uuid::nil(),
        resume: false,
        rebuild_context: None,
    }
}

fn unavailable(reason: UnavailableReason) -> AiError {
    AiError::Unavailable {
        reason,
        retry_after: None,
    }
}

fn circuit_open(e: &AiError) -> bool {
    matches!(
        e,
        AiError::Unavailable {
            reason: UnavailableReason::CircuitOpen,
            retry_after: None
        }
    )
}

/// A manual clock: `advance` moves it; nothing sleeps.
#[derive(Clone)]
struct Clock {
    base: Instant,
    offset: Arc<Mutex<Duration>>,
}

impl Clock {
    fn new() -> Self {
        Clock {
            base: Instant::now(),
            offset: Arc::new(Mutex::new(Duration::ZERO)),
        }
    }

    fn advance(&self, d: Duration) {
        *self.offset.lock().unwrap() += d;
    }

    fn breaker(&self) -> Arc<Breaker> {
        let c = self.clone();
        Arc::new(Breaker::new(BreakerConfig {
            failure_threshold: 5,
            cooldown: Duration::from_secs(60),
            now: Arc::new(move || c.base + *c.offset.lock().unwrap()),
        }))
    }
}

/// A fake whose structured calls all return `outcome(n)`.
fn scripted(
    outcome: impl Fn(usize) -> Result<FakeStructured, AiError> + Send + Sync + 'static,
) -> Arc<FakeAiClient> {
    Arc::new(FakeAiClient::new(FakeScript {
        structured: Some(Box::new(move |_, n| outcome(n))),
        ..FakeScript::default()
    }))
}

fn ok() -> Result<FakeStructured, AiError> {
    Ok(FakeStructured {
        parsed: Some(json!(1)),
        ..FakeStructured::default()
    })
}

async fn run_n(ai: &dyn AiClient, n: usize) -> Vec<Result<(), AiError>> {
    let req = sreq();
    let mut out = Vec::new();
    for _ in 0..n {
        out.push(ai.structured(&req).await.map(|_| ()));
    }
    out
}

#[test]
fn defaults_are_five_failures_and_sixty_seconds() {
    let cfg = BreakerConfig::default();
    assert_eq!(cfg.failure_threshold, 5);
    assert_eq!(cfg.cooldown, Duration::from_secs(60));
    assert_eq!(Breaker::new(cfg).state(), BreakerState::Closed);
}

#[tokio::test]
async fn open_breaker_gives_circuit_open_without_calling_inner() {
    let inner = scripted(|_| Err(unavailable(UnavailableReason::Server)));
    let b = Clock::new().breaker();
    let ai = with_breaker(inner.clone(), b.clone());
    run_n(&*ai, 5).await;
    assert_eq!(b.state(), BreakerState::Open);
    assert_eq!(inner.calls().len(), 5);

    assert!(circuit_open(&ai.structured(&sreq()).await.unwrap_err()));
    let ev: Vec<_> = ai.stream_turn(&treq(), None).collect().await;
    assert!(matches!(&ev[..], [Err(e)] if circuit_open(e)));
    let items = [BatchItem {
        custom_id: "x".into(),
        req: sreq(),
    }];
    assert!(circuit_open(&ai.batch_submit(&items).await.unwrap_err()));
    assert!(circuit_open(&ai.batch_status("b1").await.unwrap_err()));
    assert_eq!(inner.calls().len(), 5, "the inner client was not called");
}

#[tokio::test]
async fn batch_results_pass_through_unguarded_and_batches_enabled_delegates() {
    let mut batches = HashMap::new();
    batches.insert(
        "b1".to_owned(),
        FakeBatch {
            status: BatchStatus::Ended,
            results: vec![],
        },
    );
    let inner = Arc::new(FakeAiClient::new(FakeScript {
        structured: Some(Box::new(|_, _| {
            Err(unavailable(UnavailableReason::Overloaded))
        })),
        batches,
        batches_enabled: false,
        ..FakeScript::default()
    }));
    let b = Clock::new().breaker();
    let ai = with_breaker(inner.clone(), b.clone());
    assert!(!ai.batches_enabled());
    run_n(&*ai, 5).await;
    assert_eq!(b.state(), BreakerState::Open);
    struct NoSchema;
    impl SchemaResolver for NoSchema {
        fn resolve(&self, _: &str) -> Option<&'static OutputSchema> {
            None
        }
    }
    let r: Vec<_> = ai.batch_results("b1", &NoSchema).collect().await;
    // The inner client answered (its batches are disabled), not the breaker.
    assert!(matches!(
        &r[..],
        [Err(AiError::Unavailable {
            reason: UnavailableReason::NoApiKey,
            ..
        })]
    ));
    assert_eq!(inner.calls().len(), 6);

    let enabled = with_breaker(
        Arc::new(FakeAiClient::new(FakeScript::default())),
        Clock::new().breaker(),
    );
    assert!(enabled.batches_enabled());
}

#[tokio::test]
async fn passes_through_when_closed_and_keeps_request_errors() {
    let inner = scripted(|n| {
        if n == 0 {
            ok()
        } else {
            Err(AiError::Request {
                status: 400,
                api_error_type: Some("invalid_request_error".into()),
            })
        }
    });
    let ai = with_breaker(inner, Clock::new().breaker());
    assert_eq!(ai.structured(&sreq()).await.unwrap().parsed, Some(json!(1)));
    assert_eq!(
        ai.structured(&sreq()).await.unwrap_err(),
        AiError::Request {
            status: 400,
            api_error_type: Some("invalid_request_error".into())
        }
    );
}

#[tokio::test]
async fn only_unavailable_counts_as_a_failure() {
    let inner = scripted(|n| match n {
        0..10 => Err(AiError::Request {
            status: 400,
            api_error_type: None,
        }),
        10..12 => Err(AiError::Auth { status: 401 }),
        12..14 => Err(AiError::Response("bad body".into())),
        14..19 => Err(unavailable(UnavailableReason::Connection)),
        _ => ok(),
    });
    let b = Clock::new().breaker();
    let ai = with_breaker(inner.clone(), b.clone());
    run_n(&*ai, 14).await;
    assert_eq!(b.state(), BreakerState::Closed);
    run_n(&*ai, 4).await;
    assert_eq!(b.state(), BreakerState::Closed);
    run_n(&*ai, 1).await;
    assert_eq!(b.state(), BreakerState::Open);
}

#[tokio::test]
async fn _429_does_not_count_as_a_failure_but_is_still_rethrown() {
    let e429 = classify_status(429, Some("rate_limit_error"), Some(Duration::from_secs(3)));
    let e = e429.clone();
    let inner = scripted(move |_| Err(e.clone()));
    let b = Clock::new().breaker();
    let ai = with_breaker(inner, b.clone());
    for r in run_n(&*ai, 10).await {
        assert_eq!(r.unwrap_err(), e429, "returned unchanged, retry-after kept");
    }
    assert_eq!(b.state(), BreakerState::Closed);
}

#[tokio::test]
async fn overloaded_counts_as_a_failure() {
    // The sidecar's own 529 when its queue is full counts the same as the API's.
    let queue_full = classify_status(529, Some("overloaded_error"), Some(Duration::from_secs(5)));
    let e = queue_full.clone();
    let inner = scripted(move |_| Err(e.clone()));
    let b = Clock::new().breaker();
    let ai = with_breaker(inner, b.clone());
    let out = run_n(&*ai, 4).await;
    assert!(out.iter().all(|r| r.as_ref().unwrap_err() == &queue_full));
    assert_eq!(b.state(), BreakerState::Closed);
    run_n(&*ai, 1).await;
    assert_eq!(b.state(), BreakerState::Open);
}

#[tokio::test]
async fn budget_errors_do_not_trip_the_breaker() {
    let inner = scripted(|_| Err(AiError::Budget));
    let b = Clock::new().breaker();
    let ai = with_breaker(inner, b.clone());
    for r in run_n(&*ai, 10).await {
        assert_eq!(r.unwrap_err(), AiError::Budget);
    }
    assert_eq!(b.state(), BreakerState::Closed);
}

#[tokio::test]
async fn no_api_key_does_not_trip_the_breaker() {
    let inner = scripted(|_| Err(unavailable(UnavailableReason::NoApiKey)));
    let b = Clock::new().breaker();
    let ai = with_breaker(inner, b.clone());
    run_n(&*ai, 10).await;
    assert_eq!(b.state(), BreakerState::Closed);
    // Disabled batches (no key) do not trip it either.
    let inner = Arc::new(FakeAiClient::new(FakeScript {
        batches_enabled: false,
        ..FakeScript::default()
    }));
    let ai = with_breaker(inner, b.clone());
    for _ in 0..10 {
        ai.batch_status("b1").await.unwrap_err();
    }
    assert_eq!(b.state(), BreakerState::Closed);
}

#[tokio::test]
async fn consecutive_failures_are_reset_by_a_success() {
    let inner = scripted(|n| {
        if n == 4 {
            ok()
        } else {
            Err(unavailable(UnavailableReason::Server))
        }
    });
    let b = Clock::new().breaker();
    let ai = with_breaker(inner, b.clone());
    // Four failures, a success, then four more: never five in a row.
    run_n(&*ai, 9).await;
    assert_eq!(b.state(), BreakerState::Closed);
    run_n(&*ai, 1).await;
    assert_eq!(b.state(), BreakerState::Open);
}

#[tokio::test]
async fn half_open_admits_one_probe_and_closes_on_success() {
    let clock = Clock::new();
    let b = clock.breaker();
    // Turns 0..5 fail before any event; turn 5 is the probe; turn 6 runs after it closed.
    let inner = Arc::new(FakeAiClient::new(FakeScript {
        turns: (0..7)
            .map(|_| {
                Box::new(|_: &ChatTurnRequest, n: usize| {
                    if n < 5 {
                        Err(unavailable(UnavailableReason::Connection))
                    } else {
                        Ok(FakeTurn {
                            deltas: vec!["probe".into()],
                            ..FakeTurn::default()
                        })
                    }
                }) as _
            })
            .collect(),
        structured: Some(Box::new(|_, _| ok())),
        ..FakeScript::default()
    }));
    let ai = with_breaker(inner.clone(), b.clone());
    let req = treq();
    for _ in 0..5 {
        let _: Vec<_> = ai.stream_turn(&req, None).collect().await;
    }
    assert_eq!(b.state(), BreakerState::Open);

    clock.advance(Duration::from_secs(59));
    assert_eq!(b.state(), BreakerState::Open);
    assert!(circuit_open(&ai.structured(&sreq()).await.unwrap_err()));
    clock.advance(Duration::from_secs(1));
    assert_eq!(b.state(), BreakerState::HalfOpen);

    // The probe is admitted and held open mid-stream.
    let mut probe = ai.stream_turn(&req, None);
    assert!(matches!(
        probe.next().await,
        Some(Ok(TurnEvent::Message(_)))
    ));
    // While it runs, every other call is refused without reaching the inner client.
    let before = inner.calls().len();
    assert!(circuit_open(&ai.structured(&sreq()).await.unwrap_err()));
    let other = ChatTurnRequest {
        conversation_id: uuid::Uuid::from_u128(1),
        ..treq()
    };
    let ev: Vec<_> = ai.stream_turn(&other, None).collect().await;
    assert!(matches!(&ev[..], [Err(e)] if circuit_open(e)));
    assert_eq!(inner.calls().len(), before);
    assert_eq!(b.state(), BreakerState::HalfOpen);

    let rest: Vec<_> = probe.collect().await;
    assert!(matches!(rest.last(), Some(Ok(TurnEvent::Done(_)))));
    assert_eq!(b.state(), BreakerState::Closed);
    assert!(ai.structured(&sreq()).await.is_ok());
}

#[tokio::test]
async fn a_failed_probe_reopens_for_a_full_cooldown() {
    let clock = Clock::new();
    let b = clock.breaker();
    let inner = scripted(|n| {
        if n < 6 {
            Err(unavailable(UnavailableReason::Server))
        } else {
            ok()
        }
    });
    let ai = with_breaker(inner, b.clone());
    run_n(&*ai, 5).await;
    clock.advance(Duration::from_secs(60));
    assert_eq!(b.state(), BreakerState::HalfOpen);
    run_n(&*ai, 1).await; // the probe fails
    assert_eq!(b.state(), BreakerState::Open);
    clock.advance(Duration::from_secs(59));
    assert_eq!(b.state(), BreakerState::Open);
    clock.advance(Duration::from_secs(1));
    assert!(ai.structured(&sreq()).await.is_ok());
    assert_eq!(b.state(), BreakerState::Closed);
}

#[tokio::test]
async fn a_dropped_probe_frees_the_probe_slot_and_records_nothing() {
    let clock = Clock::new();
    let b = clock.breaker();
    let inner = Arc::new(FakeAiClient::new(FakeScript {
        turns: (0..8)
            .map(|_| {
                Box::new(|_: &ChatTurnRequest, n: usize| {
                    if n < 5 {
                        Err(unavailable(UnavailableReason::Server))
                    } else {
                        Ok(FakeTurn {
                            deltas: vec!["x".into()],
                            ..FakeTurn::default()
                        })
                    }
                }) as _
            })
            .collect(),
        ..FakeScript::default()
    }));
    let ai = with_breaker(inner, b.clone());
    let req = treq();
    for _ in 0..5 {
        let _: Vec<_> = ai.stream_turn(&req, None).collect().await;
    }
    clock.advance(Duration::from_secs(60));
    let mut probe = ai.stream_turn(&req, None);
    probe.next().await;
    drop(probe);
    assert_eq!(b.state(), BreakerState::HalfOpen, "nothing was recorded");
    // The next caller can probe.
    let ev: Vec<_> = ai.stream_turn(&req, None).collect().await;
    assert!(matches!(ev.last(), Some(Ok(TurnEvent::Done(_)))));
    assert_eq!(b.state(), BreakerState::Closed);
}

#[tokio::test]
async fn stream_error_event_counts_and_done_resets() {
    // Turn n: a terminal error after a delta when n % 5 != 4, else Done.
    let inner = Arc::new(FakeAiClient::new(FakeScript {
        turns: (0..12)
            .map(|_| {
                Box::new(|_: &ChatTurnRequest, n: usize| {
                    Ok(FakeTurn {
                        deltas: vec!["d".into()],
                        error: (n != 4).then(|| unavailable(UnavailableReason::Server)),
                        ..FakeTurn::default()
                    })
                }) as _
            })
            .collect(),
        ..FakeScript::default()
    }));
    let b = Clock::new().breaker();
    let ai = with_breaker(inner, b.clone());
    let req = treq();
    let turn = || async {
        let ev: Vec<_> = ai.stream_turn(&req, None).collect().await;
        ev
    };
    for _ in 0..4 {
        let ev = turn().await;
        assert!(matches!(ev.last(), Some(Err(AiError::Unavailable { .. }))));
    }
    assert_eq!(b.state(), BreakerState::Closed);
    let ev = turn().await; // n = 4: Done resets the count
    assert!(matches!(ev.last(), Some(Ok(TurnEvent::Done(_)))));
    for _ in 0..4 {
        turn().await;
    }
    assert_eq!(b.state(), BreakerState::Closed);
    // A stream dropped after its first event records nothing.
    let mut s = ai.stream_turn(&req, None);
    s.next().await;
    drop(s);
    assert_eq!(b.state(), BreakerState::Closed);
    turn().await;
    assert_eq!(b.state(), BreakerState::Open);
}

#[tokio::test]
async fn turn_in_progress_does_not_trip_the_breaker() {
    let inner = Arc::new(FakeAiClient::new(FakeScript {
        turns: vec![Box::new(|_, _| {
            Ok(FakeTurn {
                deltas: vec!["x".into()],
                ..FakeTurn::default()
            })
        })],
        ..FakeScript::default()
    }));
    let b = Clock::new().breaker();
    let ai = with_breaker(inner, b.clone());
    let req = treq();
    let mut held = ai.stream_turn(&req, None);
    held.next().await;
    for _ in 0..10 {
        let ev: Vec<_> = ai.stream_turn(&req, None).collect().await;
        assert!(matches!(
            &ev[..],
            [Err(AiError::Request { status: 409, .. })]
        ));
    }
    assert_eq!(b.state(), BreakerState::Closed);
    drop(held);
}

#[tokio::test]
async fn batch_submit_and_status_failures_count() {
    let inner = Arc::new(FakeAiClient::new(FakeScript {
        batch_submit: Some(Box::new(|_, _| Err(unavailable(UnavailableReason::Server)))),
        batch_status: Some(Box::new(|_, _| {
            Err(unavailable(UnavailableReason::Connection))
        })),
        ..FakeScript::default()
    }));
    let b = Clock::new().breaker();
    let ai = with_breaker(inner, b.clone());
    let items = [BatchItem {
        custom_id: "x".into(),
        req: sreq(),
    }];
    for _ in 0..2 {
        ai.batch_submit(&items).await.unwrap_err();
    }
    for _ in 0..2 {
        ai.batch_status("b1").await.unwrap_err();
    }
    assert_eq!(b.state(), BreakerState::Closed);
    ai.batch_status("b1").await.unwrap_err();
    assert_eq!(b.state(), BreakerState::Open);
}
