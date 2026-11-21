use chess_core::EngineScore::{self, Cp, Mate};

#[test]
fn engine_score_json_shape() {
    assert_eq!(serde_json::to_string(&Cp(-30)).unwrap(), r#"{"cp":-30}"#);
    assert_eq!(serde_json::to_string(&Mate(2)).unwrap(), r#"{"mate":2}"#);

    let parsed: Vec<Option<EngineScore>> =
        serde_json::from_str(r#"[{"cp":10},null,{"mate":-3}]"#).unwrap();
    assert_eq!(parsed, vec![Some(Cp(10)), None, Some(Mate(-3))]);
}
