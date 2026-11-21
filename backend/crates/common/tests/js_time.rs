use common::js_time;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::macros::datetime;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Stamped {
    #[serde(with = "common::js_time::iso")]
    at: OffsetDateTime,
    #[serde(with = "common::js_time::iso::option")]
    maybe: Option<OffsetDateTime>,
}

#[test]
fn timestamp_json_matches_js_to_iso_string() {
    // new Date("2026-11-21T12:34:56.789Z").toISOString() === "2026-11-21T12:34:56.789Z"
    let at = datetime!(2026-11-21 12:34:56.789 UTC);
    assert_eq!(js_time::to_iso_string(at), "2026-11-21T12:34:56.789Z");

    let v = Stamped { at, maybe: None };
    let json = serde_json::to_string(&v).unwrap();
    assert_eq!(json, r#"{"at":"2026-11-21T12:34:56.789Z","maybe":null}"#);
    assert_eq!(serde_json::from_str::<Stamped>(&json).unwrap(), v);
}

#[test]
fn whole_seconds_keep_three_digits_like_js() {
    // JS always prints milliseconds: new Date(Date.UTC(2026, 0, 1)).toISOString()
    assert_eq!(
        js_time::to_iso_string(datetime!(2026-01-01 00:00:00 UTC)),
        "2026-01-01T00:00:00.000Z"
    );
}

#[test]
fn sub_millisecond_digits_are_truncated_and_offsets_normalised_to_utc() {
    // Postgres timestamptz has microseconds; a JS Date keeps milliseconds only.
    assert_eq!(
        js_time::to_iso_string(datetime!(2026-11-21 12:34:56.789999 UTC)),
        "2026-11-21T12:34:56.789Z"
    );
    assert_eq!(
        js_time::to_iso_string(datetime!(2026-11-21 14:34:56.5 +02:00)),
        "2026-11-21T12:34:56.500Z"
    );
}

#[test]
fn parses_any_rfc3339_timestamp() {
    let v: Stamped = serde_json::from_str(
        r#"{"at":"2026-11-21T14:34:56+02:00","maybe":"2026-11-21T12:34:56.789Z"}"#,
    )
    .unwrap();
    assert_eq!(v.at, datetime!(2026-11-21 12:34:56 UTC));
    assert_eq!(v.maybe, Some(datetime!(2026-11-21 12:34:56.789 UTC)));
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Micro {
    #[serde(with = "common::js_time::iso_micros")]
    at: OffsetDateTime,
}

#[test]
fn iso_micros_prints_exactly_six_digits_in_utc() {
    assert_eq!(
        js_time::to_iso_micros_string(datetime!(2026-11-21 10:00:05.123456 UTC)),
        "2026-11-21T10:00:05.123456Z"
    );
    assert_eq!(
        js_time::to_iso_micros_string(datetime!(2026-01-01 00:00:00 UTC)),
        "2026-01-01T00:00:00.000000Z"
    );
    assert_eq!(
        js_time::to_iso_micros_string(datetime!(2026-11-21 12:00:05.5 +02:00)),
        "2026-11-21T10:00:05.500000Z"
    );
    // Nanoseconds (never produced by Postgres) are truncated, not rounded.
    assert_eq!(
        js_time::to_iso_micros_string(datetime!(2026-11-21 10:00:05.123456999 UTC)),
        "2026-11-21T10:00:05.123456Z"
    );
}

#[test]
fn iso_micros_round_trips_and_parses_any_rfc3339() {
    let v = Micro {
        at: datetime!(2026-11-21 10:00:05.123456 UTC),
    };
    let json = serde_json::to_string(&v).unwrap();
    assert_eq!(json, r#"{"at":"2026-11-21T10:00:05.123456Z"}"#);
    assert_eq!(serde_json::from_str::<Micro>(&json).unwrap(), v);
    let other: Micro = serde_json::from_str(r#"{"at":"2026-11-21T12:00:05.1+02:00"}"#).unwrap();
    assert_eq!(other.at, datetime!(2026-11-21 10:00:05.1 UTC));
}
