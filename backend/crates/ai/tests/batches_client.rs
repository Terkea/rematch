//! The minimal Messages Batches client (Task 11) against wiremock as the Anthropic base URL, with
//! the recorded shapes in `backend/fixtures/ai/api/`. Ports the batch parts of the TS client tests
//! (`packages/ai/test/client/*.test.ts`: submit, status mapping, results by resolver, errored
//! kinds). No live call is made; the key is a fake.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use ai::client::batches::{ApiKey, BatchesClient, BatchesConfig};
use ai::client::custom_id::decode_custom_id;
use ai::client::model::{AiModel, Effort};
use ai::client::port::{
    AiClient, AiError, AiErrorKind, AiUsage, BatchItem, BatchOutcome, BatchResultItem, BatchStatus,
    Message, ModelUsage, OutputSchema, ParseError, ParseIssue, Role, SchemaResolver,
    StructuredRequest, TextBlock, UnavailableReason,
};
use ai::client::runtime::AiRuntime;
use ai::client::sidecar::{SidecarClient, SidecarConfig, SidecarToken};
use ai::client::wire::MessagesRequest;
use ai::testing::fixtures_dir;
use futures::StreamExt;
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const KEY: &str = "sk-ant-test-SECRET123";
const BATCH: &str = "msgbatch_01HkcTjaV5uDC8jWR4ZsDV8d";

fn fixture(name: &str) -> String {
    let p = fixtures_dir().join("api").join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn check(v: &Value) -> Result<(), Vec<ParseIssue>> {
    if v.get("section").is_some_and(Value::is_string) && v.get("n").is_some_and(Value::is_number) {
        Ok(())
    } else {
        Err(vec![ParseIssue {
            path: "section".into(),
            code: "invalid_type".into(),
        }])
    }
}

static SCHEMA: OutputSchema = OutputSchema {
    id: "probe",
    version: 1,
    json: || json!({"type":"object","properties":{"section":{"type":"string"},"n":{"type":"number"}}}),
    check,
};

/// Resolves by the schema id and version encoded in the custom id, as a fresh process would.
struct ById;

impl SchemaResolver for ById {
    fn resolve(&self, custom_id: &str) -> Option<&'static OutputSchema> {
        let p = decode_custom_id(custom_id)?;
        (p.schema_id == SCHEMA.id && p.version == SCHEMA.version).then_some(&SCHEMA)
    }
}

fn req() -> StructuredRequest {
    StructuredRequest {
        model: AiModel::Haiku55,
        system: vec![TextBlock::cached("<core>")],
        messages: vec![Message {
            role: Role::User,
            content: vec![
                TextBlock::cached("<input>").to_json(),
                TextBlock::new("<task>").to_json(),
            ],
        }],
        max_tokens: 4000,
        effort: Effort::Low,
        schema: &SCHEMA,
    }
}

fn items() -> Vec<BatchItem> {
    ["probe__v1__weekly-u1", "probe__v1__weekly-u2"]
        .into_iter()
        .map(|id| BatchItem {
            custom_id: id.into(),
            req: req(),
        })
        .collect()
}

fn config(server: &MockServer) -> BatchesConfig {
    let mut cfg = BatchesConfig::new(ApiKey::new(KEY));
    cfg.base_url = server.uri();
    cfg.backoff_base = Duration::from_millis(10);
    cfg
}

fn client(server: &MockServer) -> BatchesClient {
    BatchesClient::new(config(server)).unwrap()
}

fn json_resp(status: u16, body: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_raw(body.to_owned(), "application/json")
}

async fn results(c: &BatchesClient) -> Vec<Result<BatchResultItem, AiError>> {
    c.results(BATCH, &ById).collect().await
}

