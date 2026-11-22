//! Port of the `parseError reasons (structured)` block of
//! `packages/ai/test/client/fix-round-1.test.ts`, run directly on `parse_model_output` with the
//! same message bodies (as recorded `MessageResponse` JSON).

use ai::client::parse::parse_model_output;
use ai::client::port::{OutputSchema, ParseError, ParseIssue};
use ai::client::wire::MessageResponse;
use serde::Deserialize;
use serde_json::{Value, json};

/// The TS test schema `z.object({ section: z.string(), n: z.number() })`.
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
            let msg = e.inner().to_string();
            let code = if msg.starts_with("missing field") {
                "missing_field"
            } else {
                "invalid_type"
            };
            vec![ParseIssue {
                path: e.path().to_string(),
                code: code.into(),
            }]
        })
}

static SCHEMA: OutputSchema = OutputSchema {
    id: "probe",
    version: 1,
    json: || json!({"type": "object"}),
    check,
};

/// The TS `msg(text, stop)` helper, as the API body.
fn msg(text: Option<&str>, stop: &str) -> MessageResponse {
    let content = match text {
        None => json!([]),
        Some(t) => json!([{ "type": "text", "text": t }]),
    };
    let body = json!({
        "id": "m", "type": "message", "role": "assistant", "model": "claude-sonnet-5-5",
        "content": content, "stop_reason": stop, "stop_sequence": null,
        "usage": { "input_tokens": 1, "output_tokens": 1 }
    });
    serde_json::from_value(body).unwrap()
}

fn run(m: &MessageResponse) -> (Option<Value>, Option<ParseError>, Option<Vec<ParseIssue>>) {
    parse_model_output(&m.content, m.stop_reason.as_deref(), Some(&SCHEMA))
}

#[test]
fn parse_error_reasons() {
    let cases = [
        ("no_text", msg(None, "end_turn"), ParseError::NoText),
        (
            "invalid_json",
            msg(Some("nope"), "end_turn"),
            ParseError::InvalidJson,
        ),
        (
            "schema_mismatch",
            msg(Some(r#"{"section":1}"#), "end_turn"),
            ParseError::SchemaMismatch,
        ),
        ("refusal", msg(Some("x"), "refusal"), ParseError::Refusal),
        (
            "truncated",
            msg(Some(r#"{"sec"#), "max_tokens"),
            ParseError::Truncated,
        ),
    ];
    for (name, body, expected) in cases {
        let (parsed, err, _) = run(&body);
        assert_eq!(parsed, None, "{name}");
        assert_eq!(err, Some(expected), "{name}");
        assert_eq!(err.unwrap().as_str(), name);
    }
}

#[test]
fn success_has_null_parse_error() {
    let (parsed, err, issues) = run(&msg(Some(r#"{"section":"a","n":1}"#), "end_turn"));
    assert_eq!(parsed, Some(json!({"section": "a", "n": 1})));
    assert_eq!(err, None);
    assert_eq!(issues, None);
}

#[test]
fn schema_mismatch_lists_paths_and_codes_only_no_content() {
    let (parsed, err, issues) = run(&msg(
        Some(r#"{"section":"TOPSECRETVALUE","n":"x"}"#),
        "end_turn",
    ));
    assert_eq!(parsed, None);
    assert_eq!(err, Some(ParseError::SchemaMismatch));
    let issues = issues.unwrap();
    assert_eq!(
        issues,
        vec![ParseIssue {
            path: "n".into(),
            code: "invalid_type".into()
        }]
    );
    let s = serde_json::to_string(&issues).unwrap();
    assert!(!s.contains("TOPSECRETVALUE"));
    assert!(!format!("{issues:?}").contains("TOPSECRETVALUE"));
}

#[test]
fn schema_mismatch_on_the_first_wrong_field() {
    let (_, err, issues) = run(&msg(Some(r#"{"section":1}"#), "end_turn"));
    assert_eq!(err, Some(ParseError::SchemaMismatch));
    assert_eq!(
        issues.unwrap()[0],
        ParseIssue {
            path: "section".into(),
            code: "invalid_type".into()
        }
    );
}

#[test]
fn thinking_only_content_is_no_text() {
    let content = vec![json!({"type": "thinking", "thinking": "", "signature": "sig"})];
    let (parsed, err, issues) = parse_model_output(&content, Some("end_turn"), Some(&SCHEMA));
    assert_eq!(
        (parsed, err, issues),
        (None, Some(ParseError::NoText), None)
    );
}

#[test]
fn thinking_blocks_are_skipped_before_the_text() {
    let content = vec![
        json!({"type": "thinking", "thinking": "", "signature": "sig"}),
        json!({"type": "text", "text": r#"{"section":"overview","n":2}"#}),
    ];
    let (parsed, err, _) = parse_model_output(&content, Some("end_turn"), Some(&SCHEMA));
    assert_eq!(parsed, Some(json!({"section": "overview", "n": 2})));
    assert_eq!(err, None);
}

#[test]
fn only_the_first_text_block_is_parsed() {
    let content = vec![
        json!({"type": "text", "text": "nope"}),
        json!({"type": "text", "text": r#"{"section":"a","n":1}"#}),
    ];
    let (_, err, _) = parse_model_output(&content, Some("end_turn"), Some(&SCHEMA));
    assert_eq!(err, Some(ParseError::InvalidJson));
}

#[test]
fn missing_schema_is_no_schema_after_refusal_and_truncation() {
    let ok = msg(Some(r#"{"section":"a","n":1}"#), "end_turn");
    let (parsed, err, _) = parse_model_output(&ok.content, Some("end_turn"), None);
    assert_eq!((parsed, err), (None, Some(ParseError::NoSchema)));
    // Refusal and truncation win over a missing schema (and over missing text).
    let (_, err, _) = parse_model_output(&[], Some("refusal"), None);
    assert_eq!(err, Some(ParseError::Refusal));
    let (_, err, _) = parse_model_output(&[], Some("max_tokens"), None);
    assert_eq!(err, Some(ParseError::Truncated));
    // A missing schema wins over missing text.
    let (_, err, _) = parse_model_output(&[], Some("end_turn"), None);
    assert_eq!(err, Some(ParseError::NoSchema));
}

#[test]
fn a_missing_stop_reason_still_parses() {
    let ok = msg(Some(r#"{"section":"a","n":1}"#), "end_turn");
    let (parsed, err, _) = parse_model_output(&ok.content, None, Some(&SCHEMA));
    assert!(parsed.is_some());
    assert_eq!(err, None);
}
