//! R3 Task 16: batch request building and result processing. Batch results go through the same
//! validation and repair-once rules as the interactive path; every billed result is metered as
//! a batch call. Fake client only.

#[path = "support/generate.rs"]
mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use ai::client::custom_id::decode_custom_id;
use ai::client::fake::{FakeAiClient, FakeBatch, FakeBatchItem, FakeScript};
use ai::client::model::{AiModel, Effort, Models};
use ai::client::port::{
    AiClient, AiError, AiUsage, BatchItem, BatchOutcome, BatchResultItem, BatchStatus, Message,
    ParseError, Role, SchemaResolver, StructuredRequest, TextBlock, UnavailableReason,
};
use ai::client::wire::MessagesRequest;
use ai::generate::batch::{
    BatchProcessed, ChangeProcessed, MAX_BATCH_BYTES, MAX_BATCH_REQUESTS, RegistryResolver,
    WeeklyProcessed, batch_custom_id, batch_item_id, batch_request_bytes, change_batch_item,
    chunk_batch, chunk_batch_by, process_change_result, process_report_result,
    process_weekly_result, section_batch_item, weekly_batch_item,
};
use ai::generate::change::build_change_request;
use ai::generate::request::build_section_request;
use ai::generate::section::{CallRecord, SectionStatus};
use ai::generate::weekly::build_weekly_request;
use ai::input::types::CoachingInput;
use ai::output::change::WeaknessChangeInput;
use ai::output::codes::{SECTION_KEYS, SectionKey, ValidationCode};
use ai::output::schema::{REPORT_SECTION_SCHEMA, WEAKNESS_CHANGE_SCHEMA, WEEKLY_REVIEW_SCHEMA};
use ai::output::weekly::WeeklyReviewInput;
use ai::testing::{input, load_fixture};
use futures::StreamExt;
use regex::Regex;
use serde_json::{Value, json};
use support::*;
use uuid::Uuid;

const ALL: [&ai::client::port::OutputSchema; 3] = [
    &REPORT_SECTION_SCHEMA,
    &WEEKLY_REVIEW_SCHEMA,
    &WEAKNESS_CHANGE_SCHEMA,
];

const BATCH_USAGE: AiUsage = AiUsage {
    input_tokens: 1200,
    output_tokens: 300,
    cache_creation_tokens: 0,
    cache_read_tokens: 4000,
};

fn b2() -> CoachingInput {
    input("b2-rapid")
}

/// A fake whose batch `fakebatch_1` ends with `results`, and whose structured calls (the sync
/// repairs) answer with `sync(n)` (`n` 1-based) and are counted.
fn fake(
    results: Vec<FakeBatchItem>,
    sync: impl Fn(usize) -> ai::client::fake::FakeStructured + Send + Sync + 'static,
) -> (FakeAiClient, Arc<AtomicUsize>) {
    let n = Arc::new(AtomicUsize::new(0));
    let counter = n.clone();
    let mut script = FakeScript {
        structured: Some(Box::new(move |_, _| {
            Ok(sync(counter.fetch_add(1, Ordering::SeqCst) + 1))
        })),
        ..FakeScript::default()
    };
    script.batches.insert(
        "fakebatch_1".to_owned(),
        FakeBatch {
            status: BatchStatus::Ended,
            results,
        },
    );
    (FakeAiClient::new(script), n)
}

fn no_sync() -> impl Fn(usize) -> ai::client::fake::FakeStructured + Send + Sync + 'static {
    |n| panic!("no sync call expected, got call {n}")
}

/// Submits `items` and returns the results of the (only) batch, resolved by the registry.
async fn submit_and_collect(ai: &FakeAiClient, items: &[BatchItem]) -> Vec<BatchResultItem> {
    let id = ai.batch_submit(items).await.unwrap();
    assert_eq!(id, "fakebatch_1");
    ai.batch_results(&id, &RegistryResolver)
        .map(|r| r.unwrap())
        .collect()
        .await
}

fn succeeded(custom_id: &str, text: Value) -> FakeBatchItem {
    FakeBatchItem {
        custom_id: custom_id.to_owned(),
        outcome: BatchOutcome::Succeeded,
        text: Some(text.to_string()),
        usage: BATCH_USAGE,
        ..FakeBatchItem::default()
    }
}

