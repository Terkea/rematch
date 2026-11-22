//! The scripted fake client. Ports the `fake client` block of
//! `packages/ai/test/client/fake-breaker.test.ts` and the `fake client resolver path` block of
//! `fix-round-1.test.ts`, plus the sidecar contract the fake models (ADR 0062): the tool loop
//! inside one turn, per-turn usage against `sessionModelUsage`, retractions, 409
//! `turn_in_progress`, refusals, truncation and the SDK's structured-output failure.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ai::client::fake::{
    FakeAiClient, FakeBatch, FakeBatchItem, FakeCallKind, FakeRetracted, FakeScript,
    FakeStructured, FakeTurn,
};
use ai::client::model::{AiModel, Effort};
use ai::client::port::{
    AiClient, AiError, AiErrorKind, AiUsage, BatchItem, BatchOutcome, BatchResultItem, BatchStatus,
    ChatTurnRequest, Message, ModelUsage, OutputSchema, ParseError, ParseIssue, Role,
    SchemaResolver, SessionPath, StructuredRequest, TextBlock, ToolChoice, ToolDef, ToolHandler,
    ToolReply, TurnEvent, TurnResult, UnavailableReason, metered_turn_usage,
};
use futures::StreamExt;
use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::{Value, json};

/// The TS schema `z.object({ section: z.string(), n: z.number() })`.
#[derive(Deserialize)]
#[allow(dead_code)]
struct Probe {
    section: String,
    n: f64,
}

fn check(v: &Value) -> Result<(), Vec<ParseIssue>> {
    serde_path_to_error::deserialize::<_, Probe>(v)
        .map(|_| ())
        .map_err(|e| {
            vec![ParseIssue {
                path: e.path().to_string(),
                code: "invalid_type".into(),
            }]
        })
}

static PROBE: OutputSchema = OutputSchema {
    id: "probe",
    version: 1,
    json: || json!({"type": "object"}),
    check,
};

/// The TS `z.any()`.
static ANY: OutputSchema = OutputSchema {
    id: "any",
    version: 1,
    json: || json!({}),
    check: |_| Ok(()),
};

fn sreq(schema: &'static OutputSchema) -> StructuredRequest {
    StructuredRequest {
        model: AiModel::Haiku55,
        system: vec![TextBlock::cached("<system>")],
        messages: vec![Message {
            role: Role::User,
            content: vec![json!({"type": "text", "text": "hi"})],
        }],
        max_tokens: 10,
        effort: Effort::Low,
        schema,
    }
}

fn tool(name: &'static str) -> ToolDef {
    ToolDef {
        name,
        description: "a tool",
        input_schema: json!({"type": "object"}),
    }
}

const CONV: &str = "0b6f1c1e-5d3a-4c1e-9a77-2f1d5a9c0e11";
const OTHER_CONV: &str = "7d1f9a20-3b4c-4d5e-8f60-718293a4b5c6";

fn treq() -> ChatTurnRequest {
    ChatTurnRequest {
        model: AiModel::Sonnet55,
        system: vec![],
        tools: vec![tool("get_eval"), tool("get_position")],
        tool_choice: ToolChoice::Auto,
        messages: vec![],
        max_tokens: 10,
        effort: Effort::Low,
        max_turns: 7,
        conversation_id: uuid::Uuid::parse_str(CONV).unwrap(),
        resume: false,
        rebuild_context: None,
    }
}

fn resumed(conv: &str) -> ChatTurnRequest {
    ChatTurnRequest {
        conversation_id: uuid::Uuid::parse_str(conv).unwrap(),
        resume: true,
        rebuild_context: Some("<summary>".into()),
        ..treq()
    }
}

fn usage(i: u64, o: u64) -> AiUsage {
    AiUsage {
        input_tokens: i,
        output_tokens: o,
        ..AiUsage::default()
    }
}

fn mu(model: &str, u: AiUsage) -> ModelUsage {
    ModelUsage {
        model: model.into(),
        usage: u,
    }
}

