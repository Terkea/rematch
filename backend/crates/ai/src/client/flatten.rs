//! Flattens a Messages-style conversation for the sidecar. The SDK's `query()` takes one user
//! message, so Rust, which owns the prompts, renders the history into tagged text blocks:
//!
//! - system blocks are joined with a blank line (`cache_control` is dropped: caching on the
//!   sidecar path is automatic);
//! - each message before the last user message becomes one text block:
//!   `<previous_user_message>`, `<previous_assistant_output>` (assistant `text` blocks only;
//!   `thinking`, `redacted_thinking` and `tool_use` are dropped) or `<instruction>` (a
//!   mid-conversation system message);
//! - the last user message's text blocks follow as they are, without `cache_control`.

use serde_json::{Value, json};

use super::port::{AiError, Message, Role, TextBlock};

fn invalid() -> AiError {
    AiError::Request {
        status: 400,
        api_error_type: Some("invalid_request_error".to_owned()),
    }
}

/// The `text` of every `text` block, in order.
fn texts(m: &Message) -> Vec<&str> {
    m.content
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|b| b.get("text").and_then(Value::as_str))
        .collect()
}

fn text_block(text: &str) -> Value {
    json!({ "type": "text", "text": text })
}

/// Returns `(systemPrompt, prompt blocks)`. The last message must be from the user.
pub fn flatten_for_sidecar(
    system: &[TextBlock],
    messages: &[Message],
) -> Result<(String, Vec<Value>), AiError> {
    let (last, history) = messages.split_last().ok_or_else(invalid)?;
    if last.role != Role::User {
        return Err(invalid());
    }
    let system_prompt = system
        .iter()
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut prompt = Vec::new();
    for m in history {
        let body = texts(m);
        if body.is_empty() {
            continue;
        }
        let tag = match m.role {
            Role::User => "previous_user_message",
            Role::Assistant => "previous_assistant_output",
            Role::System => "instruction",
        };
        prompt.push(text_block(&format!(
            "<{tag}>\n{}\n</{tag}>",
            body.join("\n\n")
        )));
    }
    let tail = texts(last);
    if tail.is_empty() {
        return Err(invalid());
    }
    prompt.extend(tail.into_iter().map(text_block));
    Ok((system_prompt, prompt))
}
