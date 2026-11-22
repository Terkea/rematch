//! `flatten_for_sidecar` (Task 11): the SDK takes user messages only, so Rust flattens the
//! conversation into one system prompt plus the prompt's text blocks. No TS test exists (the TS
//! client sent Messages API bodies); these follow the brief.

use ai::client::flatten::flatten_for_sidecar;
use ai::client::port::{AiError, Message, Role, TextBlock};
use serde_json::{Value, json};

fn text(t: &str) -> Value {
    json!({"type": "text", "text": t})
}

fn msg(role: Role, content: Vec<Value>) -> Message {
    Message { role, content }
}

fn has_cache_control(v: &Value) -> bool {
    match v {
        Value::Object(m) => m.contains_key("cache_control") || m.values().any(has_cache_control),
        Value::Array(a) => a.iter().any(has_cache_control),
        _ => false,
    }
}

#[test]
fn single_user_message_passes_blocks_without_cache_control() {
    let messages = vec![msg(
        Role::User,
        vec![
            TextBlock::cached("<coaching_input>{}</coaching_input>").to_json(),
            TextBlock::new("<section task>").to_json(),
        ],
    )];
    let (_, prompt) = flatten_for_sidecar(&[], &messages).unwrap();
    assert_eq!(
        prompt,
        vec![
            text("<coaching_input>{}</coaching_input>"),
            text("<section task>")
        ]
    );
    assert!(!prompt.iter().any(has_cache_control));
}

#[test]
fn system_blocks_join_with_blank_line() {
    let system = [TextBlock::cached("<core>"), TextBlock::new("<report+tone>")];
    let messages = vec![msg(Role::User, vec![text("hi")])];
    let (sys, _) = flatten_for_sidecar(&system, &messages).unwrap();
    assert_eq!(sys, "<core>\n\n<report+tone>");
    let (empty, _) = flatten_for_sidecar(&[], &messages).unwrap();
    assert_eq!(empty, "");
}

#[test]
fn history_renders_tagged_transcript_and_drops_thinking() {
    // The shape `build_repair_messages` produces: the first user turn, the model's answer (with
    // a thinking block), then the repair instruction as a user turn.
    let messages = vec![
        msg(
            Role::User,
            vec![TextBlock::cached("<input>").to_json(), text("<task>")],
        ),
        msg(
            Role::Assistant,
            vec![
                json!({"type": "thinking", "thinking": "SECRET THOUGHT", "signature": "sig"}),
                json!({"type": "redacted_thinking", "data": "opaque"}),
                text("{\"section\":7}"),
                json!({"type": "tool_use", "id": "toolu_1", "name": "get_position", "input": {}}),
                text("second part"),
            ],
        ),
        msg(Role::User, vec![text("Fix the JSON.")]),
    ];
    let (_, prompt) = flatten_for_sidecar(&[], &messages).unwrap();
    assert_eq!(
        prompt,
        vec![
            text("<previous_user_message>\n<input>\n\n<task>\n</previous_user_message>"),
            text(
                "<previous_assistant_output>\n{\"section\":7}\n\nsecond part\n</previous_assistant_output>"
            ),
            text("Fix the JSON."),
        ]
    );
    let all = serde_json::to_string(&prompt).unwrap();
    assert!(!all.contains("SECRET THOUGHT"));
    assert!(!all.contains("opaque"));
    assert!(!all.contains("toolu_1"));
    assert!(!prompt.iter().any(has_cache_control));
}

#[test]
fn mid_conversation_system_becomes_instruction() {
    let messages = vec![
        msg(Role::User, vec![text("q1")]),
        msg(Role::Assistant, vec![text("a1")]),
        msg(Role::System, vec![text("Answer now with what you have.")]),
        msg(Role::User, vec![text("q2")]),
    ];
    let (_, prompt) = flatten_for_sidecar(&[], &messages).unwrap();
    assert_eq!(
        prompt,
        vec![
            text("<previous_user_message>\nq1\n</previous_user_message>"),
            text("<previous_assistant_output>\na1\n</previous_assistant_output>"),
            text("<instruction>\nAnswer now with what you have.\n</instruction>"),
            text("q2"),
        ]
    );
}

#[test]
fn last_message_must_be_user() {
    let want = AiError::Request {
        status: 400,
        api_error_type: Some("invalid_request_error".into()),
    };
    for last in [Role::Assistant, Role::System] {
        let messages = vec![msg(Role::User, vec![text("q")]), msg(last, vec![text("x")])];
        assert_eq!(flatten_for_sidecar(&[], &messages), Err(want.clone()));
    }
    assert_eq!(flatten_for_sidecar(&[], &[]), Err(want));
}
