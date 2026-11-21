//! What the chat model is told up front: the coaching context and anchor blocks.
//!
//! Neither carries the account id. Facts are cited by reference: the context lists the number
//! and date paths (`refs`) that resolve against `build_evidence(input)`, and a position anchor
//! adds its position to the turn's evidence and lists ready-made tokens.

use serde_json::{Map, Value, json};

use super::anchor::ChatAnchor;
use super::data_source::PositionRecord;
use crate::input::types::{CoachingInput, TimeClass};
use crate::validator::evidence::{
    Evidence, PositionArgs, PvFrom, PvSides, build_evidence, list_date_paths, list_number_paths,
};
use crate::validator::pv::pv_to_san;

/// Top-level sections of the input the context shows; their paths are listed in `refs`.
const CONTEXT_SECTIONS: &[&str] = &["basis", "areaScores", "weaknesses", "openings", "changes"];

/// T8's `evidence_window_token` (`language.rs`, not yet on this base): the evidence window as
/// tokens. Replace with T8's function when both land.
fn evidence_window_token(tc: TimeClass) -> String {
    format!(
        "over your last {{{{num:basis.analysedGames|int}}}} analysed {} games \
         ({{{{num:basis.importedGames|int}}}} imported)",
        tc.as_str()
    )
}

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
    json!({
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
    })
}

/// The details of an anchor for [`anchor_block`]: the anchor's kind and ids, then the fields
/// the data source resolved (for a `Move` or `Position` anchor, a [`PositionRecord`] with `fen`
/// and `fenAfterPlayed`, which feed `analyse_position`). A position anchor is added to `ev` and
/// gets `refs` (`move`, `best`, `pv` tokens) that resolve against it.
pub fn anchor_details(ev: &mut Evidence, anchor: &ChatAnchor, resolved: &Value) -> Value {
    let mut out = match serde_json::to_value(anchor) {
        Ok(Value::Object(o)) => o,
        _ => Map::new(),
    };
    match resolved {
        Value::Object(r) => {
            for (k, v) in r {
                out.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        Value::Null => {}
        other => {
            out.insert("details".into(), other.clone());
        }
    }
    if let ChatAnchor::Move { game_id, ply } | ChatAnchor::Position { game_id, ply } = anchor
        && let Ok(rec) = serde_json::from_value::<PositionRecord>(resolved.clone())
        && rec.game_id == *game_id
        && rec.ply == *ply
    {
        out.insert("refs".into(), position_refs(ev, &rec));
    }
    Value::Object(out)
}

fn position_refs(ev: &mut Evidence, rec: &PositionRecord) -> Value {
    let best = PvFrom {
        from_fen: rec.fen.clone(),
        uci: rec.pv.clone(),
    };
    let key = format!("{}#{}", rec.game_id, rec.ply);
    let existed = ev.positions.contains_key(&key);
    ev.add_position(PositionArgs {
        game_id: rec.game_id.clone(),
        ply: rec.ply,
        fen: rec.fen.clone(),
        played: Some(rec.played.clone()),
        best: rec.best.clone(),
        pvs: vec![best.clone()],
        pv_sides: if existed {
            PvSides::default()
        } else {
            PvSides {
                best: Some(best),
                refutation: None,
            }
        },
    });
    let (g, p) = (&rec.game_id, rec.ply);
    let pv_len = ev.positions.get(&key).map_or_else(
        || pv_to_san(&rec.fen, &rec.pv).len(),
        |pe| pe.best_pv_san.len(),
    );
    let best = ev.positions.get(&key).and_then(|pe| pe.best.as_ref());
    json!({
        "move": format!("{{{{move:{g}:{p}}}}}"),
        "best": best.map(|_| format!("{{{{best:{g}:{p}}}}}")),
        "pv": (1..=pv_len.min(999)).map(|n| format!("{{{{pv:{g}:{p}:best:{n}}}}}")).collect::<Vec<_>>(),
    })
}

/// `"<anchor>" + canonical_json(details) + "</anchor>"`. Canonical JSON is compact with sorted
/// keys (serde_json maps are sorted; `preserve_order` is banned). `<` is written as `<`,
/// which is the same JSON, so no text inside the details can close the block.
pub fn anchor_block(details: &Value) -> String {
    let json = details.to_string().replace('<', "\\u003c");
    format!("<anchor>{json}</anchor>")
}
