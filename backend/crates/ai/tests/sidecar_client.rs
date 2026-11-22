//! The sidecar client (Task 11) against a wiremock sidecar. The bodies are the hand-written
//! fixtures in `backend/fixtures/ai/sidecar/`, written from the Task 10 contract (ADR-R3-6). No
//! live call is made and no key exists.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ai::client::batches::ApiKey;
use ai::client::model::{AiModel, Effort};
use ai::client::port::{
    AiError, AiUsage, ChatTurnRequest, Message, ModelUsage, OutputSchema, ParseError, ParseIssue,
    Role, SessionPath, StructuredRequest, TextBlock, ToolChoice, ToolDef, ToolHandler, ToolReply,
    ToolUse, TurnEvent, TurnResult, UnavailableReason, metered_turn_usage,
};
use ai::client::sidecar::{SidecarClient, SidecarConfig, SidecarToken};
use ai::client::sidecar_wire::AuthKind;
use ai::client::tool_server::ToolServer;
use ai::testing::fixtures_dir;
use futures::StreamExt;
use futures::future::BoxFuture;
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const TOKEN: &str = "sidecar-token-SECRET789";
const API_KEY: &str = "sk-ant-test-SECRET123";
const OAUTH: &str = "sk-ant-oat01-SECRET456";

fn fixture(name: &str) -> String {
    let p = fixtures_dir().join("sidecar").join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn check(v: &Value) -> Result<(), Vec<ParseIssue>> {
    let mut issues = Vec::new();
    if !v.get("section").is_some_and(Value::is_string) {
        issues.push(ParseIssue {
            path: "section".into(),
            code: "invalid_type".into(),
        });
    }
    if !v.get("n").is_some_and(Value::is_number) {
        issues.push(ParseIssue {
            path: "n".into(),
            code: "missing_field".into(),
        });
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}

static SCHEMA: OutputSchema = OutputSchema {
    id: "probe",
    version: 1,
    json: || {
        json!({"type":"object","properties":{"section":{"type":"string"},"n":{"type":"number"}},
            "required":["section","n"],"additionalProperties":false})
    },
    check,
};

fn structured() -> StructuredRequest {
    StructuredRequest {
        model: AiModel::Sonnet55,
        system: vec![TextBlock::cached("<core>"), TextBlock::cached("<tone>")],
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
        description: "Looks something up.",
        input_schema: json!({"type":"object","properties":{"gameId":{"type":"string"}},
            "required":["gameId"],"additionalProperties":false}),
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
            content: vec![json!({"type":"text","text":"Why was move 24 bad?"})],
        }],
        max_tokens: 8000,
        effort: Effort::Low,
        max_turns: 7,
        conversation_id: uuid::Uuid::parse_str("0b6f1c1e-5d3a-4c1e-9a77-2f1d5a9c0e11").unwrap(),
        resume: false,
        rebuild_context: None,
    }
}

fn config(server: &MockServer) -> SidecarConfig {
    capture();
    let mut cfg = SidecarConfig::new(server.uri(), SidecarToken::new(TOKEN));
    cfg.max_retries = 0;
    cfg
}

fn client(server: &MockServer) -> SidecarClient {
    SidecarClient::new(config(server), None).unwrap()
}

fn json_ok(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.to_owned(), "application/json")
}

fn sse(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.to_owned(), "text/event-stream")
}

async fn mount_query(server: &MockServer, resp: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/v1/query"))
        .respond_with(resp)
        .mount(server)
        .await;
}

fn sonnet(i: u64, o: u64, cr: u64, cc: u64) -> AiUsage {
    AiUsage {
        input_tokens: i,
        output_tokens: o,
        cache_read_tokens: cr,
        cache_creation_tokens: cc,
    }
}

fn connection() -> AiError {
    AiError::Unavailable {
        reason: UnavailableReason::Connection,
        retry_after: None,
    }
}

fn keys(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                out.push(k.clone());
                keys(x, out);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| keys(x, out)),
        _ => {}
    }
}

async fn collect(
    s: futures::stream::BoxStream<'_, Result<TurnEvent, AiError>>,
) -> Vec<Result<TurnEvent, AiError>> {
    s.collect().await
}

// ---- structured ------------------------------------------------------------------------------

