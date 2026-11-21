//! Port of TS plan A4's `validate-section.test.ts` and the `selectFocuses` tests (R3 Task 8).

use ai::input::types::CoachingInput;
use ai::output::codes::{SectionKey, ValidationCode};
use ai::output::report::ReportSectionEnvelope;
use ai::segments::Segment;
use ai::testing::{input, make_input};
use ai::validator::evidence::{Evidence, build_evidence};
use ai::validator::focus::select_focuses;
use ai::validator::section::{SectionContext, validate_section};
use serde_json::{Value, json};

fn setup() -> (CoachingInput, Evidence) {
    let i = input("b2-rapid");
    let ev = build_evidence(&i);
    (i, ev)
}

fn envelope(section: &str, part: &str, value: Value) -> ReportSectionEnvelope {
    let mut env = json!({
        "section": section,
        "overview": null, "weaknesses": null, "openings": null, "climbPlan": null, "areas": null,
    });
    env[part] = value;
    serde_json::from_value(env).expect("envelope")
}

const WINDOW: &str = "Over your last {{num:basis.analysedGames|int}} analysed rapid games";

fn weakness(id: &str, text: &str, game: &str, ply: u32) -> Value {
    json!({
        "text": text,
        "citations": [{"gameId": game, "ply": ply}],
        "weaknessId": id,
        "title": "Loose pieces",
    })
}

fn mixed() -> ReportSectionEnvelope {
    envelope(
        "weaknesses",
        "weaknesses",
        json!({"items": [
            weakness("W1", &format!("{WINDOW} you left pieces loose, as with {{{{move:li-w1-0:5}}}}."), "li-w1-0", 5),
            weakness("W3", "You missed Nxe4 here.", "li-w3-0", 16),
            weakness("W3", &format!("{WINDOW} forks slipped past you, as with {{{{move:li-w3-0:16}}}}."), "li-w3-0", 16),
        ]}),
    )
}

#[test]
fn a_mixed_weaknesses_section_keeps_2_valid_items_drops_1() {
    let (i, ev) = setup();
    let focuses = select_focuses(&i, true);
    let ctx = SectionContext {
        input: &i,
        evidence: &ev,
        focuses: &focuses,
    };
    let v = validate_section(SectionKey::Weaknesses, &mixed(), &ctx);
    assert!(v.fatal.is_empty(), "{:?}", v.fatal);
    let content = v.content.expect("content");
    let items = &content.weaknesses().expect("weaknesses").items;
    assert_eq!(
        items
            .iter()
            .map(|w| w.weakness_id.as_str())
            .collect::<Vec<_>>(),
        ["W1", "W3"]
    );
    assert_eq!(v.item_errors.len(), 1);
    let d = &v.item_errors[0];
    assert_eq!(
        (d.section.as_str(), d.path.as_str()),
        ("weaknesses", "weaknesses.items[1]")
    );
    assert_eq!(
        d.errors.iter().map(|e| e.code).collect::<Vec<_>>(),
        [ValidationCode::FreeTextFact]
    );
    assert_eq!(d.errors[0].path, "weaknesses.items[1].text");
}

#[test]
fn an_invalid_overview_headline_becomes_none_while_summary_is_kept() {
    let (i, ev) = setup();
    let ctx = SectionContext {
        input: &i,
        evidence: &ev,
        focuses: &[],
    };
    let env = envelope(
        "overview",
        "overview",
        json!({
            "headline": {"text": "Upgrade to Pro for the full picture.", "citations": []},
            "summary": {"text": format!("{WINDOW} your play was steady."), "citations": []},
            "strengths": [],
        }),
    );
    let v = validate_section(SectionKey::Overview, &env, &ctx);
    assert!(v.fatal.is_empty());
    let content = v.content.expect("content");
    let o = content.overview().expect("overview");
    assert!(o.headline.is_none());
    assert!(o.summary.is_some());
    assert_eq!(v.item_errors.len(), 1);
    assert_eq!(v.item_errors[0].path, "overview.headline");
    assert_eq!(v.item_errors[0].errors[0].code, ValidationCode::Marketing);
}

