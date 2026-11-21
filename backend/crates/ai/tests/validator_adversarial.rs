//! The token-era adversarial cases (R3 Task 8; ports the intent of TS plan A4's
//! `validator.adversarial.test.ts` to facts by reference), on `build_evidence(b2-rapid)`. Each case
//! asserts the exact code list, or `Ok`. `ts_parity` checks the recorded TS reference verdicts.

use ai::input::types::CoachingInput;
use ai::output::codes::{Citation, SectionKey, ValidationCode};
use ai::output::report::ReportSectionEnvelope;
use ai::testing::input;
use ai::validator::evidence::{Evidence, build_evidence};
use ai::validator::language::{RuleKind, RuleMeta, report_text_rules};
use ai::validator::section::{SectionContext, validate_section};
use ai::validator::text::{TextRules, validate_text};
use serde_json::{Value, json};

use ValidationCode as C;

fn setup() -> (CoachingInput, Evidence) {
    let i = input("b2-rapid");
    let ev = build_evidence(&i);
    (i, ev)
}

fn cite(g: &str, p: u32) -> Citation {
    Citation {
        game_id: g.to_owned(),
        ply: p,
    }
}

fn plain() -> TextRules {
    TextRules {
        require_citation: true,
        ..TextRules::default()
    }
}

fn codes(
    r: Result<Vec<ai::segments::Segment>, Vec<ai::output::codes::ValidationError>>,
) -> Vec<ValidationCode> {
    match r {
        Ok(_) => Vec::new(),
        Err(e) => e.into_iter().map(|e| e.code).collect(),
    }
}

fn check(text: &str, cites: &[Citation], rules: &TextRules) -> Vec<ValidationCode> {
    let (_, ev) = setup();
    codes(validate_text(text, cites, &ev, rules, "p"))
}

const W1: &str = "li-w1-0";

#[test]
fn case_01_real_game_wrong_ply_is_unknown_citation() {
    assert_eq!(
        check("you left a piece loose", &[cite(W1, 6)], &plain()),
        [C::UnknownCitation]
    );
}

#[test]
fn case_02_invented_game_is_unknown_citation() {
    assert_eq!(
        check("you left a piece loose", &[cite("li-zz-9", 5)], &plain()),
        [C::UnknownCitation]
    );
}

#[test]
fn case_03_another_exemplars_move_without_citing_it_is_uncited_reference() {
    assert_eq!(
        check("{{move:li-w1-1:9}} was the slip", &[cite(W1, 5)], &plain()),
        [C::UncitedReference]
    );
}

#[test]
fn case_04_free_text_mate_is_free_text_fact() {
    assert_eq!(
        check("Qxf7# was there", &[cite(W1, 5)], &plain()),
        [C::FreeTextFact]
    );
}

#[test]
fn case_05_cited_move_token_passes() {
    assert_eq!(
        check(
            "{{move:li-w1-0:5}} left a piece loose",
            &[cite(W1, 5)],
            &plain()
        ),
        []
    );
}

#[test]
fn case_06_free_text_rate_is_free_text_fact() {
    assert_eq!(
        check("you blunder 4.7 times per game", &[cite(W1, 5)], &plain()),
        [C::FreeTextFact]
    );
}

#[test]
fn case_07_num_token_passes_and_unknown_path_is_unresolved() {
    let c = [cite(W1, 5)];
    assert_eq!(
        check(
            "{{num:weaknesses.W1.posteriorRate|pct0}} of games",
            &c,
            &plain()
        ),
        []
    );
    assert_eq!(
        check("{{num:weaknesses.W1.nope}} of games", &c, &plain()),
        [C::UnresolvedToken]
    );
}

#[test]
fn case_08_pv_past_the_line_is_unresolved() {
    assert_eq!(
        check(
            "{{pv:li-w1-0:5:best:99}} was best",
            &[cite(W1, 5)],
            &plain()
        ),
        [C::UnresolvedToken]
    );
}

#[test]
fn case_09_number_word_is_free_text_fact() {
    assert_eq!(
        check("five blunders in a row", &[cite(W1, 5)], &plain()),
        [C::FreeTextFact]
    );
}

