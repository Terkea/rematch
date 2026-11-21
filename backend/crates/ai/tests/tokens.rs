//! Port of `.worktrees/m3/packages/ai/test/validator/tokens.test.ts` (`describe("parseTokens")` is
//! the `parse_tokens` module), plus the R3 `{{eng:…}}` and `{{offtopic}}` tokens.

use ai::segments::PvSide;
use ai::validator::tokens::{
    NumFormat, TOKEN_PLACEHOLDER, TokenBody, canonical_for_tokens, parse_tokens,
};

const EVAL_ID: &str = "0123456789abcdef0123456789abcdef";

mod parse_tokens {
    use super::*;

    #[test]
    fn parses_every_token_kind() {
        let r = parse_tokens(
            "a {{move:g1:6}} b {{best:g1:6}} c {{pv:g1:6:best:2}} d {{pv:g1:6:refutation:1}} e {{line:w-italian}} f {{line:w-italian:3}} g {{num:basis.analysedGames}} h {{num:weaknesses.W1.posteriorRate|pct1}} i {{date:basis.windowStart}} j {{sq:f7}}",
        );
        assert_eq!(r.errors, vec![]);
        let kinds: Vec<&str> = r.tokens.iter().map(|t| t.body.kind()).collect();
        assert_eq!(
            kinds,
            vec![
                "move", "best", "pv", "pv", "line", "line", "num", "num", "date", "sq"
            ]
        );
        assert_eq!(
            r.tokens[0].body,
            TokenBody::Move {
                game_id: "g1".into(),
                ply: 6
            }
        );
        assert!(matches!(
            r.tokens[2].body,
            TokenBody::Pv {
                side: PvSide::Best,
                n: 2,
                ..
            }
        ));
        assert!(matches!(
            r.tokens[3].body,
            TokenBody::Pv {
                side: PvSide::Refutation,
                n: 1,
                ..
            }
        ));
        assert_eq!(
            r.tokens[4].body,
            TokenBody::Line {
                line_key: "w-italian".into(),
                n: None
            }
        );
        assert_eq!(
            r.tokens[5].body,
            TokenBody::Line {
                line_key: "w-italian".into(),
                n: Some(3)
            }
        );
        assert_eq!(
            r.tokens[6].body,
            TokenBody::Num {
                path: "basis.analysedGames".into(),
                fmt: None
            }
        );
        assert!(matches!(
            r.tokens[7].body,
            TokenBody::Num {
                fmt: Some(NumFormat::Pct1),
                ..
            }
        ));
        assert_eq!(
            r.tokens[9].body,
            TokenBody::Sq {
                square: "f7".into()
            }
        );
    }

    #[test]
    fn replaces_each_token_with_a_placeholder() {
        let r = parse_tokens("x {{move:g1:6}} y {{sq:e4}}");
        assert_eq!(r.stripped, "x § y §");
        assert_eq!(TOKEN_PLACEHOLDER, '§');
    }

    #[test]
    fn reports_malformed_tokens_as_errors_and_still_strips_them() {
        let bad = [
            "{{move:g1}}",
            "{{move:g1:x}}",
            "{{move:g1:0}}",
            "{{pv:g1:6:middle:1}}",
            "{{pv:g1:6:best:0}}",
            "{{num:a.b|bogus}}",
            "{{num:}}",
            "{{sq:i9}}",
            "{{sq:e44}}",
            "{{nope:x}}",
            "{{move:g1:6",
            "move:g1:6}}",
            "{{}}",
        ];
        assert_eq!(bad.len(), 13);
        for b in bad {
            let r = parse_tokens(&format!("see {b} here"));
            assert!(!r.errors.is_empty(), "{b}");
        }
    }

    #[test]
    fn accepts_every_number_format() {
        for fmt in ["int", "pct0", "pct1", "dec1", "dec2", "signed1"] {
            assert_eq!(
                parse_tokens(&format!("{{{{num:a.b|{fmt}}}}}")).errors,
                vec![],
                "{fmt}"
            );
        }
        let all: Vec<&str> = NumFormat::ALL.iter().map(|f| f.as_str()).collect();
        assert_eq!(all, ["int", "pct0", "pct1", "dec1", "dec2", "signed1"]);
    }

    #[test]
    fn canonicalises_invisible_characters_and_full_width_forms_before_parsing() {
        let r = parse_tokens("{{mo\u{200b}ve:g1:\u{ff16}}}");
        assert_eq!(r.errors, vec![]);
        assert!(matches!(r.tokens[0].body, TokenBody::Move { ply: 6, .. }));
    }
}

