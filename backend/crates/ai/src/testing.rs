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