#[test]
fn case_10_weakness_item_without_citations_is_missing_citation() {
    let (i, ev) = setup();
    let rules = report_text_rules(&i)(
        RuleKind::Weakness,
        RuleMeta {
            weakness_id: Some("W1".into()),
            ..RuleMeta::default()
        },
    );
    let text = "Over your last {{num:basis.analysedGames|int}} analysed rapid games you left pieces loose.";
    assert_eq!(
        codes(validate_text(text, &[], &ev, &rules, "p")),
        [C::MissingCitation]
    );
}

#[test]
fn case_11_zero_width_inside_san_is_free_text_fact() {
    assert_eq!(
        check("N\u{200B}xf7 lost", &[cite(W1, 5)], &plain()),
        [C::FreeTextFact]
    );
}

#[test]
fn case_12_full_width_digits_are_free_text_fact() {
    assert_eq!(
        check("\u{FF14}.\u{FF17} blunders", &[cite(W1, 5)], &plain()),
        [C::FreeTextFact]
    );
}

#[test]
fn case_13_line_token_only_in_its_own_opening_item() {
    let (i, ev) = setup();
    let rules = report_text_rules(&i);
    let meta = |k: &str| RuleMeta {
        line_key: Some(k.into()),
        ..RuleMeta::default()
    };
    let text = "{{line:b-sicilian-najdorf:2}} starts it, as in {{move:li-op1:12}}";
    let c = [cite("li-op1", 12)];
    let own = rules(RuleKind::Opening, meta("b-sicilian-najdorf"));
    assert_eq!(codes(validate_text(text, &c, &ev, &own, "p")), []);
    let other = rules(RuleKind::Opening, meta("w-italian"));
    assert_eq!(
        codes(validate_text(text, &c, &ev, &other, "p")),
        [C::UnknownLine]
    );
}

#[test]
fn case_14_unterminated_token_is_token_syntax() {
    assert_eq!(
        check("see {{move:li-w1-0:5", &[cite(W1, 5)], &plain()),
        [C::TokenSyntax]
    );
}

// --- Section cases ---

fn envelope(section: &str, part: &str, value: Value) -> ReportSectionEnvelope {
    let mut env = json!({
        "section": section,
        "overview": null, "weaknesses": null, "openings": null, "climbPlan": null, "areas": null,
    });
    env[part] = value;
    serde_json::from_value(env).expect("envelope")
}

fn item(text: &str, cites: &[(&str, u32)]) -> Value {
    json!({
        "text": text,
        "citations": cites.iter().map(|(g, p)| json!({"gameId": g, "ply": p})).collect::<Vec<_>>(),
    })
}

const WINDOW: &str = "Over your last {{num:basis.analysedGames|int}} analysed rapid games";

fn focus(id: &str, drill: &str, cite: (&str, u32)) -> Value {
    json!({
        "weaknessId": id,
        "why": item(&format!("{WINDOW} this kept costing you, as in {{{{move:{}:{}}}}}.", cite.0, cite.1), &[cite]),
        "drills": [{"drillType": drill, "rationale": "Solve these slowly and check every capture."}],
        "habit": item("Before each move, ask what your opponent threatens.", &[]),
    })
}

fn plan(focuses: Vec<Value>) -> ReportSectionEnvelope {
    envelope(
        "climb_plan",
        "climbPlan",
        json!({
            "intro": item("Here is your plan for the coming weeks.", &[]),
            "focuses": focuses,
            "weeklyRhythm": item("Play slower games on weekdays and review them at the weekend.", &[]),
        }),
    )
}

#[test]
fn case_15_opening_verdict_other_than_computed_is_verdict_mismatch() {
    let (i, ev) = setup();
    let focuses = ["W1".to_owned(), "W3".to_owned()];
    let ctx = SectionContext {
        input: &i,
        evidence: &ev,
        focuses: &focuses,
    };
    let env = envelope(
        "openings",
        "openings",
        json!({"items": [{
            "text": "Keep playing {{line:b-sicilian-najdorf}}, as in {{move:li-op1:12}}.",
            "citations": [{"gameId": "li-op1", "ply": 12}],
            "lineKey": "b-sicilian-najdorf",
            "verdict": "keep",
        }]}),
    );
    let v = validate_section(SectionKey::Openings, &env, &ctx);
    assert!(v.fatal.is_empty());
    assert_eq!(v.item_errors.len(), 1);
    assert_eq!(v.item_errors[0].path, "openings.items[0]");
    let got: Vec<_> = v.item_errors[0].errors.iter().map(|e| e.code).collect();
    assert_eq!(got, [C::VerdictMismatch]);
    assert_eq!(
        v.content
            .expect("content")
            .openings()
            .expect("openings")
            .items
            .len(),
        0
    );
}

