//! Port of `packages/ai/test/output/output.test.ts` (m3), plus the R3 additions of Task 3: the
//! structured-output subset guard, the envelope shape against the TS export, the client-side
//! item text cap and `change_key`.

#[path = "../../coaching-input/tests/support/shape.rs"]
mod shape;

use ai::client::port::{OutputSchema, ParseIssue};
use ai::input::types::TimelineChange;
use ai::output::change::{WeaknessChangeOutput, change_key};
use ai::output::disclosure::{AI_DISCLOSURE_LABEL, AI_DISCLOSURE_NOTE, make_disclosure};
use ai::output::drills::{DrillType, drills_for};
use ai::output::report::ReportSectionEnvelope;
use ai::output::schema::{
    ITEM_TEXT_MAX_CHARS, REPORT_SECTION_SCHEMA, WEAKNESS_CHANGE_SCHEMA, WEEKLY_REVIEW_SCHEMA,
    api_schema, assert_api_subset, schema_by_id,
};
use ai::output::weekly::WeeklyReviewOutput;
use ai::testing::{fixtures_dir, input};
use serde_json::{Value, json};
use time::macros::datetime;

fn item() -> Value {
    json!({ "text": "x", "citations": [{ "gameId": "g1", "ply": 3 }] })
}

/// `{ ...empty, section, <key>: part }`.
fn envelope(section: &str, key: &str, part: Value) -> Value {
    let mut v = json!({
        "section": section,
        "overview": null, "weaknesses": null, "openings": null, "climbPlan": null, "areas": null
    });
    if !key.is_empty() {
        v[key] = part;
    }
    v
}

fn focus() -> Value {
    json!({
        "weaknessId": "W1",
        "why": item(),
        "drills": [{ "drillType": "own_mistake_puzzles", "rationale": "r" }],
        "habit": item(),
    })
}

fn plan(n: usize) -> Value {
    json!({ "intro": item(), "focuses": vec![focus(); n], "weeklyRhythm": item() })
}

fn check(schema: &OutputSchema, v: &Value) -> Result<(), Vec<ParseIssue>> {
    (schema.check)(v)
}

fn codes(r: Result<(), Vec<ParseIssue>>) -> Vec<(String, String)> {
    r.err()
        .unwrap_or_default()
        .into_iter()
        .map(|i| (i.path, i.code))
        .collect()
}

// --- describe("ReportSectionEnvelope") ---

#[test]
fn parses_a_weaknesses_envelope_with_other_parts_null() {
    let mut w = item();
    w["weaknessId"] = json!("W1");
    w["title"] = json!("t");
    let v = envelope("weaknesses", "weaknesses", json!({ "items": [w] }));
    assert_eq!(check(&REPORT_SECTION_SCHEMA, &v), Ok(()));
    let typed: ReportSectionEnvelope = serde_json::from_value(v.clone()).unwrap();
    assert!(typed.overview.is_none() && typed.weaknesses.is_some());
    // Every part is serialised, `null` included.
    assert_eq!(serde_json::to_value(&typed).unwrap(), v);
}

#[test]
fn accepts_3_focuses_rejects_4() {
    let mk = |n| {
        check(
            &REPORT_SECTION_SCHEMA,
            &envelope("climb_plan", "climbPlan", plan(n)),
        )
    };
    assert_eq!(mk(3), Ok(()));
    assert_eq!(
        codes(mk(4)),
        vec![("climbPlan.focuses".to_string(), "too_big".to_string())]
    );
}

#[test]
fn rejects_unknown_keys_and_5_strengths() {
    let mut extra = envelope("areas", "", Value::Null);
    extra["extra"] = json!(1);
    assert_eq!(
        codes(check(&REPORT_SECTION_SCHEMA, &extra)),
        // serde_path_to_error names the unknown key itself.
        vec![("extra".to_string(), "unknown_field".to_string())]
    );
    let mut s = item();
    s["area"] = json!("tactics");
    let ov = json!({ "headline": item(), "summary": item(), "strengths": vec![s; 5] });
    assert_eq!(
        codes(check(
            &REPORT_SECTION_SCHEMA,
            &envelope("overview", "overview", ov)
        )),
        vec![("overview.strengths".to_string(), "too_big".to_string())]
    );
    // A missing nullable part is an error too (present-but-nullable, as in zod).
    let mut missing = envelope("areas", "", Value::Null);
    missing.as_object_mut().unwrap().remove("openings");
    assert_eq!(
        codes(check(&REPORT_SECTION_SCHEMA, &missing)),
        vec![("openings".to_string(), "missing_field".to_string())]
    );
}