/// A handler that logs each call into a shared log and answers `{"ok":<name>}`.
struct LogHandler {
    log: Arc<Mutex<Vec<String>>>,
}

impl ToolHandler for LogHandler {
    fn call<'a>(&'a self, name: &'a str, input: Value) -> BoxFuture<'a, ToolReply> {
        Box::pin(async move {
            // Yield once, so a handler that is not awaited would be visible in the order.
            tokio::task::yield_now().await;
            self.log
                .lock()
                .unwrap()
                .push(format!("tool:{name}:{input}"));
            ToolReply {
                content: json!({ "ok": name }).to_string(),
                is_error: false,
            }
        })
    }
}

fn handler(log: &Arc<Mutex<Vec<String>>>) -> Option<Arc<dyn ToolHandler>> {
    Some(Arc::new(LogHandler { log: log.clone() }))
}

async fn collect_turn(
    ai: &dyn AiClient,
    req: &ChatTurnRequest,
    tools: Option<Arc<dyn ToolHandler>>,
) -> Vec<Result<TurnEvent, AiError>> {
    ai.stream_turn(req, tools).collect().await
}

fn done(events: &[Result<TurnEvent, AiError>]) -> TurnResult {
    match events.last() {
        Some(Ok(TurnEvent::Done(t))) => t.clone(),
        other => panic!("expected Done last, got {other:?}"),
    }
}

fn text(t: &str) -> Vec<Value> {
    vec![json!({"type": "text", "text": t})]
}

struct Resolver;

impl SchemaResolver for Resolver {
    fn resolve(&self, custom_id: &str) -> Option<&'static OutputSchema> {
        custom_id.starts_with('x').then_some(&PROBE)
    }
}

async fn results(ai: &dyn AiClient, id: &str) -> Vec<Result<BatchResultItem, AiError>> {
    ai.batch_results(id, &Resolver).collect().await
}

fn structured_script(
    f: impl Fn(&StructuredRequest, usize) -> Result<FakeStructured, AiError> + Send + Sync + 'static,
) -> FakeScript {
    FakeScript {
        structured: Some(Box::new(f)),
        ..FakeScript::default()
    }
}

