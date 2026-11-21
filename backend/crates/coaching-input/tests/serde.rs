//! Wire shape of the contract types: the M3 zod objects are strict, every key is present, and
//! nullable fields are `null` (never skipped).

use coaching_input::{
    CauseKind, CoachingInput, ExampleSide, INPUT_VERSION, InputVersion, LineSide, MetricUnit,
    Platform, PlayerColor, Score, TimeClass, Verdict,
};
use serde_json::{Value, json};

fn sample_value() -> Value {
    let path = format!(
        "{}/../../fixtures/coaching-input/valid-sample.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).expect("valid-sample.json is JSON")
}

fn sample() -> CoachingInput {
    serde_json::from_value(sample_value()).expect("valid-sample.json deserialises")
}

/// JSON has one number type: `48` (JS `JSON.stringify`) and `48.0` (serde_json for an `f64`)
/// are the same value. Compare numbers as `f64` so the round trip checks keys and values only.
fn canonical(v: &Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().expect("finite number")),
        Value::Array(a) => Value::Array(a.iter().map(canonical).collect()),
        Value::Object(o) => {
            Value::Object(o.iter().map(|(k, v)| (k.clone(), canonical(v))).collect())
        }
        other => other.clone(),
    }
}

#[test]
fn score_shapes() {
    assert_eq!(
        serde_json::to_value(Score::Cp(-30)).unwrap(),
        json!({"cp": -30})
    );
    assert_eq!(
        serde_json::to_value(Score::Mate(2)).unwrap(),
        json!({"mate": 2})
    );
    assert_eq!(
        serde_json::from_value::<Score>(json!({"cp": -30})).unwrap(),
        Score::Cp(-30)
    );
    assert_eq!(
        serde_json::from_value::<Score>(json!({"mate": 2})).unwrap(),
        Score::Mate(2)
    );
    // zod's union members are strict objects: one key, an integer.
    assert!(serde_json::from_value::<Score>(json!({"cp": 1, "mate": 2})).is_err());
    assert!(serde_json::from_value::<Score>(json!({"cp": 1.5})).is_err());
    assert!(serde_json::from_value::<Score>(json!({"pawns": 1})).is_err());
    assert!(serde_json::from_value::<Score>(json!({})).is_err());
}

/// Every new contract enum has `ALL`, `as_str`, `Display` and `FromStr` like the Task 1 enums,
/// and its serde name is `as_str`.
#[test]
fn contract_enums_round_trip_their_wire_names() {
    fn check<T>(all: &[T], kind: &str)
    where
        T: Copy
            + std::fmt::Debug
            + PartialEq
            + std::fmt::Display
            + std::str::FromStr
            + serde::Serialize
            + serde::de::DeserializeOwned,
        <T as std::str::FromStr>::Err: std::fmt::Debug,
    {
        for &v in all {
            let wire = v.to_string();
            assert_eq!(serde_json::to_value(v).unwrap(), json!(wire), "{kind}");
            assert_eq!(serde_json::from_value::<T>(json!(wire)).unwrap(), v);
            assert_eq!(wire.parse::<T>().unwrap(), v);
        }
        assert!("nope".parse::<T>().is_err(), "{kind}");
    }
    check(&Platform::ALL, "platform");
    check(&TimeClass::ALL, "time class");
    check(&PlayerColor::ALL, "player color");
    check(&LineSide::ALL, "line side");
    check(&Verdict::ALL, "verdict");
    check(&CauseKind::ALL, "cause kind");
    check(&ExampleSide::ALL, "example side");
    check(&MetricUnit::ALL, "metric unit");
    assert_eq!(MetricUnit::PerGame.as_str(), "per_game");
    assert_eq!(CauseKind::TimeControlMix.as_str(), "time_control_mix");
    assert_eq!(Platform::Chesscom.as_str(), "chesscom");
}