#[tokio::test]
async fn batch_create_sends_params_with_api_key_and_version() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages/batches"))
        .and(header("x-api-key", KEY))
        .and(header("anthropic-version", "2026-10-21"))
        .and(header("content-type", "application/json"))
        .and(header(
            "user-agent",
            "chess-improver/0.1 (+https://github.com/chess-improver)",
        ))
        .respond_with(json_resp(200, &fixture("batch-create.json")))
        .expect(1)
        .mount(&server)
        .await;
    let id = client(&server).submit(&items()).await.unwrap();
    assert_eq!(id, BATCH);

    let sent = &server.received_requests().await.unwrap()[0];
    // ADR 0043: one shared project User-Agent, never an email address.
    let ua = sent.headers.get("user-agent").unwrap().to_str().unwrap();
    assert_eq!(ua, ai::client::wire::USER_AGENT);
    assert!(!ua.contains('@'));
    let body: Value = serde_json::from_slice(&sent.body).unwrap();
    let want = serde_json::to_value(MessagesRequest::structured(&req())).unwrap();
    assert_eq!(body["requests"].as_array().unwrap().len(), 2);
    assert_eq!(body["requests"][0]["custom_id"], "probe__v1__weekly-u1");
    assert_eq!(body["requests"][0]["params"], want);
    assert!(body["requests"][0]["params"].get("stream").is_none());
    assert_eq!(
        body["requests"][0]["params"]["system"][0]["cache_control"],
        json!({"type":"ephemeral"}),
        "batches keep explicit cache breakpoints"
    );
}

#[tokio::test]
async fn batch_status_maps_processing_status() {
    for (fixture_name, want) in [
        ("batch-create.json", BatchStatus::InProgress),
        ("batch-ended.json", BatchStatus::Ended),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(format!("/v1/messages/batches/{BATCH}")))
            .and(header("x-api-key", KEY))
            .respond_with(json_resp(200, &fixture(fixture_name)))
            .mount(&server)
            .await;
        assert_eq!(client(&server).status(BATCH).await.unwrap(), want);
    }
    let server = MockServer::start().await;
    let canceling = fixture("batch-create.json").replace("in_progress", "canceling");
    Mock::given(method("GET"))
        .respond_with(json_resp(200, &canceling))
        .mount(&server)
        .await;
    assert_eq!(
        client(&server).status(BATCH).await.unwrap(),
        BatchStatus::Canceling
    );
    // An id that could change the path is refused before any request.
    let err = client(&server).status("../../v1/x").await.unwrap_err();
    assert!(matches!(err, AiError::Request { status: 400, .. }));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

async fn mount_results(server: &MockServer, body: String) {
    Mock::given(method("GET"))
        .and(path(format!("/v1/messages/batches/{BATCH}/results")))
        .and(header("x-api-key", KEY))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/binary"))
        .mount(server)
        .await;
}

#[tokio::test]
async fn submit_with_one_instance_read_results_with_a_fresh_instance() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages/batches"))
        .respond_with(json_resp(200, &fixture("batch-create.json")))
        .mount(&server)
        .await;
    mount_results(&server, fixture("batch-results.jsonl")).await;
    let id = client(&server).submit(&items()).await.unwrap();

    let fresh = client(&server);
    let all = results(&fresh).await;
    let all: Vec<BatchResultItem> = all.into_iter().map(Result::unwrap).collect();
    assert_eq!(all.len(), 9);
    let first = &all[0];
    assert_eq!(first.custom_id, "probe__v1__weekly-u1");
    assert_eq!(first.outcome, BatchOutcome::Succeeded);
    assert_eq!(first.parse_error, None);
    let r = first.result.as_ref().unwrap();
    assert_eq!(r.parsed, Some(json!({"section":"Good week.","n":2})));
    let usage = AiUsage {
        input_tokens: 1000,
        output_tokens: 200,
        cache_creation_tokens: 0,
        cache_read_tokens: 800,
    };
    assert_eq!(r.usage, usage);
    assert_eq!(r.model, "claude-haiku-5-5");
    assert_eq!(
        r.model_usage,
        vec![ModelUsage {
            model: "claude-haiku-5-5".into(),
            usage
        }]
    );
    assert_eq!(r.sdk_cost_usd, None);

    let foreign = all.iter().find(|i| i.custom_id == "other__v9__x1").unwrap();
    assert_eq!(foreign.outcome, BatchOutcome::Succeeded);
    assert_eq!(foreign.parse_error, Some(ParseError::NoSchema));
    assert_eq!(foreign.result.as_ref().unwrap().parsed, None);
    assert_eq!(id, BATCH);
}