#[tokio::test]
async fn structured_posts_the_contract_with_bearer() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(json_ok(&fixture("query-ok.json")))
        .expect(1)
        .mount(&server)
        .await;
    let res = client(&server).structured(&structured()).await.unwrap();
    assert_eq!(res.parse_error, None);

    let reqs = server.received_requests().await.unwrap();
    // ADR 0043: the fixed project User-Agent, never an email address.
    let ua = reqs[0].headers.get("user-agent").unwrap().to_str().unwrap();
    assert_eq!(
        ua,
        "chess-improver/0.1 (+https://github.com/chess-improver)"
    );
    assert_eq!(ua, ai::client::wire::USER_AGENT);
    assert!(!ua.contains('@'));
    let body: Value = serde_json::from_slice(&reqs[0].body).unwrap();
    assert_eq!(body["model"], "claude-sonnet-5-5");
    assert_eq!(body["effort"], "medium");
    assert_eq!(body["maxTurns"], 3);
    assert_eq!(body["maxTokens"], 16000);
    assert_eq!(body["timeoutMs"], 600_000);
    assert_eq!(body["systemPrompt"], "<core>\n\n<tone>");
    assert_eq!(body["outputFormat"]["type"], "json_schema");
    assert_eq!(body["outputFormat"]["schema"], (SCHEMA.json)());
    assert_eq!(
        body["prompt"],
        json!([{"type":"text","text":"<coaching_input>{}</coaching_input>"},
               {"type":"text","text":"<section task>"}])
    );
    let mut all = Vec::new();
    keys(&body, &mut all);
    for banned in [
        "cache_control",
        "temperature",
        "top_p",
        "top_k",
        "thinking",
        "fallbacks",
        "accountId",
    ] {
        assert!(!all.iter().any(|k| k == banned), "{banned} was sent");
    }
}

#[tokio::test]
async fn model_usage_maps_per_model_and_sums_usage() {
    let server = MockServer::start().await;
    mount_query(&server, json_ok(&fixture("query-two-models.json"))).await;
    let res = client(&server).structured(&structured()).await.unwrap();
    assert_eq!(
        res.model_usage,
        vec![
            ModelUsage {
                model: "claude-haiku-5-5".into(),
                usage: sonnet(120, 12, 0, 0),
            },
            ModelUsage {
                model: "claude-sonnet-5-5".into(),
                usage: sonnet(3000, 400, 2000, 100),
            },
        ]
    );
    assert_eq!(res.usage, sonnet(3120, 412, 2000, 100));
    assert_eq!(res.sdk_cost_usd, Some(0.0123));
    assert_eq!(res.model, "claude-sonnet-5-5");
    assert_eq!(res.stop_reason.as_deref(), Some("end_turn"));
    assert_eq!(
        res.parsed,
        Some(json!({"section":"Two models touched this call.","n":1}))
    );
    assert_eq!(
        res.content,
        vec![
            json!({"type":"text","text":"{\"n\":1,\"section\":\"Two models touched this call.\"}"})
        ]
    );
}

#[tokio::test]
async fn structured_output_is_checked_by_rust() {
    let server = MockServer::start().await;
    let body = json!({"subtype":"success","stopReason":"end_turn",
        "structuredOutput":{"section": 7},"text":"","numTurns":1,"durationMs":1,
        "modelUsage":{},"sdkCostUsd":0.0});
    mount_query(&server, json_ok(&body.to_string())).await;
    let res = client(&server).structured(&structured()).await.unwrap();
    assert_eq!(res.parsed, None);
    assert_eq!(res.parse_error, Some(ParseError::SchemaMismatch));
    let issues = res.parse_issues.unwrap();
    assert_eq!(
        issues,
        vec![
            ParseIssue {
                path: "section".into(),
                code: "invalid_type".into()
            },
            ParseIssue {
                path: "n".into(),
                code: "missing_field".into()
            },
        ]
    );
    let rendered = serde_json::to_string(&issues).unwrap();
    assert!(!rendered.contains('7'), "issues carry no content");
}

#[tokio::test]
async fn refusal_gives_parsed_none() {
    let server = MockServer::start().await;
    mount_query(&server, json_ok(&fixture("query-refusal.json"))).await;
    let res = client(&server).structured(&structured()).await.unwrap();
    assert_eq!(res.parsed, None);
    assert_eq!(res.parse_error, Some(ParseError::Refusal));
    assert_eq!(res.stop_reason.as_deref(), Some("refusal"));
}

#[tokio::test]
async fn max_tokens_truncation_gives_parsed_none() {
    let server = MockServer::start().await;
    // Even a schema-valid output is discarded when the model was cut off.
    let body = json!({"subtype":"success","stopReason":"max_tokens",
        "structuredOutput":{"section":"x","n":1},"text":"","numTurns":1,"durationMs":1,
        "modelUsage":{},"sdkCostUsd":0.0});
    mount_query(&server, json_ok(&body.to_string())).await;
    let res = client(&server).structured(&structured()).await.unwrap();
    assert_eq!(res.parsed, None);
    assert_eq!(res.parse_error, Some(ParseError::Truncated));
}