#[test]
fn case_16_focus_rules() {
    let (i, ev) = setup();
    let focuses = ["W1".to_owned(), "W3".to_owned()];
    let ctx = SectionContext {
        input: &i,
        evidence: &ev,
        focuses: &focuses,
    };
    let w1 = ("li-w1-0", 5);
    let w3 = ("li-w3-0", 16);

    // 4 focuses: fatal.
    let four = plan(vec![
        focus("W1", "themed_puzzles", w1),
        focus("W3", "themed_puzzles", w3),
        focus("W1", "slow_games", w1),
        focus("W3", "own_game_analysis", w3),
    ]);
    let v = validate_section(SectionKey::ClimbPlan, &four, &ctx);
    assert_eq!(
        v.fatal.iter().map(|e| e.code).collect::<Vec<_>>(),
        [C::TooManyFocuses]
    );
    assert!(v.content.is_none());

    // A focus on an unselected weakness, and a drill W1 does not allow.
    let env = plan(vec![
        focus("W1", "tablebase_drills", w1),
        focus("W5", "convert_vs_engine", w1),
        focus("W3", "themed_puzzles", w3),
    ]);
    let v = validate_section(SectionKey::ClimbPlan, &env, &ctx);
    assert!(v.fatal.is_empty(), "{:?}", v.fatal);
    let dropped: Vec<(String, Vec<ValidationCode>)> = v
        .item_errors
        .iter()
        .map(|d| (d.path.clone(), d.errors.iter().map(|e| e.code).collect()))
        .collect();
    assert_eq!(
        dropped,
        [
            ("climbPlan.focuses[0]".to_owned(), vec![C::DrillNotAllowed]),
            ("climbPlan.focuses[1]".to_owned(), vec![C::FocusNotSelected]),
        ]
    );
    let content = v.content.expect("content");
    let cp = content.climb_plan().expect("climb plan");
    assert_eq!(cp.focuses.len(), 1);
    assert_eq!(cp.focuses[0].weakness_id, "W3");
    assert!(cp.focuses[0].why.is_some() && cp.focuses[0].habit.is_some());
}

#[test]
fn case_17_areas_with_six_items_is_fatal_area_missing() {
    let (i, ev) = setup();
    let ctx = SectionContext {
        input: &i,
        evidence: &ev,
        focuses: &[],
    };
    let items: Vec<Value> = [
        "openings",
        "middlegame",
        "endgame",
        "tactics",
        "calculation",
        "time",
    ]
    .iter()
    .map(|a| json!({"text": "Steady work here.", "citations": [], "area": a}))
    .collect();
    let v = validate_section(
        SectionKey::Areas,
        &envelope("areas", "areas", json!({"items": items})),
        &ctx,
    );
    assert_eq!(
        v.fatal.iter().map(|e| e.code).collect::<Vec<_>>(),
        [C::AreaMissing]
    );
    assert!(v.content.is_none());
}

#[test]
fn case_18_envelope_for_another_section_is_fatal_wrong_section() {
    let (i, ev) = setup();
    let ctx = SectionContext {
        input: &i,
        evidence: &ev,
        focuses: &[],
    };
    let env = envelope("openings", "openings", json!({"items": []}));
    let v = validate_section(SectionKey::Weaknesses, &env, &ctx);
    assert_eq!(
        v.fatal.iter().map(|e| e.code).collect::<Vec<_>>(),
        [C::WrongSection]
    );
    assert!(v.content.is_none());
    // The right section key with the requested part null is also wrong.
    let env = envelope("weaknesses", "openings", json!({"items": []}));
    let v = validate_section(SectionKey::Weaknesses, &env, &ctx);
    assert_eq!(
        v.fatal.iter().map(|e| e.code).collect::<Vec<_>>(),
        [C::WrongSection]
    );
}