fn batch_record(model: AiModel) -> CallRecord {
    CallRecord {
        model: model.as_str().to_owned(),
        usage: BATCH_USAGE,
        batch: true,
        sdk_cost_usd: None,
    }
}

fn wire(req: &StructuredRequest) -> Value {
    serde_json::to_value(MessagesRequest::structured(req)).unwrap()
}

#[test]
fn custom_ids_match_the_api_charset_and_round_trip() {
    let charset = Regex::new("^[A-Za-z0-9_-]{1,64}$").unwrap();
    for _ in 0..1000 {
        let item = Uuid::new_v4();
        for schema in ALL {
            let id = batch_custom_id(schema, item);
            assert!(charset.is_match(&id), "{id}");
            let parts = decode_custom_id(&id).expect("decodes");
            assert_eq!(parts.schema_id, schema.id);
            assert_eq!(parts.version, schema.version);
            assert_eq!(parts.reference, item.simple().to_string());
            assert_eq!(batch_item_id(&id), Some(item));
            let resolved = RegistryResolver.resolve(&id).expect("resolves");
            assert!(std::ptr::eq(resolved, schema), "{id}");
        }
    }
    let id = batch_custom_id(&REPORT_SECTION_SCHEMA, Uuid::nil());
    assert_eq!(id.len(), 52);
    assert_eq!(id, format!("report-section__v1__{}", "0".repeat(32)));
    // A foreign or mangled id is no item and no schema.
    for bad in [
        "report-section__v1__nothex",
        "report-section__v1__",
        "weekly",
        "",
    ] {
        assert_eq!(batch_item_id(bad), None, "{bad}");
    }
    assert!(RegistryResolver.resolve("nope__v1__abc").is_none());
}

#[test]
fn batch_params_equal_sync_requests() {
    let input = b2();
    let mut ids = std::collections::HashSet::new();
    for is_first_report in [true, false] {
        for section in SECTION_KEYS {
            let item_id = Uuid::new_v4();
            let item = section_batch_item(&input, section, is_first_report, item_id);
            assert_eq!(
                item.custom_id,
                batch_custom_id(&REPORT_SECTION_SCHEMA, item_id)
            );
            assert!(ids.insert(item.custom_id.clone()));
            assert_eq!(
                wire(&item.req),
                wire(&build_section_request(&input, section, is_first_report)),
                "{section:?}"
            );
            // The params of a batch never stream.
            assert!(wire(&item.req).get("stream").is_none());
        }
    }
    assert_eq!(ids.len(), 10);
}

#[tokio::test]
async fn succeeded_valid_is_ready_with_zero_sync_calls() {
    let input = b2();
    let item = section_batch_item(&input, SectionKey::Overview, true, Uuid::new_v4());
    let (ai, sync) = fake(
        vec![succeeded(&item.custom_id, valid(SectionKey::Overview))],
        no_sync(),
    );
    let results = submit_and_collect(&ai, std::slice::from_ref(&item)).await;
    assert_eq!(results.len(), 1);
    let budget = no_budget();
    let r = results.into_iter().next().unwrap();
    let out = process_report_result(&ai, &budget, r, &input, SectionKey::Overview, true)
        .await
        .unwrap();
    let BatchProcessed::Outcome(o) = out else {
        panic!("expected an outcome, got {out:?}")
    };
    assert_eq!(o.status, SectionStatus::Ready);
    assert_eq!(o.attempts, 1);
    assert!(o.dropped.is_empty());
    assert!(o.content.is_some());
    assert_eq!(o.calls, vec![batch_record(Models::REPORT)]);
    assert_eq!(sync.load(Ordering::SeqCst), 0);
    assert!(structured_requests(&ai).is_empty());
}

fn leaking_weaknesses() -> Value {
    envelope(
        SectionKey::Weaknesses,
        json!({"items": [w1_item(LEAKING_TEXT)]}),
    )
}