#[tokio::test]
async fn refusal_truncation_and_mismatch_are_reported_like_structured() {
    let server = MockServer::start().await;
    mount_results(&server, fixture("batch-results.jsonl")).await;
    let all: Vec<BatchResultItem> = results(&client(&server))
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect();
    let by = |id: &str| all.iter().find(|i| i.custom_id == id).unwrap().clone();
    assert_eq!(
        by("probe__v1__weekly-u2").parse_error,
        Some(ParseError::Refusal)
    );
    assert_eq!(
        by("probe__v1__weekly-u3").parse_error,
        Some(ParseError::Truncated)
    );
    let mismatch = by("probe__v1__weekly-u4");
    assert_eq!(mismatch.parse_error, Some(ParseError::SchemaMismatch));
    assert_eq!(
        mismatch.result.unwrap().parse_issues,
        Some(vec![ParseIssue {
            path: "section".into(),
            code: "invalid_type".into()
        }])
    );
}

#[tokio::test]
async fn errored_items_carry_kind_by_error_type() {
    let server = MockServer::start().await;
    mount_results(&server, fixture("batch-results.jsonl")).await;
    let all: Vec<BatchResultItem> = results(&client(&server))
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect();
    let by = |id: &str| all.iter().find(|i| i.custom_id == id).unwrap().clone();
    let over = by("probe__v1__weekly-u5");
    assert_eq!(over.outcome, BatchOutcome::Errored);
    assert_eq!(over.error_type.as_deref(), Some("overloaded_error"));
    assert_eq!(over.error_kind, Some(AiErrorKind::Retryable));
    assert_eq!(over.result, None);
    let bad = by("probe__v1__weekly-u6");
    assert_eq!(bad.error_type.as_deref(), Some("invalid_request_error"));
    assert_eq!(bad.error_kind, Some(AiErrorKind::Fatal));
    assert!(
        !format!("{bad:?}").contains(KEY),
        "the error message is never read"
    );
    assert_eq!(by("probe__v1__weekly-u7").outcome, BatchOutcome::Canceled);
    assert_eq!(by("probe__v1__weekly-u8").outcome, BatchOutcome::Expired);
    assert_eq!(by("probe__v1__weekly-u8").error_kind, None);
}

#[tokio::test]
async fn results_url_on_foreign_host_is_not_followed() {
    let server = MockServer::start().await;
    let foreign = MockServer::start().await;
    let ended = fixture("batch-ended.json").replace("https://api.anthropic.com", &foreign.uri());
    Mock::given(method("GET"))
        .and(path(format!("/v1/messages/batches/{BATCH}")))
        .respond_with(json_resp(200, &ended))
        .mount(&server)
        .await;
    mount_results(&server, fixture("batch-results.jsonl")).await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&foreign)
        .await;
    let c = client(&server);
    assert_eq!(c.status(BATCH).await.unwrap(), BatchStatus::Ended);
    assert_eq!(results(&c).await.len(), 9);
    assert!(foreign.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn classifies_a_jsonl_decode_error_without_leaking_content() {
    let server = MockServer::start().await;
    let mut body = fixture("batch-results.jsonl");
    body.push_str(&format!(
        "{{\"custom_id\":\"probe__v1__z\",\"result\":{{\"type\":\"succ {KEY} TOPSECRETVALUE\n"
    ));
    mount_results(&server, body).await;
    let all = results(&client(&server)).await;
    assert_eq!(all.len(), 10);
    assert_eq!(all.iter().filter(|r| r.is_ok()).count(), 9);
    let Err(AiError::Response(msg)) = &all[9] else {
        panic!("{:?}", all[9])
    };
    assert!(!msg.contains(KEY));
    assert!(!msg.contains("TOPSECRETVALUE"));
}

/// Answers `status` (with `retry_after`) for the first `fail` requests, then `ok`.
struct FailThen {
    status: u16,
    retry_after: Option<&'static str>,
    fail: usize,
    seen: Mutex<usize>,
    ok: ResponseTemplate,
}

impl Respond for FailThen {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let mut n = self.seen.lock().unwrap();
        *n += 1;
        if *n > self.fail {
            return self.ok.clone();
        }
        let file = if self.status == 429 {
            "error-429.json"
        } else {
            "error-529.json"
        };
        let mut r = json_resp(self.status, &fixture(file));
        if let Some(ra) = self.retry_after {
            r = r.insert_header("retry-after", ra);
        }
        r
    }
}