/// Differential corpus against the TS reference validator (ADR-R3-4 "Claim validator").
mod ts_parity {
    use super::*;
    use ai::input::types::AreaId;
    use ai::validator::language::{banned_phrases, no_causal_claims, no_marketing};
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Row {
        s: String,
        c: Vec<(String, u32)>,
        k: String,
        f: String,
        w: Option<String>,
        a: Option<AreaId>,
        l: Option<String>,
        ts: Vec<ValidationCode>,
    }

    /// TS accepts, Rust rejects: the union of hardened forms in the language rules (ADR-R3-4).
    const HARDENED: [&str; 5] = [
        "You are gu\u{430}ranteed a win.",
        "Up\u{180F}grade now.",
        "Disc\u{43E}unt codes inside.",
        "It may be cau\u{301}sed by time trouble.",
        "Pr\u{435}mium tips.",
    ];
    /// Both reject, with other codes: R3's `{{offtopic}}` and `{{eng:}}` tokens are TS syntax
    /// errors, and Rust rejects them later (`unresolved_token`).
    const DIFFERS: [&str; 2] = [
        "{{offtopic}}",
        "{{eng:0123456789abcdef0123456789abcdef:1}} was the engine view",
    ];

    fn rules(i: &CoachingInput, r: &Row) -> TextRules {
        let meta = RuleMeta {
            weakness_id: r.w.clone(),
            area: r.a,
            line_key: r.l.clone(),
        };
        let report = report_text_rules(i);
        match r.k.as_str() {
            "item" => report(RuleKind::Item, meta),
            "weakness" => report(RuleKind::Weakness, meta),
            "focus" => report(RuleKind::Focus, meta),
            "summary" => report(RuleKind::Summary, meta),
            "area" => report(RuleKind::Area, meta),
            "opening" => report(RuleKind::Opening, meta),
            "causal" => TextRules {
                extra: vec![no_causal_claims()],
                ..TextRules::default()
            },
            "chat" => TextRules {
                implicit_citations: true,
                extra: vec![banned_phrases(), no_marketing()],
                ..TextRules::default()
            },
            k => panic!("kind {k}"),
        }
    }

    /// Superset safety on every row: TS rejects implies Rust rejects. Rows not listed above must
    /// give the same deduplicated codes.
    #[test]
    fn rust_rejects_everything_ts_rejects() {
        let rows: Vec<Row> = serde_json::from_str(TS_CLAIM_CORPUS).expect("corpus");
        assert!(rows.len() >= 150, "{} rows", rows.len());
        let mut fixtures = std::collections::HashMap::new();
        let (mut hardened, mut differs) = (0, 0);
        for r in &rows {
            let (i, ev) = fixtures.entry(r.f.clone()).or_insert_with(|| {
                let i = input(&r.f);
                let ev = build_evidence(&i);
                (i, ev)
            });
            let cites: Vec<Citation> = r.c.iter().map(|(g, p)| cite(g, *p)).collect();
            let mut got = codes(validate_text(&r.s, &cites, ev, &rules(i, r), "p"));
            let mut seen = std::collections::HashSet::new();
            got.retain(|c| seen.insert(*c));
            let s = r.s.as_str();
            if !r.ts.is_empty() {
                assert!(
                    !got.is_empty(),
                    "TS rejects {s:?} with {:?}, Rust accepts",
                    r.ts
                );
            }
            if HARDENED.contains(&s) {
                hardened += 1;
                assert!(
                    r.ts.is_empty() && !got.is_empty(),
                    "stale HARDENED row {s:?}: TS {:?}, Rust {got:?}",
                    r.ts
                );
            } else if DIFFERS.contains(&s) {
                differs += 1;
                assert!(got != r.ts && !r.ts.is_empty(), "stale DIFFERS row {s:?}");
            } else {
                assert_eq!(got, r.ts, "{s:?}");
            }
        }
        assert_eq!((hardened, differs), (HARDENED.len(), DIFFERS.len()));
    }

