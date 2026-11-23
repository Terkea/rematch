//! coaching-input-v2 (ADR 0085): the colour split `byColor` and the `benchmark`, on top of the
//! v1 contract. v1 inputs (stored before) still parse and serialise byte for byte.

#[path = "support/v2.rs"]
mod v2;

use coaching_input::{
    CoachingInput, INPUT_VERSION, INPUT_VERSION_V1, INPUT_VERSION_V2, InputVersion, MetricUnit,
    json_schema,
};
use serde_json::{Value, json};

fn sample_value() -> Value {
    let path = format!(
        "{}/../../fixtures/coaching-input/valid-sample.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn v1() -> CoachingInput {
    serde_json::from_value(sample_value()).unwrap()
}

fn v2() -> CoachingInput {
    let mut i = v1();
    i.input_version = InputVersion::V2;
    i.by_color = Some(v2::by_color());
    i.benchmark = Some(v2::benchmark());
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
fn versions() {
    assert_eq!(INPUT_VERSION, "coaching-input-v3");
    assert_eq!(INPUT_VERSION_V2, "coaching-input-v2");
    assert_eq!(INPUT_VERSION_V1, "coaching-input-v1");
    assert_eq!(InputVersion::default(), InputVersion::V3);
    assert_eq!(InputVersion::V1.as_str(), INPUT_VERSION_V1);
    assert_eq!(
        serde_json::to_value(InputVersion::V2).unwrap(),
        json!(INPUT_VERSION_V2)
    );
    let mut v = sample_value();
    v["inputVersion"] = json!("coaching-input-v9");
    assert!(serde_json::from_value::<CoachingInput>(v).is_err());
}

#[test]
fn v1_input_round_trips_without_the_v2_keys() {
    let input = v1();
    assert_eq!(input.input_version, InputVersion::V1);
    assert!(input.by_color.is_none() && input.benchmark.is_none());
    assert_eq!(input.validate(), Ok(()));
    let json = input.to_json();
    assert!(json.get("byColor").is_none());
    assert!(json.get("benchmark").is_none());
}

#[test]
fn v1_input_cannot_carry_v2_fields() {
    let mut i = v1();
    i.by_color = Some(v2::by_color());
    i.benchmark = Some(v2::benchmark());
    assert_eq!(paths(i.validate()), vec!["byColor", "benchmark"]);
}

#[test]
fn v2_input_validates_and_round_trips() {
    let input = v2();
    assert_eq!(input.validate(), Ok(()));
    let json = input.to_json();
    assert_eq!(json["inputVersion"], json!("coaching-input-v2"));
    assert_eq!(json["byColor"]["white"]["wins"], json!(20));
    assert_eq!(
        json["byColor"]["white"]["blundersPerGame"]["unit"],
        json!("per_game")
    );
    // Insufficient numbers are null, never 0.
    assert_eq!(json["byColor"]["black"]["score"]["value"], Value::Null);
    assert_eq!(json["byColor"]["black"]["needMore"], json!(3));
    assert_eq!(json["benchmark"]["ratingLow"], json!(675));
    let back: CoachingInput = serde_json::from_value(json).unwrap();
    assert_eq!(back, input);
}

#[test]
fn v2_needs_by_color() {
    let mut i = v2();
    i.by_color = None;
    assert_eq!(paths(i.validate()), vec!["byColor"]);
    // The benchmark is optional (none stored).
    let mut i = v2();
    i.benchmark = None;
    assert_eq!(i.validate(), Ok(()));
}

#[test]
fn insufficient_colour_states_no_number() {
    let mut i = v2();
    let b = i.by_color.as_mut().unwrap();
    b.black.score.value = Some(0.0);
    b.black.accuracy = v2::metric(Some((70.0, 60.0, 80.0)), MetricUnit::Pct, 7, 20);
    assert_eq!(
        paths(i.validate()),
        vec![
            "byColor.black.score.value",
            "byColor.black.accuracy.sufficient"
        ]
    );
}

#[test]
fn colour_refinements() {
    let mut i = v2();
    let w = &mut i.by_color.as_mut().unwrap().white;
    w.wins += 1; // W-D-L no longer adds up to games
    w.score.value = Some(0.7); // outside its interval
    w.accuracy.unit = MetricUnit::Rate;
    w.blunders_per_game.need_more = Some(2); // sufficient with needMore
    w.weaknesses[0].rate = 0.5; // outside [low, high]
    w.weaknesses[1].peer_players = Some(3); // without a peer rate
    w.weaknesses[1].series_key = "W4.missed".into();
    w.sufficient = false; // 42 games are enough
    let got = paths(i.validate());
    for want in [
        "byColor.white.games",
        "byColor.white.sufficient",
        "byColor.white.needMore",
        "byColor.white.score.value",
        "byColor.white.accuracy.unit",
        "byColor.white.blundersPerGame.needMore",
        "byColor.white.weaknesses.0.rate",
        "byColor.white.weaknesses.1.peerRate",
        "byColor.white.weaknesses.1.seriesKey",
    ] {
        assert!(got.iter().any(|p| p == want), "{want} not in {got:?}");
    }
}

#[test]
fn peers_benchmark_states_its_band() {
    let mut i = v2();
    let b = i.benchmark.as_mut().unwrap();
    b.players = None;
    b.rating_low = Some(1000);
    let got = paths(i.validate());
    assert!(got.contains(&"benchmark".to_owned()), "{got:?}");
    assert!(got.contains(&"benchmark.ratingLow".to_owned()), "{got:?}");
}

#[test]
fn schema_accepts_v1_and_v2_and_keeps_v2_keys_optional() {
    let schema = json_schema();
    let required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(!required.contains(&"byColor") && !required.contains(&"benchmark"));
    assert!(required.contains(&"changes"));
    assert!(schema.to_string().find("x-optional").is_none());
    let v = jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .unwrap();
    for (name, instance) in [("v1", v1().to_json()), ("v2", v2().to_json())] {
        let errs: Vec<String> = v.iter_errors(&instance).map(|e| e.to_string()).collect();
        assert!(errs.is_empty(), "{name}: {errs:#?}");
    }
    let mut bad = v2().to_json();
    bad["byColor"]["white"]["extra"] = json!(1);
    assert!(!v.is_valid(&bad));
    let mut bad = v2().to_json();
    bad["byColor"]["black"]
        .as_object_mut()
        .unwrap()
        .remove("score");
    assert!(!v.is_valid(&bad));
}