#[tokio::test]
async fn max_structured_retries_is_schema_mismatch() {
    let server = MockServer::start().await;
    mount_query(&server, json_ok(&fixture("query-max-retries.json"))).await;
    let res = client(&server).structured(&structured()).await.unwrap();
    assert_eq!(res.parsed, None);
    assert_eq!(res.parse_error, Some(ParseError::SchemaMismatch));
    assert_eq!(res.parse_issues, None);
    assert_eq!(
        res.usage.input_tokens, 9360,
        "a failed call is still metered"
    );
}

#[tokio::test]
async fn missing_structured_output_is_no_text() {
    let server = MockServer::start().await;
    let body = json!({"subtype":"error_during_execution","stopReason":"end_turn",
        "text":"","numTurns":1,"durationMs":1,"modelUsage":{},"sdkCostUsd":0.0});
    mount_query(&server, json_ok(&body.to_string())).await;
    let res = client(&server).structured(&structured()).await.unwrap();
    assert_eq!(res.parsed, None);
    assert_eq!(res.parse_error, Some(ParseError::NoText));
}

fn error_body(ty: &str) -> String {
    json!({"type":"error","error":{"type":ty,"message":"upstream API error"}}).to_string()
}

#[tokio::test]
async fn maps_sidecar_errors_by_status() {
    let cases: Vec<(u16, &str, Option<&str>, AiError)> = vec![
        (
            529,
            "overloaded_error",
            None,
            AiError::Unavailable {
                reason: UnavailableReason::Overloaded,
                retry_after: None,
            },
        ),
        (
            429,
            "rate_limit_error",
            Some("7"),
            AiError::Unavailable {
                reason: UnavailableReason::RateLimited,
                retry_after: Some(Duration::from_secs(7)),
            },
        ),
        (
            401,
            "authentication_error",
            None,
            AiError::Auth { status: 401 },
        ),
        (
            400,
            "invalid_request_error",
            None,
            AiError::Request {
                status: 400,
                api_error_type: Some("invalid_request_error".into()),
            },
        ),
        (
            500,
            "api_error",
            None,
            AiError::Unavailable {
                reason: UnavailableReason::Server,
                retry_after: None,
            },
        ),
        (
            409,
            "turn_in_progress",
            None,
            AiError::Request {
                status: 409,
                api_error_type: Some("turn_in_progress".into()),
            },
        ),
    ];
    for (status, ty, retry_after, want) in cases {
        let server = MockServer::start().await;
        let mut resp =
            ResponseTemplate::new(status).set_body_raw(error_body(ty), "application/json");
        if let Some(ra) = retry_after {
            resp = resp.insert_header("retry-after", ra);
        }
        mount_query(&server, resp).await;
        let err = client(&server).structured(&structured()).await.unwrap_err();
        assert_eq!(err, want, "HTTP {status}");
    }

    // A refused connection (nothing listens on the port any more).
    let server = MockServer::start().await;
    let mut cfg = config(&server);
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    cfg.base_url = format!("http://{}", closed.local_addr().unwrap());
    drop(closed);
    let err = SidecarClient::new(cfg, None)
        .unwrap()
        .structured(&structured())
        .await
        .unwrap_err();
    assert_eq!(err, connection());
}

/// Answers 529 for the first `fail` requests, then `ok`.
struct FailThen {
    fail: usize,
    seen: Mutex<usize>,
    ok: ResponseTemplate,
}

impl Respond for FailThen {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let mut n = self.seen.lock().unwrap();
        *n += 1;
        if *n <= self.fail {
            ResponseTemplate::new(529)
                .set_body_raw(error_body("overloaded_error"), "application/json")
        } else {
            self.ok.clone()
        }
    }
}

#[tokio::test]
async fn retries_529_once_before_first_byte() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query"))
        .respond_with(FailThen {
            fail: 1,
            seen: Mutex::new(0),
            ok: json_ok(&fixture("query-ok.json")),
        })
        .mount(&server)
        .await;
    let mut cfg = config(&server);
    cfg.max_retries = 1;
    let c = SidecarClient::new(cfg, None).unwrap();
    let res = c.structured(&structured()).await.unwrap();
    assert_eq!(res.parse_error, None);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);

    // Two 529s exhaust the single retry.
    server.reset().await;
    Mock::given(method("POST"))
        .and(path("/v1/query"))
        .respond_with(FailThen {
            fail: 2,
            seen: Mutex::new(0),
            ok: json_ok(&fixture("query-ok.json")),
        })
        .mount(&server)
        .await;
    let err = c.structured(&structured()).await.unwrap_err();
    assert!(matches!(
        err,
        AiError::Unavailable {
            reason: UnavailableReason::Overloaded,
            ..
        }
    ));
    assert_eq!(server.received_requests().await.unwrap().len(), 2);

    // A 500 is not retried by the sidecar client.
    server.reset().await;
    mount_query(
        &server,
        ResponseTemplate::new(500).set_body_raw(error_body("api_error"), "application/json"),
    )
    .await;
    c.structured(&structured()).await.unwrap_err();
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

