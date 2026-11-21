//! The SSE decoder and the stream accumulator (Task 10). The TS reference used the SDK's stream
//! helper, so there is no TS test to port: these follow the brief. Every input is a hand-written
//! fixture in `backend/fixtures/ai/api/` or an inline string; nothing touches the network.

use ai::client::port::{AiError, AiUsage, ToolUse, UnavailableReason};
use ai::client::sse::{SseDecoder, SseError, SseEvent, StreamAccumulator};
use ai::testing::fixtures_dir;
use serde_json::{Value, json};

fn fixture(name: &str) -> Vec<u8> {
    let path = fixtures_dir().join("api").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn decode_all(bytes: &[u8]) -> Vec<SseEvent> {
    let mut d = SseDecoder::default();
    let events = d.push(bytes);
    d.finish().expect("no trailing partial event");
    events
}

fn ev(event: Option<&str>, data: &str) -> SseEvent {
    SseEvent {
        event: event.map(str::to_owned),
        data: data.to_owned(),
    }
}

/// Feeds every event to a fresh accumulator; returns the text deltas and the first error.
fn accumulate(events: &[SseEvent]) -> (StreamAccumulator, Vec<String>, Option<AiError>) {
    let mut acc = StreamAccumulator::default();
    let mut deltas = Vec::new();
    for e in events {
        match acc.apply(e) {
            Ok(Some(t)) => deltas.push(t),
            Ok(None) => {}
            Err(err) => return (acc, deltas, Some(err)),
        }
    }
    (acc, deltas, None)
}

fn data_event(data: Value) -> SseEvent {
    let ty = data["type"].as_str().map(str::to_owned);
    SseEvent {
        event: ty,
        data: data.to_string(),
    }
}

fn message_start() -> SseEvent {
    data_event(json!({"type":"message_start","message":{"id":"m","type":"message","role":"assistant",
        "content":[],"model":"claude-sonnet-5-5","stop_reason":null,"usage":{"input_tokens":5,"output_tokens":1}}}))
}

// ---- decoder ----

#[test]
fn decodes_events_across_arbitrary_chunk_boundaries() {
    let bytes = fixture("sse-tool-use.sse");
    let one_shot = decode_all(&bytes);
    assert_eq!(one_shot.len(), 13, "every event of the fixture, the comment skipped");
    for size in 1..=64 {
        let mut d = SseDecoder::default();
        let mut events = Vec::new();
        for chunk in bytes.chunks(size) {
            events.extend(d.push(chunk));
        }
        d.finish().expect("clean end");
        assert_eq!(events, one_shot, "split size {size}");
    }
}

#[test]
fn crlf_lines_decode_like_lf() {
    let lf = fixture("sse-tool-use.sse");
    let crlf = String::from_utf8(lf.clone()).unwrap().replace('\n', "\r\n");
    let want = decode_all(&lf);
    for size in [1, 2, 3, 7, 64] {
        let mut d = SseDecoder::default();
        let mut events = Vec::new();
        for chunk in crlf.as_bytes().chunks(size) {
            events.extend(d.push(chunk));
        }
        d.finish().expect("clean end");
        assert_eq!(events, want, "split size {size}");
    }
}

#[test]
fn split_utf8_is_reassembled() {
    let bytes = fixture("sse-split-utf8.bin");
    let at = bytes.iter().position(|&b| b == 0xC3).expect("an é in the fixture") + 1;
    let mut d = SseDecoder::default();
    assert!(d.push(&bytes[..at]).is_empty());
    let events = d.push(&bytes[at..]);
    d.finish().expect("clean end");
    assert_eq!(events.len(), 1);
    let data: Value = serde_json::from_str(&events[0].data).unwrap();
    assert_eq!(data["delta"]["text"], "café");
    assert_eq!(events[0].event.as_deref(), Some("content_block_delta"));
}

#[test]
fn multi_line_data_comments_and_valueless_fields() {
    let input = ": hello\nevent: x\ndata: a\ndata:b\ndata\nid: 7\nretry: 10\n\ndata: only\n\n";
    assert_eq!(
        decode_all(input.as_bytes()),
        vec![ev(Some("x"), "a\nb\n"), ev(None, "only")]
    );
}

#[test]
fn a_blank_line_without_data_dispatches_nothing() {
    assert!(decode_all(b"\n\nevent: lonely\n\n: c\n\n").is_empty());
    // An empty `data:` is still an event, with empty data.
    assert_eq!(decode_all(b"data:\n\n"), vec![ev(None, "")]);
}

#[test]
fn trailing_partial_event_is_an_error() {
    let mut d = SseDecoder::default();
    assert!(d.push(b"data: {\"type\":\"ping\"}\n").is_empty());
    assert_eq!(d.finish(), Err(SseError::TrailingPartialEvent));

    let mut d = SseDecoder::default();
    assert!(d.push(b"data: {\"type\":\"pi").is_empty());
    assert_eq!(d.finish(), Err(SseError::TrailingPartialEvent));
}

// ---- accumulator ----

#[test]
fn accumulates_text_and_tool_use() {
    let (acc, deltas, err) = accumulate(&decode_all(&fixture("sse-tool-use.sse")));
    assert_eq!(err, None);
    assert_eq!(deltas, vec!["Let me ", "check."]);
    let turn = acc.finish().expect("complete turn");
    assert_eq!(turn.model, "claude-sonnet-5-5");
    assert_eq!(turn.stop_reason.as_deref(), Some("tool_use"));
    assert_eq!(
        turn.usage,
        AiUsage {
            input_tokens: 2048,
            output_tokens: 57,
            cache_creation_tokens: 1500,
            cache_read_tokens: 0,
        }
    );
    assert_eq!(
        turn.tool_uses,
        vec![ToolUse {
            id: "toolu_01".into(),
            name: "get_position".into(),
            input: Some(json!({"gameId":"g1","ply":24})),
        }]
    );
    assert_eq!(
        turn.content,
        vec![
            json!({"type":"text","text":"Let me check."}),
            json!({"type":"tool_use","id":"toolu_01","name":"get_position","input":{"gameId":"g1","ply":24}}),
        ]
    );
}

#[test]
fn accumulates_plain_text() {
    let (acc, deltas, err) = accumulate(&decode_all(&fixture("sse-text.sse")));
    assert_eq!(err, None);
    assert_eq!(deltas, vec!["Your knight ", "was loose."]);
    let turn = acc.finish().unwrap();
    assert_eq!(turn.stop_reason.as_deref(), Some("end_turn"));
    assert_eq!(turn.tool_uses, vec![]);
    assert_eq!(turn.content, vec![json!({"type":"text","text":"Your knight was loose."})]);
    assert_eq!(turn.usage.output_tokens, 9);
    assert_eq!(turn.usage.cache_read_tokens, 900);
}

#[test]
fn thinking_block_is_kept_raw_for_replay() {
    let (acc, deltas, err) = accumulate(&decode_all(&fixture("sse-thinking-text.sse")));
    assert_eq!(err, None);
    assert_eq!(deltas, vec!["Good question."], "thinking is never a text delta");
    let turn = acc.finish().unwrap();
    let expected: Value =
        serde_json::from_str(r#"{"type":"thinking","thinking":"","signature":"sig"}"#).unwrap();
    assert_eq!(
        serde_json::to_string(&turn.content[0]).unwrap(),
        serde_json::to_string(&expected).unwrap()
    );
    assert_eq!(turn.content[1], json!({"type":"text","text":"Good question."}));
    // `message_delta` cache usage overrides the start value when present.
    assert_eq!(turn.usage.cache_read_tokens, 640);
    assert_eq!(turn.usage.output_tokens, 31);
}

#[test]
fn unknown_blocks_are_kept_raw_and_unknown_deltas_ignored() {
    let events = vec![
        message_start(),
        data_event(json!({"type":"content_block_start","index":0,
            "content_block":{"type":"redacted_thinking","data":"opaque"}})),
        data_event(json!({"type":"content_block_delta","index":0,"delta":{"type":"future_delta","x":1}})),
        data_event(json!({"type":"content_block_stop","index":0})),
        data_event(json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":3}})),
        data_event(json!({"type":"message_stop"})),
    ];
    let (acc, deltas, err) = accumulate(&events);
    assert_eq!(err, None);
    assert!(deltas.is_empty());
    let turn = acc.finish().unwrap();
    assert_eq!(turn.content, vec![json!({"type":"redacted_thinking","data":"opaque"})]);
}

#[test]
fn invalid_partial_json_gives_none_input() {
    for partial in ["{\"gameId\":\"g1\",\"pl", "[1,2]", "{} trailing"] {
        let events = vec![
            message_start(),
            data_event(json!({"type":"content_block_start","index":0,
                "content_block":{"type":"tool_use","id":"toolu_x","name":"get_position","input":{}}})),
            data_event(json!({"type":"content_block_delta","index":0,
                "delta":{"type":"input_json_delta","partial_json":partial}})),
            data_event(json!({"type":"content_block_stop","index":0})),
            data_event(json!({"type":"message_delta","delta":{"stop_reason":"max_tokens"},"usage":{"output_tokens":8}})),
            data_event(json!({"type":"message_stop"})),
        ];
        let (acc, _, err) = accumulate(&events);
        assert_eq!(err, None);
        let turn = acc.finish().unwrap();
        assert_eq!(turn.tool_uses[0].input, None, "{partial}");
        assert_eq!(turn.content[0]["input"], json!({}), "{partial}");
    }
}

#[test]
fn empty_partial_json_is_an_empty_object() {
    let events = vec![
        message_start(),
        data_event(json!({"type":"content_block_start","index":0,
            "content_block":{"type":"tool_use","id":"toolu_y","name":"list_games","input":{}}})),
        data_event(json!({"type":"content_block_stop","index":0})),
        data_event(json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":4}})),
        data_event(json!({"type":"message_stop"})),
    ];
    let (acc, _, err) = accumulate(&events);
    assert_eq!(err, None);
    let turn = acc.finish().unwrap();
    assert_eq!(turn.tool_uses[0].input, Some(json!({})));
}

#[test]
fn error_event_is_overloaded() {
    let (_, _, err) = accumulate(&decode_all(&fixture("sse-error-overloaded.sse")));
    assert_eq!(
        err,
        Some(AiError::Unavailable {
            reason: UnavailableReason::Overloaded,
            retry_after: None,
        })
    );
}

#[test]
fn other_error_events_are_server_errors() {
    for ty in ["api_error", "something_new"] {
        let mut acc = StreamAccumulator::default();
        let err = acc
            .apply(&data_event(json!({"type":"error","error":{"type":ty,"message":"sk-ant-x"}})))
            .unwrap_err();
        assert_eq!(
            err,
            AiError::Unavailable {
                reason: UnavailableReason::Server,
                retry_after: None,
            },
            "{ty}"
        );
    }
}

#[test]
fn missing_message_stop_is_connection_error() {
    let mut events = decode_all(&fixture("sse-text.sse"));
    events.pop();
    let (acc, _, err) = accumulate(&events);
    assert_eq!(err, None);
    assert_eq!(
        acc.finish(),
        Err(AiError::Unavailable {
            reason: UnavailableReason::Connection,
            retry_after: None,
        })
    );
}

#[test]
fn undecodable_event_data_is_a_response_error() {
    let mut acc = StreamAccumulator::default();
    let err = acc.apply(&ev(Some("message_start"), "{not json")).unwrap_err();
    assert!(matches!(err, AiError::Response(_)), "{err:?}");
}
