//! `validate_text`: the check order, token scoping and rendering (R3 Task 8, ADR-R3-4).

use ai::output::codes::{Citation, ValidationCode, ValidationError};
use ai::segments::Segment;
use ai::testing::input;
use ai::validator::evidence::{Evidence, build_evidence, create_evidence};
use ai::validator::text::{TextCtx, TextRule, TextRules, validate_text};

use ValidationCode as C;

fn ev() -> Evidence {
    build_evidence(&input("b2-rapid"))
}

fn cite(g: &str, p: u32) -> Citation {
    Citation {
        game_id: g.to_owned(),
        ply: p,
    }
}

fn codes(r: &Result<Vec<Segment>, Vec<ValidationError>>) -> Vec<ValidationCode> {
    match r {
        Ok(_) => Vec::new(),
        Err(e) => e.iter().map(|e| e.code).collect(),
    }
}

fn run(text: &str, cites: &[Citation], rules: &TextRules) -> Vec<ValidationCode> {
    codes(&validate_text(text, cites, &ev(), rules, "x.text"))
}

#[test]
fn a_clean_paragraph_renders_segments() {
    let out = validate_text(
        "You played {{move:li-w1-0:5}} on {{sq:e4}}.",
        &[cite("li-w1-0", 5)],
        &ev(),
        &TextRules::default(),
        "x",
    )
    .expect("valid");
    assert_eq!(
        out,
        [
            Segment::Text {
                text: "You played ".into()
            },
            Segment::Move {
                game_id: "li-w1-0".into(),
                ply: 5,
                san: "d4".into()
            },
            Segment::Text {
                text: " on ".into()
            },
            Segment::Square {
                square: "e4".into()
            },
            Segment::Text { text: ".".into() },
        ]
    );
}

#[test]
fn errors_carry_the_path_and_the_offending_fragment() {
    let r = validate_text(
        "see {{nope:x}} here",
        &[],
        &ev(),
        &TextRules::default(),
        "weaknesses.items[0].text",
    );
    let e = r.expect_err("invalid");
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].code, C::TokenSyntax);
    assert_eq!(e[0].path, "weaknesses.items[0].text");
    assert!(e[0].detail.contains("{{nope:x}}"), "{}", e[0].detail);
}

#[test]
fn the_first_failing_stage_wins() {
    // A bad citation, a malformed token and a leak: only the citation stage reports.
    let t = "Qxf7 {{nope}}";
    assert_eq!(
        run(t, &[cite("li-w1-0", 6)], &TextRules::default()),
        [C::UnknownCitation]
    );
    // Without the bad citation, token syntax comes before the leak.
    assert_eq!(run(t, &[], &TextRules::default()), [C::TokenSyntax]);
    // An unresolved token and an uncited one: resolution comes first.
    let t = "{{move:li-w1-1:9}} and {{num:basis.nope}}";
    assert_eq!(run(t, &[], &TextRules::default()), [C::UnresolvedToken]);
}

#[test]
fn require_citation_and_unknown_citations_report_together_in_stage_one() {
    let rules = TextRules {
        require_citation: true,
        ..TextRules::default()
    };
    assert_eq!(run("steady play", &[], &rules), [C::MissingCitation]);
    assert_eq!(
        run(
            "steady play",
            &[cite("li-w1-0", 6), cite("nope", 1)],
            &rules
        ),
        [C::UnknownCitation, C::UnknownCitation]
    );
    // Duplicate citations are fine.
    assert_eq!(
        run(
            "steady play",
            &[cite("li-w1-0", 5), cite("li-w1-0", 5)],
            &rules
        ),
        []
    );
}

#[test]
fn free_text_fact_detail_lists_kind_and_text_pairs() {
    let e =
        validate_text("Qxf7 and 31%", &[], &ev(), &TextRules::default(), "p").expect_err("leak");
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].code, C::FreeTextFact);
    assert!(e[0].detail.contains("square:\"f7\""), "{}", e[0].detail);
    assert!(
        e[0].detail.contains("digit:\"31\", percent:\"%\""),
        "{}",
        e[0].detail
    );
}