// ---- chat ------------------------------------------------------------------------------------

struct Echo;

impl ToolHandler for Echo {
    fn call<'a>(&'a self, name: &'a str, input: Value) -> BoxFuture<'a, ToolReply> {
        Box::pin(async move {
            ToolReply {
                content: json!({"tool": name, "input": input}).to_string(),
                is_error: false,
            }
        })
    }
}

#[tokio::test]
async fn stream_turn_yields_deltas_tool_uses_then_done() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(sse(&fixture("chat-tool-then-text.sse")))
        .mount(&server)
        .await;
    let tools = ToolServer::start(
        "127.0.0.1:0".parse().unwrap(),
        "http://rust-worker:7311".into(),
    )
    .await
    .unwrap();
    let c = SidecarClient::new(config(&server), Some(tools.clone())).unwrap();
    let req = chat(ToolChoice::Auto);
    let handler: Arc<dyn ToolHandler> = Arc::new(Echo);
    let events = collect(c.stream_turn(&req, Some(handler))).await;
    let events: Vec<TurnEvent> = events.into_iter().map(Result::unwrap).collect();
    let usage = sonnet(4096, 57, 1500, 300);
    let entry = vec![ModelUsage {
        model: "claude-sonnet-5-5".into(),
        usage,
    }];
    assert_eq!(
        events,
        vec![
            TurnEvent::Message("msg_01".into()),
            TurnEvent::TextDelta("Let me check.".into()),
            TurnEvent::Message("msg_02".into()),
            TurnEvent::TextDelta("Your knight ".into()),
            TurnEvent::TextDelta("was loose.".into()),
            TurnEvent::Done(TurnResult {
                content: vec![json!({"type":"text","text":"Your knight was loose."})],
                stop_reason: Some("end_turn".into()),
                tool_uses: vec![ToolUse {
                    id: "toolu_01".into(),
                    name: "get_position".into(),
                    input: Some(json!({"gameId":"g1","ply":24})),
                }],
                usage,
                model: "claude-sonnet-5-5".into(),
                model_usage: entry.clone(),
                sdk_cost_usd: Some(0.0118),
                session_model_usage: entry,
                session: Some(SessionPath::New),
            }),
        ]
    );

    let reqs = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(&reqs[0].body).unwrap();
    assert_eq!(body["conversationId"], req.conversation_id.to_string());
    assert_eq!(body["resume"], false);
    assert!(body.get("rebuildContext").is_none());
    assert_eq!(body["maxTurns"], 7);
    assert_eq!(body["timeoutMs"], 180_000);
    assert_eq!(body["systemPrompt"], "<chat core>");
    let names: Vec<&str> = body["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["get_position", "get_shape"], "sorted by name");
    assert_eq!(body["tools"][0]["inputSchema"], tool("x").input_schema);
    assert_eq!(
        body["toolCallback"]["url"],
        "http://rust-worker:7311/v1/tools"
    );
    let token = body["toolCallback"]["token"].as_str().unwrap();
    assert_eq!(token.len(), 64);
    assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
    let mut all = Vec::new();
    keys(&body, &mut all);
    assert!(
        !all.iter()
            .any(|k| k == "cache_control" || k == "outputFormat")
    );
    assert_eq!(
        tools.registrations(),
        0,
        "the turn's registration ends with the stream"
    );
}

