//! The JSON Schema shape walker: compares a zod export with a schemars schema keyword by
//! keyword (types, enums, bounds, required, nullability, unions, `additionalProperties`).
//! Shared by `coaching-input/tests/parity.rs` and `ai/tests/input_shape.rs` via `#[path]`.
#![allow(dead_code)]

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

/// Bounds compared in both directions: present in one schema means present and equal in the
/// other.
pub const BOUNDS: &[&str] = &[
    "minLength",
    "maxLength",
    "minItems",
    "maxItems",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "pattern",
    "format",
];

/// Keywords the walker descends into or compares structurally.
pub const STRUCTURE: &[&str] = &[
    "type",
    "enum",
    "const",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "anyOf",
    "oneOf",
];

/// Keywords without validation meaning. Every keyword outside `STRUCTURE`, `BOUNDS` and these
/// (`uniqueItems`, `allOf`, `not`, `prefixItems`, `default`, ...) is reported, so a constraint
/// the walker does not understand cannot pass unnoticed on either side.
pub const ANNOTATIONS: &[&str] = &["$schema", "$defs", "title", "description"];

#[derive(Clone, Copy, Debug)]
pub enum Side {
    Ts,
    Rust,
}

pub struct Walk<'a> {
    ts_root: &'a Value,
    rs_root: &'a Value,
    pub diffs: Vec<String>,
    pub objects: usize,
    pub enums: usize,
}

pub fn walk<'a>(ts_root: &'a Value, rs_root: &'a Value) -> Walk<'a> {
    Walk {
        ts_root,
        rs_root,
        diffs: Vec::new(),
        objects: 0,
        enums: 0,
    }
}

pub fn branches(s: &Value) -> Option<&Vec<Value>> {
    s.get("anyOf")
        .or_else(|| s.get("oneOf"))
        .and_then(Value::as_array)
}

pub fn enum_set(s: &Value) -> Option<BTreeSet<String>> {
    if let Some(c) = s.get("const") {
        return Some(BTreeSet::from([c.to_string()]));
    }
    s.get("enum")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(Value::to_string).collect())
}

pub fn same_bound(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

pub fn key_set(s: &Value, key: &str) -> BTreeSet<String> {
    match s.get(key) {
        Some(Value::Object(o)) => o.keys().cloned().collect(),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| v.as_str().unwrap_or_default().to_owned())
            .collect(),
        _ => BTreeSet::new(),
    }
}

pub fn as_map(s: &Value) -> Map<String, Value> {
    s.as_object()
        .cloned()
        .unwrap_or_else(|| panic!("expected an object schema, got {s}"))
}

impl<'a> Walk<'a> {
    fn diff(&mut self, path: &str, what: String) {
        self.diffs.push(format!("{path}: {what}"));
    }

