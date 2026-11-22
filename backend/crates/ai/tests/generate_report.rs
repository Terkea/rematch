//! Port of TS plan B3's `report.test.ts` (R3 Task 14): the five sections, overview first, at most
//! `concurrency` in flight, stop on the first error. Fake client only.

#[path = "support/generate.rs"]
mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ai::client::fake::{FakeAiClient, FakeScript, FakeStructured};
use ai::client::port::{AiError, AiUsage, UnavailableReason};
use ai::generate::report::{ReportCause, generate_report};
use ai::generate::section::SectionOutcome;
use ai::output::codes::{SECTION_KEYS, SectionKey};
use ai::testing::input;
use futures::future::BoxFuture;
use support::*;

type Seen = Arc<Mutex<Vec<SectionKey>>>;

fn recorder() -> (
    Seen,
    impl Fn(SectionOutcome) -> BoxFuture<'static, anyhow::Result<()>> + Send + Sync,
) {
    let seen: Seen = Arc::default();
    let s = seen.clone();
    let f = move |o: SectionOutcome| -> BoxFuture<'static, anyhow::Result<()>> {
        s.lock().unwrap().push(o.section);
        Box::pin(async { Ok(()) })
    };
    (seen, f)
}

/// A fake with a 1 s delay per call (paused time) answering every section validly, except where
/// `fail` returns an error.
fn slow_fake(fail: impl Fn(SectionKey) -> Option<AiError> + Send + Sync + 'static) -> FakeAiClient {
    let i = input("b2-rapid");
    let inner = i.clone();
    FakeAiClient::new(FakeScript {
        structured: Some(Box::new(move |req, _| {
            let k = section_of(&inner, req);
            match fail(k) {
                Some(e) => Err(e),
                None => Ok(out(valid(k))),
            }
        })),
        delay: Some(Duration::from_secs(1)),
        ..FakeScript::default()
    })
}

