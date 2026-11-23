//! Test helpers shared by every R3 lane (port of `test/fixtures/make-input.ts`). Public so
//! integration tests can use them; nothing here touches the network or a database.

use serde_json::{Map, Value};

use crate::input::types::CoachingInput;

/// `backend/fixtures/ai`.
pub fn fixtures_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/ai")
}

/// Reads `backend/fixtures/ai/input/coaching-input.<name>.json` (`b2-rapid`, `b0-blitz`,
/// `early-read`). Panics with the path when it is missing or not JSON.
pub fn load_fixture(name: &str) -> Value {
    let path = fixtures_dir().join(format!("input/coaching-input.{name}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn merge(base: &Value, over: &Value) -> Value {
    match (base, over) {
        (_, Value::Array(o)) => {
            // Arrays merge element-wise onto the base array; extra elements are appended.
            let b: &[Value] = match base {
                Value::Array(b) => b,
                _ => &[],
            };
            Value::Array(
                o.iter()
                    .enumerate()
                    .map(|(i, v)| match b.get(i) {
                        Some(bv) => merge(bv, v),
                        None => v.clone(),
                    })
                    .collect(),
            )
        }
        (Value::Object(b), Value::Object(o)) => {
            let mut out: Map<String, Value> = b.clone();
            for (k, v) in o {
                let merged = match b.get(k) {
                    Some(bv) => merge(bv, v),
                    None => v.clone(),
                };
                out.insert(k.clone(), merged);
            }
            Value::Object(out)
        }
        _ => over.clone(),
    }
}

/// Deep-merges `overrides` onto the b2-rapid fixture (TS `makeInput`). Objects merge by key,
/// arrays merge by index and extra elements are appended; any other value replaces.
pub fn make_input(overrides: Value) -> Value {
    merge(&load_fixture("b2-rapid"), &overrides)
}

/// Loads a fixture and deserialises it (shape only; Task 2's refinements are not applied).
pub fn input(name: &str) -> CoachingInput {
    serde_json::from_value(load_fixture(name))
        .unwrap_or_else(|e| panic!("fixture {name} does not deserialize: {e}"))
}

/// `backend/fixtures/ai/input/colour-split.v2.json`: a v2 colour split (White with enough games,
/// a compared and an uncompared weakness; Black below the minimum) and a peer benchmark.
pub fn colour_split() -> Value {
    let path = fixtures_dir().join("input/colour-split.v2.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A fixture as coaching-input-v2: [`input`] with [`colour_split`] merged on (ADR 0085).
pub fn v2_input(name: &str) -> CoachingInput {
    serde_json::from_value(merge(&load_fixture(name), &colour_split()))
        .unwrap_or_else(|e| panic!("v2 fixture {name} does not deserialize: {e}"))
}

/// `backend/fixtures/ai/input/trajectory.v3.json`: a v3 trajectory (ADR 0089), a fall from a
/// peak of 1,097 to 816 with then against now, for the v3 prompts and the evidence.
pub fn trajectory() -> Value {
    let path = fixtures_dir().join("input/trajectory.v3.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A fixture as coaching-input-v3: [`v2_input`]'s merge plus [`trajectory`] (ADR 0089).
pub fn v3_input(name: &str) -> CoachingInput {
    let v2 = merge(&load_fixture(name), &colour_split());
    serde_json::from_value(merge(&v2, &trajectory()))
        .unwrap_or_else(|e| panic!("v3 fixture {name} does not deserialize: {e}"))
}

/// ADR 0094: a plan context with every field set (a running focus too early to tell, a week and
/// a month of practice, a verdict per drilled source and the pattern comparison), so every
/// path of the plan is citable in tests.
pub fn plan_fixture() -> crate::plan::PlanContext {
    serde_json::from_value(serde_json::json!({
        "asOf": "2026-11-23",
        "focus": {
            "seriesKey": "W4.missed", "label": "Missing a mate", "side": "both",
            "startedOn": "2026-11-16", "targetRate": 0.11, "gamesN": 20, "gamesPlayed": 6,
            "progress": { "events": 1, "chances": 9, "rate": 0.111, "low": 0.03, "high": 0.3 },
            "status": "too_early", "moreChances": 1, "outcome": null,
            "baselineRate": 0.14, "baselineEvents": 13, "baselineChances": 91
        },
        "training": {
            "lastWeek": {
                "days": 7, "daysPractised": 4, "attempts": 30, "right": 21, "firstTryRate": 0.7,
                "itemsRetired": 2,
                "bySource": [
                    { "source": "W4.missed", "label": "Missing a mate", "attempts": 10,
                      "right": 8, "firstTryRate": 0.8 },
                    { "source": "blunder", "label": "Blunders", "attempts": 20, "right": 13,
                      "firstTryRate": 0.65 }
                ]
            },
            "lastMonth": {
                "days": 30, "daysPractised": 9, "attempts": 70, "right": 45,
                "firstTryRate": 0.643, "itemsRetired": 3,
                "bySource": [
                    { "source": "W4.missed", "label": "Missing a mate", "attempts": 25,
                      "right": 18, "firstTryRate": 0.72 }
                ]
            },
            "activeItems": 41
        },
        "isItWorking": [
            { "source": "W4.missed", "label": "Missing a mate", "drilled": 12,
              "firstDrill": "2026-11-02",
              "before": { "events": 13, "chances": 91, "rate": 0.143, "low": 0.1, "high": 0.19 },
              "after": { "events": 2, "chances": 18, "rate": 0.111, "low": 0.04, "high": 0.25 },
              "verdict": "too_early" }
        ],
        "patterns": {
            "since": "2026-11-02",
            "drilled": { "events": 30, "chances": 40, "rate": 0.75, "low": 0.65, "high": 0.83 },
            "heldOut": { "events": 6, "chances": 12, "rate": 0.5, "low": 0.32, "high": 0.68 },
            "verdict": "too_early"
        }
    }))
    .expect("the plan fixture parses")
}
