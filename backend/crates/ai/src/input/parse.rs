//! `parse_coaching_input`: raw JSON to a validated `CoachingInput` (port of `parseCoachingInput`).
//!
//! Deserialising uses `serde_path_to_error`, so a shape error (unknown key, wrong type, bad
//! enum, missing key) names its path in zod's dotted form (`weaknesses.0.rank`). Then
//! `validate` applies the refinements.

use serde_json::Value;
use serde_path_to_error::Segment;

use super::types::CoachingInput;
pub use super::validate::{CoachingInputIssue, validate};

/// `^W(1[0-2]|[1-9])[a-d]?$`, at most 5 characters.
pub fn is_weakness_id(s: &str) -> bool {
    let Some(rest) = s.strip_prefix('W') else {
        return false;
    };
    let digits = rest.strip_suffix(['a', 'b', 'c', 'd']).unwrap_or(rest);
    s.len() <= 5
        && digits.bytes().all(|b| b.is_ascii_digit())
        && !digits.starts_with('0')
        && matches!(digits.parse::<u8>(), Ok(1..=12))
}

fn dotted(path: &serde_path_to_error::Path) -> String {
    path.iter()
        .filter_map(|seg| match seg {
            Segment::Seq { index } => Some(index.to_string()),
            Segment::Map { key } => Some(key.clone()),
            Segment::Enum { .. } | Segment::Unknown => None,
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// Keys of `ChangeExample`: the flattened `ExemplarPosition` keys plus `side`.
const CHANGE_EXAMPLE_KEYS: [&str; 14] = [
    "gameId",
    "ply",
    "playedAt",
    "fen",
    "played",
    "best",
    "bestPv",
    "refutationPv",
    "tags",
    "evalBefore",
    "evalAfter",
    "winBefore",
    "winAfter",
    "side",
];

/// Unknown keys that serde cannot reject: `deny_unknown_fields` does not work with a
/// `#[serde(flatten)]` field (`ChangeExample`), so those keys are checked on the raw JSON.
fn flattened_unknown_keys(raw: &Value) -> Vec<CoachingInputIssue> {
    let mut out = vec![];
    let Some(changes) = raw.get("changes").and_then(Value::as_array) else {
        return out;
    };
    for (ci, change) in changes.iter().enumerate() {
        let Some(examples) = change.get("examples").and_then(Value::as_array) else {
            continue;
        };
        for (ei, example) in examples.iter().enumerate() {
            let Some(obj) = example.as_object() else {
                continue;
            };
            for key in obj
                .keys()
                .filter(|k| !CHANGE_EXAMPLE_KEYS.contains(&k.as_str()))
            {
                out.push(CoachingInputIssue {
                    path: format!("changes.{ci}.examples.{ei}"),
                    message: format!("Unrecognized key: \"{key}\""),
                });
            }
        }
    }
    out
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

/// Deserialises and validates. `Err` lists every issue (a shape error is reported with the raw
/// flattened-key issues only, since nothing typed exists to refine).
pub fn parse_coaching_input(raw: &Value) -> Result<CoachingInput, Vec<CoachingInputIssue>> {
    let mut issues = flattened_unknown_keys(raw);
    let typed: CoachingInput = match serde_path_to_error::deserialize(raw) {
        Ok(t) => t,
        Err(e) => {
            let path = dotted(e.path());
            let message = score_extra_key(raw, &path).unwrap_or_else(|| e.inner().to_string());
            issues.push(CoachingInputIssue { path, message });
            return Err(issues);
        }
    };
    issues.extend(validate(&typed));
    if issues.is_empty() {
        Ok(typed)
    } else {
        Err(issues)
    }
}
