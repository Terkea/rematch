//! `parse_coaching_input`: raw JSON to a validated `CoachingInput` (port of `parseCoachingInput`).
//!
//! Deserialising uses `serde_path_to_error`, so a shape error (unknown key, wrong type, bad
//! enum, missing key) names its path in zod's dotted form (`weaknesses.0.rank`). Then
//! `validate` applies the refinements.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;
use serde_path_to_error::Segment;

use super::types::CoachingInput;
pub use super::validate::{CoachingInputIssue, validate};

/// `^W(1[0-2]|[1-9])[a-d]?$`, at most 5 characters.
pub fn is_weakness_id(s: &str) -> bool {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(coaching_input::limits::WEAKNESS_ID_PATTERN).expect("valid pattern")
    });
    s.chars().count() <= coaching_input::limits::WEAKNESS_ID_MAX_CHARS && RE.is_match(s)
}

fn dotted(path: &serde_path_to_error::Path) -> String {
    path.iter()
        .filter_map(|seg| match seg {
            Segment::Seq { index } => Some(index.to_string()),
            Segment::Map { key } => Some(key.clone()),
            Segment::Enum { variant } => Some(variant.clone()),
            Segment::Unknown => None,
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// `Score` is an externally tagged enum, so serde reports an extra key beside `cp`/`mate` as
/// "expected map with a single key" without naming it. Looks the value up and names the key.
fn score_extra_key(raw: &Value, path: &str) -> Option<String> {
    let mut v = raw;
    for seg in path.split('.').filter(|s| !s.is_empty()) {
        v = match seg.parse::<usize>() {
            Ok(i) => v.get(i)?,
            Err(_) => v.get(seg)?,
        };
    }
    let extra: Vec<&String> = v
        .as_object()?
        .keys()
        .filter(|k| !matches!(k.as_str(), "cp" | "mate"))
        .collect();
    let first = extra.first()?;
    Some(format!("Unrecognized key: \"{first}\""))
}

/// Maps a serde shape error to zod's path and message. serde names the parent for a missing
/// key (`missing field `k``), zod the key itself, so the key is appended. serde's "expected map
/// with a single key" (an extra key beside `cp`/`mate` in a `Score`) is rewritten to name the
/// key. Every other message is serde's own.
fn shape_issue(
    raw: &Value,
    e: &serde_path_to_error::Error<serde_json::Error>,
) -> CoachingInputIssue {
    let mut path = dotted(e.path());
    let message = e.inner().to_string();
    if let Some(key) = message
        .strip_prefix("missing field `")
        .and_then(|m| m.split('`').next())
    {
        path = if path.is_empty() {
            key.to_string()
        } else {
            format!("{path}.{key}")
        };
        return CoachingInputIssue { path, message };
    }
    let message = if message.contains("expected map with a single key") {
        score_extra_key(raw, &path).unwrap_or(message)
    } else {
        message
    };
    CoachingInputIssue { path, message }
}

/// Deserialises and validates. `Err` lists every refinement issue, or the single shape error
/// (nothing typed exists to refine).
pub fn parse_coaching_input(raw: &Value) -> Result<CoachingInput, Vec<CoachingInputIssue>> {
    let typed: CoachingInput = match serde_path_to_error::deserialize(raw) {
        Ok(t) => t,
        Err(e) => {
            return Err(vec![shape_issue(raw, &e)]);
        }
    };
    let issues = validate(&typed);
    if issues.is_empty() {
        Ok(typed)
    } else {
        Err(issues)
    }
}
