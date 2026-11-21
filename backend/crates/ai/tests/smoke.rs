#[test]
fn version_is_0_1_0() {
    assert_eq!(ai::VERSION, "0.1.0");
}

#[test]
fn model_ids_and_codes_are_exact() {
    use ai::client::model::{AiModel, Effort, Models};
    use ai::output::codes::{SECTION_KEYS, SectionKey, ValidationCode};

    assert_eq!(Models::REPORT.as_str(), "claude-sonnet-5-5");
    assert_eq!(Models::CHAT, AiModel::Sonnet55);
    assert_eq!(Models::EXPLAIN.as_str(), "claude-haiku-5-5");
    assert_eq!(Models::WEEKLY, AiModel::Haiku55);
    assert_eq!(
        "claude-haiku-5-5".parse::<AiModel>().unwrap(),
        AiModel::Haiku55
    );
    assert!("claude-haiku-5-5-20261001".parse::<AiModel>().is_err());
    assert_eq!(serde_json::to_value(Effort::Medium).unwrap(), "medium");

    let keys: Vec<_> = SECTION_KEYS.iter().map(|k| k.as_str()).collect();
    assert_eq!(
        keys,
        ["overview", "weaknesses", "openings", "climb_plan", "areas"]
    );
    assert_eq!(
        serde_json::to_value(SectionKey::ClimbPlan).unwrap(),
        "climb_plan"
    );
    for c in ValidationCode::ALL {
        assert_eq!(serde_json::to_value(c).unwrap(), c.as_str());
    }
    assert_eq!(ValidationCode::FreeTextFact.as_str(), "free_text_fact");
}

#[test]
fn text_block_and_segment_json() {
    use ai::client::port::TextBlock;
    use ai::segments::{PvSide, Segment};
    use serde_json::json;

    assert_eq!(
        TextBlock::cached("x").to_json(),
        json!({"type": "text", "text": "x", "cache_control": {"type": "ephemeral"}})
    );
    assert_eq!(
        TextBlock::new("y").to_json(),
        json!({"type": "text", "text": "y"})
    );
    let seg = Segment::Pv {
        game_id: "g1".into(),
        ply: 3,
        side: PvSide::Refutation,
        n: 2,
        san: "Nxd4".into(),
    };
    let v = serde_json::to_value(&seg).unwrap();
    assert_eq!(
        v,
        json!({"kind": "pv", "gameId": "g1", "ply": 3, "side": "refutation", "n": 2, "san": "Nxd4"})
    );
    assert_eq!(serde_json::from_value::<Segment>(v).unwrap(), seg);
    let eng = Segment::Eng {
        eval_id: "e".into(),
        line: None,
        n: None,
        text: "t".into(),
    };
    assert_eq!(
        serde_json::to_value(&eng).unwrap(),
        json!({"kind": "eng", "evalId": "e", "line": null, "n": null, "text": "t"})
    );
}

#[test]
fn segment_nullable_keys_are_present() {
    use ai::segments::Segment;
    use serde_json::json;

    let line = json!({"kind": "line", "lineKey": "k", "n": null, "san": "e4"});
    let seg: Segment = serde_json::from_value(line.clone()).unwrap();
    assert_eq!(serde_json::to_value(&seg).unwrap(), line);
    // A nullable key is never skipped: a missing `n`, `line` or `n` (eng) is rejected.
    for raw in [
        json!({"kind": "line", "lineKey": "k", "san": "e4"}),
        json!({"kind": "eng", "evalId": "e", "n": null, "text": "t"}),
        json!({"kind": "eng", "evalId": "e", "line": 1, "text": "t"}),
    ] {
        assert!(
            serde_json::from_value::<Segment>(raw.clone()).is_err(),
            "accepted {raw}"
        );
    }
}