/// Walks every node; counts objects and checks each has `additionalProperties: false`.
fn walk_objects(n: &Value, objects: &mut usize) {
    match n {
        Value::Array(a) => a.iter().for_each(|x| walk_objects(x, objects)),
        Value::Object(o) => {
            if o.get("type") == Some(&json!("object")) {
                *objects += 1;
                assert_eq!(o.get("additionalProperties"), Some(&json!(false)), "{n}");
            }
            o.values().for_each(|x| walk_objects(x, objects));
        }
        _ => {}
    }
}

#[test]
fn json_schema_additional_properties_false_on_every_object_no_ref() {
    let js = (REPORT_SECTION_SCHEMA.json)();
    assert!(!js.to_string().contains("$ref"));
    assert!(!js.to_string().contains("$defs"));
    let mut objects = 0;
    walk_objects(&js, &mut objects);
    assert!(objects > 5, "only {objects} objects");
}

// --- describe("drillsFor") ---

#[test]
fn w12_is_session_rules_only() {
    assert_eq!(drills_for("W12", None), vec![DrillType::SessionRules]);
}

#[test]
fn w3_deep() {
    assert!(drills_for("W3", Some("deep")).contains(&DrillType::LongCalculation));
    assert!(!drills_for("W3", Some("deep")).contains(&DrillType::ThemedPuzzles));
    assert!(drills_for("W3", None).contains(&DrillType::ThemedPuzzles));
}

#[test]
fn w1_includes_own_game_analysis() {
    assert!(drills_for("W1", None).contains(&DrillType::OwnGameAnalysis));
}

#[test]
fn every_id_returns_a_non_empty_list_of_known_drill_types() {
    let ids = [
        "W1", "W2", "W3", "W4", "W5", "W6", "W7", "W8", "W9a", "W9b", "W9c", "W9d", "W10", "W11",
        "W12",
    ];
    for id in ids {
        let d = drills_for(id, None);
        assert!(!d.is_empty(), "{id}");
        for x in d {
            assert!(DrillType::ALL.contains(&x));
        }
    }
    assert_eq!(
        drills_for("W9b", None),
        vec![
            DrillType::BlitzSafety,
            DrillType::ClockPressure,
            DrillType::OwnGameAnalysis
        ]
    );
    // The 18 TS values, in DRILL_TYPES order, snake_case on the wire.
    let wire: Vec<Value> = DrillType::ALL
        .iter()
        .map(|d| serde_json::to_value(d).unwrap())
        .collect();
    assert_eq!(
        wire,
        json!([
            "own_mistake_puzzles",
            "safe_unsafe",
            "themed_puzzles",
            "long_calculation",
            "mate_patterns",
            "convert_vs_engine",
            "defend_vs_engine",
            "opening_trainer",
            "opening_trap_puzzles",
            "tablebase_drills",
            "clock_pressure",
            "critical_or_not",
            "blitz_safety",
            "model_games",
            "guess_the_move",
            "session_rules",
            "slow_games",
            "own_game_analysis"
        ])
        .as_array()
        .unwrap()
        .clone()
    );
}

#[test]
fn unknown_id_yields_empty_list() {
    assert_eq!(drills_for("W99", None), Vec::<DrillType>::new());
}

// --- describe("disclosure") ---

#[test]
fn make_disclosure_test() {
    let d = make_disclosure(
        "claude-sonnet-5-5",
        "report@1",
        datetime!(2026-11-21 10:00:00 UTC),
    );
    assert_eq!(
        serde_json::to_value(&d).unwrap(),
        json!({
            "label": "Written by AI",
            "note": AI_DISCLOSURE_NOTE,
            "model": "claude-sonnet-5-5",
            "promptVersion": "report@1",
            "generatedAt": "2026-11-21T10:00:00.000Z",
        })
    );
    assert_eq!(d.generated_at, "2026-11-21T10:00:00.000Z");
    assert_eq!(AI_DISCLOSURE_LABEL, "Written by AI");
    assert_eq!(
        AI_DISCLOSURE_NOTE,
        "This coaching was written by an AI model (Claude) from your analysed games. It can contain mistakes. The engine data shown alongside it is the source of truth."
    );
}

