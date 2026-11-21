//! Parity with the M3 zod contract through its JSON Schema export (`ts-schema.json`, see
//! `fixtures/coaching-input/SOURCE`), and the committed schemars schema.

use std::collections::BTreeSet;

use coaching_input::{
    Alternative, AreaId, AreaScore, BandId, Basis, CandidateCause, CauseKind, CauseValue,
    ChangeExample, CoachingInput, ExampleSide, ExemplarPosition, InputVersion, LineSide, Metric,
    MetricUnit, MoveRef, OpeningVerdict, Platform, PlayerColor, RankedWeakness, Score, Strength,
    TimeClass, TimeControl, TimelineChange, Verdict, WeaknessState, json_schema,
};
use serde_json::{Map, Value, json};
use time::macros::{date, datetime};

fn fixture_path(name: &str) -> String {
    format!(
        "{}/../../fixtures/coaching-input/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn read_json(path: &str) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn ts_schema() -> Value {
    read_json(&fixture_path("ts-schema.json"))
}

fn sample_value() -> Value {
    read_json(&fixture_path("valid-sample.json"))
}

fn validator(schema: &Value) -> jsonschema::Validator {
    jsonschema::options()
        .should_validate_formats(true)
        .build(schema)
        .expect("schema compiles")
}

fn errors(validator: &jsonschema::Validator, instance: &Value) -> Vec<String> {
    validator
        .iter_errors(instance)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect()
}

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const AFTER_E4: &str = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";

fn exemplar(some: bool, black: bool) -> ExemplarPosition {
    let (ply, fen, played) = if black {
        (2, AFTER_E4, ("c5", "c7c5"))
    } else {
        (1, START, ("e4", "e2e4"))
    };
    ExemplarPosition {
        game_id: "li-AbCd1234".into(),
        ply,
        played_at: datetime!(2026-11-01 18:30:00.25 UTC),
        fen: fen.into(),
        played: MoveRef {
            san: played.0.into(),
            uci: played.1.into(),
        },
        best: some.then(|| MoveRef {
            san: "d4".into(),
            uci: "d2d4".into(),
        }),
        best_pv: vec!["d2d4".into(), "d7d5".into(), "c2c4".into()],
        refutation_pv: vec!["e7e5".into(), "g1f3".into()],
        tags: vec!["hanging_piece".into(), "mate_in_2".into()],
        eval_before: Score::Cp(20),
        eval_after: Score::Mate(-3),
        win_before: 51.8,
        win_after: 0.0,
    }
}

/// A contract value built in Rust with every `Option` either `Some` or `None`.
fn full_input(some: bool) -> CoachingInput {
    CoachingInput {
        input_version: InputVersion::V1,
        account_id: "6f1c1d52-3a4b-4c8e-9d2a-0b7a5e1f3c44".parse().unwrap(),
        platform: Platform::Chesscom,
        time_class: TimeClass::Blitz,
        time_control: some.then_some(TimeControl {
            base_seconds: 180,
            increment_seconds: 2,
        }),
        band: BandId::B3,
        rating: some.then_some(1850),
        generated_at: datetime!(2026-11-21 10:00:05.123456 UTC),
        basis: Basis {
            analysed_games: 40,
            imported_games: 120,
            window_start: date!(2026 - 05 - 21),
            window_end: date!(2026 - 11 - 21),
            backfill_complete: some,
        },
        area_scores: AreaId::ALL
            .iter()
            .map(|&area| AreaScore {
                area,
                score: some.then_some(55.5),
                n: 40,
                sufficient: some,
                need_more: (!some).then_some(10),
            })
            .collect(),
        metrics: vec![Metric {
            key: "w1_rate".into(),
            area: AreaId::Tactics,
            label: "Hanging pieces per opportunity".into(),
            value: 0.25,
            unit: MetricUnit::Rate,
            n: 300,
            low: some.then_some(0.2),
            high: some.then_some(0.3),
            band_median: some.then_some(0.22),
            band_percentile: some.then_some(61.0),
            sufficient: some,
            need_more: (!some).then_some(5),
        }],
        weaknesses: vec![RankedWeakness {
            id: "W3".into(),
            variant: some.then(|| "fork".into()),
            label: "Missed forks".into(),
            area: AreaId::Tactics,
            rank: 1,
            posterior_rate: 0.16,
            interval_low: 0.1,
            interval_high: 0.28,
            band_median: 0.09,
            band_percentile: 70.0,
            excess_epl_per_game: 0.7,
            confidence: 0.79,
            priority: 8.0,
            state: WeaknessState::Active,
            p_imp: some.then_some(0.4),
            events: 28,
            opportunities: 180,
            exemplars: vec![exemplar(some, false), exemplar(some, true)],
        }],
        strengths: vec![Strength {
            area: AreaId::Endgame,
            label: "Converts won endgames".into(),
            metric_key: some.then(|| "endgame_conversion".into()),
            exemplars: vec![exemplar(some, true)],
        }],
        openings: vec![OpeningVerdict {
            line_key: "white:italian-game".into(),
            name: "Italian Game".into(),
            eco: some.then(|| "C50".into()),
            color: PlayerColor::White,
            side: LineSide::Chosen,
            moves_san: vec!["e4".into(), "e5".into(), "Nf3".into()],
            games: 12,
            score: 0.5,
            performance_vs_expected: some.then_some(-0.1),
            avg_eval_at_opening_end: some.then_some(Score::Cp(25)),
            verdict: Verdict::Fix,
            deviation_ply: some.then_some(7),
            deviation_move_san: some.then(|| "Bc4".into()),
            alternatives: vec![Alternative {
                name: "Scotch Game".into(),
                moves_san: vec!["e4".into(), "e5".into(), "Nf3".into(), "Nc6".into()],
                band_score: 0.52,
                games: 3000,
            }],
            examples: vec![exemplar(some, false)],
        }],
        changes: vec![TimelineChange {
            series_key: "W1".into(),
            weakness_id: some.then(|| "W1".into()),
            label: "Hanging pieces".into(),
            from_state: WeaknessState::NotAnIssue,
            to_state: WeaknessState::Emerging,
            onset_game_id: "li-AbCd1234".into(),
            onset_date: date!(2026 - 10 - 02),
            onset_from: date!(2026 - 09 - 28),
            onset_to: date!(2026 - 10 - 06),
            rate_before: 0.05,
            rate_after: 0.12,
            posterior_prob: 0.81,
            candidate_causes: vec![CandidateCause {
                kind: CauseKind::TimeControlMix,
                description: "Most games moved from 300+0 to 180+0".into(),
                before: CauseValue::Text("300+0".into()),
                after: CauseValue::Number(0.75),
            }],
            examples: vec![
                ChangeExample {
                    position: exemplar(some, false),
                    side: ExampleSide::Before,
                },
                ChangeExample {
                    position: exemplar(some, true),
                    side: ExampleSide::After,
                },
            ],
        }],
    }
}

fn instances() -> Vec<(&'static str, Value)> {
    vec![
        ("valid-sample.json", sample_value()),
        ("full_input(Some)", full_input(true).to_json()),
        ("full_input(None)", full_input(false).to_json()),
    ]
}

#[test]
fn built_inputs_are_valid() {
    assert_eq!(full_input(true).validate(), Ok(()));
    assert_eq!(full_input(false).validate(), Ok(()));
    // The None variant really has nulls where the zod schema allows them.
    let v = full_input(false).to_json();
    assert_eq!(v["timeControl"], Value::Null);
    assert_eq!(v["openings"][0]["avgEvalAtOpeningEnd"], Value::Null);
    assert_eq!(v["changes"][0]["examples"][1]["best"], Value::Null);
}

#[test]
fn sample_validates_against_ts_schema() {
    let ts = validator(&ts_schema());
    for (name, instance) in instances() {
        let errs = errors(&ts, &instance);
        assert!(errs.is_empty(), "{name} fails the TS schema: {errs:#?}");
    }
}

#[test]
fn sample_validates_against_rust_schema() {
    let rs = validator(&json_schema());
    for (name, instance) in instances() {
        let errs = errors(&rs, &instance);
        assert!(errs.is_empty(), "{name} fails the Rust schema: {errs:#?}");
    }
}

#[test]
fn ts_schema_rejects_what_rust_rejects() {
    let ts = validator(&ts_schema());
    let rs = validator(&json_schema());
    // (case, value, JSON pointer of the instance both schemas must report)
    let mut cases: Vec<(&str, Value, &str)> = Vec::new();

    let mut v = sample_value();
    v["focus"] = json!([]);
    cases.push(("unknown root key", v, ""));
    let mut v = sample_value();
    v["changes"][0]["examples"][0]["note"] = json!("x");
    cases.push((
        "unknown key in a change example",
        v,
        "/changes/0/examples/0",
    ));
    let mut v = sample_value();
    v["weaknesses"][0]["exemplars"][0]
        .as_object_mut()
        .unwrap()
        .remove("best");
    cases.push(("missing nullable key", v, "/weaknesses/0/exemplars/0"));
    let mut v = sample_value();
    v["openings"][0]["avgEvalAtOpeningEnd"] = json!({"cp": 1, "mate": 2});
    cases.push(("score with two keys", v, "/openings/0/avgEvalAtOpeningEnd"));
    let mut v = sample_value();
    v["openings"][0]["avgEvalAtOpeningEnd"] = json!({});
    cases.push((
        "score with neither key",
        v,
        "/openings/0/avgEvalAtOpeningEnd",
    ));
    let mut v = sample_value();
    v["inputVersion"] = json!("coaching-input-v2");
    cases.push(("other input version", v, "/inputVersion"));

    for (name, v, pointer) in cases {
        for (side, validator) in [("TS", &ts), ("Rust", &rs)] {
            let paths: Vec<String> = validator
                .iter_errors(&v)
                .map(|e| e.instance_path().to_string())
                .collect();
            assert!(
                paths.iter().any(|p| p == pointer),
                "{side} schema: {name} must fail at {pointer:?}, got {paths:?}"
            );
        }
        assert!(
            serde_json::from_value::<CoachingInput>(v).is_err(),
            "Rust deserialises {name}"
        );
    }
}

#[test]
fn rust_schema_requires_every_property_and_keeps_null() {
    // schemars leaves a plain `Option` field optional; the schema transform makes every
    // property required while `Option` stays `T | null`.
    let schema = json_schema();
    let rs = validator(&schema);
    let exemplar = &schema["$defs"]["ExemplarPosition"];
    let required: BTreeSet<&str> = exemplar["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let props: BTreeSet<&str> = exemplar["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert!(required.contains("best"));
    assert_eq!(required, props);

    let mut with_null = sample_value();
    with_null["weaknesses"][0]["exemplars"][0]["best"] = Value::Null;
    with_null["rating"] = Value::Null;
    assert!(rs.is_valid(&with_null), "{:#?}", errors(&rs, &with_null));
    let mut missing = sample_value();
    missing.as_object_mut().unwrap().remove("rating");
    assert!(!rs.is_valid(&missing));
}

// ---- shape comparison --------------------------------------------------------------------

/// Bounds compared in both directions: present in one schema means present and equal in the
/// other.
const BOUNDS: &[&str] = &[
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
const STRUCTURE: &[&str] = &[
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
const ANNOTATIONS: &[&str] = &["$schema", "$defs", "title", "description"];

#[derive(Clone, Copy, Debug)]
enum Side {
    Ts,
    Rust,
}

struct Walk<'a> {
    ts_root: &'a Value,
    rs_root: &'a Value,
    diffs: Vec<String>,
    objects: usize,
    enums: usize,
}

fn walk<'a>(ts_root: &'a Value, rs_root: &'a Value) -> Walk<'a> {
    Walk {
        ts_root,
        rs_root,
        diffs: Vec::new(),
        objects: 0,
        enums: 0,
    }
}

fn branches(s: &Value) -> Option<&Vec<Value>> {
    s.get("anyOf")
        .or_else(|| s.get("oneOf"))
        .and_then(Value::as_array)
}

fn enum_set(s: &Value) -> Option<BTreeSet<String>> {
    if let Some(c) = s.get("const") {
        return Some(BTreeSet::from([c.to_string()]));
    }
    s.get("enum")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(Value::to_string).collect())
}

fn same_bound(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

fn key_set(s: &Value, key: &str) -> BTreeSet<String> {
    match s.get(key) {
        Some(Value::Object(o)) => o.keys().cloned().collect(),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| v.as_str().unwrap_or_default().to_owned())
            .collect(),
        _ => BTreeSet::new(),
    }
}

fn as_map(s: &Value) -> Map<String, Value> {
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

    fn compare(&mut self, path: &str, ts: &Value, rs: &Value) {
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

#[test]
fn schemars_schema_matches_ts_schema_shape() {
    let ts = ts_schema();
    let rs = json_schema();
    assert_eq!(
        rs["$schema"],
        json!("https://json-schema.org/draft/2020-12/schema")
    );
    let mut w = walk(&ts, &rs);
    w.compare("$", &ts, &rs);
    assert!(
        w.diffs.is_empty(),
        "schema shape differs:\n{}",
        w.diffs.join("\n")
    );
    // Guard against a walker that silently stops early. Objects: 13 (CoachingInput,
    // TimeControl, Basis, AreaScore, Metric, RankedWeakness, Strength, OpeningVerdict, its two
    // Score members, Alternative, TimelineChange, CandidateCause) plus 7 per exemplar site
    // (exemplar, played, best, two Score members each for evalBefore and evalAfter) at 4 sites.
    // Enums: 4 at the root (inputVersion, platform, timeClass, band), area x4, unit, state x3,
    // color, side x2, verdict, kind.
    assert!(w.objects >= 13 + 7 * 4, "walked only {} objects", w.objects);
    assert!(w.enums >= 17, "walked only {} enums", w.enums);
}

#[test]
fn walker_reports_unknown_keywords_and_sibling_differences() {
    fn diffs(ts: &Value, rs: &Value) -> Vec<String> {
        let mut w = walk(ts, rs);
        w.compare("$", ts, rs);
        w.diffs
    }
    fn object(a: Value) -> Value {
        json!({
            "$defs": {"A": {"type": "string", "maxLength": 3}},
            "type": "object",
            "properties": {"a": a},
            "required": ["a"],
            "additionalProperties": false
        })
    }
    fn has(diffs: &[String], needle: &str) -> bool {
        diffs.iter().any(|d| d.contains(needle))
    }
    let base = object(json!({"type": "string", "maxLength": 3}));
    assert_eq!(diffs(&base, &base), Vec::<String>::new());

    // A keyword the walker does not compare, on either side.
    for keyword in ["uniqueItems", "allOf", "not", "prefixItems", "default"] {
        let mut other = base.clone();
        other["properties"]["a"][keyword] = json!(true);
        let d = diffs(&base, &other);
        assert!(has(&d, &format!("Rust uses keyword {keyword:?}")), "{d:?}");
        let d = diffs(&other, &base);
        assert!(has(&d, &format!("Ts uses keyword {keyword:?}")), "{d:?}");
    }

    // A `$ref` with its target's keywords matches the inline form; a sibling bound counts.
    let by_ref = object(json!({"$ref": "#/$defs/A", "description": "x"}));
    assert_eq!(diffs(&base, &by_ref), Vec::<String>::new());
    let sibling = object(json!({"$ref": "#/$defs/A", "minLength": 1}));
    assert!(has(&diffs(&base, &sibling), "$.a: minLength"));
    let conflict = object(json!({"$ref": "#/$defs/A", "maxLength": 4}));
    assert!(has(&diffs(&base, &conflict), "conflicts with 3"));

    // Keywords beside a union are compared, and so are those beside a nullable member.
    let union = object(json!({"anyOf": [{"type": "string"}, {"type": "number"}]}));
    let mut union_bound = union.clone();
    union_bound["properties"]["a"]["maxLength"] = json!(5);
    assert!(has(&diffs(&union, &union_bound), "$.a: maxLength"));
    let nullable = object(json!({"anyOf": [{"$ref": "#/$defs/A"}, {"type": "null"}]}));
    let inline = object(json!({"type": ["string", "null"], "maxLength": 3}));
    assert_eq!(diffs(&nullable, &inline), Vec::<String>::new());
    let mut nullable_bound = nullable.clone();
    nullable_bound["properties"]["a"]["minLength"] = json!(1);
    assert!(has(&diffs(&nullable, &nullable_bound), "$.a: minLength"));
}

#[test]
fn committed_schema_is_current() {
    let path = format!(
        "{}/schema/coaching-input-v1.schema.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let generated = json_schema();
    if std::env::var("COACHING_SCHEMA_WRITE").as_deref() == Ok("1") {
        let text = serde_json::to_string_pretty(&generated).unwrap() + "\n";
        std::fs::create_dir_all(std::path::Path::new(&path).parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
    }
    let committed = read_json(&path);
    assert!(
        generated == committed,
        "schema/coaching-input-v1.schema.json is stale: regenerate with COACHING_SCHEMA_WRITE=1 \
         cargo test -p coaching-input --test parity committed_schema_is_current"
    );
}
