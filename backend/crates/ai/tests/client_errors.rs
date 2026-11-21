//! Port of the `error taxonomy` and retry-after parts of
//! `packages/ai/test/client/fix-round-1.test.ts`, plus redaction and the wire shapes. Transport
//! errors come from a local wiremock server only; nothing here calls the live API.

use std::time::{Duration, SystemTime};

use ai::client::errors::{
    CLIENT_BUILDER_ERROR, CLIENT_REDIRECT_ERROR, classify_batch_error_type, classify_status,
    classify_transport, parse_retry_after, redact,
};
use ai::client::model::{AiModel, Effort};
use ai::client::port::{
    AiError, AiErrorKind, AiUsage, BatchOutcome, BatchStatus, ChatTurnRequest, Message,
    OutputSchema, ParseIssue, Role, StructuredRequest, TextBlock, ToolChoice, ToolDef,
    UnavailableReason,
};
use ai::client::wire::{
    BatchCreate, BatchObject, BatchResult, BatchResultLine, ErrorBody, MessageResponse,
    MessagesRequest,
};
use serde_json::{Value, json};
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "sk-ant-test-SECRET123";

fn unavailable(reason: UnavailableReason, retry_after: Option<Duration>) -> AiError {
    AiError::Unavailable {
        reason,
        retry_after,
    }
}

fn error_type(body: &str) -> Option<String> {
    serde_json::from_str::<ErrorBody>(body)
        .ok()
        .and_then(|b| b.error.kind)
}

// ---- taxonomy ---------------------------------------------------------------------------------

#[test]
fn _401_and_403_are_ai_auth_error() {
    for status in [401, 403] {
        let t = error_type(r#"{"type":"error","error":{"type":"x","message":"m"}}"#);
        assert_eq!(
            classify_status(status, t.as_deref(), None),
            AiError::Auth { status }
        );
    }
}

#[test]
fn _402_billing_error_and_413_are_non_retryable_requests() {
    let t = error_type(r#"{"type":"error","error":{"type":"billing_error","message":"m"}}"#);
    let e = classify_status(402, t.as_deref(), Some(Duration::from_secs(3)));
    assert_eq!(
        e,
        AiError::Request {
            status: 402,
            api_error_type: Some("billing_error".into())
        }
    );
    assert!(!e.is_unavailable());
    assert_eq!(e.retry_after(), None);
    let e = classify_status(413, Some("request_too_large"), None);
    assert_eq!(
        e,
        AiError::Request {
            status: 413,
            api_error_type: Some("request_too_large".into())
        }
    );
    assert!(!e.is_unavailable());
}

#[test]
fn _400_stays_a_plain_ai_request_error() {
    let t = error_type(
        r#"{"type":"error","error":{"type":"invalid_request_error","message":"m"},"request_id":"req_1"}"#,
    );
    assert_eq!(
        classify_status(400, t.as_deref(), None),
        AiError::Request {
            status: 400,
            api_error_type: Some("invalid_request_error".into())
        }
    );
    assert_eq!(
        classify_status(404, None, None),
        AiError::Request {
            status: 404,
            api_error_type: None
        }
    );
}

#[test]
fn _429_exposes_retry_after() {
    let ra = parse_retry_after(Some("7"), SystemTime::now());
    assert_eq!(ra, Some(Duration::from_secs(7)));
    let e = classify_status(429, Some("rate_limit_error"), ra);
    assert_eq!(
        e,
        unavailable(UnavailableReason::RateLimited, Some(Duration::from_secs(7)))
    );
    assert_eq!(e.retry_after(), Some(Duration::from_millis(7000)));
}

#[test]
fn _429_without_retry_after_has_no_retry_after() {
    let e = classify_status(
        429,
        Some("rate_limit_error"),
        parse_retry_after(None, SystemTime::now()),
    );
    assert_eq!(e.retry_after(), None);
    assert!(e.is_unavailable());
}

#[test]
fn overloaded_and_server_errors_are_unavailable() {
    assert_eq!(
        classify_status(529, Some("overloaded_error"), None),
        unavailable(UnavailableReason::Overloaded, None)
    );
    assert_eq!(
        classify_status(529, None, None),
        unavailable(UnavailableReason::Overloaded, None)
    );
    // An overloaded_error body on another status is still overloaded.
    assert_eq!(
        classify_status(500, Some("overloaded_error"), None),
        unavailable(UnavailableReason::Overloaded, None)
    );
    for status in [500, 502, 503, 504] {
        assert_eq!(
            classify_status(status, Some("api_error"), Some(Duration::from_secs(1))),
            unavailable(UnavailableReason::Server, None)
        );
    }
}

#[test]
fn batch_error_types_map_to_kinds() {
    use AiErrorKind::*;
    assert_eq!(
        classify_batch_error_type(Some("overloaded_error")),
        Retryable
    );
    assert_eq!(classify_batch_error_type(Some("api_error")), Retryable);
    assert_eq!(
        classify_batch_error_type(Some("invalid_request_error")),
        Fatal
    );
    assert_eq!(
        classify_batch_error_type(Some("authentication_error")),
        Fatal
    );
    assert_eq!(classify_batch_error_type(Some("permission_error")), Fatal);
    assert_eq!(classify_batch_error_type(Some("something_else")), Unknown);
    assert_eq!(classify_batch_error_type(None), Unknown);
}

// ---- retry-after ------------------------------------------------------------------------------

#[test]
fn retry_after_parses_seconds_and_http_dates() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_445_412_480); // 2015-10-21 07:28:00Z
    assert_eq!(parse_retry_after(Some("0"), now), Some(Duration::ZERO));
    assert_eq!(
        parse_retry_after(Some("1.5"), now),
        Some(Duration::from_millis(1500))
    );
    assert_eq!(
        parse_retry_after(Some(" 12 "), now),
        Some(Duration::from_secs(12))
    );
    assert_eq!(
        parse_retry_after(Some("Wed, 21 Oct 2015 07:28:30 GMT"), now),
        Some(Duration::from_secs(30))
    );
    // A date in the past clamps to zero.
    assert_eq!(
        parse_retry_after(Some("Wed, 21 Oct 2015 07:00:00 GMT"), now),
        Some(Duration::ZERO)
    );
    for bad in ["", "-1", "soon", "NaN", "inf"] {
        assert_eq!(parse_retry_after(Some(bad), now), None, "{bad:?}");
    }
    assert_eq!(parse_retry_after(None, now), None);
}