#[tokio::test]
async fn retries_529_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages/batches"))
        .respond_with(FailThen {
            status: 529,
            retry_after: None,
            fail: 2,
            seen: Mutex::new(0),
            ok: json_resp(200, &fixture("batch-create.json")),
        })
        .mount(&server)
        .await;
    assert_eq!(client(&server).submit(&items()).await.unwrap(), BATCH);
    assert_eq!(server.received_requests().await.unwrap().len(), 3);

    // max_retries (2) exhausted: the third 529 is returned.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(FailThen {
            status: 529,
            retry_after: None,
            fail: 3,
            seen: Mutex::new(0),
            ok: json_resp(200, &fixture("batch-create.json")),
        })
        .mount(&server)
        .await;
    let err = client(&server).status(BATCH).await.unwrap_err();
    assert_eq!(
        err,
        AiError::Unavailable {
            reason: UnavailableReason::Overloaded,
            retry_after: None
        }
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 3);

    // A 400 is never retried.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(json_resp(
            400,
            r#"{"type":"error","error":{"type":"invalid_request_error","message":"no"}}"#,
        ))
        .mount(&server)
        .await;
    let err = client(&server).status(BATCH).await.unwrap_err();
    assert!(matches!(err, AiError::Request { status: 400, .. }));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn retry_after_over_cap_is_not_slept() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(FailThen {
            status: 429,
            retry_after: Some("3600"),
            fail: 10,
            seen: Mutex::new(0),
            ok: json_resp(200, &fixture("batch-create.json")),
        })
        .mount(&server)
        .await;
    let started = Instant::now();
    let err = client(&server).status(BATCH).await.unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(
        err,
        AiError::Unavailable {
            reason: UnavailableReason::RateLimited,
            retry_after: Some(Duration::from_secs(3600))
        }
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);

    // Under the cap, the retry-after is honoured and the call succeeds.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(FailThen {
            status: 429,
            retry_after: Some("0.05"),
            fail: 1,
            seen: Mutex::new(0),
            ok: json_resp(200, &fixture("batch-create.json")),
        })
        .mount(&server)
        .await;
    assert_eq!(
        client(&server).status(BATCH).await.unwrap(),
        BatchStatus::InProgress
    );
}

#[tokio::test]
async fn runtime_without_batches_key_gives_no_api_key_and_batches_disabled() {
    let sidecar = MockServer::start().await;
    let cfg = SidecarConfig::new(sidecar.uri(), SidecarToken::new("t"));
    let rt = AiRuntime {
        sidecar: SidecarClient::new(cfg.clone(), None).unwrap(),
        batches: None,
    };
    assert!(!rt.batches_enabled());
    let no_key = AiError::Unavailable {
        reason: UnavailableReason::NoApiKey,
        retry_after: None,
    };
    assert_eq!(rt.batch_submit(&items()).await.unwrap_err(), no_key);
    assert_eq!(rt.batch_status(BATCH).await.unwrap_err(), no_key);
    let r: Vec<_> = rt.batch_results(BATCH, &ById).collect().await;
    assert_eq!(r, vec![Err(no_key)]);

    let api = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(json_resp(200, &fixture("batch-ended.json")))
        .mount(&api)
        .await;
    let rt = AiRuntime {
        sidecar: SidecarClient::new(cfg, None).unwrap(),
        batches: Some(client(&api)),
    };
    assert!(rt.batches_enabled());
    assert_eq!(rt.batch_status(BATCH).await.unwrap(), BatchStatus::Ended);
}

// ---- fix round 1: a batch is paid, so submit never re-sends one that may exist ----------------

#[tokio::test]
async fn a_500_on_submit_is_sent_once() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages/batches"))
        .respond_with(json_resp(
            500,
            r#"{"type":"error","error":{"type":"api_error","message":"x"}}"#,
        ))
        .mount(&server)
        .await;
    let err = client(&server).submit(&items()).await.unwrap_err();
    assert_eq!(
        err,
        AiError::Unavailable {
            reason: UnavailableReason::Server,
            retry_after: None
        }
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    // `status` keeps the full policy: a 500 there is retried.
    Mock::given(method("GET"))
        .respond_with(json_resp(
            500,
            r#"{"type":"error","error":{"type":"api_error","message":"x"}}"#,
        ))
        .mount(&server)
        .await;
    let err = client(&server).status(BATCH).await.unwrap_err();
    assert!(err.is_unavailable());
    assert_eq!(server.received_requests().await.unwrap().len(), 1 + 3);
}

