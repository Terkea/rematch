//! Model output parsing, shared by the HTTP adapter, batch results and the fake client (port of
//! `parse.ts`). A `None` value always carries a reason; issues hold paths and codes only.

use serde_json::Value;

use super::port::{OutputSchema, ParseError, ParseIssue};

/// The text of the first `text` block (thinking and other blocks are skipped).
fn first_text(content: &[Value]) -> Option<&str> {
    content
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
        .find_map(|b| b.get("text").and_then(Value::as_str))
}

/// Order: refusal, `max_tokens` (truncated), missing schema, no text, invalid JSON, schema check.
pub fn parse_model_output(
    content: &[Value],
    stop_reason: Option<&str>,
    schema: Option<&OutputSchema>,
) -> (Option<Value>, Option<ParseError>, Option<Vec<ParseIssue>>) {
    let fail = |e| (None, Some(e), None);
    match stop_reason {
        Some("refusal") => return fail(ParseError::Refusal),
        Some("max_tokens") => return fail(ParseError::Truncated),
        _ => {}
    }
    let Some(schema) = schema else {
        return fail(ParseError::NoSchema);
    };
    let Some(text) = first_text(content) else {
        return fail(ParseError::NoText);
    };
    let Ok(json) = serde_json::from_str::<Value>(text) else {
        return fail(ParseError::InvalidJson);
    };
    match (schema.check)(&json) {
        Ok(()) => (Some(json), None, None),
        Err(issues) => (None, Some(ParseError::SchemaMismatch), Some(issues)),
    }
}