// ---- transport (local wiremock only) ----------------------------------------------------------

fn http() -> reqwest::Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .timeout(Duration::from_millis(300))
        .build()
        .unwrap()
}

#[tokio::test]
async fn connect_failures_are_unavailable_connection() {
    // Nothing listens on a dropped listener's port (wiremock pools its servers, so use std).
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let url = format!("http://127.0.0.1:{port}/v1/messages?k={KEY}");
    let e = http().get(&url).send().await.unwrap_err();
    assert_eq!(
        classify_transport(&e),
        unavailable(UnavailableReason::Connection, None)
    );
}

#[tokio::test]
async fn timeouts_are_unavailable_connection() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
        .mount(&server)
        .await;
    let e = http().get(server.uri()).send().await.unwrap_err();
    assert!(e.is_timeout());
    assert_eq!(
        classify_transport(&e),
        unavailable(UnavailableReason::Connection, None)
    );
}

#[tokio::test]
async fn decode_errors_are_response_errors_without_content() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(format!(r#"{{"id":"TOPSECRETVALUE {KEY}","#)),
        )
        .mount(&server)
        .await;
    let resp = http().get(server.uri()).send().await.unwrap();
    let e = resp.json::<MessageResponse>().await.unwrap_err();
    let c = classify_transport(&e);
    assert!(matches!(c, AiError::Response(_)), "{c:?}");
    for s in [format!("{c}"), format!("{c:?}")] {
        assert!(!s.contains("SECRET123"), "{s}");
        assert!(!s.contains("TOPSECRETVALUE"), "{s}");
    }
}

#[tokio::test]
async fn builder_errors_are_fatal_requests() {
    let e = http().get("not a url").send().await.unwrap_err();
    assert!(e.is_builder());
    let c = classify_transport(&e);
    assert_eq!(
        c,
        AiError::Request {
            status: 0,
            api_error_type: Some(CLIENT_BUILDER_ERROR.into())
        }
    );
    assert!(!c.is_unavailable());
}

#[tokio::test]
async fn redirect_errors_are_fatal_requests() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(
            ResponseTemplate::new(302).insert_header("location", format!("{}/x", server.uri())),
        )
        .mount(&server)
        .await;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(0))
        .build()
        .unwrap();
    let e = client.get(server.uri()).send().await.unwrap_err();
    assert!(e.is_redirect(), "{e:?}");
    assert_eq!(
        classify_transport(&e),
        AiError::Request {
            status: 0,
            api_error_type: Some(CLIENT_REDIRECT_ERROR.into())
        }
    );
}

// ---- redaction --------------------------------------------------------------------------------