#[tokio::test]
async fn resumed_turn_sends_rebuild_context_and_is_metered_by_difference() {
    let server = MockServer::start().await;
    let done = json!({"subtype":"success","stopReason":"end_turn","text":"ok","numTurns":1,
        "durationMs":1,
        "modelUsage":{"claude-sonnet-5-5":{"inputTokens":10,"outputTokens":5,"cacheReadInputTokens":0,"cacheCreationInputTokens":0}},
        "sessionModelUsage":{"claude-sonnet-5-5":{"inputTokens":110,"outputTokens":25,"cacheReadInputTokens":7,"cacheCreationInputTokens":0}},
        "sdkCostUsd":0.01,"session":"resumed"});
    Mock::given(method("POST"))
        .and(path("/v1/chat"))
        .respond_with(sse(&format!("event: done\ndata: {done}\n\n")))
        .mount(&server)
        .await;
    let mut req = chat(ToolChoice::None);
    req.resume = true;
    req.rebuild_context = Some("<summary>".into());
    let c = client(&server);
    let events = collect(c.stream_turn(&req, None)).await;
    let Some(Ok(TurnEvent::Done(turn))) = events.last().cloned() else {
        panic!("{events:?}")
    };
    let body: Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(body["resume"], true);
    assert_eq!(body["rebuildContext"], "<summary>");
    assert_eq!(turn.session, Some(SessionPath::Resumed));

    let prev = vec![ModelUsage {
        model: "claude-sonnet-5-5".into(),
        usage: sonnet(90, 15, 7, 0),
    }];
    assert_eq!(
        metered_turn_usage(&turn, Some(&prev)),
        vec![ModelUsage {
            model: "claude-sonnet-5-5".into(),
            usage: sonnet(20, 10, 0, 0),
        }]
    );
    // A negative difference (the totals were reset by a rebuild) falls back to modelUsage.
    let reset = vec![ModelUsage {
        model: "claude-sonnet-5-5".into(),
        usage: sonnet(500, 15, 7, 0),
    }];
    assert_eq!(metered_turn_usage(&turn, Some(&reset)), turn.model_usage);
    // No stored previous total, or a turn that was not resumed: modelUsage.
    assert_eq!(metered_turn_usage(&turn, None), turn.model_usage);
    let mut rebuilt = turn.clone();
    rebuilt.session = Some(SessionPath::Rebuilt);
    assert_eq!(metered_turn_usage(&rebuilt, Some(&prev)), turn.model_usage);

    // resume without a rebuild context is rejected before any request.
    let mut bad = chat(ToolChoice::None);
    bad.resume = true;
    let events = collect(c.stream_turn(&bad, None)).await;
    assert!(matches!(
        events.as_slice(),
        [Err(AiError::Request { status: 400, .. })]
    ));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn tool_choice_none_sends_no_tools_and_no_callback() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat"))
        .respond_with(sse(&fixture("chat-tool-then-text.sse")))
        .mount(&server)
        .await;
    let tools = ToolServer::start(
        "127.0.0.1:0".parse().unwrap(),
        "http://rust-worker:7311".into(),
    )
    .await
    .unwrap();
    let c = SidecarClient::new(config(&server), Some(tools.clone())).unwrap();
    let req = chat(ToolChoice::None);
    let handler: Arc<dyn ToolHandler> = Arc::new(Echo);
    let mut s = c.stream_turn(&req, Some(handler));
    let first = s.next().await.unwrap().unwrap();
    assert_eq!(first, TurnEvent::Message("msg_01".into()));
    assert_eq!(tools.registrations(), 0, "no callback is registered");
    drop(s);
    let body: Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(body["tools"], json!([]));
    assert!(body.get("toolCallback").is_none());
}

#[tokio::test]
async fn stream_dropped_mid_message_is_connection_error() {
    let server = MockServer::start().await;
    let mut body = fixture("chat-tool-then-text.sse");
    body.truncate(body.find("event: done").unwrap());
    Mock::given(method("POST"))
        .and(path("/v1/chat"))
        .respond_with(sse(&body))
        .mount(&server)
        .await;
    let events = collect(client(&server).stream_turn(&chat(ToolChoice::Auto), None)).await;
    assert_eq!(events.last().unwrap(), &Err(connection()));
    assert_eq!(
        events.iter().filter(|e| e.is_err()).count(),
        1,
        "exactly one terminal error"
    );
    assert!(events.iter().all(|e| !matches!(e, Ok(TurnEvent::Done(_)))));
}

#[tokio::test]
async fn stream_idle_timeout_fires() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat"))
        .respond_with(sse(&fixture("chat-tool-then-text.sse")).set_delay(Duration::from_secs(2)))
        .mount(&server)
        .await;
    let mut cfg = config(&server);
    cfg.stream_idle_timeout = Duration::from_millis(200);
    let c = SidecarClient::new(cfg, None).unwrap();
    let started = Instant::now();
    let events = collect(c.stream_turn(&chat(ToolChoice::Auto), None)).await;
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(events, vec![Err(connection())]);
}

#[tokio::test]
async fn chat_error_event_is_classified() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat"))
        .respond_with(sse(&fixture("chat-error-overloaded.sse")))
        .mount(&server)
        .await;
    let events = collect(client(&server).stream_turn(&chat(ToolChoice::Auto), None)).await;
    assert_eq!(
        events,
        vec![
            Ok(TurnEvent::Message("msg_01".into())),
            Ok(TurnEvent::TextDelta("Hm".into())),
            Err(AiError::Unavailable {
                reason: UnavailableReason::Overloaded,
                retry_after: None,
            }),
        ]
    );

    // A shutdown (503) and a timeout (504) are server errors; a pre-stream 409 is "busy".
    for (status, want) in [
        (503, UnavailableReason::Server),
        (504, UnavailableReason::Server),
    ] {
        let server = MockServer::start().await;
        let ev = json!({"type":"error","error":{"type":"api_error","message":"x"},"status":status});
        Mock::given(method("POST"))
            .and(path("/v1/chat"))
            .respond_with(sse(&format!("event: error\ndata: {ev}\n\n")))
            .mount(&server)
            .await;
        let events = collect(client(&server).stream_turn(&chat(ToolChoice::Auto), None)).await;
        assert_eq!(
            events,
            vec![Err(AiError::Unavailable {
                reason: want,
                retry_after: None
            })]
        );
    }
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat"))
        .respond_with(
            ResponseTemplate::new(409)
                .set_body_raw(error_body("turn_in_progress"), "application/json"),
        )
        .mount(&server)
        .await;
    let events = collect(client(&server).stream_turn(&chat(ToolChoice::Auto), None)).await;
    assert_eq!(
        events,
        vec![Err(AiError::Request {
            status: 409,
            api_error_type: Some("turn_in_progress".into())
        })]
    );
}

