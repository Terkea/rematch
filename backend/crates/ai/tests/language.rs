//! Port of TS plan A5's `language.test.ts` in token form (R3 Task 8, ADR-R3-4 "Language
//! rules"), plus the union cases.

use ai::input::types::{AreaId, CoachingInput, TimeClass};
use ai::output::codes::{Citation, ValidationCode};
use ai::testing::input;
use ai::validator::evidence::build_evidence;
use ai::validator::language::{
    RuleKind, RuleMeta, banned_phrases, confidence_phrase, evidence_window, evidence_window_token,
    format_evidence_window, likelihood, likelihood_phrase, no_causal_claims, no_marketing,
    report_text_rules, states_insufficient_data,
};
use ai::validator::text::{TextRule, TextRules, validate_text};

use ValidationCode as C;

fn with(rule: TextRule, text: &str) -> Vec<ValidationCode> {
    with_input(&input("b2-rapid"), rule, text)
}

fn with_input(i: &CoachingInput, rule: TextRule, text: &str) -> Vec<ValidationCode> {
    let ev = build_evidence(i);
    let rules = TextRules {
        extra: vec![rule],
        ..TextRules::default()
    };
    match validate_text(text, &[], &ev, &rules, "p") {
        Ok(_) => Vec::new(),
        Err(e) => e.into_iter().map(|e| e.code).collect(),
    }
}

#[test]
fn guaranteed_to_gain_is_banned() {
    assert_eq!(
        with(
            banned_phrases(),
            "You are guaranteed to gain rating points."
        ),
        [C::BannedPhrase]
    );
}

#[test]
fn this_will_improve_your_rating_is_banned() {
    assert_eq!(
        with(banned_phrases(), "This will improve your rating."),
        [C::BannedPhrase]
    );
}

#[test]
fn banned_words_need_word_boundaries() {
    for ok in [
        "Guarantor duties aside, play on.",
        "An improvement may follow.",
        "Provenance matters.",
    ] {
        assert_eq!(with(banned_phrases(), ok), [], "{ok}");
    }
    for bad in [
        "DEFINITELY",
        "it is certainly so",
        "a proven plan",
        "it was proved",
        "you will reach the next level",
    ] {
        let want: &[ValidationCode] = if bad == "it was proved" {
            &[]
        } else {
            &[C::BannedPhrase]
        };
        assert_eq!(with(banned_phrases(), bad), want, "{bad}");
    }
}

#[test]
fn likelihood_with_p_imp_0_9() {
    let req = likelihood_phrase(Some(0.9));
    assert_eq!(req, Some("likely"));
    assert_eq!(
        with(likelihood(req), "This is very likely to stick."),
        [C::LikelihoodMismatch]
    );
    assert_eq!(with(likelihood(req), "This is likely to stick."), []);
    assert_eq!(with(likelihood(req), "It is unlikely to matter."), []);
    assert_eq!(
        with(likelihood(req), "Change is unclear yet."),
        [C::LikelihoodMismatch]
    );
    // With no required phrase, every likelihood phrase is a mismatch.
    assert_eq!(
        with(likelihood(None), "This is likely."),
        [C::LikelihoodMismatch]
    );
}

#[test]
fn a_weakness_text_without_the_evidence_window_token_is_missing_evidence_window() {
    let i = input("b2-rapid");
    let ev = build_evidence(&i);
    let rules = report_text_rules(&i)(
        RuleKind::Weakness,
        RuleMeta {
            weakness_id: Some("W1".into()),
            ..RuleMeta::default()
        },
    );
    let c = [Citation {
        game_id: "li-w1-0".into(),
        ply: 5,
    }];
    let check = |t: &str| match validate_text(t, &c, &ev, &rules, "p") {
        Ok(_) => Vec::new(),
        Err(e) => e.into_iter().map(|e| e.code).collect::<Vec<_>>(),
    };
    assert_eq!(
        check("In rapid games you leave pieces loose, as in {{move:li-w1-0:5}}."),
        [C::MissingEvidenceWindow]
    );
    assert_eq!(
        check("Over your last {{num:basis.importedGames|int}} rapid games you leave pieces loose."),
        [C::MissingEvidenceWindow]
    );
    assert_eq!(
        check("Over your last {{num:basis.analysedGames|int}} games you leave pieces loose."),
        [C::MissingEvidenceWindow]
    );
    assert_eq!(
        check("Over your last {{num:basis.analysedGames|int}} rapid games you leave pieces loose."),
        []
    );
    let window = format!(
        "You leave pieces loose {}.",
        evidence_window_token(TimeClass::Rapid)
    );
    assert_eq!(check(&window), []);
}