#[test]
fn redact_strips_keys() {
    assert_eq!(
        redact(&format!("bad key {KEY} here")),
        "bad key [redacted] here"
    );
    assert_eq!(
        redact("a sk-ant-api03-AbC_d-9 and sk-ant-x"),
        "a [redacted] and [redacted]"
    );
    assert_eq!(redact("no key"), "no key");
    // A bare prefix is not a key.
    assert_eq!(redact("sk-ant-"), "sk-ant-");
}

#[tokio::test]
async fn errors_never_render_a_key() {
    let body = format!(r#"{{"type":"error","error":{{"type":"{KEY}","message":"{KEY}"}}}}"#);
    let t = error_type(&body);
    assert!(t.is_some());

    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_string(body.clone()))
        .mount(&server)
        .await;
    let resp = http().get(server.uri()).send().await.unwrap();
    let decode = classify_transport(&resp.json::<MessageResponse>().await.unwrap_err());

    let usage = AiUsage::default();
    let errors = vec![
        classify_status(400, t.as_deref(), None),
        classify_status(401, t.as_deref(), None),
        classify_status(429, t.as_deref(), Some(Duration::from_secs(1))),
        classify_status(500, t.as_deref(), None),
        classify_status(529, t.as_deref(), None),
        decode,
        AiError::Response(redact(&body)),
        cost_unknown(&format!("claude-{KEY}"), &usage),
        AiError::Budget,
        unavailable(UnavailableReason::CircuitOpen, None),
        unavailable(UnavailableReason::NoApiKey, None),
    ];
    for e in &errors {
        for s in [format!("{e}"), format!("{e:?}")] {
            assert!(!s.contains("SECRET123"), "{s}");
        }
    }
    // Every variant is covered.
    let has = |f: fn(&AiError) -> bool| errors.iter().any(f);
    assert!(has(|e| matches!(e, AiError::Unavailable { .. })));
    assert!(has(|e| matches!(e, AiError::Auth { .. })));
    assert!(has(|e| matches!(e, AiError::Request { .. })));
    assert!(has(|e| matches!(e, AiError::Response(_))));
    assert!(has(|e| matches!(e, AiError::Budget)));
    assert!(has(|e| matches!(e, AiError::UnknownModel(_))));
}

fn cost_unknown(model: &str, u: &AiUsage) -> AiError {
    let table = ai::client::cost::PricingTable::seed();
    ai::client::cost::cost_micro_usd(&table, model, u, false).unwrap_err()
}

// ---- wire shapes ------------------------------------------------------------------------------

static SCHEMA: OutputSchema = OutputSchema {
    id: "report-section",
    version: 1,
    json: || json!({"type":"object","properties":{"section":{"type":"string"}},"required":["section"],"additionalProperties":false}),
    check: |_| Ok::<(), Vec<ParseIssue>>(()),
};

fn structured() -> StructuredRequest {
    StructuredRequest {
        model: AiModel::Sonnet55,
        system: vec![
            TextBlock::cached("<core>"),
            TextBlock::cached("<report+tone>"),
        ],
        messages: vec![Message {
            role: Role::User,
            content: vec![
                TextBlock::cached("<coaching_input>{}</coaching_input>").to_json(),
                TextBlock::new("<section task>").to_json(),
            ],
        }],
        max_tokens: 16000,
        effort: Effort::Medium,
        schema: &SCHEMA,
    }
}

fn tool(name: &'static str) -> ToolDef {
    ToolDef {
        name,
        description: "…",
        input_schema: json!({"type":"object","properties":{"gameId":{"type":"string"},"ply":{"type":"integer"}},"required":["gameId","ply"],"additionalProperties":false}),
    }
}

fn chat(choice: ToolChoice) -> ChatTurnRequest {
    ChatTurnRequest {
        model: AiModel::Sonnet55,
        system: vec![TextBlock::cached("<chat core>")],
        tools: vec![tool("get_shape"), tool("get_position")],
        tool_choice: choice,
        messages: vec![Message {
            role: Role::User,
            content: vec![json!({"type":"text","text":"hi"})],
        }],
        max_tokens: 64000,
        effort: Effort::Low,
    }
}

