//! M5b Task B (G2): the committed JSON Schema of `WeeklyReviewInput` (TS plan B4), which the
//! weekly-input builder in the `profile` crate validates its output against. It is generated
//! the way the coaching-input contract schema is (`coaching_input::schema`): every property
//! required, `Option` fields nullable.

use ai::output::weekly::weekly_input_json_schema;
use serde_json::Value;

const PATH: &str = "../../fixtures/ai/input/weekly-review-input.schema.json";

#[test]
fn committed_weekly_input_schema_is_current() {
    let path = format!("{}/{PATH}", env!("CARGO_MANIFEST_DIR"));
    let generated = weekly_input_json_schema();
    if std::env::var("WEEKLY_SCHEMA_WRITE").as_deref() == Ok("1") {
        let text = serde_json::to_string_pretty(&generated).unwrap() + "\n";
        std::fs::write(&path, text).unwrap();
    }
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let committed: Value = serde_json::from_str(&text).unwrap();
    assert!(
        generated == committed,
        "{PATH} is stale: regenerate with WEEKLY_SCHEMA_WRITE=1 \
         cargo test -p ai --test weekly_input_schema"
    );
}

#[test]
fn weekly_input_schema_requires_every_b4_field() {
    let s = weekly_input_json_schema();
    let mut required: Vec<&str> = s["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    required.sort_unstable();
    let mut want = vec![
        "inputVersion",
        "timeClass",
        "band",
        "weekStart",
        "weekEnd",
        "gamesThisWeek",
        "basis",
        "adherence",
        "metricDeltas",
        "focusUpdates",
        "exemplars",
        "changes",
    ];
    want.sort_unstable();
    assert_eq!(required, want);
    assert_eq!(s["additionalProperties"], Value::Bool(false));
    assert_eq!(s["properties"]["exemplars"]["maxItems"], 2);
}