#[tokio::test]
async fn retract_events_are_forwarded() {
    let server = MockServer::start().await;
    let done = json!({"subtype":"success","stopReason":"end_turn","text":"fallback text",
        "numTurns":1,"durationMs":1,"modelUsage":{},"sessionModelUsage":{},"sdkCostUsd":0.0,
        "session":"new"});
    let body = format!(
        "event: delta\ndata: {}\n\nevent: retract\ndata: {}\n\nevent: delta\ndata: {}\n\nevent: done\ndata: {done}\n\n",
        json!({"text":"refused","messageId":"msg_1"}),
        json!({"messageIds":["msg_1"],"reason":"refusal"}),
        json!({"text":"fallback text","messageId":"msg_2"}),
    );
    Mock::given(method("POST"))
        .and(path("/v1/chat"))
        .respond_with(sse(&body))
        .mount(&server)
        .await;
    let events = collect(client(&server).stream_turn(&chat(ToolChoice::Auto), None)).await;
    let events: Vec<TurnEvent> = events.into_iter().map(Result::unwrap).collect();
    assert_eq!(
        events[..5],
        [
            TurnEvent::Message("msg_1".into()),
            TurnEvent::TextDelta("refused".into()),
            TurnEvent::Retract {
                message_ids: vec!["msg_1".into()],
                reason: "refusal".into()
            },
            TurnEvent::Message("msg_2".into()),
            TurnEvent::TextDelta("fallback text".into()),
        ]
    );
    let TurnEvent::Done(t) = &events[5] else {
        panic!()
    };
    assert_eq!(
        t.content,
        vec![json!({"type":"text","text":"fallback text"})]
    );
}

#[tokio::test]
async fn max_turns_subtype_maps_to_max_turns_stop_reason() {
    let server = MockServer::start().await;
    let done = json!({"subtype":"error_max_turns","stopReason":"tool_use","text":"",
        "numTurns":7,"durationMs":1,"modelUsage":{},"sessionModelUsage":{},"sdkCostUsd":0.0,
        "session":"new"});
    Mock::given(method("POST"))
        .and(path("/v1/chat"))
        .respond_with(sse(&format!("event: done\ndata: {done}\n\n")))
        .mount(&server)
        .await;
    let events = collect(client(&server).stream_turn(&chat(ToolChoice::Auto), None)).await;
    let Some(Ok(TurnEvent::Done(t))) = events.last() else {
        panic!("{events:?}")
    };
    assert_eq!(t.stop_reason.as_deref(), Some("max_turns"));
    assert_eq!(t.content, Vec::<Value>::new());
}

#[tokio::test]
async fn undecodable_event_data_is_a_response_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat"))
        .respond_with(sse(&format!(
            "event: delta\ndata: {{not json {API_KEY}\n\nevent: done\ndata: {{}}\n\n"
        )))
        .mount(&server)
        .await;
    let events = collect(client(&server).stream_turn(&chat(ToolChoice::Auto), None)).await;
    assert_eq!(events.len(), 1, "{events:?}");
    let Err(AiError::Response(msg)) = &events[0] else {
        panic!("{events:?}")
    };
    assert!(!msg.contains(API_KEY));
    assert!(!msg.contains("not json"));
}

#[tokio::test]
async fn health_parses_auth_kind() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(json_ok(
            &json!({"ok":true,"auth":"api_key","inFlight":1,"queued":0,"maxConcurrency":4,
                "sdkVersion":"0.3.325","mirrorErrors":0,"nulStrippedEntries":0,"lockLost":0})
            .to_string(),
        ))
        .mount(&server)
        .await;
    let h = client(&server).health().await.unwrap();
    assert!(h.ok);
    assert_eq!(h.auth, AuthKind::ApiKey);
    assert_eq!((h.in_flight, h.queued, h.max_concurrency), (1, 0, 4));
    assert_eq!(h.sdk_version, "0.3.325");
    for (wire, kind) in [
        ("cloud", AuthKind::Cloud),
        ("oauth", AuthKind::Oauth),
        ("none", AuthKind::None),
    ] {
        let v: AuthKind = serde_json::from_value(json!(wire)).unwrap();
        assert_eq!(v, kind);
    }
}