    // BEGIN GENERATED by backend/tools/gen-claim-corpus.mjs; do not edit by hand.
    // node 22.22.0, icu 77.1, unicode 16.0, m3 4ff8220aed41afb2dd23e04314511f6ebcb1f8e7, 153 rows
    const TS_CLAIM_CORPUS: &str = r#"[
{"s":"You played {{move:li-w1-0:5}} and then 12. Nf3 d5 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then 1. e4 e5 2. Nf3 Nc6 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then Instead d5 was best later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then ...d6 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then Nxe5 d6 (e4) later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then e2-e4 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then Ng1-f3 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then e2xd5 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then e8Q later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then e8(Q) later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then O\u2013O later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then O\u2011O\u2011O later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then O\u2014O later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then O\u2212O later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then O-O later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then 0-0-0 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then N\u2066f3 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then N\u034ff3 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then \u265e\ufe0ff3 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then forty percent later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then thirty games later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then ten per cent later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then one game later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then three openings later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then a third of games later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then twenty-one games later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then 2.5x later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then 3 times later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then nf3 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then NF3 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then \u2659e4 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then Pe4 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then Nf3Nc6 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then exd6e.p. later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then exd6 e.p. later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then the 14th of October later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then 14 Oct later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then 14/10 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then 2026-10-14 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then .5% later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then 1,5 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then 1 000 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then 1\u00a0000 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then \uff11\uff12 games later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then \u0661\u0662 games later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then half of your games later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then a quarter later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then a dozen later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then twice as often later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then the second game later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then ten later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then \u04354 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then N\u200bf\u200c3 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then N\u3164f3 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then \u265f later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"You played {{move:li-w1-0:5}} and then \u2654 later.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"you left a piece loose","c":[["li-w1-0",6]],"k":"item","f":"b2-rapid","ts":["unknown_citation"]},
{"s":"you left a piece loose","c":[["li-zz-9",5]],"k":"item","f":"b2-rapid","ts":["unknown_citation"]},
{"s":"{{move:li-w1-1:9}} was the slip","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["uncited_reference"]},
{"s":"Qxf7# was there","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"{{move:li-w1-0:5}} left a piece loose","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":[]},
{"s":"you blunder 4.7 times per game","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"{{num:weaknesses.W1.posteriorRate|pct0}} of games","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":[]},
{"s":"{{num:weaknesses.W1.nope}} of games","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["unresolved_token"]},
{"s":"{{pv:li-w1-0:5:best:99}} was best","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["unresolved_token"]},
{"s":"five blunders in a row","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"Over your last {{num:basis.analysedGames|int}} analysed rapid games you left pieces loose.","c":[],"k":"weakness","f":"b2-rapid","w":"W1","ts":["missing_citation"]},
{"s":"N\u200bxf7 lost","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"\uff14.\uff17 blunders","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"{{line:b-sicilian-najdorf:2}} starts it, as in {{move:li-op1:12}}","c":[["li-op1",12]],"k":"opening","f":"b2-rapid","l":"b-sicilian-najdorf","ts":[]},
{"s":"{{line:b-sicilian-najdorf:2}} starts it, as in {{move:li-op1:12}}","c":[["li-op1",12]],"k":"opening","f":"b2-rapid","l":"w-italian","ts":["unknown_line"]},
{"s":"see {{move:li-w1-0:5","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["token_syntax"]},
{"s":"{{best:li-w1-0:5}} then {{pv:li-w1-0:5:refutation:1}} on {{sq:e4}}","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":[]},
{"s":"{{best:li-w1-1:9}} was better","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["uncited_reference"]},
{"s":"{{pv:li-w1-1:9:best:1}} was better","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["uncited_reference"]},
{"s":"{{move:li-w1-1:9}} was the slip","c":[["li-w1-0",5],["li-w1-1",9]],"k":"item","f":"b2-rapid","ts":[]},
{"s":"{{move:li-w1-1:9}} was the slip","c":[],"k":"chat","f":"b2-rapid","ts":[]},
{"s":"{{line:w-italian}} is your line","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":[]},
{"s":"{{line:w-nope}} is your line","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["unresolved_token"]},
{"s":"{{line:w-italian:99}} is your line","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["unresolved_token"]},
{"s":"{{date:basis.windowStart}} was the start","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":[]},
{"s":"{{date:basis.nope}} was the start","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["unresolved_token"]},
{"s":"{{num:basis.importedGames|int}} imported and {{num:basis.analysedGames}} analysed","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":[]},
{"s":"{{num:weaknesses.W1.posteriorRate|bogus}} of games","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["token_syntax"]},
{"s":"this is {{offtopic}} here","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["token_syntax"]},
{"s":"{{offtopic}}","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["token_syntax"]},
{"s":"{{eng:0123456789abcdef0123456789abcdef:1}} was the engine view","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["token_syntax"]},
{"s":"a {{move:li-w1-0:5}} } b","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["token_syntax"]},
{"s":"{{{sq:e4}}} here","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["token_syntax"]},
{"s":"{{mo\u200bve:li-w1-0:5}} here","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":[]},
{"s":"{{move:li-w1-0:\uff15}} here","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":[]},
{"s":"{{sq:\uff45\uff14}} here","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":[]},
{"s":"{{move:li-w1-0:5\n}} here","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["token_syntax"]},
{"s":"you lose half the games","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"twice as often as before","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"a double attack","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"you played h4 too early","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["free_text_fact"]},
{"s":"steady play","c":[],"k":"item","f":"b2-rapid","ts":[]},
{"s":"steady play","c":[["li-w1-0",5],["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":[]},
{"s":"steady play","c":[["li-w1-0",6],["nope",1]],"k":"item","f":"b2-rapid","ts":["unknown_citation"]},
{"s":"Over your last {{num:basis.analysedGames|int}} analysed rapid games you left pieces loose, as with {{move:li-w1-0:5}}.","c":[["li-w1-0",5]],"k":"weakness","f":"b2-rapid","w":"W1","ts":[]},
{"s":"Over your last {{num:basis.analysedGames|int}} analysed rapid games forks slipped past you, as with {{move:li-w3-0:16}}.","c":[["li-w3-0",16]],"k":"weakness","f":"b2-rapid","w":"W3","ts":[]},
{"s":"In rapid games you leave pieces loose, as in {{move:li-w1-0:5}}.","c":[["li-w1-0",5]],"k":"weakness","f":"b2-rapid","w":"W1","ts":["missing_evidence_window"]},
{"s":"Over your last {{num:basis.importedGames|int}} rapid games you leave pieces loose.","c":[["li-w1-0",5]],"k":"weakness","f":"b2-rapid","w":"W1","ts":["missing_evidence_window"]},
{"s":"Over your last {{num:basis.analysedGames|int}} games you leave pieces loose.","c":[["li-w1-0",5]],"k":"weakness","f":"b2-rapid","w":"W1","ts":["missing_evidence_window"]},
{"s":"Over your last {{num:basis.analysedGames|int}} analysed rapid games this is likely to stick.","c":[["li-w1-0",5]],"k":"weakness","f":"b2-rapid","w":"W1","ts":["likelihood_mismatch"]},
{"s":"Over your last {{num:basis.analysedGames|int}} analysed rapid games this is unlikely to matter.","c":[["li-w1-0",5]],"k":"weakness","f":"b2-rapid","w":"W1","ts":[]},
{"s":"Over your last {{num:basis.analysedGames|int}} analysed rapid games this is very likely to stick.","c":[["li-w1-0",5]],"k":"weakness","f":"b2-rapid","w":"W9b","ts":["likelihood_mismatch"]},
{"s":"Over your last {{num:basis.analysedGames|int}} analysed rapid games change is no clear change yet.","c":[["li-w1-0",5]],"k":"weakness","f":"b2-rapid","w":"W9b","ts":[]},
{"s":"Over your last {{num:basis.analysedGames|int}} analysed rapid games this is unclear yet.","c":[["li-w1-0",5]],"k":"weakness","f":"b2-rapid","w":"W11","ts":[]},
{"s":"Over your last {{num:basis.analysedGames|int}} analysed rapid games you lose on time.","c":[["li-w1-0",5]],"k":"focus","f":"b2-rapid","w":"general","ts":[]},
{"s":"Over your last {{num:basis.analysedGames|int}} analysed rapid games your play was steady.","c":[],"k":"summary","f":"b2-rapid","ts":[]},
{"s":"Your play was steady.","c":[],"k":"summary","f":"b2-rapid","ts":["missing_evidence_window"]},
{"s":"You are guaranteed to gain rating points.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["banned_phrase"]},
{"s":"This will improve your rating.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["banned_phrase"]},
{"s":"This will raise your rating soon.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["banned_phrase"]},
{"s":"You will reach the next level.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["banned_phrase"]},
{"s":"DEFINITELY a pattern.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["banned_phrase"]},
{"s":"It is certainly so.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["banned_phrase"]},
{"s":"A proven plan.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["banned_phrase"]},
{"s":"It was proved.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":[]},
{"s":"Guarantor duties aside, play on.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":[]},
{"s":"Provenance matters.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":[]},
{"s":"Upgrade to Pro to see more.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["marketing"]},
{"s":"Look for upgrades of your worst piece.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":[]},
{"s":"Share your report with a friend.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["marketing"]},
{"s":"Share your card.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["marketing"]},
{"s":"Try a free trial.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["marketing"]},
{"s":"Subscribe for tips.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["marketing"]},
{"s":"A premium move.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["marketing"]},
{"s":"Refer a friend.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":["marketing"]},
{"s":"This may have been caused by switching to blitz.","c":[["li-w1-0",5]],"k":"causal","f":"b2-rapid","ts":["causal_claim"]},
{"s":"This was caused by switching to blitz.","c":[["li-w1-0",5]],"k":"causal","f":"b2-rapid","ts":["causal_claim","missing_hedge"]},
{"s":"This started around the time you switched to blitz.","c":[["li-w1-0",5]],"k":"causal","f":"b2-rapid","ts":[]},
{"s":"You switched to blitz.","c":[["li-w1-0",5]],"k":"causal","f":"b2-rapid","ts":["missing_hedge"]},
{"s":"It could be due to the clock.","c":[["li-w1-0",5]],"k":"causal","f":"b2-rapid","ts":["causal_claim"]},
{"s":"The reason is the clock, it might be.","c":[["li-w1-0",5]],"k":"causal","f":"b2-rapid","ts":["causal_claim"]},
{"s":"This coincides with your switch.","c":[["li-w1-0",5]],"k":"causal","f":"b2-rapid","ts":[]},
{"s":"At about the same time you changed openings.","c":[["li-w1-0",5]],"k":"causal","f":"b2-rapid","ts":[]},
{"s":"That led to losses, possibly.","c":[["li-w1-0",5]],"k":"causal","f":"b2-rapid","ts":["causal_claim"]},
{"s":"This proves nothing, but it may matter.","c":[["li-w1-0",5]],"k":"causal","f":"b2-rapid","ts":["causal_claim"]},
{"s":"Mayhem at the board.","c":[["li-w1-0",5]],"k":"causal","f":"b2-rapid","ts":["missing_hedge"]},
{"s":"Your endgame sample is still small.","c":[],"k":"area","f":"early-read","a":"endgame","ts":["insufficient_data_not_stated"]},
{"s":"We need {{num:areaScores.endgame.needMore|int}} more endgame games.","c":[],"k":"area","f":"early-read","a":"endgame","ts":[]},
{"s":"We need {{num:areaScores.calculation.needMore|int}} more games.","c":[],"k":"area","f":"early-read","a":"endgame","ts":["insufficient_data_not_stated"]},
{"s":"Your openings look steady.","c":[],"k":"area","f":"early-read","a":"openings","ts":[]},
{"s":"You found {{move:li-s1:11}}.","c":[],"k":"area","f":"b2-rapid","a":"openings","ts":["uncited_reference"]},
{"s":"You are gu\u0430ranteed a win.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":[]},
{"s":"Up\u180fgrade now.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":[]},
{"s":"Disc\u043eunt codes inside.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":[]},
{"s":"It may be cau\u0301sed by time trouble.","c":[["li-w1-0",5]],"k":"causal","f":"b2-rapid","ts":[]},
{"s":"Pr\u0435mium tips.","c":[["li-w1-0",5]],"k":"item","f":"b2-rapid","ts":[]},
{"s":"It m\u0430y be the clock.","c":[["li-w1-0",5]],"k":"causal","f":"b2-rapid","ts":["missing_hedge"]}
]"#;
    // END GENERATED
}