fn sections_called(ai: &FakeAiClient) -> Vec<SectionKey> {
    let i = input("b2-rapid");
    ai.calls()
        .iter()
        .map(|c| {
            let task = c.request_json["messages"][0]["content"]
                .as_array()
                .unwrap()
                .last()
                .cloned()
                .unwrap();
            SECTION_KEYS
                .into_iter()
                .find(|k| {
                    ai::generate::request::build_section_request(&i, *k, true).messages[0]
                        .content
                        .last()
                        == Some(&task)
                })
                .unwrap()
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn generate_report_runs_overview_first_and_at_most_2_in_flight() {
    let i = input("b2-rapid");
    let ai = slow_fake(|_| None);
    let budget = no_budget();
    let (seen, on) = recorder();
    let outcomes = generate_report(&ai, &budget, &i, true, &[], 2, &on)
        .await
        .unwrap();
    let called = sections_called(&ai);
    assert_eq!(called.len(), 5);
    assert_eq!(called[0], SectionKey::Overview);
    assert!(ai.max_in_flight() <= 2);
    // Concurrency is used: two sections ran at once after overview.
    assert_eq!(ai.max_in_flight(), 2);
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 5);
    assert_eq!(seen[0], SectionKey::Overview);
    // Outcomes come back in SECTION_KEYS order, all ready.
    assert_eq!(
        outcomes.iter().map(|o| o.section).collect::<Vec<_>>(),
        SECTION_KEYS
    );
    assert!(outcomes.iter().all(ready), "{outcomes:?}");
}

#[tokio::test(start_paused = true)]
async fn concurrency_1_runs_sections_one_at_a_time() {
    let i = input("b2-rapid");
    let ai = slow_fake(|_| None);
    let budget = no_budget();
    let (_, on) = recorder();
    generate_report(&ai, &budget, &i, true, &[], 1, &on)
        .await
        .unwrap();
    assert_eq!(ai.max_in_flight(), 1);
    assert_eq!(sections_called(&ai), SECTION_KEYS);
}

#[tokio::test(start_paused = true)]
async fn unavailable_mid_run_stops_scheduling() {
    let i = input("b2-rapid");
    let unavailable = AiError::Unavailable {
        reason: UnavailableReason::Server,
        retry_after: None,
    };
    let e = unavailable.clone();
    let ai = slow_fake(move |k| (k == SectionKey::Openings).then(|| e.clone()));
    let budget = no_budget();
    // Storing weaknesses takes a while, so openings fails while weaknesses is still in flight.
    let seen: Seen = Arc::default();
    let s = seen.clone();
    let on = move |o: SectionOutcome| -> BoxFuture<'static, anyhow::Result<()>> {
        let s = s.clone();
        Box::pin(async move {
            if o.section == SectionKey::Weaknesses {
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            s.lock().unwrap().push(o.section);
            Ok(())
        })
    };
    let r = generate_report(&ai, &budget, &i, true, &[], 2, &on).await;
    match r {
        Err(e) => {
            assert_eq!(e.as_ai(), Some(&unavailable));
            // Openings' call failed unbilled; the delivered sections were metered by on_section.
            assert!(e.unmetered.is_empty());
        }
        Ok(o) => panic!("expected the unavailable error, got {o:?}"),
    }
    // Overview, then weaknesses and openings together; weaknesses (in flight) was delivered.
    assert_eq!(
        *seen.lock().unwrap(),
        [SectionKey::Overview, SectionKey::Weaknesses]
    );
    let called = sections_called(&ai);
    assert_eq!(
        called,
        [
            SectionKey::Overview,
            SectionKey::Weaknesses,
            SectionKey::Openings
        ]
    );
}

#[tokio::test]
async fn skip_runs_only_missing_sections() {
    let i = input("b2-rapid");
    let ai = per_section(&i, |k, _| Ok(out(valid(k))));
    let budget = no_budget();
    let (seen, on) = recorder();
    let outcomes = generate_report(
        &ai,
        &budget,
        &i,
        true,
        &[SectionKey::Overview, SectionKey::Weaknesses],
        2,
        &on,
    )
    .await
    .unwrap();
    assert_eq!(structured_requests(&ai).len(), 3);
    assert_eq!(
        outcomes.iter().map(|o| o.section).collect::<Vec<_>>(),
        [
            SectionKey::Openings,
            SectionKey::ClimbPlan,
            SectionKey::Areas
        ]
    );
    assert_eq!(seen.lock().unwrap().len(), 3);
}

#[tokio::test(start_paused = true)]
async fn budget_hook_stops_before_the_call() {
    let i = input("b2-rapid");
    let ai = slow_fake(|_| None);
    let count = Arc::new(AtomicUsize::new(0));
    let c = count.clone();
    let budget: BudgetFn = Box::new(move || {
        let n = c.fetch_add(1, Ordering::SeqCst) + 1;
        Box::pin(async move { if n >= 3 { Err(AiError::Budget) } else { Ok(()) } })
    });
    let (seen, on) = recorder();
    let r = generate_report(&ai, &budget, &i, true, &[], 2, &on).await;
    let err = r.expect_err("the budget stops the run");
    assert_eq!(err.as_ai(), Some(&AiError::Budget));
    // Overview (hook 1) and one of the two sections started after it (hook 2) were called; the
    // other one's hook refused the third call. Which one gets hook 3 is the poll order's choice.
    assert_eq!(structured_requests(&ai).len(), 2);
    assert_eq!(count.load(Ordering::SeqCst), 3);
    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0], SectionKey::Overview);
    assert!(
        [SectionKey::Weaknesses, SectionKey::Openings].contains(&seen[1]),
        "{seen:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn an_on_section_error_stops_scheduling_and_is_returned() {
    let i = input("b2-rapid");
    let ai = slow_fake(|_| None);
    let budget = no_budget();
    let on = |o: SectionOutcome| -> BoxFuture<'static, anyhow::Result<()>> {
        Box::pin(async move {
            match o.section {
                SectionKey::Weaknesses => anyhow::bail!("db down"),
                // Still storing openings when weaknesses fails: no refill in between.
                SectionKey::Openings => tokio::time::sleep(Duration::from_secs(5)).await,
                _ => {}
            }
            Ok(())
        })
    };
    let r = generate_report(&ai, &budget, &i, true, &[], 2, &on).await;
    match r {
        Err(e) => {
            match &e.cause {
                ReportCause::OnSection(c) => assert_eq!(c.to_string(), "db down"),
                other => panic!("expected the on_section error, got {other:?}"),
            }
            // The section whose storing failed was not metered: its call comes back.
            assert_eq!(e.unmetered.len(), 1);
        }
        Ok(o) => panic!("expected the on_section error, got {o:?}"),
    }
    // Overview, weaknesses and openings started; nothing after the failure.
    assert_eq!(sections_called(&ai).len(), 3);
}

#[tokio::test]
async fn an_overview_error_starts_nothing_else() {
    let i = input("b2-rapid");
    let ai = per_section(&i, |_, _| {
        Err(AiError::Unavailable {
            reason: UnavailableReason::Connection,
            retry_after: None,
        })
    });
    let budget = no_budget();
    let (seen, on) = recorder();
    let r = generate_report(&ai, &budget, &i, true, &[], 2, &on).await;
    assert!(r.unwrap_err().as_ai().is_some_and(AiError::is_unavailable));
    assert_eq!(structured_requests(&ai).len(), 1);
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_failed_repair_hands_the_paid_first_call_back_as_unmetered() {
    let i = input("b2-rapid");
    let ai = per_section(&i, |k, n| match (k, n) {
        (SectionKey::Weaknesses, 1) => Ok(FakeStructured {
            usage: AiUsage {
                input_tokens: 5_000,
                output_tokens: 700,
                ..AiUsage::default()
            },
            ..out(envelope(
                SectionKey::Weaknesses,
                serde_json::json!({"items": [w1_item(LEAKING_TEXT)]}),
            ))
        }),
        (SectionKey::Weaknesses, _) => Err(AiError::Unavailable {
            reason: UnavailableReason::Overloaded,
            retry_after: None,
        }),
        _ => Ok(out(valid(k))),
    });
    let budget = no_budget();
    let (_, on) = recorder();
    let err = generate_report(&ai, &budget, &i, true, &[], 2, &on)
        .await
        .unwrap_err();
    assert!(err.as_ai().is_some_and(AiError::is_unavailable));
    assert_eq!(err.unmetered.len(), 1);
    assert_eq!(err.unmetered[0].usage.input_tokens, 5_000);
    assert_eq!(err.unmetered[0].usage.output_tokens, 700);
}