#[tokio::test]
async fn a_timeout_on_submit_is_sent_once() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages/batches"))
        .respond_with(
            json_resp(200, &fixture("batch-create.json")).set_delay(Duration::from_secs(2)),
        )
        .mount(&server)
        .await;
    let mut cfg = config(&server);
    cfg.request_timeout = Duration::from_millis(200);
    let started = Instant::now();
    let err = BatchesClient::new(cfg)
        .unwrap()
        .submit(&items())
        .await
        .unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(err.is_unavailable(), "{err:?}");
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_429_on_submit_is_retried() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages/batches"))
        .respond_with(FailThen {
            status: 429,
            retry_after: Some("0.01"),
            fail: 1,
            seen: Mutex::new(0),
            ok: json_resp(200, &fixture("batch-create.json")),
        })
        .mount(&server)
        .await;
    assert_eq!(client(&server).submit(&items()).await.unwrap(), BATCH);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

/// The fixture's lines, plus a *valid* `succeeded` line padded with an ignored field to `pad`
/// bytes: without the cap it would decode fine, so only the cap can reject it.
fn padded_lines(pad: usize) -> (Vec<String>, String) {
    let lines: Vec<String> = fixture("batch-results.jsonl")
        .lines()
        .map(str::to_owned)
        .collect();
    let mut v: Value = serde_json::from_str(&lines[0]).unwrap();
    v["custom_id"] = json!("probe__v1__big");
    v["pad"] = json!("x".repeat(pad));
    let big = v.to_string();
    serde_json::from_str::<ai::client::wire::BatchResultLine>(&big).expect("decodes uncapped");
    (lines, big)
}

fn too_large(r: &Result<BatchResultItem, AiError>) {
    let Err(AiError::Response(msg)) = r else {
        panic!("{:?}", r.as_ref().map(|i| &i.custom_id))
    };
    assert!(msg.contains("too large"), "{msg}");
    assert!(!msg.contains("could not be decoded"), "{msg}");
    assert!(!msg.contains("xxxx"));
}

fn capped_client(server: &MockServer, max: usize) -> BatchesClient {
    let mut cfg = config(server);
    cfg.max_result_line = max;
    BatchesClient::new(cfg).unwrap()
}

#[tokio::test]
async fn an_oversized_results_line_is_a_response_error_and_is_skipped() {
    // With the real 4 MiB cap.
    let server = MockServer::start().await;
    let (lines, big) = padded_lines(ai::client::batches::MAX_RESULT_LINE);
    mount_results(&server, format!("{}\n{big}\n{}\n", lines[0], lines[1])).await;
    let all = results(&client(&server)).await;
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].as_ref().unwrap().custom_id, "probe__v1__weekly-u1");
    too_large(&all[1]);
    assert_eq!(all[2].as_ref().unwrap().custom_id, "probe__v1__weekly-u2");
}

#[tokio::test]
async fn a_complete_oversized_line_is_too_large() {
    // A small body arrives as one chunk, so the oversized line is complete (newline included)
    // when it is checked: this pins the complete-line cap.
    let server = MockServer::start().await;
    let (lines, big) = padded_lines(1500);
    mount_results(&server, format!("{}\n{big}\n{}\n", lines[0], lines[1])).await;
    let all: Vec<_> = capped_client(&server, 1024)
        .results(BATCH, &ById)
        .collect()
        .await;
    assert_eq!(all.len(), 3);
    assert!(all[0].is_ok());
    too_large(&all[1]);
    assert_eq!(all[2].as_ref().unwrap().custom_id, "probe__v1__weekly-u2");
}

#[tokio::test]
async fn an_oversized_last_line_without_a_newline_is_too_large() {
    // No newline ever arrives: only the no-newline guard can reject it.
    let server = MockServer::start().await;
    let (lines, big) = padded_lines(1500);
    mount_results(&server, format!("{}\n{big}", lines[0])).await;
    let all: Vec<_> = capped_client(&server, 1024)
        .results(BATCH, &ById)
        .collect()
        .await;
    assert_eq!(all.len(), 2);
    assert!(all[0].is_ok());
    too_large(&all[1]);
}