#[test]
fn evidence_window_rule_alone() {
    let r = || evidence_window(TimeClass::Blitz);
    assert_eq!(
        with(r(), "over {{num:basis.analysedGames}} blitz games"),
        []
    );
    assert_eq!(
        with(r(), "over {{num:basis.analysedGames}} rapid games"),
        [C::MissingEvidenceWindow]
    );
    assert_eq!(
        with(r(), "over {{num:basis.analysedGames}} blitzkrieg games"),
        [C::MissingEvidenceWindow]
    );
}

#[test]
fn caused_by_is_a_causal_claim() {
    assert_eq!(
        with(
            no_causal_claims(),
            "This may have been caused by switching to blitz."
        ),
        [C::CausalClaim]
    );
    assert_eq!(
        with(no_causal_claims(), "This was caused by switching to blitz."),
        [C::CausalClaim, C::MissingHedge]
    );
}

#[test]
fn around_the_time_passes() {
    assert_eq!(
        with(
            no_causal_claims(),
            "This started around the time you switched to blitz."
        ),
        []
    );
}

#[test]
fn you_switched_to_blitz_is_missing_hedge() {
    assert_eq!(
        with(no_causal_claims(), "You switched to blitz."),
        [C::MissingHedge]
    );
}

#[test]
fn upgrade_to_pro_is_marketing() {
    assert_eq!(
        with(no_marketing(), "Upgrade to Pro to see more."),
        [C::Marketing]
    );
    assert_eq!(
        with(no_marketing(), "Share your report with a friend."),
        [C::Marketing]
    );
    assert_eq!(
        with(no_marketing(), "Look for upgrades of your worst piece."),
        []
    );
}

#[test]
fn an_insufficient_area_text_without_the_need_more_token_is_insufficient_data_not_stated() {
    let i = input("early-read");
    let rules = report_text_rules(&i);
    let ev = build_evidence(&i);
    let meta = |a| RuleMeta {
        area: Some(a),
        ..RuleMeta::default()
    };
    let check = |a, t: &str| match validate_text(t, &[], &ev, &rules(RuleKind::Area, meta(a)), "p")
    {
        Ok(_) => Vec::new(),
        Err(e) => e.into_iter().map(|e| e.code).collect::<Vec<_>>(),
    };
    assert_eq!(
        check(AreaId::Endgame, "Your endgame sample is still small."),
        [C::InsufficientDataNotStated]
    );
    // Another area's needMore does not count.
    assert_eq!(
        check(
            AreaId::Endgame,
            "We need {{num:areaScores.calculation.needMore|int}} more games."
        ),
        [C::InsufficientDataNotStated]
    );
    assert_eq!(
        check(
            AreaId::Endgame,
            "We need {{num:areaScores.endgame.needMore|int}} more endgame games to judge this."
        ),
        []
    );
    // A sufficient area needs no count.
    assert_eq!(check(AreaId::Openings, "Your openings look steady."), []);
    assert_eq!(
        with_input(
            &i,
            states_insufficient_data(AreaId::Mindset),
            "Too early to say."
        ),
        [C::InsufficientDataNotStated]
    );
}

#[test]
fn format_evidence_window_matches_the_example() {
    let i = input("b2-rapid");
    assert_eq!(
        format_evidence_window(&i.basis, TimeClass::Rapid),
        "over your last 214 analysed rapid games (3,912 imported)"
    );
    assert_eq!(
        evidence_window_token(TimeClass::Rapid),
        "over your last {{num:basis.analysedGames|int}} analysed rapid games ({{num:basis.importedGames|int}} imported)"
    );
}

#[test]
fn likelihood_phrase_boundaries() {
    assert_eq!(likelihood_phrase(Some(0.95)), Some("very likely"));
    assert_eq!(likelihood_phrase(Some(0.9499)), Some("likely"));
    assert_eq!(likelihood_phrase(Some(0.85)), Some("likely"));
    assert_eq!(likelihood_phrase(Some(0.8499)), Some("unclear yet"));
    assert_eq!(likelihood_phrase(Some(0.5)), Some("unclear yet"));
    assert_eq!(likelihood_phrase(Some(0.4999)), Some("no clear change yet"));
    assert_eq!(likelihood_phrase(None), None);
}

