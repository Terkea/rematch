//! coaching-input-v3 (ADR 0089): the rating `trajectory` and then against now, on top of v2.
//! v1 and v2 inputs (stored before) still parse and cannot carry it.

#[path = "support/v2.rs"]
mod v2;
#[path = "support/v3.rs"]
mod v3;

use coaching_input::{
    ChangeVerdict, CoachingInput, INPUT_VERSION, InputVersion, WindowBasis, json_schema,
};
use serde_json::{Value, json};
use time::macros::date;

fn sample_value() -> Value {
    let path = format!(
        "{}/../../fixtures/coaching-input/valid-sample.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn v3_input() -> CoachingInput {
    let mut i: CoachingInput = serde_json::from_value(sample_value()).unwrap();
    i.input_version = InputVersion::V3;
    i.by_color = Some(v2::by_color());
    i.benchmark = Some(v2::benchmark());
    i.trajectory = Some(v3::trajectory());
    i
}

fn paths(r: Result<(), Vec<coaching_input::Issue>>) -> Vec<String> {
    r.err()
        .unwrap_or_default()
        .into_iter()
        .map(|i| i.path)
        .collect()
}

#[test]
fn v3_input_validates_and_round_trips() {
    let input = v3_input();
    assert_eq!(input.validate(), Ok(()));
    let json = input.to_json();
    assert_eq!(json["inputVersion"], json!(INPUT_VERSION));
    let t = &json["trajectory"];
    assert_eq!(t["peak"], json!({"rating": 1097, "date": "2026-04-18"}));
    assert_eq!(t["change"], json!(-281));
    assert_eq!(t["thenVsNow"]["then"]["basis"], json!("peak"));
    assert_eq!(t["thenVsNow"]["rating"]["then"]["unit"], json!("elo"));
    assert_eq!(
        t["thenVsNow"]["accuracy"]["verdict"],
        json!("no_clear_change")
    );
    assert_eq!(t["thenVsNow"]["weaknesses"][0]["now"]["value"], json!(0.43));
    let back: CoachingInput = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(back, input);
    let schema = jsonschema::options()
        .should_validate_formats(true)
        .build(&json_schema())
        .unwrap();
    let errors: Vec<String> = schema.iter_errors(&json).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn v3_needs_a_trajectory_and_earlier_versions_cannot_carry_one() {
    let mut i = v3_input();
    i.trajectory = None;
    assert_eq!(paths(i.validate()), vec!["trajectory"]);
    let mut i = v3_input();
    i.input_version = InputVersion::V2;
    assert_eq!(paths(i.validate()), vec!["trajectory"]);
}

#[test]
fn an_insufficient_trajectory_states_nothing_but_its_shortfall() {
    let mut i = v3_input();
    i.trajectory = Some(v3::short());
    assert_eq!(i.validate(), Ok(()));
    let mut t = v3::short();
    t.change = Some(-10);
    t.need_more = None;
    i.trajectory = Some(t);
    assert_eq!(
        paths(i.validate()),
        vec!["trajectory.needMore", "trajectory.change"]
    );
}

#[test]
fn the_windows_are_ordered_and_named_by_their_role() {
    let mut i = v3_input();
    let t = i.trajectory.as_mut().unwrap();
    let v = t.then_vs_now.as_mut().unwrap();
    v.then.to = date!(2026 - 10 - 30);
    v.now.basis = WindowBasis::Peak;
    assert_eq!(
        paths(i.validate()),
        vec![
            "trajectory.thenVsNow.now.basis",
            "trajectory.thenVsNow.then.to"
        ]
    );
}

#[test]
fn a_verdict_carries_its_difference_and_not_enough_games_none() {
    let mut i = v3_input();
    let v = i.trajectory.as_mut().unwrap().then_vs_now.as_mut().unwrap();
    v.score.verdict = ChangeVerdict::NotEnoughGames;
    v.accuracy.diff = None;
    assert_eq!(
        paths(i.validate()),
        vec![
            "trajectory.thenVsNow.accuracy.diff",
            "trajectory.thenVsNow.score.diff",
            "trajectory.thenVsNow.score.diffLow",
            "trajectory.thenVsNow.score.diffHigh",
        ]
    );
}

#[test]
fn a_weakness_rate_is_a_share_with_its_counts() {
    let mut i = v3_input();
    let w = &mut i
        .trajectory
        .as_mut()
        .unwrap()
        .then_vs_now
        .as_mut()
        .unwrap()
        .weaknesses[0];
    w.now_events = 90;
    w.then.value = Some(1.4);
    assert_eq!(
        paths(i.validate()),
        vec![
            "trajectory.thenVsNow.weaknesses.0.then.value",
            "trajectory.thenVsNow.weaknesses.0.then.value",
            "trajectory.thenVsNow.weaknesses.0.nowEvents",
        ]
    );
}

#[test]
fn months_are_in_order_and_in_range() {
    let mut i = v3_input();
    let t = i.trajectory.as_mut().unwrap();
    t.months.reverse();
    t.months[0].month = "2026-13".into();
    assert_eq!(
        paths(i.validate()),
        vec!["trajectory.months.0.month", "trajectory.months.1.month"]
    );
}

#[test]
fn the_schema_keeps_trajectory_optional() {
    let schema = json_schema();
    let required = schema["required"].as_array().unwrap();
    assert!(!required.contains(&json!("trajectory")));
    assert!(schema["properties"]["trajectory"].is_object());
    assert_eq!(
        schema["properties"]["inputVersion"]["enum"],
        json!([
            "coaching-input-v1",
            "coaching-input-v2",
            "coaching-input-v3"
        ])
    );
}