// ---- fix round 1 -----------------------------------------------------------------------------

fn mu(model: &str, usage: AiUsage) -> ModelUsage {
    ModelUsage {
        model: model.into(),
        usage,
    }
}

async fn resumed_two_models() -> TurnResult {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat"))
        .respond_with(sse(&fixture("chat-resumed-two-models.sse")))
        .mount(&server)
        .await;
    let mut req = chat(ToolChoice::None);
    req.resume = true;
    req.rebuild_context = Some("<summary>".into());
    let events = collect(client(&server).stream_turn(&req, None)).await;
    match events.last() {
        Some(Ok(TurnEvent::Done(t))) => t.clone(),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn resumed_two_model_turn_meters_each_model_by_difference() {
    let turn = resumed_two_models().await;
    assert_eq!(turn.session, Some(SessionPath::Resumed));
    let prev = vec![
        mu("claude-haiku-5-5", sonnet(120, 12, 0, 0)),
        mu("claude-sonnet-5-5", sonnet(2000, 100, 6000, 1200)),
    ];
    assert_eq!(
        metered_turn_usage(&turn, Some(&prev)),
        vec![
            mu("claude-haiku-5-5", sonnet(40, 4, 0, 0)),
            // The difference also counts internal calls the stream sum (512 input) missed.
            mu("claude-sonnet-5-5", sonnet(560, 30, 3000, 0)),
        ]
    );
    assert_eq!(
        turn.model_usage,
        vec![
            mu("claude-haiku-5-5", sonnet(40, 4, 0, 0)),
            mu("claude-sonnet-5-5", sonnet(512, 30, 3000, 0)),
        ]
    );
}

#[tokio::test]
async fn an_empty_previous_total_falls_back_to_model_usage() {
    let turn = resumed_two_models().await;
    // An empty stored total is no baseline: it would meter the whole session again.
    assert_eq!(metered_turn_usage(&turn, Some(&[])), turn.model_usage);
    assert_eq!(metered_turn_usage(&turn, None), turn.model_usage);
}

#[tokio::test]
async fn a_model_missing_from_now_falls_back_to_model_usage() {
    let turn = resumed_two_models().await;
    let prev = vec![
        mu("claude-haiku-5-5", sonnet(120, 12, 0, 0)),
        mu("claude-sonnet-5-5", sonnet(2000, 100, 6000, 1200)),
        mu("claude-opus-5-5", sonnet(1, 1, 0, 0)),
    ];
    assert_eq!(metered_turn_usage(&turn, Some(&prev)), turn.model_usage);
}

#[tokio::test]
async fn a_retracted_tool_use_is_removed_from_tool_uses() {
    let server = MockServer::start().await;
    let done = json!({"subtype":"success","stopReason":"end_turn","text":"fixed","numTurns":2,
        "durationMs":1,"modelUsage":{},"sessionModelUsage":{},"sdkCostUsd":0.0,"session":"new"});
    let body = format!(
        "event: tool_use\ndata: {}\n\nevent: tool_use\ndata: {}\n\nevent: retract\ndata: {}\n\nevent: done\ndata: {done}\n\n",
        json!({"id":"toolu_bad","name":"mcp__coach__get_position","input":{},"messageId":"msg_tool"}),
        json!({"id":"toolu_ok","name":"mcp__coach__get_shape","input":{"gameId":"g"},"messageId":"msg_ok"}),
        json!({"messageIds":["msg_tool"],"reason":"superseded"}),
    );
    Mock::given(method("POST"))
        .and(path("/v1/chat"))
        .respond_with(sse(&body))
        .mount(&server)
        .await;
    let events = collect(client(&server).stream_turn(&chat(ToolChoice::Auto), None)).await;
    let Some(Ok(TurnEvent::Done(t))) = events.last() else {
        panic!("{events:?}")
    };
    assert_eq!(
        t.tool_uses,
        vec![ToolUse {
            id: "toolu_ok".into(),
            name: "get_shape".into(),
            input: Some(json!({"gameId":"g"})),
        }]
    );
}

#[tokio::test]
async fn the_registration_ends_when_the_terminal_event_is_queued() {
    for file in ["chat-tool-then-text.sse", "chat-error-overloaded.sse"] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat"))
            .respond_with(sse(&fixture(file)))
            .mount(&server)
            .await;
        let tools = ToolServer::start("127.0.0.1:0".parse().unwrap(), "http://w:7311".into())
            .await
            .unwrap();
        let c = SidecarClient::new(config(&server), Some(tools.clone())).unwrap();
        let req = chat(ToolChoice::Auto);
        let handler: Arc<dyn ToolHandler> = Arc::new(Echo);
        let mut s = c.stream_turn(&req, Some(handler));
        let first = s.next().await.unwrap();
        assert!(first.is_ok(), "{file}");
        // The whole body is one chunk, so the terminal item is already queued: the stream is
        // still alive, but the turn's tools are gone.
        assert_eq!(tools.registrations(), 0, "{file}");
        while s.next().await.is_some() {}
    }
}

#[tokio::test]
async fn an_oversized_sse_line_is_a_response_error() {
    let server = MockServer::start().await;
    let huge = "x".repeat(ai::client::sse::MAX_LINE + 1);
    Mock::given(method("POST"))
        .and(path("/v1/chat"))
        .respond_with(sse(&format!("event: delta\ndata: {huge}\n\n")))
        .mount(&server)
        .await;
    let events = collect(client(&server).stream_turn(&chat(ToolChoice::Auto), None)).await;
    assert_eq!(events.len(), 1);
    assert!(
        matches!(&events[0], Err(AiError::Response(_))),
        "{:?}",
        events[0].as_ref().err()
    );
}

// ---- secrets ---------------------------------------------------------------------------------

/// Collects every event and span field rendered by the `tracing` macros, from every test in this
/// file. It is the global subscriber (installed by [`config`], before any client exists): a
/// scoped one races with callsite registration on parallel test threads.
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<String>>);

