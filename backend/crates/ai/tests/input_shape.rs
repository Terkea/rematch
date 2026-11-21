//! The Rust `CoachingInput` mirror has the same shape as the zod contract, exported once from
//! m3/base@4ff8220 into `fixtures/ai/ts-export/coaching-input.schema.json`.

use std::collections::BTreeMap;

use ai::input::types::{CoachingInput, Score};
use ai::testing::{fixtures_dir, input, load_fixture, make_input};
use serde_json::{Value, json};

/// Per schema path: (sorted property names, sorted required names, enum values, nullable).
type Shape = BTreeMap<String, (Vec<String>, Vec<String>, Vec<String>, bool)>;

fn resolve<'a>(root: &'a Value, mut node: &'a Value) -> &'a Value {
    while let Some(Value::String(r)) = node.get("$ref") {
        let name = r
            .strip_prefix("#/$defs/")
            .unwrap_or_else(|| panic!("unsupported $ref {r}"));
        node = &root["$defs"][name];
    }
    node
}

fn is_null_schema(v: &Value) -> bool {
    v.get("type") == Some(&json!("null"))
}

fn walk(root: &Value, node: &Value, path: &str, nullable: bool, out: &mut Shape) {
    let node = resolve(root, node);
    // anyOf: [X, {type: null}] is a nullable X.
    if let Some(Value::Array(any)) = node.get("anyOf") {
        let non_null: Vec<&Value> = any.iter().filter(|v| !is_null_schema(v)).collect();
        let has_null = non_null.len() < any.len();
        if non_null.len() == 1 {
            return walk(root, non_null[0], path, nullable || has_null, out);
        }
        out.insert(
            path.to_owned(),
            (vec![], vec![], vec![], nullable || has_null),
        );
        for (i, branch) in non_null.iter().enumerate() {
            walk(root, branch, &format!("{path}|{i}"), false, out);
        }
        return;
    }
    let mut nullable = nullable;
    // type: [X, "null"] is a nullable X.
    if let Some(Value::Array(types)) = node.get("type") {
        nullable |= types.contains(&json!("null"));
    }
    let mut props: Vec<String> = node
        .get("properties")
        .and_then(Value::as_object)
        .map(|p| p.keys().cloned().collect())
        .unwrap_or_default();
    props.sort();
    let mut required: Vec<String> = node
        .get("required")
        .and_then(Value::as_array)
        .map(|r| r.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default();
    required.sort();
    let mut values: Vec<String> = Vec::new();
    if let Some(Value::Array(e)) = node.get("enum") {
        for v in e {
            if v.is_null() {
                nullable = true;
            } else {
                values.push(v.to_string());
            }
        }
    }
    if let Some(c) = node.get("const") {
        values.push(c.to_string());
    }
    out.insert(path.to_owned(), (props, required, values, nullable));
    if let Some(Value::Object(p)) = node.get("properties") {
        for (k, v) in p {
            walk(root, v, &format!("{path}.{k}"), false, out);
        }
    }
    if let Some(items) = node.get("items") {
        walk(root, items, &format!("{path}[]"), false, out);
    }
}

fn schema_shape(schema: &Value) -> Shape {
    let mut out = Shape::new();
    walk(schema, schema, "$", false, &mut out);
    out
}

fn ts_export() -> Value {
    let path = fixtures_dir().join("ts-export/coaching-input.schema.json");
    let text = std::fs::read_to_string(&path).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn coaching_input_shape_matches_ts_export() {
    let ts = ts_export();
    assert_eq!(ts["source"], "m3/base@4ff8220");
    let rust = serde_json::to_value(schemars::schema_for!(CoachingInput)).unwrap();
    let (r, t) = (schema_shape(&rust), schema_shape(&ts["schema"]));
    let r_keys: Vec<_> = r.keys().collect();
    let t_keys: Vec<_> = t.keys().collect();
    assert_eq!(r_keys, t_keys, "schema paths differ");
    for (path, shape) in &t {
        assert_eq!(&r[path], shape, "shape differs at {path}");
    }
}

/// Every number as an f64, so `48` (fixture) and `48.0` (an `f64` field) compare equal.
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
    for name in ["b2-rapid", "b0-blitz", "early-read"] {
        let parsed = input(name);
        // The mirror round-trips: every key is serialised (nulls included).
        assert_eq!(
            numbers_as_f64(&serde_json::to_value(&parsed).unwrap()),
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
fn missing_nullable_key_is_rejected() {
    let mut raw = load_fixture("b2-rapid");
    raw.as_object_mut().unwrap().remove("rating");
    assert!(serde_json::from_value::<CoachingInput>(raw).is_err());
}

#[test]
fn make_input_merges_arrays_by_index() {
    let raw = make_input(json!({ "weaknesses": [{ "rank": 7 }], "rating": null }));
    let base = load_fixture("b2-rapid");
    assert_eq!(raw["weaknesses"][0]["rank"], 7);
    assert_eq!(raw["weaknesses"][0]["id"], base["weaknesses"][0]["id"]);
    // As in make-input.ts, the result has the override array's length.
    assert_eq!(raw["weaknesses"].as_array().unwrap().len(), 1);
    let appended = make_input(
        json!({ "strengths": [{}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {"x": 1}] }),
    );
    assert_eq!(appended["strengths"][12], json!({"x": 1}));
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
