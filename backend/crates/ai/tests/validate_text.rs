//! `validate_text`: the check order, token scoping and rendering (R3 Task 8, ADR 0060).

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
        "You played {{move:li-w1-0:5}} on {{sq:e5}}.",
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
                square: "e5".into()
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
fn token_syntax_is_the_only_barrier_and_errors_are_sorted() {
    // A bad citation, a malformed token and a leak: stages 1 and 2 report, the leak is not
    // scanned (prose inside a malformed token never is).
    let t = "Qxf7 {{nope}}";
    assert_eq!(
        run(t, &[cite("li-w1-0", 6)], &TextRules::default()),
        [C::UnknownCitation, C::TokenSyntax]
    );
    // Without a syntax error every stage reports, in code order.
    let t = "Qxf7 {{move:li-w1-1:9}} and {{num:basis.nope}}";
    assert_eq!(
        run(t, &[cite("li-w1-0", 6)], &TextRules::default()),
        [
            C::UnknownCitation,
            C::UnresolvedToken,
            C::FreeTextFact,
            C::UncitedReference
        ]
    );
    // Stage 4 is skipped when stage 3 fired: "half" alone would be an extracted number.
    assert_eq!(
        run("Qxf7 in half the games", &[], &TextRules::default()),
        [C::FreeTextFact]
    );
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
    let r = validate_text("a {{num:basis.analysedGames}} b", &[], &ev(), &rules, "p");
    let e = r.expect_err("probe fires");
    assert_eq!(
        (e[0].code, e[0].detail.as_str(), e[0].path.as_str()),
        (C::Marketing, "probe", "p")
    );
    // The rules run alongside a leak, but never past a token-syntax error.
    let probe: TextRule = Box::new(|_, _| vec![(C::Marketing, "probe".into())]);
    let rules = TextRules {
        extra: vec![probe],
        ..TextRules::default()
    };
    assert_eq!(run("Qxf7", &[], &rules), [C::Marketing, C::FreeTextFact]);
    let never: TextRule = Box::new(|_, _| panic!("must not run"));
    let rules = TextRules {
        extra: vec![never],
        ..TextRules::default()
    };
    assert_eq!(run("Qxf7 {{nope}}", &[], &rules), [C::TokenSyntax]);
}

#[test]
fn glued_tokens_and_scoped_squares() {
    let c = [cite("li-w1-0", 5)];
    let d = TextRules::default();
    assert_eq!(run("N{{sq:e5}} here", &c, &d), [C::GluedToken]);
    assert_eq!(
        run("{{sq:e5}}{{sq:f3}} here", &c, &d),
        [C::GluedToken, C::GluedToken]
    );
    assert_eq!(
        run("{{sq:e5}}.{{sq:f3}} here", &c, &d),
        [C::GluedToken, C::GluedToken]
    );
    // Sentence punctuation is not glue.
    assert_eq!(run("It was {{sq:e5}}. Then {{sq:f3}}, again.", &c, &d), []);
    // A square must belong to a cited position.
    assert_eq!(run("It was {{sq:a1}}.", &c, &d), [C::UncitedReference]);
    // In chat, position tokens are the citations.
    let chat = TextRules {
        implicit_citations: true,
        ..TextRules::default()
    };
    assert_eq!(run("{{move:li-w1-0:5}} hit {{sq:e5}}.", &[], &chat), []);
    assert_eq!(run("{{sq:e5}} matters.", &[], &chat), [C::UncitedReference]);
}

#[test]
fn num_formats_are_bound_to_values_and_paths() {
    let d = TextRules::default();
    let c = [cite("li-w1-0", 5)];
    for bad in [
        "{{num:basis.analysedGames|pct0}} games",
        "{{num:weaknesses.W1.posteriorRate|int}} games",
        "{{num:weaknesses.W1.posteriorRate}} games",
        "{{num:basis.analysedGames|signed1}} games",
    ] {
        assert_eq!(run(bad, &c, &d), [C::UnresolvedToken], "{bad}");
    }
    for ok in [
        "{{num:weaknesses.W1.posteriorRate|pct1}} of chances",
        "{{num:basis.analysedGames|int}} games",
        "{{num:weaknesses.W1.exemplars.0.evalAfter.pawns|signed1}} pawns",
        "{{num:weaknesses.W1.excessEplPerGame|dec2}} pawns",
    ] {
        assert_eq!(run(ok, &c, &d), [], "{ok}");
    }
}

/// R2-1 (b): "first" as discourse or sequencing ("First, …", "your first move", "at first") is
/// no numeric fact, and a sentence-opening "Second," or "Third," is sequencing. A counted claim
/// is still caught, by its digit or number word.
#[test]
fn discourse_ordinals_are_not_free_text_facts() {
    let d = TextRules::default();
    for ok in [
        "First, slow down when the position gets sharp.",
        "Check every capture before your first move in a new position.",
        "At first this will feel slow.",
        "First of all, look at your opponent's threats.",
        "Your first job is to keep your pieces safe.",
        "First, check captures. Second, check threats. Third, pick a plan.",
        "Firstly, keep it simple.",
    ] {
        assert_eq!(run(ok, &[], &d), [], "{ok}");
    }
    for bad in [
        // A counted claim keeps its number.
        "Your first 10 games went well.",
        "In your first ten games you hung a piece.",
        // An ordinal that is not sequencing stays a number word.
        "You lose a third of your endgames.",
        "This is your second loss today.",
        "Your second game was lost on time.",
        "Play one game a day.",
    ] {
        assert_eq!(run(bad, &[], &d), [C::FreeTextFact], "{bad}");
    }
}