static CAPTURE: std::sync::OnceLock<Capture> = std::sync::OnceLock::new();

fn capture() -> Capture {
    CAPTURE
        .get_or_init(|| {
            let c = Capture::default();
            tracing::subscriber::set_global_default(c.clone()).expect("first subscriber");
            c
        })
        .clone()
}

struct Visitor<'a>(&'a mut String);

impl tracing::field::Visit for Visitor<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.push_str(&format!("{}={value:?} ", field.name()));
    }
}

impl tracing::Subscriber for Capture {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        let mut s = self.0.lock().unwrap();
        span.record(&mut Visitor(&mut s));
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, values: &tracing::span::Record<'_>) {
        let mut s = self.0.lock().unwrap();
        values.record(&mut Visitor(&mut s));
    }
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut s = self.0.lock().unwrap();
        event.record(&mut Visitor(&mut s));
        s.push('\n');
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

#[tokio::test]
async fn secrets_never_leak() {
    let capture = capture();
    let secrets = [TOKEN, API_KEY, OAUTH];
    let echo = format!("{TOKEN} {API_KEY} {OAUTH}");

    let mut errors: Vec<AiError> = Vec::new();
    for status in [400u16, 401, 429, 500, 529] {
        let server = MockServer::start().await;
        let body = json!({"type":"error","error":{"type":format!("x_{echo}"),"message":echo}});
        mount_query(
            &server,
            ResponseTemplate::new(status).set_body_raw(body.to_string(), "application/json"),
        )
        .await;
        errors.push(client(&server).structured(&structured()).await.unwrap_err());
    }
    let server = MockServer::start().await;
    mount_query(&server, json_ok(&format!("{{\"broken\": \"{echo}"))).await;
    errors.push(client(&server).structured(&structured()).await.unwrap_err());
    let server = MockServer::start().await;
    let ev =
        json!({"type":"error","error":{"type":format!("t_{echo}"),"message":echo},"status":400});
    Mock::given(method("POST"))
        .and(path("/v1/chat"))
        .respond_with(sse(&format!(
            "event: error\ndata: {ev}\n\nevent: delta\ndata: {{{echo}\n\n"
        )))
        .mount(&server)
        .await;
    for e in collect(client(&server).stream_turn(&chat(ToolChoice::Auto), None)).await {
        if let Err(e) = e {
            errors.push(e);
        }
    }
    assert!(errors.len() >= 7);

    let mut rendered: Vec<String> = errors
        .iter()
        .flat_map(|e| [format!("{e}"), format!("{e:?}")])
        .collect();
    let token = SidecarToken::new(TOKEN);
    let key = ApiKey::new(API_KEY);
    let oauth = ApiKey::new(OAUTH);
    rendered.extend([
        format!("{token}"),
        format!("{token:?}"),
        format!("{key}"),
        format!("{key:?}"),
        format!("{oauth:?}"),
        format!("{:?}", config(&server)),
    ]);
    let logs = capture.0.lock().unwrap().clone();
    assert!(
        logs.contains("/v1/query"),
        "debug logs were captured: {logs}"
    );
    rendered.push(logs);
    for r in &rendered {
        for s in secrets {
            assert!(!r.contains(s), "{s} leaked in {r}");
        }
    }
}