#[tokio::test]
async fn records_calls_and_replays_deltas_in_order() {
    let ai = FakeAiClient::new(FakeScript {
        structured: Some(Box::new(|_, n| {
            Ok(FakeStructured {
                parsed: Some(json!({ "n": n })),
                ..FakeStructured::default()
            })
        })),
        turns: vec![Box::new(|_, _| {
            Ok(FakeTurn {
                tool_calls: vec![("get_eval".into(), json!({"x": 1}))],
                deltas: vec!["a".into(), "b".into(), "c".into()],
                ..FakeTurn::default()
            })
        })],
        ..FakeScript::default()
    });
    let r = ai.structured(&sreq(&ANY)).await.unwrap();
    assert_eq!(r.parsed, Some(json!({"n": 0})));
    assert_eq!(r.parse_error, None);
    assert_eq!(r.stop_reason.as_deref(), Some("end_turn"));

    let log = Arc::new(Mutex::new(Vec::new()));
    let events = collect_turn(&ai, &treq(), handler(&log)).await;
    let deltas: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            Ok(TurnEvent::TextDelta(t)) => Some(t.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(deltas, ["a", "b", "c"]);
    assert!(matches!(&events[0], Ok(TurnEvent::Message(_))));
    let turn = done(&events);
    assert_eq!(turn.content, text("abc"));
    assert_eq!(turn.stop_reason.as_deref(), Some("end_turn"));
    assert_eq!(turn.tool_uses.len(), 1);
    assert_eq!(turn.tool_uses[0].id, "fake_tu_1");
    assert_eq!(turn.tool_uses[0].name, "get_eval");
    assert_eq!(turn.tool_uses[0].input, Some(json!({"x": 1})));
    assert_eq!(turn.model, "claude-sonnet-5-5");

    let calls = ai.calls();
    let kinds: Vec<FakeCallKind> = calls.iter().map(|c| c.kind).collect();
    assert_eq!(kinds, [FakeCallKind::Structured, FakeCallKind::Turn]);
    assert_eq!(calls[0].request_json["model"], "claude-haiku-5-5");
    assert_eq!(calls[0].request_json["schema"]["id"], "any");
    assert_eq!(calls[1].request_json["conversation_id"], CONV);
    assert_eq!(calls[1].request_json["tools"][0]["name"], "get_eval");
}

#[tokio::test]
async fn propagates_thrown_errors() {
    let ai = FakeAiClient::new(structured_script(|_, _| {
        Err(AiError::Unavailable {
            reason: UnavailableReason::Server,
            retry_after: None,
        })
    }));
    let e = ai.structured(&sreq(&ANY)).await.unwrap_err();
    assert_eq!(
        e,
        AiError::Unavailable {
            reason: UnavailableReason::Server,
            retry_after: None
        }
    );
    // A missing script fails loudly instead of returning something plausible.
    let empty = FakeAiClient::new(FakeScript::default());
    let e = empty.structured(&sreq(&ANY)).await.unwrap_err();
    assert!(matches!(e, AiError::Response(m) if m.starts_with("fake:")));
    let ev = collect_turn(&empty, &treq(), None).await;
    assert!(matches!(&ev[..], [Err(AiError::Response(m))] if m.starts_with("fake:")));
}

#[tokio::test]
async fn serves_scripted_batches() {
    let mut batches = HashMap::new();
    batches.insert(
        "b1".to_owned(),
        FakeBatch {
            status: BatchStatus::Ended,
            results: vec![FakeBatchItem {
                custom_id: "x".into(),
                outcome: BatchOutcome::Expired,
                ..FakeBatchItem::default()
            }],
        },
    );
    batches.insert(
        "fakebatch_2".to_owned(),
        FakeBatch {
            status: BatchStatus::InProgress,
            results: vec![],
        },
    );
    let ai = FakeAiClient::new(FakeScript {
        batches,
        ..FakeScript::default()
    });
    assert!(ai.batches_enabled());
    assert_eq!(ai.batch_status("b1").await.unwrap(), BatchStatus::Ended);
    let out = results(&ai, "b1").await;
    assert_eq!(out.len(), 1);
    let item = out[0].as_ref().unwrap();
    assert_eq!(item.outcome, BatchOutcome::Expired);
    assert_eq!(item.result, None);
    assert_eq!(item.error_kind, None);

    // Submitted ids are `fakebatch_<n>`, and the scripted batch of that id is served.
    let items = [BatchItem {
        custom_id: "x__v1__a".into(),
        req: sreq(&PROBE),
    }];
    assert_eq!(ai.batch_submit(&items).await.unwrap(), "fakebatch_1");
    assert_eq!(ai.batch_submit(&items).await.unwrap(), "fakebatch_2");
    assert_eq!(
        ai.batch_status("fakebatch_2").await.unwrap(),
        BatchStatus::InProgress
    );
    // An unknown id is the API's 404.
    let e = ai.batch_status("nope").await.unwrap_err();
    assert!(matches!(e, AiError::Request { status: 404, .. }));
    let r = results(&ai, "nope").await;
    assert!(matches!(
        &r[..],
        [Err(AiError::Request { status: 404, .. })]
    ));

    let kinds: Vec<FakeCallKind> = ai.calls().iter().map(|c| c.kind).collect();
    assert_eq!(
        kinds,
        [
            FakeCallKind::BatchStatus,
            FakeCallKind::BatchResults,
            FakeCallKind::BatchSubmit,
            FakeCallKind::BatchSubmit,
            FakeCallKind::BatchStatus,
            FakeCallKind::BatchStatus,
            FakeCallKind::BatchResults,
        ]
    );
    assert_eq!(ai.calls()[2].request_json[0]["custom_id"], "x__v1__a");
}

#[tokio::test]
async fn scripts_schema_mismatch_invalid_json_refusal_and_no_schema() {
    let outs = [
        FakeStructured {
            parsed: Some(json!({"section": 1})),
            ..FakeStructured::default()
        },
        FakeStructured {
            text: Some("zzz".into()),
            ..FakeStructured::default()
        },
        FakeStructured {
            text: Some("x".into()),
            stop_reason: Some("refusal".into()),
            ..FakeStructured::default()
        },
        FakeStructured {
            parsed: Some(json!({"section": "s", "n": 1})),
            ..FakeStructured::default()
        },
        FakeStructured {
            text: Some("{\"section\":".into()),
            stop_reason: Some("max_tokens".into()),
            ..FakeStructured::default()
        },
        // The SDK gave up after its structured-output retries.
        FakeStructured {
            structured_output_failed: true,
            ..FakeStructured::default()
        },
        // No text at all.
        FakeStructured::default(),
    ];
    let n_outs = outs.len();
    let ai = FakeAiClient::new(structured_script(move |_, n| Ok(outs[n].clone())));
    let mut got = Vec::new();
    for _ in 0..n_outs {
        got.push(ai.structured(&sreq(&PROBE)).await.unwrap());
    }
    let errors: Vec<Option<ParseError>> = got.iter().map(|r| r.parse_error).collect();
    assert_eq!(
        errors,
        [
            Some(ParseError::SchemaMismatch),
            Some(ParseError::InvalidJson),
            Some(ParseError::Refusal),
            None,
            Some(ParseError::Truncated),
            Some(ParseError::SchemaMismatch),
            Some(ParseError::NoText),
        ]
    );
    assert_eq!(got[0].parse_issues.as_ref().unwrap()[0].path, "section");
    assert_eq!(got[3].parsed, Some(json!({"section": "s", "n": 1})));
    assert_eq!(got[4].stop_reason.as_deref(), Some("max_tokens"));
    assert_eq!(got[5].parse_issues, None);
    assert!(
        got.iter()
            .all(|r| r.parsed.is_none() == r.parse_error.is_some())
    );

    let ok = r#"{"section":"s","n":1}"#;
    let mut batches = HashMap::new();
    batches.insert(
        "b1".to_owned(),
        FakeBatch {
            status: BatchStatus::Ended,
            results: vec![
                FakeBatchItem {
                    custom_id: "x__v1__a".into(),
                    outcome: BatchOutcome::Succeeded,
                    text: Some(ok.into()),
                    ..FakeBatchItem::default()
                },
                FakeBatchItem {
                    custom_id: "y__v1__a".into(),
                    outcome: BatchOutcome::Succeeded,
                    text: Some(ok.into()),
                    ..FakeBatchItem::default()
                },
                FakeBatchItem {
                    custom_id: "z".into(),
                    outcome: BatchOutcome::Errored,
                    error_type: Some("overloaded_error".into()),
                    ..FakeBatchItem::default()
                },
            ],
        },
    );
    let b = FakeAiClient::new(FakeScript {
        batches,
        ..FakeScript::default()
    });
    let out: Vec<BatchResultItem> = results(&b, "b1")
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect();
    let errors: Vec<Option<ParseError>> = out.iter().map(|o| o.parse_error).collect();
    assert_eq!(errors, [None, Some(ParseError::NoSchema), None]);
    assert_eq!(
        out[0].result.as_ref().unwrap().parsed,
        Some(json!({"section": "s", "n": 1}))
    );
    assert_eq!(
        out[1].result.as_ref().unwrap().parse_error,
        Some(ParseError::NoSchema)
    );
    assert_eq!(out[2].error_kind, Some(AiErrorKind::Retryable));
    assert_eq!(out[2].error_type.as_deref(), Some("overloaded_error"));
    assert_eq!(out[2].result, None);
}

#[tokio::test]
async fn scripts_429_and_overloaded_by_throwing() {
    let ai = FakeAiClient::new(structured_script(|_, n| {
        Err(AiError::Unavailable {
            reason: if n == 0 {
                UnavailableReason::RateLimited
            } else {
                UnavailableReason::Overloaded
            },
            retry_after: (n == 0).then(|| Duration::from_millis(1500)),
        })
    }));
    let e = ai.structured(&sreq(&ANY)).await.unwrap_err();
    assert_eq!(e.retry_after(), Some(Duration::from_millis(1500)));
    let e = ai.structured(&sreq(&ANY)).await.unwrap_err();
    assert!(matches!(
        e,
        AiError::Unavailable {
            reason: UnavailableReason::Overloaded,
            ..
        }
    ));
}

#[tokio::test]
async fn fake_turn_calls_the_tool_handler_before_deltas() {
    let ai = FakeAiClient::new(FakeScript {
        turns: vec![Box::new(|_, _| {
            Ok(FakeTurn {
                tool_calls: vec![
                    ("get_position".into(), json!({"ply": 3})),
                    ("get_eval".into(), json!({"ply": 4})),
                ],
                deltas: vec!["one".into(), "two".into()],
                ..FakeTurn::default()
            })
        })],
        ..FakeScript::default()
    });
    let log = Arc::new(Mutex::new(Vec::new()));
    let req = treq();
    let mut s = ai.stream_turn(&req, handler(&log));
    while let Some(ev) = s.next().await {
        if let Ok(TurnEvent::TextDelta(t)) = ev {
            log.lock().unwrap().push(format!("delta:{t}"));
        }
    }
    drop(s);
    assert_eq!(
        *log.lock().unwrap(),
        [
            r#"tool:get_position:{"ply":3}"#,
            r#"tool:get_eval:{"ply":4}"#,
            "delta:one",
            "delta:two",
        ]
    );
    let calls = ai.calls();
    assert_eq!(
        calls[0].tool_replies,
        [
            (
                "get_position".to_owned(),
                json!({"ply": 3}),
                ToolReply {
                    content: r#"{"ok":"get_position"}"#.into(),
                    is_error: false
                }
            ),
            (
                "get_eval".to_owned(),
                json!({"ply": 4}),
                ToolReply {
                    content: r#"{"ok":"get_eval"}"#.into(),
                    is_error: false
                }
            ),
        ]
    );
}

#[tokio::test]
async fn fake_turn_with_tool_calls_but_no_handler_errors() {
    let script = || FakeScript {
        turns: vec![Box::new(|_, _| {
            Ok(FakeTurn {
                tool_calls: vec![("get_eval".into(), json!({}))],
                deltas: vec!["x".into()],
                ..FakeTurn::default()
            })
        })],
        ..FakeScript::default()
    };
    let loud = |ev: &[Result<TurnEvent, AiError>]| matches!(ev, [Err(AiError::Response(m))] if m.starts_with("fake:"));
    // No handler.
    let ai = FakeAiClient::new(script());
    assert!(loud(&collect_turn(&ai, &treq(), None).await));
    // `ToolChoice::None` offers no tool, so the model cannot call one.
    let ai = FakeAiClient::new(script());
    let log = Arc::new(Mutex::new(Vec::new()));
    let req = ChatTurnRequest {
        tool_choice: ToolChoice::None,
        ..treq()
    };
    assert!(loud(&collect_turn(&ai, &req, handler(&log)).await));
    // A tool the request did not offer.
    let ai = FakeAiClient::new(script());
    let req = ChatTurnRequest {
        tools: vec![tool("get_position")],
        ..treq()
    };
    assert!(loud(&collect_turn(&ai, &req, handler(&log)).await));
    assert!(log.lock().unwrap().is_empty());
}

#[tokio::test]
async fn fake_model_usage_defaults_to_requested_model() {
    let u = usage(10, 5);
    let ai = FakeAiClient::new(FakeScript {
        structured: Some(Box::new(move |_, n| {
            Ok(FakeStructured {
                parsed: Some(json!({})),
                usage: u,
                model_usage: (n == 1).then(|| {
                    vec![
                        mu("claude-haiku-5-5", usage(3, 1)),
                        mu("claude-sonnet-5-5", usage(4, 2)),
                    ]
                }),
                ..FakeStructured::default()
            })
        })),
        turns: vec![Box::new(move |_, _| {
            Ok(FakeTurn {
                usage: u,
                ..FakeTurn::default()
            })
        })],
        ..FakeScript::default()
    });
    let r = ai.structured(&sreq(&ANY)).await.unwrap();
    assert_eq!(r.model, "claude-haiku-5-5");
    assert_eq!(r.model_usage, [mu("claude-haiku-5-5", u)]);
    assert_eq!(r.usage, u);
    // Scripted per-model usage wins, and `usage` is its sum.
    let r = ai.structured(&sreq(&ANY)).await.unwrap();
    assert_eq!(r.model_usage.len(), 2);
    assert_eq!(r.usage, usage(7, 3));

    let t = done(&collect_turn(&ai, &treq(), None).await);
    assert_eq!(t.model_usage, [mu("claude-sonnet-5-5", u)]);
    assert_eq!(t.usage, u);
    assert_eq!(t.content, Vec::<Value>::new());
}

#[tokio::test]
async fn session_model_usage_is_a_running_total_metered_by_difference() {
    let ai = FakeAiClient::new(FakeScript {
        turns: (0..4)
            .map(|_| {
                Box::new(|_: &ChatTurnRequest, n: usize| {
                    Ok(FakeTurn {
                        deltas: vec![format!("t{n}")],
                        usage: usage(100 * (n as u64 + 1), 10),
                        ..FakeTurn::default()
                    })
                }) as _
            })
            .collect(),
        ..FakeScript::default()
    });
    let first = done(&collect_turn(&ai, &treq(), None).await);
    assert_eq!(first.session, Some(SessionPath::New));
    assert_eq!(
        first.session_model_usage,
        [mu("claude-sonnet-5-5", usage(100, 10))]
    );

    let second = done(&collect_turn(&ai, &resumed(CONV), None).await);
    assert_eq!(second.session, Some(SessionPath::Resumed));
    // The SDK's running total, never this turn's usage.
    assert_eq!(
        second.session_model_usage,
        [mu("claude-sonnet-5-5", usage(300, 20))]
    );
    assert_eq!(
        second.model_usage,
        [mu("claude-sonnet-5-5", usage(200, 10))]
    );
    assert_eq!(
        metered_turn_usage(&second, Some(&first.session_model_usage)),
        second.model_usage
    );
    // Metering the raw total would bill the first turn twice.
    assert_ne!(second.session_model_usage, second.model_usage);

    // A conversation the sidecar has no session for is rebuilt, and its total starts over.
    let rebuilt = done(&collect_turn(&ai, &resumed(OTHER_CONV), None).await);
    assert_eq!(rebuilt.session, Some(SessionPath::Rebuilt));
    assert_eq!(rebuilt.session_model_usage, rebuilt.model_usage);

    // A resume without rebuild context is the client's local 400, before any script runs.
    let req = ChatTurnRequest {
        rebuild_context: None,
        ..resumed(CONV)
    };
    let ev = collect_turn(&ai, &req, None).await;
    assert!(matches!(
        &ev[..],
        [Err(AiError::Request { status: 400, .. })]
    ));
    let fourth = done(&collect_turn(&ai, &resumed(CONV), None).await);
    assert_eq!(fourth.content, text("t3"));
}

#[tokio::test]
async fn a_second_concurrent_turn_of_one_conversation_is_turn_in_progress() {
    let ai = FakeAiClient::new(FakeScript {
        turns: (0..3)
            .map(|_| {
                Box::new(|_: &ChatTurnRequest, n: usize| {
                    Ok(FakeTurn {
                        deltas: vec![format!("t{n}")],
                        ..FakeTurn::default()
                    })
                }) as _
            })
            .collect(),
        ..FakeScript::default()
    });
    let req = treq();
    let mut a = ai.stream_turn(&req, None);
    assert!(matches!(a.next().await, Some(Ok(TurnEvent::Message(_)))));

    let busy = collect_turn(&ai, &resumed(CONV), None).await;
    assert!(matches!(
        &busy[..],
        [Err(AiError::Request { status: 409, api_error_type: Some(t) })] if t == "turn_in_progress"
    ));
    // Another conversation is not blocked, and the 409 consumed no script entry.
    let other = ChatTurnRequest {
        conversation_id: uuid::Uuid::parse_str(OTHER_CONV).unwrap(),
        ..treq()
    };
    assert_eq!(
        done(&collect_turn(&ai, &other, None).await).content,
        text("t1")
    );

    let rest: Vec<_> = a.collect().await;
    assert_eq!(done(&rest).content, text("t0"));
    // The lock is released with the terminal event.
    let again = done(&collect_turn(&ai, &resumed(CONV), None).await);
    assert_eq!(again.content, text("t2"));

    // A dropped stream releases the lock too.
    let ai = FakeAiClient::new(FakeScript {
        turns: (0..2)
            .map(|_| {
                Box::new(|_: &ChatTurnRequest, _| {
                    Ok(FakeTurn {
                        deltas: vec!["x".into()],
                        ..FakeTurn::default()
                    })
                }) as _
            })
            .collect(),
        ..FakeScript::default()
    });
    let mut a = ai.stream_turn(&req, None);
    a.next().await;
    drop(a);
    assert!(done(&collect_turn(&ai, &req, None).await).content == text("x"));
}

#[tokio::test]
async fn retracted_text_and_refusals_emit_retract_events() {
    let ai = FakeAiClient::new(FakeScript {
        turns: vec![
            Box::new(|_, _| {
                Ok(FakeTurn {
                    retracted: Some(FakeRetracted {
                        deltas: vec!["draft".into()],
                        reason: "superseded".into(),
                    }),
                    deltas: vec!["final".into()],
                    ..FakeTurn::default()
                })
            }),
            Box::new(|_, _| {
                Ok(FakeTurn {
                    deltas: vec!["I can't".into()],
                    stop_reason: Some("refusal".into()),
                    ..FakeTurn::default()
                })
            }),
        ],
        ..FakeScript::default()
    });
    let ev = collect_turn(&ai, &treq(), None).await;
    let Ok(TurnEvent::Message(m0)) = &ev[0] else {
        panic!("{ev:?}")
    };
    assert_eq!(ev[1], Ok(TurnEvent::TextDelta("draft".into())));
    assert_eq!(
        ev[2],
        Ok(TurnEvent::Retract {
            message_ids: vec![m0.clone()],
            reason: "superseded".into()
        })
    );
    let Ok(TurnEvent::Message(m1)) = &ev[3] else {
        panic!("{ev:?}")
    };
    assert_ne!(m0, m1);
    assert_eq!(ev[4], Ok(TurnEvent::TextDelta("final".into())));
    assert_eq!(done(&ev).content, text("final"));
    assert_eq!(ev.len(), 6);

    let ev = collect_turn(&ai, &resumed(CONV), None).await;
    let Ok(TurnEvent::Message(m)) = &ev[0] else {
        panic!("{ev:?}")
    };
    assert_eq!(
        ev[2],
        Ok(TurnEvent::Retract {
            message_ids: vec![m.clone()],
            reason: "refusal".into()
        })
    );
    let t = done(&ev);
    assert_eq!(t.stop_reason.as_deref(), Some("refusal"));
    assert_eq!(t.content, Vec::<Value>::new());
}

#[tokio::test]
async fn a_scripted_stream_error_follows_the_deltas() {
    let ai = FakeAiClient::new(FakeScript {
        turns: vec![Box::new(|_, _| {
            Ok(FakeTurn {
                deltas: vec!["partial".into()],
                error: Some(AiError::Unavailable {
                    reason: UnavailableReason::Server,
                    retry_after: None,
                }),
                ..FakeTurn::default()
            })
        })],
        ..FakeScript::default()
    });
    let ev = collect_turn(&ai, &treq(), None).await;
    assert_eq!(ev.len(), 3);
    assert_eq!(ev[1], Ok(TurnEvent::TextDelta("partial".into())));
    assert!(matches!(ev[2], Err(AiError::Unavailable { .. })));
}

#[tokio::test(start_paused = true)]
async fn max_in_flight_counts_concurrent_calls() {
    let ai = FakeAiClient::new(FakeScript {
        delay: Some(Duration::from_secs(1)),
        ..structured_script(|_, _| {
            Ok(FakeStructured {
                parsed: Some(json!({})),
                ..FakeStructured::default()
            })
        })
    });
    let req = sreq(&ANY);
    ai.structured(&req).await.unwrap();
    assert_eq!(ai.max_in_flight(), 1);
    let (a, b, c) = tokio::join!(
        ai.structured(&req),
        ai.structured(&req),
        ai.structured(&req)
    );
    assert!(a.is_ok() && b.is_ok() && c.is_ok());
    assert_eq!(ai.max_in_flight(), 3);
}

#[tokio::test]
async fn disabled_batches_give_no_api_key() {
    let ai = FakeAiClient::new(FakeScript {
        batches_enabled: false,
        ..FakeScript::default()
    });
    assert!(!ai.batches_enabled());
    let no_key = |e: &AiError| {
        matches!(
            e,
            AiError::Unavailable {
                reason: UnavailableReason::NoApiKey,
                ..
            }
        )
    };
    let items = [BatchItem {
        custom_id: "x".into(),
        req: sreq(&ANY),
    }];
    assert!(no_key(&ai.batch_submit(&items).await.unwrap_err()));
    assert!(no_key(&ai.batch_status("fakebatch_1").await.unwrap_err()));
    let r = results(&ai, "fakebatch_1").await;
    assert!(matches!(&r[..], [Err(e)] if no_key(e)));
}

#[tokio::test]
async fn batch_results_carry_the_submitted_model_and_usage() {
    let mut batches = HashMap::new();
    batches.insert(
        "fakebatch_1".to_owned(),
        FakeBatch {
            status: BatchStatus::Ended,
            results: vec![
                FakeBatchItem {
                    custom_id: "x__v1__a".into(),
                    outcome: BatchOutcome::Succeeded,
                    text: Some(r#"{"section":"s","n":1}"#.into()),
                    usage: usage(7, 3),
                    ..FakeBatchItem::default()
                },
                FakeBatchItem {
                    custom_id: "x__v1__b".into(),
                    outcome: BatchOutcome::Succeeded,
                    text: Some(r#"{"section":"s","n":1}"#.into()),
                    model: Some("claude-sonnet-5-5".into()),
                    ..FakeBatchItem::default()
                },
            ],
        },
    );
    let ai = FakeAiClient::new(FakeScript {
        batches,
        ..FakeScript::default()
    });
    let items = [BatchItem {
        custom_id: "x__v1__a".into(),
        req: sreq(&PROBE),
    }];
    ai.batch_submit(&items).await.unwrap();
    let out: Vec<BatchResultItem> = results(&ai, "fakebatch_1")
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect();
    let a = out[0].result.as_ref().unwrap();
    assert_eq!(a.model, "claude-haiku-5-5");
    assert_eq!(a.model_usage, [mu("claude-haiku-5-5", usage(7, 3))]);
    assert_eq!(a.usage, usage(7, 3));
    assert_eq!(a.sdk_cost_usd, None);
    assert_eq!(out[1].result.as_ref().unwrap().model, "claude-sonnet-5-5");
}