#[test]
fn confidence_phrase_boundaries() {
    assert_eq!(confidence_phrase(0.9), "clearly");
    assert_eq!(confidence_phrase(0.8999), "probably");
    assert_eq!(confidence_phrase(0.7), "probably");
    assert_eq!(confidence_phrase(0.6999), "possibly");
}

/// The union rule: forbidden phrases are found in the stripped text and in its hardened
/// normalisation, required phrases only in the stripped text.
mod union {
    use super::*;

    #[test]
    fn homoglyphs_and_hardened_invisibles_do_not_hide_a_forbidden_phrase() {
        // Cyrillic а in "guaranteed".
        assert_eq!(
            with(banned_phrases(), "You are gu\u{430}ranteed a win."),
            [C::BannedPhrase]
        );
        // U+180F is invisible only to the hardened path; it survives the token canonical form.
        assert_eq!(with(no_marketing(), "Up\u{180F}grade now."), [C::Marketing]);
        // Cyrillic е in "very".
        assert_eq!(with(likelihood(Some("likely")), "This is likely."), []);
        assert_eq!(
            with(likelihood(Some("likely")), "This is v\u{435}ry likely."),
            [C::LikelihoodMismatch]
        );
        // Combining mark inside "caused".
        assert_eq!(
            with(
                no_causal_claims(),
                "It may be cau\u{301}sed by time trouble."
            ),
            [C::CausalClaim]
        );
    }

    #[test]
    fn a_disguised_hedge_does_not_count() {
        assert_eq!(
            with(no_causal_claims(), "It m\u{430}y be the clock."),
            [C::MissingHedge]
        );
    }
}

/// Fix round 2: a dedicated test of the C4 rule, `engine_claims`, on its own.
mod engine_claims_rule {
    use super::*;
    use ai::validator::language::engine_claims;

    fn check(text: &str, plies: &[u32]) -> Vec<ValidationCode> {
        let ev = build_evidence(&input("b2-rapid"));
        let rules = TextRules {
            extra: vec![engine_claims()],
            ..TextRules::default()
        };
        let cites: Vec<Citation> = plies
            .iter()
            .map(|p| Citation {
                game_id: "li-w1-0".into(),
                ply: *p,
            })
            .collect();
        match validate_text(text, &cites, &ev, &rules, "p") {
            Ok(_) => Vec::new(),
            Err(e) => e.into_iter().map(|e| e.code).collect(),
        }
    }

    #[test]
    fn engine_words_need_engine_evidence() {
        for bad in [
            "The engine disliked {{move:li-w1-0:5}}.",
            "Stockfish prefers another plan.",
            "You were winning before {{move:li-w1-0:5}}.",
            "It was equal before {{move:li-w1-0:5}}.",
            "You threw away the game with {{move:li-w1-0:5}}.",
            "That was not the best move.",
            "The ENGINE agrees.",
        ] {
            assert_eq!(check(bad, &[5]), [C::UncitedReference], "{bad}");
        }
    }

    #[test]
    fn a_best_pv_or_cited_eval_token_supports_them() {
        for ok in [
            "{{best:li-w1-0:5}} kept the advantage.",
            "The engine line {{pv:li-w1-0:5:best:1}} wins material.",
            "You were winning ({{num:weaknesses.W1.exemplars.0.evalBefore.pawns|signed1}}) before {{move:li-w1-0:5}}.",
            "You lost the thread with {{move:li-w1-0:5}}.",
        ] {
            assert_eq!(check(ok, &[5]), [], "{ok}");
        }
        // An eval of an uncited exemplar does not count.
        assert_eq!(
            check(
                "You were losing ({{num:weaknesses.W1.exemplars.1.evalAfter.pawns|signed1}}).",
                &[5]
            ),
            [C::UncitedReference]
        );
    }

    #[test]
    fn blunder_or_mistake_may_not_sit_next_to_the_engines_move() {
        for bad in [
            "{{best:li-w1-0:5}} was a blunder.",
            "That mistake, {{best:li-w1-0:5}}, cost you.",
            "The blunder was {{pv:li-w1-0:5:best:1}} here.",
        ] {
            assert_eq!(check(bad, &[5]), [C::UncitedReference], "{bad}");
        }
        // Far from the best token, or next to a played move, it is fine.
        assert_eq!(check("{{move:li-w1-0:5}} was a blunder.", &[5]), []);
        assert_eq!(
            check(
                "{{best:li-w1-0:5}} was the calm reply, and much later on in that game came a blunder.",
                &[5]
            ),
            []
        );
    }
}