#[test]
fn structured_request_matches_the_wire_shape_in_order() {
    let s = serde_json::to_string(&MessagesRequest::structured(&structured())).unwrap();
    let expected = concat!(
        r#"{"model":"claude-sonnet-5-5","max_tokens":16000,"#,
        r#""system":[{"cache_control":{"type":"ephemeral"},"text":"<core>","type":"text"},{"cache_control":{"type":"ephemeral"},"text":"<report+tone>","type":"text"}],"#,
        r#""messages":[{"role":"user","content":[{"cache_control":{"type":"ephemeral"},"text":"<coaching_input>{}</coaching_input>","type":"text"},{"text":"<section task>","type":"text"}]}],"#,
        r#""output_config":{"effort":"medium","format":{"type":"json_schema","schema":{"additionalProperties":false,"properties":{"section":{"type":"string"}},"required":["section"],"type":"object"}}}}"#,
    );
    assert_eq!(s, expected);
}

#[test]
fn chat_turn_request_streams_with_sorted_strict_tools_and_auto_cache() {
    let v = serde_json::to_value(MessagesRequest::chat_turn(&chat(ToolChoice::Auto))).unwrap();
    assert_eq!(v["stream"], true);
    assert_eq!(v["cache_control"], json!({"type":"ephemeral"}));
    assert_eq!(v["tool_choice"], json!({"type":"auto"}));
    assert_eq!(v["output_config"], json!({"effort":"low"}));
    assert_eq!(v["max_tokens"], 64000);
    let names: Vec<&str> = v["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["get_position", "get_shape"]);
    assert_eq!(
        v["tools"][0],
        json!({"name":"get_position","description":"…","strict":true,"eager_input_streaming":true,
            "input_schema":{"type":"object","properties":{"gameId":{"type":"string"},"ply":{"type":"integer"}},"required":["gameId","ply"],"additionalProperties":false}})
    );
    // Tool field order is fixed too.
    let s = serde_json::to_string(&v["tools"][0]).unwrap();
    assert!(s.starts_with(r#"{"description""#)); // Value maps are sorted; check the struct below.
    let t = MessagesRequest::chat_turn(&chat(ToolChoice::Auto));
    let s = serde_json::to_string(&t.tools.unwrap()[0]).unwrap();
    assert!(s.starts_with(r#"{"name":"get_position","description":"…","strict":true,"eager_input_streaming":true,"input_schema":"#));
}

#[test]
fn chat_turn_with_no_tools_sends_no_tool_fields() {
    let mut req = chat(ToolChoice::None);
    req.tools.clear();
    let v = serde_json::to_value(MessagesRequest::chat_turn(&req)).unwrap();
    assert!(v.get("tools").is_none());
    assert!(v.get("tool_choice").is_none());
    assert_eq!(v["stream"], true);
}

fn keys_everywhere(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                out.push(k.clone());
                keys_everywhere(x, out);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| keys_everywhere(x, out)),
        _ => {}
    }
}

#[test]
fn request_serialises_without_forbidden_fields() {
    let reqs = [
        MessagesRequest::structured(&structured()),
        MessagesRequest::chat_turn(&chat(ToolChoice::Auto)),
        MessagesRequest::chat_turn(&chat(ToolChoice::None)),
    ];
    for r in &reqs {
        let v = serde_json::to_value(r).unwrap();
        let mut keys = Vec::new();
        keys_everywhere(&v, &mut keys);
        for forbidden in [
            "temperature",
            "top_p",
            "top_k",
            "thinking",
            "budget_tokens",
            "fallbacks",
            "speed",
            "betas",
        ] {
            assert!(!keys.iter().any(|k| k == forbidden), "{forbidden} in {v}");
        }
        if let Some(tc) = v.get("tool_choice") {
            let t = tc["type"].as_str().unwrap();
            assert!(t == "auto" || t == "none", "{t}");
            assert_eq!(tc.as_object().unwrap().len(), 1);
        }
        // No prefill: the last message is the user's.
        let last = v["messages"].as_array().unwrap().last().unwrap();
        assert_eq!(last["role"], "user");
    }
    let none = serde_json::to_value(MessagesRequest::chat_turn(&chat(ToolChoice::None))).unwrap();
    assert_eq!(none["tool_choice"], json!({"type":"none"}));
}

#[test]
fn batch_create_wraps_params_without_stream() {
    let b = BatchCreate::new(vec![(
        "report-section__v1__0123456789abcdef0123456789abcdef".to_owned(),
        MessagesRequest::structured(&structured()),
    )]);
    let v = serde_json::to_value(&b).unwrap();
    let item = &v["requests"][0];
    assert_eq!(
        item["custom_id"],
        "report-section__v1__0123456789abcdef0123456789abcdef"
    );
    assert_eq!(item["params"]["model"], "claude-sonnet-5-5");
    assert!(item["params"].get("stream").is_none());
    // A streamed request loses `stream` when batched.
    let b = BatchCreate::new(vec![(
        "c".to_owned(),
        MessagesRequest::chat_turn(&chat(ToolChoice::Auto)),
    )]);
    let v = serde_json::to_value(&b).unwrap();
    assert!(v["requests"][0]["params"].get("stream").is_none());
}

#[test]
fn message_response_reads_the_recorded_body() {
    let body = r#"{"id":"msg_01","type":"message","role":"assistant","model":"claude-sonnet-5-5",
        "content":[{"type":"thinking","thinking":"","signature":"sig"},{"type":"text","text":"{\"section\":\"overview\"}"}],
        "stop_reason":"end_turn","stop_sequence":null,"stop_details":null,
        "usage":{"input_tokens":12,"output_tokens":800,"cache_creation_input_tokens":0,"cache_read_input_tokens":5100,"service_tier":"standard"},
        "container":null}"#;
    let m: MessageResponse = serde_json::from_str(body).unwrap();
    assert_eq!(m.id, "msg_01");
    assert_eq!(m.model, "claude-sonnet-5-5");
    assert_eq!(m.content.len(), 2);
    assert_eq!(m.content[0]["type"], "thinking");
    assert_eq!(m.stop_reason.as_deref(), Some("end_turn"));
    assert_eq!(
        m.usage.to_ai_usage(),
        AiUsage {
            input_tokens: 12,
            output_tokens: 800,
            cache_creation_tokens: 0,
            cache_read_tokens: 5100
        }
    );
    // Cache counters may be absent.
    let m: MessageResponse = serde_json::from_str(
        r#"{"id":"m","type":"message","role":"assistant","model":"claude-haiku-5-5","content":[],"stop_reason":null,"usage":{"input_tokens":1,"output_tokens":2}}"#,
    )
    .unwrap();
    assert_eq!(m.usage.to_ai_usage().cache_read_tokens, 0);
    assert_eq!(m.stop_reason, None);
}

#[test]
fn error_body_keeps_only_the_type() {
    let b: ErrorBody = serde_json::from_str(
        r#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"},"request_id":"req_1"}"#,
    )
    .unwrap();
    assert_eq!(b.error.kind.as_deref(), Some("rate_limit_error"));
    assert!(!format!("{b:?}").contains("slow down"));
}

#[test]
fn batch_object_and_result_lines_read() {
    let o: BatchObject = serde_json::from_str(
        r#"{"id":"msgbatch_1","type":"message_batch","processing_status":"ended",
        "request_counts":{"processing":0,"succeeded":2,"errored":1,"canceled":0,"expired":0},
        "results_url":"https://api.anthropic.com/v1/messages/batches/msgbatch_1/results",
        "created_at":"2026-11-21T10:00:00Z","ended_at":"2026-11-21T10:05:00Z","expires_at":"2026-11-22T10:00:00Z",
        "archived_at":null,"cancel_initiated_at":null}"#,
    )
    .unwrap();
    assert_eq!(o.processing_status, BatchStatus::Ended);
    assert_eq!(o.request_counts.succeeded, 2);
    assert_eq!(o.request_counts.errored, 1);
    assert!(o.results_url.unwrap().ends_with("/results"));
    let o: BatchObject = serde_json::from_str(
        r#"{"id":"msgbatch_2","type":"message_batch","processing_status":"in_progress","results_url":null}"#,
    )
    .unwrap();
    assert_eq!(o.processing_status, BatchStatus::InProgress);
    assert_eq!(o.results_url, None);

    let lines = [
        (
            r#"{"custom_id":"a","result":{"type":"succeeded","message":{"id":"m","type":"message","role":"assistant","model":"claude-haiku-5-5","content":[{"type":"text","text":"{}"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}}}"#,
            BatchOutcome::Succeeded,
            None,
        ),
        (
            r#"{"custom_id":"b","result":{"type":"errored","error":{"type":"error","error":{"type":"invalid_request_error","message":"m"}}}}"#,
            BatchOutcome::Errored,
            Some("invalid_request_error"),
        ),
        (
            r#"{"custom_id":"c","result":{"type":"canceled"}}"#,
            BatchOutcome::Canceled,
            None,
        ),
        (
            r#"{"custom_id":"d","result":{"type":"expired"}}"#,
            BatchOutcome::Expired,
            None,
        ),
    ];
    for (line, outcome, err) in lines {
        let l: BatchResultLine = serde_json::from_str(line).unwrap();
        assert_eq!(l.result.outcome(), outcome);
        assert_eq!(l.result.error_type(), err);
        if let BatchResult::Succeeded { message } = &l.result {
            assert_eq!(message.model, "claude-haiku-5-5");
        }
    }
}