    fn root(&self, side: Side) -> &'a Value {
        match side {
            Side::Ts => self.ts_root,
            Side::Rust => self.rs_root,
        }
    }

    /// Adds the validation keywords of `extra` (the siblings of a `$ref`, or of a union whose
    /// only non-null member is `base`) to `base`. In 2020-12 they apply next to it, so they are
    /// compared like the member's own keywords; a keyword both sides set differently is a diff.
    fn merge_siblings(
        &mut self,
        side: Side,
        path: &str,
        base: &mut Map<String, Value>,
        extra: &Map<String, Value>,
    ) {
        for (k, v) in extra {
            if ANNOTATIONS.contains(&k.as_str()) {
                continue;
            }
            match base.get(k) {
                None => {
                    base.insert(k.clone(), v.clone());
                }
                Some(existing) if existing == v => {}
                Some(existing) => {
                    let what = format!("{side:?} sibling {k} {v} conflicts with {existing}");
                    self.diff(path, what);
                }
            }
        }
    }

    /// Resolves `$ref`s, keeping their sibling keywords.
    fn resolve(&mut self, side: Side, path: &str, s: &Value) -> Value {
        let mut s = s.clone();
        while let Some(r) = s.get("$ref").and_then(Value::as_str).map(str::to_owned) {
            let pointer = r.strip_prefix('#').expect("only local $refs");
            let target = self
                .root(side)
                .pointer(pointer)
                .unwrap_or_else(|| panic!("dangling $ref {r}"));
            let mut merged = as_map(target);
            let mut siblings = as_map(&s);
            siblings.remove("$ref");
            self.merge_siblings(side, path, &mut merged, &siblings);
            s = Value::Object(merged);
        }
        s
    }

    /// Resolves a schema and splits its nullability off: `{"type": [T, "null"]}` and
    /// `{"anyOf": [S, {"type": "null"}]}` both become `(true, S)`, with the union's other
    /// keywords merged into `S`.
    fn normalize(&mut self, side: Side, path: &str, s: &Value) -> (bool, Value) {
        let mut s = self.resolve(side, path, s);
        let mut nullable = false;
        if let Some(Value::Array(types)) = s.get_mut("type") {
            let before = types.len();
            types.retain(|t| t != "null");
            nullable |= types.len() != before;
            if types.len() == 1 {
                let t = types[0].clone();
                s["type"] = t;
            }
        }
        if let Some(Value::Array(members)) = s.get("anyOf").cloned() {
            let mut rest = Vec::new();
            let mut had_null = false;
            for m in &members {
                let resolved = self.resolve(side, path, m);
                if resolved.get("type") == Some(&json!("null")) {
                    had_null = true;
                    let extra: Vec<String> = as_map(&resolved)
                        .keys()
                        .filter(|k| *k != "type" && !ANNOTATIONS.contains(&k.as_str()))
                        .cloned()
                        .collect();
                    if !extra.is_empty() {
                        self.diff(path, format!("{side:?} null member has keywords {extra:?}"));
                    }
                } else {
                    rest.push(m.clone());
                }
            }
            if had_null {
                nullable = true;
                if rest.len() == 1 {
                    let mut outer = as_map(&s);
                    outer.remove("anyOf");
                    let member = self.resolve(side, path, &rest[0]);
                    let mut member = as_map(&member);
                    self.merge_siblings(side, path, &mut member, &outer);
                    s = Value::Object(member);
                } else {
                    s["anyOf"] = Value::Array(rest);
                }
            }
        }
        (nullable, s)
    }

    fn check_keywords(&mut self, path: &str, side: Side, s: &Value) {
        let Some(obj) = s.as_object() else {
            self.diff(path, format!("{side:?} schema is not an object: {s}"));
            return;
        };
        for k in obj.keys() {
            let k = k.as_str();
            if !(STRUCTURE.contains(&k) || BOUNDS.contains(&k) || ANNOTATIONS.contains(&k)) {
                self.diff(
                    path,
                    format!("{side:?} uses keyword {k:?} the walker does not compare"),
                );
            }
        }
        if obj.contains_key("anyOf") && obj.contains_key("oneOf") {
            self.diff(path, format!("{side:?} has both anyOf and oneOf"));
        }
    }

    pub fn compare(&mut self, path: &str, ts: &Value, rs: &Value) {
        let (ts_null, t) = self.normalize(Side::Ts, path, ts);
        let (rs_null, r) = self.normalize(Side::Rust, path, rs);
        if ts_null != rs_null {
            self.diff(path, format!("nullable ts={ts_null} rs={rs_null}"));
        }
        self.check_keywords(path, Side::Ts, &t);
        self.check_keywords(path, Side::Rust, &r);

        // This node's own keywords, including those beside a union's members.
        if t.get("type") != r.get("type") {
            let what = format!("type {:?} vs {:?}", t.get("type"), r.get("type"));
            self.diff(path, what);
        }
        let (te, re) = (enum_set(&t), enum_set(&r));
        if te.is_some() {
            self.enums += 1;
        }
        if te != re {
            self.diff(path, format!("enum {te:?} vs {re:?}"));
        }
        for &key in BOUNDS {
            match (t.get(key), r.get(key)) {
                (None, None) => {}
                (Some(a), Some(b)) if same_bound(a, b) => {}
                (a, b) => self.diff(path, format!("{key} {a:?} vs {b:?}")),
            }
        }

        match (branches(&t), branches(&r)) {
            (Some(tb), Some(rb)) => {
                if tb.len() == rb.len() {
                    for (i, (tm, rm)) in tb.iter().zip(rb).enumerate() {
                        self.compare(&format!("{path}|{i}"), tm, rm);
                    }
                } else {
                    let what = format!("{} vs {} union members", tb.len(), rb.len());
                    self.diff(path, what);
                }
            }
            (None, None) => {}
            _ => self.diff(path, "union on one side only".into()),
        }

        let object_keys = ["properties", "required", "additionalProperties"];
        if object_keys
            .iter()
            .any(|k| t.get(k).is_some() || r.get(k).is_some())
        {
            self.objects += 1;
            let (tp, rp) = (key_set(&t, "properties"), key_set(&r, "properties"));
            if tp != rp {
                self.diff(path, format!("properties {tp:?} vs {rp:?}"));
            }
            let (tr, rr) = (key_set(&t, "required"), key_set(&r, "required"));
            if tr != tp {
                self.diff(path, format!("TS required {tr:?} is not every property"));
            }
            if rr != rp {
                self.diff(path, format!("Rust required {rr:?} is not every property"));
            }
            for (side, s) in [(Side::Ts, &t), (Side::Rust, &r)] {
                if s.get("additionalProperties") != Some(&Value::Bool(false)) {
                    self.diff(
                        path,
                        format!("{side:?} object allows additional properties"),
                    );
                }
            }
            for key in tp.intersection(&rp) {
                let (ts_prop, rs_prop) = (&t["properties"][key], &r["properties"][key]);
                self.compare(&format!("{path}.{key}"), ts_prop, rs_prop);
            }
        }
        match (t.get("items"), r.get("items")) {
            (None, None) => {}
            (Some(ti), Some(ri)) => self.compare(&format!("{path}[]"), ti, ri),
            _ => self.diff(path, "items on one side only".into()),
        }
    }
}

/// Compares `rs` against `ts` from the root; returns the walk (diffs and counters).
pub fn compare_schemas<'a>(ts: &'a Value, rs: &'a Value) -> Walk<'a> {
    let mut w = walk(ts, rs);
    w.compare("$", ts, rs);
    w
}

/// The v1 shape of the Rust schema: without v2's optional `byColor` and `benchmark` (and their
/// definitions), and `inputVersion` as v1's literal. The zod export is v1's (ADR 0085).
pub fn v1_shape(mut rs: Value) -> Value {
    for key in ["byColor", "benchmark"] {
        rs["properties"].as_object_mut().unwrap().remove(key);
    }
    for def in [
        "ByColor",
        "ColorSummary",
        "ColorMetric",
        "ColorWeakness",
        "Benchmark",
        "BenchmarkSource",
    ] {
        rs["$defs"].as_object_mut().unwrap().remove(def);
    }
    rs["properties"]["inputVersion"] = json!({"type": "string", "const": "coaching-input-v1"});
    rs
}