#[tokio::test]
async fn succeeded_invalid_gets_exactly_one_sync_call() {
    let input = b2();
    let item = section_batch_item(&input, SectionKey::Weaknesses, true, Uuid::new_v4());
    let (ai, sync) = fake(
        vec![succeeded(&item.custom_id, leaking_weaknesses())],
        |_| out(valid(SectionKey::Weaknesses)),
    );
    let results = submit_and_collect(&ai, std::slice::from_ref(&item)).await;
    let budget = no_budget();
    let r = results.into_iter().next().unwrap();
    let out = process_report_result(&ai, &budget, r, &input, SectionKey::Weaknesses, true)
        .await
        .unwrap();
    let BatchProcessed::Outcome(o) = out else {
        panic!("expected an outcome, got {out:?}")
    };
    assert_eq!(o.status, SectionStatus::Ready, "{:?}", o.dropped);
    assert_eq!(o.attempts, 2);
    assert_eq!(sync.load(Ordering::SeqCst), 1);
    // The batch result first (batch pricing), then the sync repair.
    assert_eq!(o.calls.len(), 2);
    assert_eq!(o.calls[0], batch_record(Models::REPORT));
    assert!(!o.calls[1].batch);
    // The repair replays the batch answer and lists the errors with their paths.
    let reqs = structured_requests(&ai);
    assert_eq!(reqs.len(), 1);
    let messages = reqs[0]["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[1]["role"], "assistant");
    let repair = messages[2].to_string();
    assert!(repair.contains("weaknesses.items[0].text"), "{repair}");
}

#[tokio::test]
async fn a_repair_that_still_fails_drops_the_item_and_nothing_unvalidated_is_kept() {
    let input = b2();
    let item = section_batch_item(&input, SectionKey::Weaknesses, true, Uuid::new_v4());
    let (ai, sync) = fake(
        vec![succeeded(&item.custom_id, leaking_weaknesses())],
        |_| out(leaking_weaknesses()),
    );
    let results = submit_and_collect(&ai, std::slice::from_ref(&item)).await;
    let budget = no_budget();
    let r = results.into_iter().next().unwrap();
    let BatchProcessed::Outcome(o) =
        process_report_result(&ai, &budget, r, &input, SectionKey::Weaknesses, true)
            .await
            .unwrap()
    else {
        panic!("expected an outcome")
    };
    assert_eq!(sync.load(Ordering::SeqCst), 1);
    let kept = serde_json::to_string(&o.content).unwrap();
    assert!(!kept.contains("Nxe4"), "{kept}");
    assert!(
        o.dropped
            .iter()
            .any(|d| d.path == "weaknesses.items[0]" || d.path.starts_with("weaknesses")),
        "{:?}",
        o.dropped
    );
}

#[tokio::test]
async fn a_refused_or_truncated_batch_result_gets_one_sync_call() {
    for reason in ["refusal", "max_tokens"] {
        let input = b2();
        let item = section_batch_item(&input, SectionKey::Overview, true, Uuid::new_v4());
        let (ai, sync) = fake(
            vec![FakeBatchItem {
                stop_reason: Some(reason.to_owned()),
                ..succeeded(&item.custom_id, json!(""))
            }],
            |_| out(valid(SectionKey::Overview)),
        );
        let results = submit_and_collect(&ai, std::slice::from_ref(&item)).await;
        let budget = no_budget();
        let r = results.into_iter().next().unwrap();
        let BatchProcessed::Outcome(o) =
            process_report_result(&ai, &budget, r, &input, SectionKey::Overview, true)
                .await
                .unwrap()
        else {
            panic!("{reason}: expected an outcome")
        };
        assert_eq!(o.status, SectionStatus::Ready, "{reason}");
        assert_eq!(o.attempts, 2, "{reason}");
        assert_eq!(sync.load(Ordering::SeqCst), 1, "{reason}");
        assert_eq!(o.calls[0], batch_record(Models::REPORT), "{reason}");
    }
}

#[tokio::test]
async fn a_failed_repair_hands_the_billed_batch_call_back_for_metering() {
    let input = b2();
    let item = section_batch_item(&input, SectionKey::Weaknesses, true, Uuid::new_v4());
    let mut script = FakeScript {
        structured: Some(Box::new(|_, _| {
            Err(AiError::Unavailable {
                reason: UnavailableReason::Server,
                retry_after: None,
            })
        })),
        ..FakeScript::default()
    };
    script.batches.insert(
        "fakebatch_1".to_owned(),
        FakeBatch {
            status: BatchStatus::Ended,
            results: vec![succeeded(&item.custom_id, leaking_weaknesses())],
        },
    );
    let ai = FakeAiClient::new(script);
    let results = submit_and_collect(&ai, std::slice::from_ref(&item)).await;
    let r = results.into_iter().next().unwrap();
    let budget = no_budget();
    let e = process_report_result(
        &ai,
        &budget,
        r.clone(),
        &input,
        SectionKey::Weaknesses,
        true,
    )
    .await
    .unwrap_err();
    assert!(e.error.is_unavailable());
    assert_eq!(e.calls, vec![batch_record(Models::REPORT)]);

    // The budget hook refusing the repair does the same.
    let refuse: BudgetFn = Box::new(|| Box::pin(async { Err(AiError::Budget) }));
    let e = process_report_result(&ai, &refuse, r, &input, SectionKey::Weaknesses, true)
        .await
        .unwrap_err();
    assert_eq!(e.error, AiError::Budget);
    assert_eq!(e.calls, vec![batch_record(Models::REPORT)]);
}

#[tokio::test]
async fn expired_errored_canceled_and_no_schema_are_retry_sync() {
    let input = b2();
    let items: Vec<BatchItem> = (0..6)
        .map(|_| section_batch_item(&input, SectionKey::Overview, true, Uuid::new_v4()))
        .collect();
    let cid = |i: usize| items[i].custom_id.clone();
    let unknown = format!("nope__v1__{}", Uuid::new_v4().simple());
    let results = vec![
        FakeBatchItem {
            custom_id: cid(0),
            outcome: BatchOutcome::Expired,
            ..FakeBatchItem::default()
        },
        FakeBatchItem {
            custom_id: cid(1),
            outcome: BatchOutcome::Errored,
            error_type: Some("overloaded_error".to_owned()),
            ..FakeBatchItem::default()
        },
        FakeBatchItem {
            custom_id: cid(2),
            outcome: BatchOutcome::Canceled,
            ..FakeBatchItem::default()
        },
        // Succeeded, but with a custom id no schema resolves: billed, unusable.
        succeeded(&unknown, valid(SectionKey::Overview)),
        // Succeeded with text that is not JSON: billed, unusable.
        FakeBatchItem {
            text: Some("not json".to_owned()),
            ..succeeded(&cid(4), json!(null))
        },
        // Succeeded, schema mismatch: billed, unusable.
        succeeded(&cid(5), json!({"section": "overview"})),
    ];
    let (ai, sync) = fake(results, no_sync());
    let got = submit_and_collect(&ai, &items).await;
    assert_eq!(got.len(), 6);
    let expected_parse = [
        None,
        None,
        None,
        Some(ParseError::NoSchema),
        Some(ParseError::InvalidJson),
        Some(ParseError::SchemaMismatch),
    ];
    let budget = no_budget();
    for (i, r) in got.into_iter().enumerate() {
        assert_eq!(r.parse_error, expected_parse[i], "item {i}");
        let billed = r.result.is_some();
        let out = process_report_result(&ai, &budget, r, &input, SectionKey::Overview, true)
            .await
            .unwrap();
        let BatchProcessed::RetrySync { calls } = out else {
            panic!("item {i}: expected RetrySync, got {out:?}")
        };
        if billed {
            // The response model of a custom id the fake never saw is `fake`.
            let model = if i == 3 {
                "fake"
            } else {
                Models::REPORT.as_str()
            };
            assert_eq!(calls.len(), 1, "item {i}");
            assert_eq!(calls[0].model, model, "item {i}");
            assert_eq!(calls[0].usage, BATCH_USAGE, "item {i}");
            assert!(calls[0].batch, "item {i}");
        } else {
            // Expired, errored and canceled requests carry no usage and are not billed.
            assert!(calls.is_empty(), "item {i}");
        }
    }
    assert_eq!(sync.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn version_mismatch_resolves_to_no_schema() {
    let item_id = Uuid::new_v4();
    let v9 = format!("report-section__v9__{}", item_id.simple());
    assert!(RegistryResolver.resolve(&v9).is_none());
    assert_eq!(batch_item_id(&v9), Some(item_id));

    let input = b2();
    let (ai, sync) = fake(vec![succeeded(&v9, valid(SectionKey::Overview))], no_sync());
    let mut script_items = vec![section_batch_item(
        &input,
        SectionKey::Overview,
        true,
        item_id,
    )];
    script_items[0].custom_id = v9.clone();
    let got = submit_and_collect(&ai, &script_items).await;
    let r = got.into_iter().next().unwrap();
    assert_eq!(r.parse_error, Some(ParseError::NoSchema));
    assert!(r.result.as_ref().unwrap().parsed.is_none());
    let budget = no_budget();
    let out = process_report_result(&ai, &budget, r, &input, SectionKey::Overview, true)
        .await
        .unwrap();
    assert!(
        matches!(&out, BatchProcessed::RetrySync { calls } if calls == &vec![batch_record(Models::REPORT)]),
        "{out:?}"
    );
    assert_eq!(sync.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_result_for_another_schema_is_retry_sync() {
    // A weekly custom id handed to the report processor is never validated as a section.
    let input = b2();
    let id = batch_custom_id(&WEEKLY_REVIEW_SCHEMA, Uuid::new_v4());
    let (ai, sync) = fake(vec![succeeded(&id, valid(SectionKey::Overview))], no_sync());
    let mut items = vec![section_batch_item(
        &input,
        SectionKey::Overview,
        true,
        Uuid::new_v4(),
    )];
    items[0].custom_id = id;
    let r = submit_and_collect(&ai, &items).await.remove(0);
    let budget = no_budget();
    let out = process_report_result(&ai, &budget, r, &input, SectionKey::Overview, true)
        .await
        .unwrap();
    assert!(
        matches!(&out, BatchProcessed::RetrySync { calls } if calls.len() == 1 && calls[0].batch),
        "{out:?}"
    );

    // Even when the misrouted result parsed cleanly (as a resolver of another process might
    // have it), it is never accepted as a section.
    let mut misrouted = r_ok(&input);
    misrouted.custom_id = batch_custom_id(&WEEKLY_REVIEW_SCHEMA, Uuid::new_v4());
    let out = process_report_result(&ai, &budget, misrouted, &input, SectionKey::Overview, true)
        .await
        .unwrap();
    assert!(
        matches!(&out, BatchProcessed::RetrySync { calls } if calls.len() == 1 && calls[0].batch),
        "{out:?}"
    );
    // The control: the same result under its own custom id is ready.
    let out = process_report_result(
        &ai,
        &budget,
        r_ok(&input),
        &input,
        SectionKey::Overview,
        true,
    )
    .await
    .unwrap();
    assert!(matches!(out, BatchProcessed::Outcome(_)), "{out:?}");
    assert_eq!(sync.load(Ordering::SeqCst), 0);
}

/// A succeeded, cleanly parsed overview result under a report-section custom id.
fn r_ok(_input: &CoachingInput) -> BatchResultItem {
    let v = valid(SectionKey::Overview);
    let content = vec![json!({"type": "text", "text": v.to_string()})];
    BatchResultItem {
        custom_id: batch_custom_id(&REPORT_SECTION_SCHEMA, Uuid::new_v4()),
        outcome: BatchOutcome::Succeeded,
        result: Some(ai::client::port::StructuredResult {
            parsed: Some(v),
            parse_error: None,
            parse_issues: None,
            content,
            stop_reason: Some("end_turn".to_owned()),
            usage: BATCH_USAGE,
            model: Models::REPORT.as_str().to_owned(),
            request_id: None,
            model_usage: vec![ai::client::port::ModelUsage {
                model: Models::REPORT.as_str().to_owned(),
                usage: BATCH_USAGE,
            }],
            sdk_cost_usd: None,
        }),
        error_type: None,
        error_kind: None,
        parse_error: None,
    }
}

/// A small request whose user text is `n` bytes of `x`.
fn tiny(n: usize) -> BatchItem {
    BatchItem {
        custom_id: batch_custom_id(&REPORT_SECTION_SCHEMA, Uuid::new_v4()),
        req: StructuredRequest {
            model: Models::REPORT,
            system: vec![TextBlock::new("s")],
            messages: vec![Message {
                role: Role::User,
                content: vec![TextBlock::new("x".repeat(n)).to_json()],
            }],
            max_tokens: 16,
            effort: Effort::Low,
            schema: &REPORT_SECTION_SCHEMA,
        },
    }
}

fn ids(chunks: &[Vec<BatchItem>]) -> Vec<String> {
    chunks
        .iter()
        .flatten()
        .map(|i| i.custom_id.clone())
        .collect()
}

#[test]
fn chunking_respects_count_and_bytes() {
    assert_eq!(MAX_BATCH_REQUESTS, 10_000);
    assert_eq!(MAX_BATCH_BYTES, 200 * 1024 * 1024);
    assert!(chunk_batch(Vec::new()).is_empty());

    // Count: 10_001 requests are two batches, in order.
    let items: Vec<BatchItem> = (0..10_001).map(|_| tiny(1)).collect();
    let order: Vec<String> = items.iter().map(|i| i.custom_id.clone()).collect();
    let chunks = chunk_batch(items);
    assert_eq!(chunks.iter().map(Vec::len).collect::<Vec<_>>(), [10_000, 1]);
    assert_eq!(ids(&chunks), order);

    // Bytes, with small limits: every chunk's create body fits, order is kept.
    let items: Vec<BatchItem> = (0..20).map(|i| tiny(100 + i * 37)).collect();
    let order: Vec<String> = items.iter().map(|i| i.custom_id.clone()).collect();
    // Each request carries the section schema (several KiB): about three fit.
    let one = serde_json::to_vec(&MessagesRequest::structured(&tiny(0).req))
        .unwrap()
        .len();
    let limit = one * 3 + 1_500;
    let chunks = chunk_batch_by(items, 1_000, limit);
    assert!(chunks.len() > 1);
    for c in &chunks {
        assert!(!c.is_empty());
        let body = serde_json::to_vec(&json!({
            "requests": c.iter().map(|i| json!({
                "custom_id": i.custom_id,
                "params": MessagesRequest::structured(&i.req),
            })).collect::<Vec<_>>()
        }))
        .unwrap();
        assert!(body.len() <= limit, "{} > {limit}", body.len());
        assert_eq!(batch_request_bytes(c), body.len());
    }
    assert_eq!(ids(&chunks), order);
    // Greedy: no chunk could have taken the next chunk's first item.
    for w in chunks.windows(2) {
        let mut joined = w[0].clone();
        joined.push(w[1][0].clone());
        assert!(batch_request_bytes(&joined) > limit);
    }

    // Count and bytes together: the tighter one wins.
    let chunks = chunk_batch_by((0..10).map(|_| tiny(10)).collect(), 3, 1_000_000);
    assert_eq!(
        chunks.iter().map(Vec::len).collect::<Vec<_>>(),
        [3, 3, 3, 1]
    );

    // An item larger than the limit on its own is sent alone (the API rejects it, not us).
    let chunks = chunk_batch_by(vec![tiny(10), tiny(5_000), tiny(10)], 100, one + 1_000);
    assert_eq!(chunks.iter().map(Vec::len).collect::<Vec<_>>(), [1, 1, 1]);

    // The real byte limit: two 101 MiB requests are two batches.
    let big = 101 * 1024 * 1024;
    let chunks = chunk_batch(vec![tiny(big), tiny(big)]);
    assert_eq!(chunks.iter().map(Vec::len).collect::<Vec<_>>(), [1, 1]);
}

// ---- weekly reviews and change notes ----------------------------------------------------------

fn weekly() -> WeeklyReviewInput {
    let b2 = load_fixture("b2-rapid");
    serde_json::from_value(json!({
        "inputVersion": "weekly-input-v1",
        "timeClass": b2["timeClass"],
        "band": b2["band"],
        "weekStart": "2026-11-16",
        "weekEnd": "2026-11-22",
        "gamesThisWeek": 14,
        "basis": b2["basis"],
        "adherence": { "minutesDone": 90, "minutesPlanned": 120 },
        "metricDeltas": [{
            "key": "blunder_rate", "area": "tactics", "label": "Blunders",
            "before": { "value": 0.2, "n": 40, "low": 0.15, "high": 0.25 },
            "after": { "value": 0.1, "n": 14, "low": null, "high": null },
            "verdict": "no_clear_change"
        }],
        "focusUpdates": [{ "weaknessId": "W1", "label": "Hanging pieces", "pImp": 0.6, "state": "active" }],
        "exemplars": [b2["weaknesses"][0]["exemplars"][0]],
        "changes": b2["changes"],
    }))
    .unwrap()
}

fn item(text: &str) -> Value {
    json!({ "text": text, "citations": [] })
}

fn review(outcome: &str) -> Value {
    json!({
        "headline": item("A steady week of practice."),
        "process": item("You put in {{num:adherence.minutesDone|int}} minutes of practice this week."),
        "outcomes": [{ "text": outcome, "citations": [], "weaknessId": "W1" }],
        "exemplars": [{
            "text": "In this game {{move:li-w1-0:5}} left a piece loose.",
            "citations": [{ "gameId": "li-w1-0", "ply": 5 }],
        }],
        "nextWeek": item("Next week, look for loose pieces before you move."),
    })
}

const GOOD_OUTCOME: &str =
    "Whether the work on hanging pieces shows in your results is unclear yet.";
const WRONG_OUTCOME: &str =
    "It is very likely that the work on hanging pieces shows in your results.";

#[tokio::test]
async fn weekly_batch_items_round_trip() {
    let input = weekly();
    let item_id = Uuid::new_v4();
    let item = weekly_batch_item(&input, item_id);
    assert_eq!(
        item.custom_id,
        batch_custom_id(&WEEKLY_REVIEW_SCHEMA, item_id)
    );
    assert_eq!(batch_item_id(&item.custom_id), Some(item_id));
    assert_eq!(wire(&item.req), wire(&build_weekly_request(&input)));

    let (ai, sync) = fake(
        vec![succeeded(&item.custom_id, review(GOOD_OUTCOME))],
        no_sync(),
    );
    let r = submit_and_collect(&ai, std::slice::from_ref(&item))
        .await
        .remove(0);
    let budget = no_budget();
    let out = process_weekly_result(&ai, &budget, r, &input)
        .await
        .unwrap();
    let WeeklyProcessed::Outcome(o) = out else {
        panic!("expected an outcome, got {out:?}")
    };
    assert!(o.dropped.is_empty(), "{:?}", o.dropped);
    assert_eq!(o.content.outcomes.len(), 1);
    assert_eq!(o.calls, vec![batch_record(Models::WEEKLY)]);
    assert_eq!(sync.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn weekly_invalid_result_gets_one_sync_repair_and_expired_is_retry_sync() {
    let input = weekly();
    let item = weekly_batch_item(&input, Uuid::new_v4());
    let expired = weekly_batch_item(&input, Uuid::new_v4());
    let (ai, sync) = fake(
        vec![
            succeeded(&item.custom_id, review(WRONG_OUTCOME)),
            FakeBatchItem {
                custom_id: expired.custom_id.clone(),
                outcome: BatchOutcome::Expired,
                ..FakeBatchItem::default()
            },
        ],
        |_| out(review(GOOD_OUTCOME)),
    );
    let mut got = submit_and_collect(&ai, &[item, expired]).await;
    let budget = no_budget();
    let WeeklyProcessed::Outcome(o) = process_weekly_result(&ai, &budget, got.remove(0), &input)
        .await
        .unwrap()
    else {
        panic!("expected an outcome")
    };
    assert_eq!(sync.load(Ordering::SeqCst), 1);
    assert!(o.dropped.is_empty(), "{:?}", o.dropped);
    assert_eq!(o.content.outcomes[0].text, GOOD_OUTCOME);
    assert_eq!(o.calls.len(), 2);
    assert_eq!(o.calls[0], batch_record(Models::WEEKLY));
    assert!(!o.calls[1].batch);

    let out = process_weekly_result(&ai, &budget, got.remove(0), &input)
        .await
        .unwrap();
    assert!(
        matches!(&out, WeeklyProcessed::RetrySync { calls } if calls.is_empty()),
        "{out:?}"
    );
    assert_eq!(sync.load(Ordering::SeqCst), 1);
}

fn change_input() -> WeaknessChangeInput {
    let b2 = load_fixture("b2-rapid");
    serde_json::from_value(json!({
        "change": b2["changes"][0],
        "band": b2["band"],
        "timeClass": b2["timeClass"],
        "basis": b2["basis"],
    }))
    .unwrap()
}

const K: &str = "tactics_blunder_rate";

fn note(text: &str) -> Value {
    json!({
        "text": text,
        "citations": [{"gameId": "li-c1-0", "ply": 21}, {"gameId": "li-c1-2", "ply": 25}],
    })
}

fn hedged_text() -> String {
    format!(
        "Since about {{{{date:changes.{K}.onsetDate}}}} you may be missing back-rank threats about {{{{num:changes.{K}.ratioAfterBefore|dec1}}}} times as often, around the time you switched to blitz."
    )
}

#[tokio::test]
async fn change_batch_items_round_trip_and_repair_once() {
    let input = change_input();
    let good = change_batch_item(&input, Uuid::new_v4());
    let bad = change_batch_item(&input, Uuid::new_v4());
    assert_eq!(
        decode_custom_id(&good.custom_id).unwrap().schema_id,
        "weakness-change"
    );
    assert_eq!(wire(&good.req), wire(&build_change_request(&input)));
    let (ai, sync) = fake(
        vec![
            succeeded(&good.custom_id, note(&hedged_text())),
            // A causal claim: rejected, repaired once.
            succeeded(
                &bad.custom_id,
                note("You blunder more because you switched to blitz."),
            ),
        ],
        |_| out(note(&hedged_text())),
    );
    let mut got = submit_and_collect(&ai, &[good, bad]).await;
    let budget = no_budget();

    let ChangeProcessed::Outcome(o) = process_change_result(&ai, &budget, got.remove(0), &input)
        .await
        .unwrap()
    else {
        panic!("expected an outcome")
    };
    assert_eq!(o.status, SectionStatus::Ready);
    assert_eq!(o.calls, vec![batch_record(Models::EXPLAIN)]);
    assert_eq!(sync.load(Ordering::SeqCst), 0);

    let ChangeProcessed::Outcome(o) = process_change_result(&ai, &budget, got.remove(0), &input)
        .await
        .unwrap()
    else {
        panic!("expected an outcome")
    };
    assert_eq!(sync.load(Ordering::SeqCst), 1);
    assert_eq!(o.status, SectionStatus::Ready, "{:?}", o.dropped);
    assert_eq!(o.text.as_deref(), Some(hedged_text().as_str()));
    assert_eq!(o.calls.len(), 2);
    assert_eq!(o.calls[0], batch_record(Models::EXPLAIN));
    assert!(!o.calls[1].batch);
    // The repair's errors name the failure by code and path.
    let reqs = structured_requests(&ai);
    let repair = reqs[0]["messages"][2].to_string();
    assert!(
        repair.contains(ValidationCode::CausalClaim.as_str()) || repair.contains("text:"),
        "{repair}"
    );
}

/// A fake whose batch holds `results` and whose sync repairs are all unavailable.
fn unavailable_repairs(results: Vec<FakeBatchItem>) -> FakeAiClient {
    let mut script = FakeScript {
        structured: Some(Box::new(|_, _| {
            Err(AiError::Unavailable {
                reason: UnavailableReason::Overloaded,
                retry_after: None,
            })
        })),
        ..FakeScript::default()
    };
    script.batches.insert(
        "fakebatch_1".to_owned(),
        FakeBatch {
            status: BatchStatus::Ended,
            results,
        },
    );
    FakeAiClient::new(script)
}

#[tokio::test]
async fn a_failed_weekly_or_change_repair_hands_the_billed_batch_call_back() {
    let budget = no_budget();

    let weekly_input = weekly();
    let item = weekly_batch_item(&weekly_input, Uuid::new_v4());
    let ai = unavailable_repairs(vec![succeeded(&item.custom_id, review(WRONG_OUTCOME))]);
    let r = submit_and_collect(&ai, std::slice::from_ref(&item))
        .await
        .remove(0);
    let e = process_weekly_result(&ai, &budget, r, &weekly_input)
        .await
        .unwrap_err();
    assert!(e.error.is_unavailable());
    assert_eq!(e.calls, vec![batch_record(Models::WEEKLY)]);

    let change = change_input();
    let item = change_batch_item(&change, Uuid::new_v4());
    let ai = unavailable_repairs(vec![succeeded(
        &item.custom_id,
        note("You blunder more because you switched to blitz."),
    )]);
    let r = submit_and_collect(&ai, std::slice::from_ref(&item))
        .await
        .remove(0);
    let e = process_change_result(&ai, &budget, r, &change)
        .await
        .unwrap_err();
    assert!(e.error.is_unavailable());
    assert_eq!(e.calls, vec![batch_record(Models::EXPLAIN)]);
}