#[test]
fn offtopic_is_valid_only_as_the_whole_paragraph() {
    assert_eq!(
        run("this is {{offtopic}} here", &[], &TextRules::default()),
        [C::TokenSyntax]
    );
    assert_eq!(
        run("{{offtopic}} {{offtopic}}", &[], &TextRules::default()),
        [C::TokenSyntax, C::TokenSyntax]
    );
    // As the whole paragraph it parses, but a report paragraph cannot resolve it: the chat
    // engine handles the marker before validating.
    assert_eq!(
        run(" {{offtopic}} ", &[], &TextRules::default()),
        [C::UnresolvedToken]
    );
}

#[test]
fn implicit_citations_skip_the_scoping_check() {
    let t = "{{move:li-w1-1:9}} was the slip";
    assert_eq!(run(t, &[], &TextRules::default()), [C::UncitedReference]);
    let chat = TextRules {
        implicit_citations: true,
        ..TextRules::default()
    };
    assert_eq!(run(t, &[], &chat), []);
}

#[test]
fn best_and_pv_tokens_are_scoped_too() {
    let c = [cite("li-w1-0", 5)];
    assert_eq!(
        run("{{best:li-w1-1:9}}", &c, &TextRules::default()),
        [C::UncitedReference]
    );
    assert_eq!(
        run("{{pv:li-w1-1:9:best:1}}", &c, &TextRules::default()),
        [C::UncitedReference]
    );
    assert_eq!(
        run(
            "{{best:li-w1-0:5}} then {{pv:li-w1-0:5:refutation:1}}",
            &c,
            &TextRules::default()
        ),
        []
    );
}

#[test]
fn line_keys_none_allows_any_known_line_and_some_restricts() {
    let t = "{{line:w-italian}} is your line";
    assert_eq!(run(t, &[], &TextRules::default()), []);
    let none = TextRules {
        line_keys: Some(Vec::new()),
        ..TextRules::default()
    };
    assert_eq!(run(t, &[], &none), [C::UnknownLine]);
    // An unknown line never resolves, whatever the keys.
    assert_eq!(
        run("{{line:w-nope}}", &[], &TextRules::default()),
        [C::UnresolvedToken]
    );
}

#[test]
fn unformattable_numbers_and_unknown_engine_ids_do_not_resolve() {
    let mut ev = create_evidence();
    ev.put_number_path("big", 1e22);
    let r = validate_text(
        "{{num:big|int}} games",
        &[],
        &ev,
        &TextRules::default(),
        "p",
    );
    assert_eq!(codes(&r), [C::UnresolvedToken]);
    let r = validate_text(
        "{{eng:0123456789abcdef0123456789abcdef:1}}",
        &[],
        &ev,
        &TextRules::default(),
        "p",
    );
    assert_eq!(codes(&r), [C::UnresolvedToken]);
}

#[test]
fn extra_rules_run_last_on_the_stripped_text_with_the_tokens() {
    let seen: TextRule = Box::new(|stripped: &str, ctx: &TextCtx| {
        assert_eq!(stripped, "a § b");
        assert_eq!(ctx.tokens.len(), 1);
        vec![(C::Marketing, "probe".into())]
    });
    let rules = TextRules {
        extra: vec![seen],
        ..TextRules::default()
    };
    let r = validate_text("a {{sq:e4}} b", &[], &ev(), &rules, "p");
    let e = r.expect_err("probe fires");
    assert_eq!(
        (e[0].code, e[0].detail.as_str(), e[0].path.as_str()),
        (C::Marketing, "probe", "p")
    );
    // Not reached when an earlier stage fails.
    let never: TextRule = Box::new(|_, _| panic!("must not run"));
    let rules = TextRules {
        extra: vec![never],
        ..TextRules::default()
    };
    assert_eq!(run("Qxf7", &[], &rules), [C::FreeTextFact]);
}
