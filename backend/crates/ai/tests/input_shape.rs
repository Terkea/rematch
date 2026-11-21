//! `ai::input::types` (R2's `coaching-input` contract, re-exported) has the shape of the zod
//! contract exported once from m3/base@4ff8220 into
//! `fixtures/ai/ts-export/coaching-input.schema.json` (provenance in `ts-export/SOURCE`).

#[path = "../../coaching-input/tests/support/shape.rs"]
mod shape;

use ai::input::types::{CoachingInput, Score, json_schema};
use ai::testing::{fixtures_dir, input, load_fixture, make_input};
use serde_json::{Value, json};

const FIXTURES: [&str; 3] = ["b2-rapid", "b0-blitz", "early-read"];

fn ts_export() -> Value {
    let path = fixtures_dir().join("ts-export/coaching-input.schema.json");
    let text = std::fs::read_to_string(&path).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn coaching_input_shape_matches_ts_export() {
    let ts = ts_export();
    assert_eq!(ts["source"], "m3/base@4ff8220");
    let rs = json_schema();
    // R2's walker: types, enums, bounds, required, nullability, unions, additionalProperties.
    let w = shape::compare_schemas(&ts["schema"], &rs);
    assert!(
        w.diffs.is_empty(),
        "schema shape differs:\n{}",
        w.diffs.join("\n")
    );
    // Same guards as R2's parity test: the walk did not stop early.
    assert!(w.objects >= 13 + 7 * 4, "walked only {} objects", w.objects);
    assert!(w.enums >= 17, "walked only {} enums", w.enums);
}

/// JSON has one number type: `48` (fixture) and `48.0` (an `f64` field) are the same value.
fn numbers_as_f64(v: &Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.iter().map(numbers_as_f64).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), numbers_as_f64(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[test]
fn fixtures_deserialize() {
    for name in FIXTURES {
        let parsed = input(name);
        // Every key is serialised (nulls included) and the typed dates write back unchanged.
        assert_eq!(
            numbers_as_f64(&parsed.to_json()),
            numbers_as_f64(&load_fixture(name)),
            "{name} round-trip"
        );
    }
}

#[test]
fn unknown_key_is_rejected() {
    let raw = make_input(json!({ "focus": [] }));
    assert!(serde_json::from_value::<CoachingInput>(raw).is_err());
}

#[test]
fn unknown_key_in_a_change_example_is_rejected() {
    // `ChangeExample` flattens `ExemplarPosition` and denies unknown fields: an extra key at
    // that level, or inside a nested exemplar object, must still fail.
    let base = input("b2-rapid");
    assert!(
        !base.changes.is_empty() && !base.changes[0].examples.is_empty(),
        "b2-rapid has a change example"
    );
    for over in [
        json!({ "changes": [{ "examples": [{ "note": "x" }] }] }),
        json!({ "changes": [{ "examples": [{ "played": { "lan": "e2e4" } }] }] }),
        json!({ "changes": [{ "examples": [{ "evalBefore": { "cp": 1, "x": 2 } }] }] }),
    ] {
        let raw = make_input(over.clone());
        assert!(
            serde_json::from_value::<CoachingInput>(raw).is_err(),
            "accepted {over}"
        );
    }
    // The untouched example still deserialises (the merge itself is valid).
    let same = make_input(json!({ "changes": [{ "examples": [{}] }] }));
    assert!(serde_json::from_value::<CoachingInput>(same).is_ok());
}

/// Every nullable property path of the schema, as segments (`[]` = any array element).
fn nullable_paths(root: &Value, node: &Value, path: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    let mut node = node;
    while let Some(Value::String(r)) = node.get("$ref") {
        node = root.pointer(r.strip_prefix('#').unwrap()).unwrap();
    }
    if let Some(Value::Array(any)) = node.get("anyOf") {
        for member in any.iter().filter(|m| m.get("type") != Some(&json!("null"))) {
            nullable_paths(root, member, path, out);
        }
        return;
    }
    if let Some(Value::Object(props)) = node.get("properties") {
        for (key, prop) in props {
            let mut p = prop;
            while let Some(Value::String(r)) = p.get("$ref") {
                p = root.pointer(r.strip_prefix('#').unwrap()).unwrap();
            }
            let is_nullable = match (p.get("type"), p.get("anyOf")) {
                (Some(Value::Array(t)), _) => t.contains(&json!("null")),
                (_, Some(Value::Array(a))) => {
                    a.iter().any(|m| m.get("type") == Some(&json!("null")))
                }
                _ => false,
            };
            path.push(key.clone());
            if is_nullable {
                out.push(path.clone());
            }
            nullable_paths(root, prop, path, out);
            path.pop();
        }
    }
    if let Some(items) = node.get("items") {
        path.push("[]".into());
        nullable_paths(root, items, path, out);
        path.pop();
    }
}

/// Removes the key at `path` (taking element 0 of every array); false if the parent is absent.
fn remove_at(v: &mut Value, path: &[String]) -> bool {
    let (last, parents) = path.split_last().unwrap();
    let mut cur = v;
    for seg in parents {
        let next = if seg == "[]" {
            cur.get_mut(0)
        } else {
            cur.get_mut(seg)
        };
        match next {
            Some(n) if !n.is_null() => cur = n,
            _ => return false,
        }
    }
    cur.as_object_mut().and_then(|o| o.remove(last)).is_some()
}

#[test]
fn missing_nullable_key_is_rejected() {
    let schema = json_schema();
    let mut paths = Vec::new();
    nullable_paths(&schema, &schema, &mut Vec::new(), &mut paths);
    // 19 nullable fields in the contract; `best` appears at four exemplar sites.
    assert_eq!(paths.len(), 22, "{paths:?}");
    // No fixture has an opening example, so add a variant of b2-rapid with one (a weakness
    // exemplar; the shape is what matters here).
    let mut with_opening_example = load_fixture("b2-rapid");
    let exemplar = with_opening_example["weaknesses"][0]["exemplars"][0].clone();
    with_opening_example["openings"][0]["examples"] = json!([exemplar]);
    assert!(
        serde_json::from_value::<CoachingInput>(with_opening_example.clone()).is_ok(),
        "the variant itself deserialises"
    );
    let mut inputs: Vec<(&str, Value)> = FIXTURES.iter().map(|n| (*n, load_fixture(n))).collect();
    inputs.push(("b2-rapid+opening-example", with_opening_example));
    for path in &paths {
        let mut tested = false;
        for (name, base) in &inputs {
            let mut raw = base.clone();
            if remove_at(&mut raw, path) {
                tested = true;
                assert!(
                    serde_json::from_value::<CoachingInput>(raw).is_err(),
                    "{name}: missing {} accepted",
                    path.join(".")
                );
            }
        }
        assert!(tested, "no fixture reaches {}", path.join("."));
    }
}

#[test]
fn make_input_merges_arrays_by_index() {
    let raw = make_input(json!({ "weaknesses": [{ "rank": 7 }], "rating": null }));
    let base = load_fixture("b2-rapid");
    assert_eq!(raw["weaknesses"][0]["rank"], 7);
    assert_eq!(raw["weaknesses"][0]["id"], base["weaknesses"][0]["id"]);
    // As in make-input.ts, the result has the override array's length.
    assert_eq!(raw["weaknesses"].as_array().unwrap().len(), 1);
    let mut over = vec![json!({}); 12];
    over.push(json!({ "x": 1 }));
    let appended = make_input(json!({ "strengths": over }));
    assert_eq!(appended["strengths"][12], json!({ "x": 1 }));
    assert!(raw["rating"].is_null());
}

#[test]
fn score_is_strict() {
    assert!(serde_json::from_value::<Score>(json!({ "cp": 1, "x": 2 })).is_err());
    assert!(serde_json::from_value::<Score>(json!({ "x": 2 })).is_err());
    assert!(serde_json::from_value::<Score>(json!({})).is_err());
    assert!(serde_json::from_value::<Score>(json!({ "cp": 1, "mate": 2 })).is_err());
    assert!(serde_json::from_value::<Score>(json!({ "cp": 1.5 })).is_err());
    assert_eq!(
        serde_json::from_value::<Score>(json!({ "mate": -3 })).unwrap(),
        Score::Mate(-3)
    );
    assert_eq!(
        serde_json::to_value(Score::Cp(42)).unwrap(),
        json!({ "cp": 42 })
    );
}