// --- R3 additions ---

const ALLOWED: &[&str] = &[
    "type",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "enum",
    "const",
    "anyOf",
    "description",
];

/// The test's own walk (independent of `assert_api_subset`).
fn walk_subset(n: &Value, path: &str, nodes: &mut usize) {
    let o = n
        .as_object()
        .unwrap_or_else(|| panic!("{path}: not an object"));
    *nodes += 1;
    for k in o.keys() {
        assert!(ALLOWED.contains(&k.as_str()), "{path}: keyword {k}");
    }
    if let Some(ap) = o.get("additionalProperties") {
        assert_eq!(ap, &json!(false), "{path}");
    }
    if let Some(Value::Object(props)) = o.get("properties") {
        let mut names: Vec<&str> = props.keys().map(String::as_str).collect();
        let mut req: Vec<&str> = o["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        names.sort_unstable();
        req.sort_unstable();
        assert_eq!(names, req, "{path}: required");
        for (k, p) in props {
            walk_subset(p, &format!("{path}.{k}"), nodes);
        }
    }
    if let Some(items) = o.get("items") {
        walk_subset(items, &format!("{path}[]"), nodes);
    }
    if let Some(Value::Array(any)) = o.get("anyOf") {
        for (i, m) in any.iter().enumerate() {
            walk_subset(m, &format!("{path}|{i}"), nodes);
        }
    }
}

#[test]
fn api_subset_guard_walks_every_node() {
    for schema in [
        &REPORT_SECTION_SCHEMA,
        &WEEKLY_REVIEW_SCHEMA,
        &WEAKNESS_CHANGE_SCHEMA,
    ] {
        let js = (schema.json)();
        assert_eq!(assert_api_subset(&js), Ok(()), "{}", schema.id);
        let mut nodes = 0;
        walk_subset(&js, "$", &mut nodes);
        assert!(nodes > 5, "{}: walked only {nodes} nodes", schema.id);
        assert_eq!(
            schema_by_id(schema.id, schema.version).map(|s| s.id),
            Some(schema.id)
        );
    }
    assert_eq!(REPORT_SECTION_SCHEMA.id, "report-section");
    assert_eq!(WEEKLY_REVIEW_SCHEMA.id, "weekly-review");
    assert_eq!(WEAKNESS_CHANGE_SCHEMA.id, "weakness-change");
    assert!(schema_by_id("report-section", 2).is_none());
    assert!(schema_by_id("nope", 1).is_none());
    // Each schema is the sanitized schemars output of its type.
    assert_eq!(
        (REPORT_SECTION_SCHEMA.json)(),
        api_schema::<ReportSectionEnvelope>()
    );
    assert_eq!(
        (WEEKLY_REVIEW_SCHEMA.json)(),
        api_schema::<WeeklyReviewOutput>()
    );
    assert_eq!(
        (WEAKNESS_CHANGE_SCHEMA.json)(),
        api_schema::<WeaknessChangeOutput>()
    );
}

#[test]
fn guard_rejects_ref_and_constraints() {
    let obj = |prop: Value| {
        json!({
            "type": "object",
            "properties": { "a": prop },
            "required": ["a"],
            "additionalProperties": false
        })
    };
    assert_eq!(assert_api_subset(&obj(json!({ "type": "string" }))), Ok(()));
    let bad = [
        obj(json!({ "$ref": "#/$defs/A" })),
        obj(json!({ "type": "string", "minLength": 1 })),
        obj(json!({ "type": "integer", "minimum": 0 })),
        obj(json!({ "type": "array", "items": { "type": "string" }, "maxItems": 3 })),
        obj(json!({ "type": ["string", "null"] })),
        obj(
            json!({ "type": "object", "properties": {}, "required": [], "additionalProperties": true }),
        ),
        obj(
            json!({ "type": "object", "properties": { "b": { "type": "string" } }, "required": [], "additionalProperties": false }),
        ),
        obj(json!({ "type": "object", "properties": {}, "required": [] })),
        json!({ "type": "object", "properties": {}, "required": [], "additionalProperties": false, "$defs": {} }),
    ];
    for b in bad {
        assert!(assert_api_subset(&b).is_err(), "accepted {b}");
    }
}

#[test]
fn envelope_shape_matches_ts_export() {
    let path = fixtures_dir().join("ts-export/report-section-envelope.schema.json");
    let ts: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(ts["source"], "m3/base@4ff8220");
    let mut ts_schema = ts["schema"].clone();
    // The SDK keeps `minItems: 1` on `drills` (the API accepts minItems 0 and 1). We drop every
    // array bound from the API schema and check it client-side instead (ADR-R3-4), so that one
    // keyword is removed from the TS side before the comparison.
    let drills = ts_schema
        .pointer_mut("/properties/climbPlan/anyOf/0/properties/focuses/items/properties/drills")
        .unwrap();
    assert_eq!(
        drills.as_object_mut().unwrap().remove("minItems"),
        Some(json!(1))
    );
    let rs = api_schema::<ReportSectionEnvelope>();
    let w = shape::compare_schemas(&ts_schema, &rs);
    assert!(
        w.diffs.is_empty(),
        "schema shape differs:\n{}",
        w.diffs.join("\n")
    );
    assert!(w.objects >= 20, "walked only {} objects", w.objects);
    assert!(w.enums >= 5, "walked only {} enums", w.enums);
}

#[test]
fn item_text_over_2000_chars_is_too_big() {
    assert_eq!(ITEM_TEXT_MAX_CHARS, 2000);
    let with_text = |t: String| {
        let mut ov = json!({ "headline": item(), "summary": item(), "strengths": [] });
        ov["summary"]["text"] = json!(t);
        envelope("overview", "overview", ov)
    };
    // Code points, not bytes: 2000 two-byte characters are fine.
    assert_eq!(
        check(&REPORT_SECTION_SCHEMA, &with_text("é".repeat(2000))),
        Ok(())
    );
    assert_eq!(
        codes(check(&REPORT_SECTION_SCHEMA, &with_text("x".repeat(2001)))),
        vec![("overview.summary.text".to_string(), "too_big".to_string())]
    );
    assert_eq!(
        codes(check(&REPORT_SECTION_SCHEMA, &with_text(String::new()))),
        vec![("overview.summary.text".to_string(), "too_small".to_string())]
    );
    // The same cap on weekly and change outputs.
    let weekly = json!({
        "headline": item(), "process": item(), "outcomes": [item(), item(), item(), item()],
        "exemplars": [], "nextWeek": { "text": "x".repeat(2001), "citations": [] }
    });
    assert_eq!(
        codes(check(&WEEKLY_REVIEW_SCHEMA, &weekly)),
        vec![
            ("outcomes".to_string(), "too_big".to_string()),
            ("nextWeek.text".to_string(), "too_big".to_string()),
        ]
    );
    let change = json!({ "text": "x".repeat(2001), "citations": [] });
    assert_eq!(
        codes(check(&WEAKNESS_CHANGE_SCHEMA, &change)),
        vec![("text".to_string(), "too_big".to_string())]
    );
    // Drills: 1 to 3.
    let mut p = plan(1);
    p["focuses"][0]["drills"] = json!([]);
    assert_eq!(
        codes(check(
            &REPORT_SECTION_SCHEMA,
            &envelope("climb_plan", "climbPlan", p)
        )),
        vec![(
            "climbPlan.focuses.0.drills".to_string(),
            "too_small".to_string()
        )]
    );
}

#[test]
fn change_key_format() {
    let b2 = input("b2-rapid");
    let c: &TimelineChange = b2.changes.first().expect("b2-rapid has a change");
    assert_eq!(
        change_key(c),
        format!("{}@{}", c.series_key, c.onset_game_id)
    );
    let mut c = c.clone();
    c.series_key = "W1:rate".into();
    c.onset_game_id = "g42".into();
    assert_eq!(change_key(&c), "W1:rate@g42");
}