#[test]
fn an_unknown_weakness_and_a_leaking_title_drop_their_items() {
    let (i, ev) = setup();
    let ctx = SectionContext {
        input: &i,
        evidence: &ev,
        focuses: &[],
    };
    let good = format!("{WINDOW} you left pieces loose, as with {{{{move:li-w1-0:5}}}}.");
    let mut titled = weakness("W1", &good, "li-w1-0", 5);
    titled["title"] = json!("Hanging the e4 pawn");
    let env = envelope(
        "weaknesses",
        "weaknesses",
        json!({"items": [weakness("W99", &good, "li-w1-0", 5), titled]}),
    );
    let v = validate_section(SectionKey::Weaknesses, &env, &ctx);
    let got: Vec<(String, Vec<ValidationCode>, String)> = v
        .item_errors
        .iter()
        .map(|d| {
            (
                d.path.clone(),
                d.errors.iter().map(|e| e.code).collect(),
                d.errors[0].path.clone(),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            (
                "weaknesses.items[0]".into(),
                vec![ValidationCode::UnknownWeakness],
                "weaknesses.items[0].weaknessId".into()
            ),
            (
                "weaknesses.items[1]".into(),
                vec![ValidationCode::FreeTextFact],
                "weaknesses.items[1].title".into()
            ),
        ]
    );
}

#[test]
fn strengths_and_areas_may_have_no_citation_but_then_no_position_token() {
    let (i, ev) = setup();
    let ctx = SectionContext {
        input: &i,
        evidence: &ev,
        focuses: &[],
    };
    let env = envelope(
        "overview",
        "overview",
        json!({
            "headline": {"text": "Solid foundations.", "citations": []},
            "summary": {"text": format!("{WINDOW} your play was steady."), "citations": []},
            "strengths": [
                {"text": "Your openings are sound.", "citations": [], "area": "openings"},
                {"text": "You found {{move:li-s1:11}}.", "citations": [], "area": "openings"},
                {"text": "You found {{move:li-s1:11}}.", "citations": [{"gameId": "li-s1", "ply": 11}], "area": "openings"},
            ],
        }),
    );
    let v = validate_section(SectionKey::Overview, &env, &ctx);
    let content = v.content.expect("content");
    assert_eq!(content.overview().expect("overview").strengths.len(), 2);
    assert_eq!(v.item_errors.len(), 1);
    assert_eq!(v.item_errors[0].path, "overview.strengths[1]");
    assert_eq!(
        v.item_errors[0].errors[0].code,
        ValidationCode::UncitedReference
    );
}

#[test]
fn the_general_focus_is_the_only_focus_when_none_is_selected() {
    let i = input("early-read");
    let ev = build_evidence(&i);
    let focuses = select_focuses(&i, true);
    assert!(focuses.is_empty());
    let ctx = SectionContext {
        input: &i,
        evidence: &ev,
        focuses: &focuses,
    };
    let f = |id: &str, drill: &str| {
        json!({
            "weaknessId": id,
            "why": {"text": "Over your last {{num:basis.analysedGames|int}} analysed rapid games you rushed, as in {{move:li-erw1-0:3}}.", "citations": [{"gameId": "li-erw1-0", "ply": 3}]},
            "drills": [{"drillType": drill, "rationale": "Go through your own games."}],
            "habit": {"text": "Sit on your hands before each move.", "citations": []},
        })
    };
    let env = envelope(
        "climb_plan",
        "climbPlan",
        json!({
            "intro": {"text": "A general plan for now.", "citations": []},
            "focuses": [f("general", "own_game_analysis"), f("W1", "themed_puzzles"), f("general", "session_rules")],
            "weeklyRhythm": {"text": "Play slower games on weekdays.", "citations": []},
        }),
    );
    let v = validate_section(SectionKey::ClimbPlan, &env, &ctx);
    let content = v.content.expect("content");
    let cp = content.climb_plan().expect("climb plan");
    assert_eq!(cp.focuses.len(), 1);
    assert!(cp.focuses[0].why.is_some(), "{:?}", v.item_errors);
    let paths: Vec<_> = v.item_errors.iter().map(|d| d.path.as_str()).collect();
    assert_eq!(paths, ["climbPlan.focuses[1]", "climbPlan.focuses[2]"]);
    assert_eq!(
        v.item_errors[1]
            .errors
            .iter()
            .map(|e| e.code)
            .collect::<Vec<_>>(),
        [
            ValidationCode::FocusNotSelected,
            ValidationCode::DrillNotAllowed
        ]
    );
}

#[test]
fn select_focuses_on_b2_rapid() {
    let i = input("b2-rapid");
    assert_eq!(select_focuses(&i, true), ["W1", "W3"]);
    assert_eq!(select_focuses(&i, false), ["W1", "W3", "W5"]);
}

#[test]
fn select_focuses_on_early_read_is_empty() {
    assert!(select_focuses(&input("early-read"), true).is_empty());
    assert!(select_focuses(&input("early-read"), false).is_empty());
}

#[test]
fn select_focuses_never_returns_w12() {
    let raw = make_input(json!({"weaknesses": [{"id": "W12"}, {}, {}, {}, {}, {}]}));
    let i: CoachingInput = serde_json::from_value(raw).expect("input");
    assert_eq!(i.weaknesses[0].id, "W12");
    assert_eq!(select_focuses(&i, false), ["W3", "W5", "W7a"]);
}

#[test]
fn select_focuses_orders_by_rank_and_filters_state_and_confidence() {
    let raw = make_input(json!({"weaknesses": [
        {"rank": 9}, {"state": "resolved"}, {"confidence": 0.69}, {}, {"state": "relapsed"}
    ]}));
    let i: CoachingInput = serde_json::from_value(raw).expect("input");
    // W1 moves to rank 9; W3 is resolved; W5 is below 0.7; W7a stays; W9b is relapsed (0.7).
    assert_eq!(select_focuses(&i, false), ["W7a", "W9b", "W1"]);
}

#[test]
fn validated_items_carry_segments() {
    let (i, ev) = setup();
    let ctx = SectionContext {
        input: &i,
        evidence: &ev,
        focuses: &[],
    };
    let v = validate_section(SectionKey::Weaknesses, &mixed(), &ctx);
    let content = v.content.expect("content");
    let w1 = &content.weaknesses().expect("weaknesses").items[0];
    assert_eq!(
        w1.segments,
        [
            Segment::Text {
                text: "Over your last ".into()
            },
            Segment::Num {
                path: "basis.analysedGames".into(),
                value: 214.0,
                display: "214".into()
            },
            Segment::Text {
                text: " analysed rapid games you left pieces loose, as with ".into()
            },
            Segment::Move {
                game_id: "li-w1-0".into(),
                ply: 5,
                san: "d4".into()
            },
            Segment::Text { text: ".".into() },
        ]
    );
    // The validated shape serialises in the TS form.
    let json = serde_json::to_value(&content).expect("json");
    assert!(json["overview"].is_null() && json["weaknesses"]["items"][0]["segments"].is_array());
}
