//! What the chat model is told up front: the coaching context and anchor blocks.
//!
//! Neither carries the account id. Facts are cited by reference: the context lists the number
//! and date paths (`refs`) that resolve against `build_evidence(input)`, and a position anchor
//! adds its position to the turn's evidence and lists ready-made tokens.

use serde_json::{Map, Value, json};

use super::anchor::ChatAnchor;
use super::data_source::PositionRecord;
use super::execute::{PosRef, add_position};
use crate::generate::context::canonical_json_escaped;
use crate::input::types::CoachingInput;
use crate::validator::evidence::{Evidence, build_evidence, list_date_paths, list_number_paths};
use crate::validator::language::evidence_window_token;

/// Top-level sections of the input the context shows; their paths are listed in `refs`.
const CONTEXT_SECTIONS: &[&str] = &[
    "basis",
    "areaScores",
    "weaknesses",
    "openings",
    "changes",
    "byColor",
    "benchmark",
];

fn in_context(path: &str) -> bool {
    let section = path.split('.').next().unwrap_or("");
    CONTEXT_SECTIONS.contains(&section)
        && !path.contains(".exemplars.")
        && !path.contains(".examples.")
}

fn pick(v: &Value, keys: &[&str]) -> Value {
    let mut out = Map::new();
    for k in keys {
        if let Some(x) = v.get(*k) {
            out.insert((*k).to_owned(), x.clone());
        }
    }
    Value::Object(out)
}

/// The chat's coaching context: band, time class, basis, the evidence-window token, area
/// scores, weaknesses without exemplars, the opening verdict list, changes, and the number and
/// date paths (`refs`) the model may cite with `{{num:}}` and `{{date:}}`. No account id.
///
/// A v2 input (ADR 0084) adds the colour split `byColor` (each colour's games, W-D-L, score,
/// accuracy, blunders per game and its own weakness rows, so the coach can answer "how do I do
/// as Black?") and the `benchmark` band. A v1 input's context is unchanged.
pub fn build_chat_context(input: &CoachingInput) -> Value {
    let json = input.to_json();
    let weaknesses: Vec<Value> = json["weaknesses"]
        .as_array()
        .map(|ws| {
            ws.iter()
                .map(|w| {
                    let mut w = w.clone();
                    if let Some(o) = w.as_object_mut() {
                        o.remove("exemplars");
                    }
                    w
                })
                .collect()
        })
        .unwrap_or_default();
    let list = |key: &str, fields: &[&str]| -> Vec<Value> {
        json[key]
            .as_array()
            .map(|xs| xs.iter().map(|x| pick(x, fields)).collect())
            .unwrap_or_default()
    };
    let ev = build_evidence(input);
    let nums: Vec<String> = list_number_paths(&ev)
        .into_iter()
        .filter(|p| in_context(p))
        .collect();
    let dates: Vec<String> = list_date_paths(&ev)
        .into_iter()
        .filter(|p| in_context(p))
        .collect();
    let mut ctx = json!({
        "band": json["band"],
        "timeClass": json["timeClass"],
        "basis": json["basis"],
        "evidenceWindow": evidence_window_token(input.time_class),
        "areaScores": json["areaScores"],
        "weaknesses": weaknesses,
        "openings": list(
            "openings",
            &["lineKey", "name", "eco", "color", "side", "verdict", "games", "score"],
        ),
        "changes": list(
            "changes",
            &["seriesKey", "weaknessId", "label", "fromState", "toState", "onsetDate"],
        ),
        "refs": {"num": nums, "date": dates},
    });
    for key in ["byColor", "benchmark"] {
        if let Some(v) = json.get(key) {
            ctx[key] = v.clone();
        }
    }
    ctx
}

/// Whether `resolved` is the record of `anchor`: it must carry every id of the anchor, equal.
/// (`DbCoachDataSource::resolve_anchor` returns records with these camelCase id fields: a
/// position record's `gameId` and `ply`, a weakness's `id` or `weaknessId`, an opening line's
/// `lineKey`, a game's `gameId`, a report section's `reportId` and `section`.)
fn resolved_matches(anchor: &ChatAnchor, resolved: &Value) -> bool {
    let str_is = |key: &str, want: &str| resolved.get(key).and_then(Value::as_str) == Some(want);
    let ply_is = |want: u32| resolved.get("ply").and_then(Value::as_u64) == Some(u64::from(want));
    match anchor {
        ChatAnchor::Move { game_id, ply } | ChatAnchor::Position { game_id, ply } => {
            str_is("gameId", game_id) && ply_is(*ply)
        }
        ChatAnchor::Game { game_id } => str_is("gameId", game_id),
        ChatAnchor::Weakness { weakness_id } => {
            str_is("weaknessId", weakness_id)
                || (resolved.get("weaknessId").is_none() && str_is("id", weakness_id))
        }
        ChatAnchor::OpeningLine { line_key } => str_is("lineKey", line_key),
        ChatAnchor::ReportSection { report_id, section } => {
            str_is("reportId", &report_id.to_string())
                && serde_json::to_value(section).ok().as_ref() == resolved.get("section")
        }
    }
}

/// The details of an anchor for [`anchor_block`]: the anchor's kind and ids, then the fields
/// the data source resolved (for a `Move` or `Position` anchor, a [`PositionRecord`] with `fen`
/// and `fenAfterPlayed`, which feed `analyse_position`). A resolved record that does not carry
/// the anchor's own ids is dropped, for every kind: only the anchor's ids remain. A position
/// anchor is added to `ev` and gets `refs` (`move`, `best`, `pv` tokens) that resolve against it.
pub fn anchor_details(ev: &mut Evidence, anchor: &ChatAnchor, resolved: &Value) -> Value {
    let mut out = match serde_json::to_value(anchor) {
        Ok(Value::Object(o)) => o,
        _ => Map::new(),
    };
    let Value::Object(r) = resolved else {
        return Value::Object(out);
    };
    if !resolved_matches(anchor, resolved) {
        return Value::Object(out);
    }
    for (k, v) in r {
        out.entry(k.clone()).or_insert_with(|| v.clone());
    }
    if matches!(
        anchor,
        ChatAnchor::Move { .. } | ChatAnchor::Position { .. }
    ) && let Ok(rec) = serde_json::from_value::<PositionRecord>(resolved.clone())
        && let Some(refs) = add_position(ev, PosRef::of(&rec), None)
    {
        out.insert("refs".into(), refs);
    }
    Value::Object(out)
}

/// `"<anchor>" + canonical_json(details) + "</anchor>"`, with `<` escaped ([`canonical_json_escaped`]).
pub fn anchor_block(details: &Value) -> String {
    format!("<anchor>{}</anchor>", canonical_json_escaped(details))
}

/// `"<coaching_context>" + canonical_json(build_chat_context(input)) + "</coaching_context>"`,
/// with `<` escaped ([`canonical_json_escaped`]).
pub fn context_block(input: &CoachingInput) -> String {
    format!(
        "<coaching_context>{}</coaching_context>",
        canonical_json_escaped(&build_chat_context(input))
    )
}