mod rust_only {
    use super::*;

    #[test]
    fn parses_eng_and_offtopic_tokens() {
        let r = parse_tokens(&format!(
            "{{{{eng:{EVAL_ID}:tried}}}} {{{{eng:{EVAL_ID}:2}}}} {{{{eng:{EVAL_ID}:2:3}}}} {{{{offtopic}}}}"
        ));
        assert_eq!(r.errors, vec![]);
        let bodies: Vec<TokenBody> = r.tokens.into_iter().map(|t| t.body).collect();
        assert_eq!(
            bodies,
            vec![
                TokenBody::Eng {
                    eval_id: EVAL_ID.into(),
                    line: None,
                    n: None
                },
                TokenBody::Eng {
                    eval_id: EVAL_ID.into(),
                    line: Some(2),
                    n: None
                },
                TokenBody::Eng {
                    eval_id: EVAL_ID.into(),
                    line: Some(2),
                    n: Some(3)
                },
                TokenBody::Offtopic,
            ]
        );
        assert_eq!(r.stripped, "§ § § §");

        for bad in [
            "{{eng:abc:1}}".to_owned(),
            format!("{{{{eng:{EVAL_ID}:4}}}}"),
            format!("{{{{eng:{EVAL_ID}:0}}}}"),
            "{{offtopic:x}}".to_owned(),
            // Uppercase hex, a missing line, an extra part, n = 0, and "tried" with an n.
            format!("{{{{eng:{}:1}}}}", EVAL_ID.to_uppercase()),
            format!("{{{{eng:{EVAL_ID}}}}}"),
            format!("{{{{eng:{EVAL_ID}:1:2:3}}}}"),
            format!("{{{{eng:{EVAL_ID}:1:0}}}}"),
            format!("{{{{eng:{EVAL_ID}:tried:1}}}}"),
        ] {
            let r = parse_tokens(&format!("see {bad} here"));
            assert_eq!(r.errors.len(), 1, "{bad}");
            assert!(r.tokens.is_empty(), "{bad}");
            assert_eq!(r.stripped, "see § here", "{bad}");
        }
    }

    #[test]
    fn token_index_is_a_byte_offset_in_the_canonical_text() {
        let text = "\u{e9}t\u{e9} {{sq:e4}} and\u{200b} {{move:g1:6}}";
        let canon = canonical_for_tokens(text);
        assert_eq!(canon, "\u{e9}t\u{e9} {{sq:e4}} and {{move:g1:6}}");
        let r = parse_tokens(text);
        for t in &r.tokens {
            assert_eq!(&canon[t.index..t.index + t.raw.len()], t.raw);
        }
        assert_eq!(r.tokens[0].index, 6);
    }

    #[test]
    fn error_messages_and_positions_follow_ts() {
        let r = parse_tokens("a {{move:g1}} {{num:a.b|}} {{nope:x}} } {{sq:e4");
        let got: Vec<(&str, &str, usize)> = r
            .errors
            .iter()
            .map(|e| (e.message.as_str(), e.raw.as_str(), e.index))
            .collect();
        // Token errors are byte offsets in the canonical text; stray braces in `stripped`.
        assert_eq!(
            got,
            vec![
                ("move token needs {{move:<gameId>:<ply>}}", "{{move:g1}}", 2),
                ("unknown number format \"\"", "{{num:a.b|}}", 14),
                ("unknown token kind \"nope\"", "{{nope:x}}", 27),
                ("stray or unterminated brace", "}", 11),
                ("stray or unterminated brace", "{{", 13),
            ]
        );
        assert_eq!(r.stripped, "a § § § } {{sq:e4");
    }

    #[test]
    fn line_keys_may_contain_colons_and_num_paths_hashes() {
        let r = parse_tokens("{{line:w:italian:2}} {{line:w:italian:x}} {{num:a#1.b|int}}");
        assert_eq!(r.errors, vec![]);
        let bodies: Vec<TokenBody> = r.tokens.into_iter().map(|t| t.body).collect();
        assert_eq!(
            bodies,
            vec![
                TokenBody::Line {
                    line_key: "w:italian".into(),
                    n: Some(2)
                },
                TokenBody::Line {
                    line_key: "w:italian:x".into(),
                    n: None
                },
                TokenBody::Num {
                    path: "a#1.b".into(),
                    fmt: Some(NumFormat::Int)
                },
            ]
        );
    }
}
