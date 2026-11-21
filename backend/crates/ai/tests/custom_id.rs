//! Port of the `custom ids` block of `packages/ai/test/client/fix-round-1.test.ts`.

use ai::client::custom_id::{CUSTOM_ID_MAX, CustomIdParts, decode_custom_id, encode_custom_id};

fn parts(schema_id: &str, version: u32, reference: &str) -> CustomIdParts {
    CustomIdParts {
        schema_id: schema_id.into(),
        version,
        reference: reference.into(),
    }
}

#[test]
fn round_trips() {
    let id = encode_custom_id(&parts("report-section", 3, "run12_sec-a")).unwrap();
    assert_eq!(id, "report-section__v3__run12_sec-a");
    assert_eq!(
        decode_custom_id(&id),
        Some(parts("report-section", 3, "run12_sec-a"))
    );
}

#[test]
fn refs_may_contain_double_underscores() {
    let id = encode_custom_id(&parts("a", 1, "x__y")).unwrap();
    assert_eq!(decode_custom_id(&id).unwrap().reference, "x__y");
}

#[test]
fn matches_the_api_charset_and_length() {
    let re = regex::Regex::new("^[A-Za-z0-9_-]{1,64}$").unwrap();
    let id = encode_custom_id(&parts("s", 1, &"r".repeat(40))).unwrap();
    assert!(re.is_match(&id));
    assert_eq!(CUSTOM_ID_MAX, 64);
    assert!(encode_custom_id(&parts("s", 1, &"r".repeat(60))).is_err());
    assert!(encode_custom_id(&parts("a@b", 1, "r")).is_err());
    assert!(encode_custom_id(&parts("a_b", 1, "r")).is_err());
    assert!(encode_custom_id(&parts("a", 1, "r:1")).is_err());
    assert!(encode_custom_id(&parts("", 1, "r")).is_err());
    assert!(encode_custom_id(&parts("a", 1, "")).is_err());
}

#[test]
fn exactly_64_chars_is_allowed() {
    // "s__v1__" is 7 chars.
    let id = encode_custom_id(&parts("s", 1, &"r".repeat(57))).unwrap();
    assert_eq!(id.len(), 64);
    assert!(decode_custom_id(&id).is_some());
    assert!(encode_custom_id(&parts("s", 1, &"r".repeat(58))).is_err());
    assert_eq!(decode_custom_id(&format!("{id}r")), None);
}

#[test]
fn decode_rejects_foreign_ids() {
    assert_eq!(decode_custom_id("plain"), None);
    assert_eq!(decode_custom_id("a__vx__r"), None);
    assert_eq!(decode_custom_id("a__v99999999999__r"), None);
}

#[test]
fn report_section_ids_with_a_32_hex_reference_fit() {
    let reference = "0123456789abcdef0123456789abcdef";
    let id = encode_custom_id(&parts("report-section", 1, reference)).unwrap();
    assert_eq!(id, format!("report-section__v1__{reference}"));
    assert!(id.len() <= CUSTOM_ID_MAX);
}