#[test]
fn chess_core_conversions_cover_every_variant() {
    use chess_core::{Color, TimeClass as CoreTimeClass};
    let core = [
        CoreTimeClass::Bullet,
        CoreTimeClass::Blitz,
        CoreTimeClass::Rapid,
        CoreTimeClass::Classical,
        CoreTimeClass::Daily,
    ];
    let mapped: Vec<TimeClass> = core.iter().map(|&tc| TimeClass::from(tc)).collect();
    assert_eq!(mapped, TimeClass::ALL);
    for tc in core {
        assert_eq!(TimeClass::from(tc).as_str(), tc.as_str());
    }
    assert_eq!(PlayerColor::from(Color::White), PlayerColor::White);
    assert_eq!(PlayerColor::from(Color::Black), PlayerColor::Black);
    for c in [Color::White, Color::Black] {
        assert_eq!(PlayerColor::from(c).as_str(), c.as_str());
    }
}

#[test]
fn option_fields_serialise_as_null() {
    let mut input = sample();
    input.weaknesses[0].exemplars[0].best = None;
    input.rating = None;
    input.time_control = None;
    let v = input.to_json();
    let exemplar = v["weaknesses"][0]["exemplars"][0].as_object().unwrap();
    assert!(exemplar.contains_key("best"), "best must be present");
    assert_eq!(exemplar["best"], Value::Null);
    let root = v.as_object().unwrap();
    assert_eq!(root["rating"], Value::Null);
    assert_eq!(root["timeControl"], Value::Null);
    // The sample's own nulls survive as nulls too.
    assert_eq!(v["weaknesses"][0]["variant"], Value::Null);
    assert_eq!(v["openings"][0]["deviationPly"], Value::Null);
    assert_eq!(v["areaScores"][0]["needMore"], Value::Null);
}

#[test]
fn unknown_key_is_rejected() {
    let mut v = sample_value();
    v["focus"] = json!([]);
    let err = serde_json::from_value::<CoachingInput>(v).unwrap_err();
    assert!(err.to_string().contains("focus"), "{err}");

    // Nested objects are strict too, including the flattened change example.
    let mut v = sample_value();
    v["weaknesses"][0]["exemplars"][0]["note"] = json!("x");
    assert!(serde_json::from_value::<CoachingInput>(v).is_err());
    let mut v = sample_value();
    v["changes"][0]["examples"][0]["note"] = json!("x");
    assert!(serde_json::from_value::<CoachingInput>(v).is_err());
    let mut v = sample_value();
    v["basis"]["games"] = json!(1);
    assert!(serde_json::from_value::<CoachingInput>(v).is_err());
}

#[test]
fn missing_nullable_key_is_rejected() {
    // zod `.nullable()` is not `.optional()`: the key must be present.
    let mut v = sample_value();
    v["weaknesses"][0]["exemplars"][0]
        .as_object_mut()
        .unwrap()
        .remove("best");
    assert!(serde_json::from_value::<CoachingInput>(v).is_err());
    let mut v = sample_value();
    v["changes"][0]["examples"][0]
        .as_object_mut()
        .unwrap()
        .remove("best");
    assert!(serde_json::from_value::<CoachingInput>(v).is_err());
    let mut v = sample_value();
    v.as_object_mut().unwrap().remove("rating");
    assert!(serde_json::from_value::<CoachingInput>(v).is_err());
}

#[test]
fn input_version_is_the_literal() {
    assert_eq!(INPUT_VERSION, "coaching-input-v1");
    assert_eq!(
        serde_json::to_value(InputVersion::V1).unwrap(),
        json!(INPUT_VERSION)
    );
    assert_eq!(InputVersion::V1.as_str(), INPUT_VERSION);
    let mut v = sample_value();
    v["inputVersion"] = json!("coaching-input-v2");
    assert!(serde_json::from_value::<CoachingInput>(v).is_err());
}

#[test]
fn round_trip_sample() {
    let original = sample_value();
    let input: CoachingInput = serde_json::from_value(original.clone()).unwrap();
    let again = input.to_json();
    assert_eq!(canonical(&again), canonical(&original));
    // Timestamps keep the JS `toISOString()` shape, dates stay `YYYY-MM-DD`.
    assert_eq!(again["generatedAt"], json!("2026-11-20T09:00:00.000Z"));
    assert_eq!(again["basis"]["windowStart"], json!("2025-12-01"));
    assert_eq!(again["changes"][0]["onsetDate"], json!("2026-09-14"));
    // And the typed value survives a second round.
    let twice: CoachingInput = serde_json::from_value(again).unwrap();
    assert_eq!(twice, input);
}
