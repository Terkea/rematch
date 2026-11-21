//! Port of `.worktrees/m3/packages/ai/test/validator/leaks.test.ts` (each `describe` is a module and
//! each `it` title is snake_case), plus the Rust extras, the TS ∪ hardened union and the recorded
//! differential corpus for `parse_tokens` and `find_leaks`.

use ai::validator::leaks::{Leak, LeakKind, find_leaks, find_leaks_with};
use ai::validator::normalize::NormMode;
use ai::validator::tokens::parse_tokens;

/// TS `ESCAPES`, verbatim (code points written out so the file cannot lose them).
const ESCAPES: [&str; 56] = [
    "12. Nf3 d5",
    "1. e4 e5 2. Nf3 Nc6",
    "Instead d5 was best",
    "...d6",
    "Nxe5 d6 (e4)",
    "e2-e4",
    "Ng1-f3",
    "e2xd5",
    "e8Q",
    "e8(Q)",
    "O\u{2013}O",
    "O\u{2011}O\u{2011}O",
    "O\u{2014}O",
    "O\u{2212}O",
    "O-O",
    "0-0-0",
    "N\u{2066}f3",
    "N\u{34f}f3",
    "\u{265e}\u{fe0f}f3",
    "forty percent",
    "thirty games",
    "ten per cent",
    "one game",
    "three openings",
    "a third of games",
    "twenty-one games",
    "2.5x",
    "3 times",
    "nf3",
    "NF3",
    "\u{2659}e4",
    "Pe4",
    "Nf3Nc6",
    "exd6e.p.",
    "exd6 e.p.",
    "the 14th of October",
    "14 Oct",
    "14/10",
    "2026-10-14",
    ".5%",
    "1,5",
    "1 000",
    "1\u{a0}000",
    "\u{ff11}\u{ff12} games",
    "\u{661}\u{662} games",
    "half of your games",
    "a quarter",
    "a dozen",
    "twice as often",
    "the second game",
    "ten",
    "\u{435}4",
    "N\u{200b}f\u{200c}3",
    "N\u{3164}f3",
    "\u{265f}",
    "\u{2654}",
];

/// Rust extras for the escape loop (the brief's list).
const RUST_ESCAPES: [&str; 5] = [
    "\u{ff2e}\u{ff46}\u{ff13}",
    "e\u{0301}4",
    "Nf3\u{FE0F}",
    "twentieth move",
    "O\u{2011}O\u{2011}O",
];

fn kinds(s: &str) -> Vec<LeakKind> {
    find_leaks(s).into_iter().map(|l| l.kind).collect()
}

mod find_leaks_flags_every_escape_string {
    use super::*;

    #[test]
    fn find_leaks_flags_every_escape_string() {
        assert_eq!(ESCAPES.len(), 56);
        let missed: Vec<&str> = ESCAPES
            .iter()
            .chain(RUST_ESCAPES.iter())
            .copied()
            .filter(|s| find_leaks(&parse_tokens(s).stripped).is_empty())
            .collect();
        assert!(missed.is_empty(), "missed: {missed:?}");
    }
}

mod find_leaks_leaves_clean_prose_alone {
    use super::*;

    #[test]
    fn find_leaks_leaves_clean_prose_alone() {
        let clean = [
            "you tend to rush in sharp positions",
            "the {{sq:e4}} square",
            "you lost the {{move:li-w1a:6}} knight too early; {{best:li-w1a:6}} was safer",
            "your rate was {{num:weaknesses.W1.posteriorRate|pct0}} over {{num:basis.analysedGames|int}} games",
            "Pay attention to king safety and loose pieces. Castling early helps.",
        ];
        for s in clean {
            let r = parse_tokens(s);
            assert_eq!(r.errors, vec![], "{s}");
            assert_eq!(find_leaks(&r.stripped), vec![], "{s}");
        }
    }
}

mod find_leaks_reports_kinds {
    use super::*;

    #[test]
    fn find_leaks_reports_kinds() {
        assert!(kinds("play O-O").contains(&LeakKind::Castling));
        assert!(kinds("play \u{265e}").contains(&LeakKind::Figurine));
        assert!(kinds("about 40").contains(&LeakKind::Digit));
        assert!(kinds("about forty").contains(&LeakKind::NumberWord));
        assert!(kinds("a 14 Oct date").contains(&LeakKind::Date));
    }

    #[test]
    fn detector_order_and_dedup_follow_ts() {
        let l = |kind, text: &str| Leak {
            kind,
            text: text.to_owned(),
        };
        assert_eq!(
            find_leaks("e4 e4 Nf3 ten tens"),
            vec![
                l(LeakKind::Digit, "4"),
                l(LeakKind::Digit, "3"),
                l(LeakKind::NumberWord, "ten"),
                l(LeakKind::NumberWord, "tens"),
                l(LeakKind::Square, "e4"),
                l(LeakKind::Square, "f3"),
            ]
        );
        assert_eq!(
            serde_json::to_value(l(LeakKind::NumberWord, "ten")).unwrap(),
            serde_json::json!({"kind": "number_word", "text": "ten"})
        );
    }
}

/// The TS ∪ hardened union (ADR-R3-2, Task 6): the hardened path reuses `HOMOGLYPHS`, the H2
/// invisibles, the H3 mark drop and the H5 dashes, so it must never hide a TS leak, and its
/// boundaries treat a folded char as a non-letter (origin-aware), so an accented neighbour cannot
/// glue onto a word that needed folding.
mod union {
    use super::*;

    #[test]
    fn the_union_never_loses_a_ts_leak() {
        for s in [
            "ten\u{e9}",
            "ten\u{180f}x",
            "Nf3\u{e9}",
            "O-O\u{e9}",
            "e.p.\u{e9}",
            "\u{e9}one game",
            "three\u{3b9}",
        ] {
            let ts = find_leaks_with(s, NormMode::TsExact);
            assert!(!ts.is_empty(), "{s}");
            assert!(find_leaks(s).starts_with(&ts), "{s}: {:?}", find_leaks(s));
        }
    }

    #[test]
    fn hardened_finds_what_ts_misses() {
        for s in [
            // Greek iota and Armenian o are not in TS's CONFUSABLES; the neighbour is accented.
            "f\u{3b9}ve\u{e9} games",
            "\u{555}-\u{555}\u{e9}",
            "\u{555}-\u{555}",
            // H2: an invisible TS keeps, inside a word.
            "t\u{180f}en games",
            // H3: a mark TS keeps, inside a word.
            "tw\u{301}enty games",
            // H5: a dash TS does not know.
            "O\u{2d7}O",
            // Armenian o in "once", Cyrillic y in "twenty".
            "\u{585}nce",
            "twent\u{443}",
        ] {
            assert_eq!(find_leaks_with(s, NormMode::TsExact), vec![], "{s}");
            assert!(!find_leaks(s).is_empty(), "{s}");
        }
    }

    #[test]
    fn hardened_text_is_reported_in_lowercase() {
        assert!(find_leaks("f\u{3b9}ve\u{e9} games").contains(&Leak {
            kind: LeakKind::NumberWord,
            text: "five".into()
        }));
    }

    #[test]
    fn hardened_keeps_clean_prose_clean() {
        for s in [
            "Pay attention to king safety and loose pieces. Castling early helps.",
            "Caf\u{e9} talk: often your pieces are loose.",
            "\u{c9}t\u{e9} and na\u{ef}ve plans",
        ] {
            assert_eq!(find_leaks(s), vec![], "{s}");
        }
    }
}

/// Differential corpus: TS's `canonicalForTokens`, `parseTokens` and `findLeaks` recorded by
/// `backend/tools/gen-validator-corpus.mjs`. Every row: the canonical text, the stripped text, the
/// tokens and errors (indexes converted to UTF-16) and the TS-exact leak path equal TS exactly, and
/// `find_leaks` starts with TS's list. Rows in `HARDENED` must gain extras (so the list cannot go
/// stale); every other row must equal TS exactly.
mod ts_parity {
    use super::*;
    use ai::validator::tokens::{TokenBody, canonical_for_tokens};
    use serde_json::{Value, json};

    const HARDENED: &[&str] = &[
        "f\u{3b9}ve\u{e9} games",
        "\u{555}-\u{555}\u{e9}",
        "t\u{180f}en games",
        "tw\u{301}enty games",
        "O\u{2d7}O",
        "\u{585}nce",
        "\u{431}\u{430}\u{437}\u{430}",
        // H3 (the mark is dropped, so `e4` is a square) and H5 (U+02D7 is a dash).
        "e\u{301}4",
        "a1-b2 a1xb2 a1\u{2014}b2 a1\u{ff0d}b2 a1\u{2d7}b2",
    ];

    fn utf16(s: &str, byte: usize) -> usize {
        s[..byte].encode_utf16().count()
    }

    fn body_json(b: &TokenBody) -> Value {
        match b {
            TokenBody::Move { game_id, ply } => json!({"kind":"move","gameId":game_id,"ply":ply}),
            TokenBody::Best { game_id, ply } => json!({"kind":"best","gameId":game_id,"ply":ply}),
            TokenBody::Pv {
                game_id,
                ply,
                side,
                n,
            } => {
                json!({"kind":"pv","gameId":game_id,"ply":ply,"side":side.as_str(),"n":n})
            }
            TokenBody::Line { line_key, n } => json!({"kind":"line","lineKey":line_key,"n":n}),
            TokenBody::Num { path, fmt } => {
                json!({"kind":"num","path":path,"fmt":fmt.map(|f| f.as_str())})
            }
            TokenBody::Date { path } => json!({"kind":"date","path":path}),
            TokenBody::Sq { square } => json!({"kind":"sq","square":square}),
            other => json!({"kind": other.kind()}),
        }
    }

    fn leaks(v: &Value) -> Vec<Leak> {
        serde_json::from_value::<Vec<Value>>(v.clone())
            .unwrap()
            .iter()
            .map(|l| Leak {
                kind: serde_json::from_value(l["kind"].clone()).unwrap(),
                text: l["text"].as_str().unwrap().to_owned(),
            })
            .collect()
    }

    fn check(case: &Value, hardened: bool, bad: &mut Vec<String>) {
        let s = case["s"].as_str().unwrap();
        let canon = canonical_for_tokens(s);
        if canon != case["c"].as_str().unwrap() {
            bad.push(format!("canonical {s:?}: {canon:?}"));
        }
        let r = parse_tokens(s);
        if r.stripped != case["st"].as_str().unwrap() {
            bad.push(format!("stripped {s:?}: {:?}", r.stripped));
        }
        let tokens: Vec<Value> = r
            .tokens
            .iter()
            .map(|t| {
                let mut v = body_json(&t.body);
                v["raw"] = json!(t.raw);
                v["index"] = json!(utf16(&canon, t.index));
                v
            })
            .collect();
        if json!(tokens) != case["tk"] {
            bad.push(format!("tokens {s:?}: {tokens:?}"));
        }
        let errors: Vec<Value> = r
            .errors
            .iter()
            .map(|e| {
                let base = if e.message == "stray or unterminated brace" {
                    &r.stripped
                } else {
                    &canon
                };
                json!({"message": e.message, "raw": e.raw, "index": utf16(base, e.index)})
            })
            .collect();
        if json!(errors) != case["er"] {
            bad.push(format!("errors {s:?}: {errors:?}"));
        }
        let mut extras = false;
        for (what, input, key) in [("stripped", r.stripped.as_str(), "l"), ("raw", s, "r")] {
            let ts = leaks(&case[key]);
            let exact = find_leaks_with(input, NormMode::TsExact);
            if exact != ts {
                bad.push(format!("ts path {what} {s:?}: {exact:?}"));
            }
            let all = find_leaks(input);
            if !all.starts_with(&ts) {
                bad.push(format!("union {what} {s:?} lost a TS leak: {all:?}"));
            } else if all.len() > ts.len() {
                extras = true;
                if !hardened {
                    bad.push(format!("union {what} {s:?} has extras: {all:?}"));
                }
            }
        }
        if hardened && !extras {
            bad.push(format!("hardened {s:?} no longer differs from TS"));
        }
    }

    #[test]
    fn rust_matches_the_recorded_ts_output_on_the_leak_corpus() {
        let corpus: Vec<Value> = serde_json::from_str(TS_LEAK_CORPUS).unwrap();
        assert!(corpus.len() > 100, "{} rows", corpus.len());
        let inputs: Vec<&str> = corpus.iter().map(|c| c["s"].as_str().unwrap()).collect();
        for e in ESCAPES.iter().chain(HARDENED) {
            assert!(inputs.contains(e), "{e:?} is not in the corpus");
        }
        let mut bad = Vec::new();
        for case in &corpus {
            let s = case["s"].as_str().unwrap();
            check(case, HARDENED.contains(&s), &mut bad);
        }
        assert!(
            bad.is_empty(),
            "{} mismatches:\n{}",
            bad.len(),
            bad.join("\n")
        );
    }

    // BEGIN GENERATED by backend/tools/gen-validator-corpus.mjs; do not edit by hand.
    // node 22.22.0, icu 77.1, unicode 16.0, m3 4ff8220aed41afb2dd23e04314511f6ebcb1f8e7, 150 inputs
    const TS_LEAK_CORPUS: &str = r#"[
{"s":"12. Nf3 d5","c":"12. Nf3 d5","st":"12. Nf3 d5","tk":[],"er":[],"l":[{"kind":"digit","text":"12"},{"kind":"digit","text":"3"},{"kind":"digit","text":"5"},{"kind":"square","text":"f3"},{"kind":"square","text":"d5"}],"r":[{"kind":"digit","text":"12"},{"kind":"digit","text":"3"},{"kind":"digit","text":"5"},{"kind":"square","text":"f3"},{"kind":"square","text":"d5"}]},
{"s":"1. e4 e5 2. Nf3 Nc6","c":"1. e4 e5 2. Nf3 Nc6","st":"1. e4 e5 2. Nf3 Nc6","tk":[],"er":[],"l":[{"kind":"digit","text":"1"},{"kind":"digit","text":"4"},{"kind":"digit","text":"5"},{"kind":"digit","text":"2"},{"kind":"digit","text":"3"},{"kind":"digit","text":"6"},{"kind":"square","text":"e4"},{"kind":"square","text":"e5"},{"kind":"square","text":"f3"},{"kind":"square","text":"c6"}],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"4"},{"kind":"digit","text":"5"},{"kind":"digit","text":"2"},{"kind":"digit","text":"3"},{"kind":"digit","text":"6"},{"kind":"square","text":"e4"},{"kind":"square","text":"e5"},{"kind":"square","text":"f3"},{"kind":"square","text":"c6"}]},
{"s":"Instead d5 was best","c":"Instead d5 was best","st":"Instead d5 was best","tk":[],"er":[],"l":[{"kind":"digit","text":"5"},{"kind":"square","text":"d5"}],"r":[{"kind":"digit","text":"5"},{"kind":"square","text":"d5"}]},
{"s":"...d6","c":"...d6","st":"...d6","tk":[],"er":[],"l":[{"kind":"digit","text":"6"},{"kind":"square","text":"d6"}],"r":[{"kind":"digit","text":"6"},{"kind":"square","text":"d6"}]},
{"s":"Nxe5 d6 (e4)","c":"Nxe5 d6 (e4)","st":"Nxe5 d6 (e4)","tk":[],"er":[],"l":[{"kind":"digit","text":"5"},{"kind":"digit","text":"6"},{"kind":"digit","text":"4"},{"kind":"square","text":"e5"},{"kind":"square","text":"d6"},{"kind":"square","text":"e4"}],"r":[{"kind":"digit","text":"5"},{"kind":"digit","text":"6"},{"kind":"digit","text":"4"},{"kind":"square","text":"e5"},{"kind":"square","text":"d6"},{"kind":"square","text":"e4"}]},
{"s":"e2-e4","c":"e2-e4","st":"e2-e4","tk":[],"er":[],"l":[{"kind":"digit","text":"2"},{"kind":"digit","text":"4"},{"kind":"square","text":"e2"},{"kind":"square","text":"e4"},{"kind":"san","text":"e2-e4"}],"r":[{"kind":"digit","text":"2"},{"kind":"digit","text":"4"},{"kind":"square","text":"e2"},{"kind":"square","text":"e4"},{"kind":"san","text":"e2-e4"}]},
{"s":"Ng1-f3","c":"Ng1-f3","st":"Ng1-f3","tk":[],"er":[],"l":[{"kind":"digit","text":"1"},{"kind":"digit","text":"3"},{"kind":"square","text":"g1"},{"kind":"square","text":"f3"},{"kind":"san","text":"g1-f3"}],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"3"},{"kind":"square","text":"g1"},{"kind":"square","text":"f3"},{"kind":"san","text":"g1-f3"}]},
{"s":"e2xd5","c":"e2xd5","st":"e2xd5","tk":[],"er":[],"l":[{"kind":"digit","text":"2"},{"kind":"digit","text":"5"},{"kind":"square","text":"e2"},{"kind":"square","text":"d5"},{"kind":"san","text":"e2xd5"}],"r":[{"kind":"digit","text":"2"},{"kind":"digit","text":"5"},{"kind":"square","text":"e2"},{"kind":"square","text":"d5"},{"kind":"san","text":"e2xd5"}]},
{"s":"e8Q","c":"e8Q","st":"e8Q","tk":[],"er":[],"l":[{"kind":"digit","text":"8"},{"kind":"square","text":"e8"},{"kind":"san","text":"e8q"}],"r":[{"kind":"digit","text":"8"},{"kind":"square","text":"e8"},{"kind":"san","text":"e8q"}]},
{"s":"e8(Q)","c":"e8(Q)","st":"e8(Q)","tk":[],"er":[],"l":[{"kind":"digit","text":"8"},{"kind":"square","text":"e8"},{"kind":"san","text":"e8(q)"}],"r":[{"kind":"digit","text":"8"},{"kind":"square","text":"e8"},{"kind":"san","text":"e8(q)"}]},
{"s":"O\u2013O","c":"O\u2013O","st":"O\u2013O","tk":[],"er":[],"l":[{"kind":"castling","text":"o\u2013o"}],"r":[{"kind":"castling","text":"o\u2013o"}]},
{"s":"O\u2011O\u2011O","c":"O\u2010O\u2010O","st":"O\u2010O\u2010O","tk":[],"er":[],"l":[{"kind":"castling","text":"o\u2010o\u2010o"}],"r":[{"kind":"castling","text":"o\u2010o\u2010o"}]},
{"s":"O\u2014O","c":"O\u2014O","st":"O\u2014O","tk":[],"er":[],"l":[{"kind":"castling","text":"o\u2014o"}],"r":[{"kind":"castling","text":"o\u2014o"}]},
{"s":"O\u2212O","c":"O\u2212O","st":"O\u2212O","tk":[],"er":[],"l":[{"kind":"castling","text":"o\u2212o"}],"r":[{"kind":"castling","text":"o\u2212o"}]},
{"s":"O-O","c":"O-O","st":"O-O","tk":[],"er":[],"l":[{"kind":"castling","text":"o-o"}],"r":[{"kind":"castling","text":"o-o"}]},
{"s":"0-0-0","c":"0-0-0","st":"0-0-0","tk":[],"er":[],"l":[{"kind":"digit","text":"0"},{"kind":"castling","text":"0-0-0"}],"r":[{"kind":"digit","text":"0"},{"kind":"castling","text":"0-0-0"}]},
{"s":"N\u2066f3","c":"Nf3","st":"Nf3","tk":[],"er":[],"l":[{"kind":"digit","text":"3"},{"kind":"square","text":"f3"}],"r":[{"kind":"digit","text":"3"},{"kind":"square","text":"f3"}]},
{"s":"N\u034ff3","c":"Nf3","st":"Nf3","tk":[],"er":[],"l":[{"kind":"digit","text":"3"},{"kind":"square","text":"f3"}],"r":[{"kind":"digit","text":"3"},{"kind":"square","text":"f3"}]},
{"s":"\u265e\ufe0ff3","c":"\u265ef3","st":"\u265ef3","tk":[],"er":[],"l":[{"kind":"digit","text":"3"},{"kind":"square","text":"f3"},{"kind":"figurine","text":"\u265e"}],"r":[{"kind":"digit","text":"3"},{"kind":"square","text":"f3"},{"kind":"figurine","text":"\u265e"}]},
{"s":"forty percent","c":"forty percent","st":"forty percent","tk":[],"er":[],"l":[{"kind":"number_word","text":"forty"},{"kind":"number_word","text":"percent"}],"r":[{"kind":"number_word","text":"forty"},{"kind":"number_word","text":"percent"}]},
{"s":"thirty games","c":"thirty games","st":"thirty games","tk":[],"er":[],"l":[{"kind":"number_word","text":"thirty"}],"r":[{"kind":"number_word","text":"thirty"}]},
{"s":"ten per cent","c":"ten per cent","st":"ten per cent","tk":[],"er":[],"l":[{"kind":"number_word","text":"ten"},{"kind":"number_word","text":"per cent"}],"r":[{"kind":"number_word","text":"ten"},{"kind":"number_word","text":"per cent"}]},
{"s":"one game","c":"one game","st":"one game","tk":[],"er":[],"l":[{"kind":"number_word","text":"one"}],"r":[{"kind":"number_word","text":"one"}]},
{"s":"three openings","c":"three openings","st":"three openings","tk":[],"er":[],"l":[{"kind":"number_word","text":"three"}],"r":[{"kind":"number_word","text":"three"}]},
{"s":"a third of games","c":"a third of games","st":"a third of games","tk":[],"er":[],"l":[{"kind":"number_word","text":"third"}],"r":[{"kind":"number_word","text":"third"}]},
{"s":"twenty-one games","c":"twenty-one games","st":"twenty-one games","tk":[],"er":[],"l":[{"kind":"number_word","text":"twenty"},{"kind":"number_word","text":"one"}],"r":[{"kind":"number_word","text":"twenty"},{"kind":"number_word","text":"one"}]},
{"s":"2.5x","c":"2.5x","st":"2.5x","tk":[],"er":[],"l":[{"kind":"digit","text":"2"},{"kind":"digit","text":"5"},{"kind":"date","text":"2.5"}],"r":[{"kind":"digit","text":"2"},{"kind":"digit","text":"5"},{"kind":"date","text":"2.5"}]},
{"s":"3 times","c":"3 times","st":"3 times","tk":[],"er":[],"l":[{"kind":"digit","text":"3"}],"r":[{"kind":"digit","text":"3"}]},
{"s":"nf3","c":"nf3","st":"nf3","tk":[],"er":[],"l":[{"kind":"digit","text":"3"},{"kind":"square","text":"f3"}],"r":[{"kind":"digit","text":"3"},{"kind":"square","text":"f3"}]},
{"s":"NF3","c":"NF3","st":"NF3","tk":[],"er":[],"l":[{"kind":"digit","text":"3"},{"kind":"square","text":"f3"}],"r":[{"kind":"digit","text":"3"},{"kind":"square","text":"f3"}]},
{"s":"\u2659e4","c":"\u2659e4","st":"\u2659e4","tk":[],"er":[],"l":[{"kind":"digit","text":"4"},{"kind":"square","text":"e4"},{"kind":"figurine","text":"\u2659"}],"r":[{"kind":"digit","text":"4"},{"kind":"square","text":"e4"},{"kind":"figurine","text":"\u2659"}]},
{"s":"Pe4","c":"Pe4","st":"Pe4","tk":[],"er":[],"l":[{"kind":"digit","text":"4"},{"kind":"square","text":"e4"}],"r":[{"kind":"digit","text":"4"},{"kind":"square","text":"e4"}]},
{"s":"Nf3Nc6","c":"Nf3Nc6","st":"Nf3Nc6","tk":[],"er":[],"l":[{"kind":"digit","text":"3"},{"kind":"digit","text":"6"},{"kind":"square","text":"f3"},{"kind":"square","text":"c6"}],"r":[{"kind":"digit","text":"3"},{"kind":"digit","text":"6"},{"kind":"square","text":"f3"},{"kind":"square","text":"c6"}]},
{"s":"exd6e.p.","c":"exd6e.p.","st":"exd6e.p.","tk":[],"er":[],"l":[{"kind":"digit","text":"6"},{"kind":"square","text":"d6"},{"kind":"san","text":"e.p."}],"r":[{"kind":"digit","text":"6"},{"kind":"square","text":"d6"},{"kind":"san","text":"e.p."}]},
{"s":"exd6 e.p.","c":"exd6 e.p.","st":"exd6 e.p.","tk":[],"er":[],"l":[{"kind":"digit","text":"6"},{"kind":"square","text":"d6"},{"kind":"san","text":"e.p."}],"r":[{"kind":"digit","text":"6"},{"kind":"square","text":"d6"},{"kind":"san","text":"e.p."}]},
{"s":"the 14th of October","c":"the 14th of October","st":"the 14th of October","tk":[],"er":[],"l":[{"kind":"digit","text":"14"},{"kind":"date","text":"14th of october"}],"r":[{"kind":"digit","text":"14"},{"kind":"date","text":"14th of october"}]},
{"s":"14 Oct","c":"14 Oct","st":"14 Oct","tk":[],"er":[],"l":[{"kind":"digit","text":"14"},{"kind":"date","text":"14 oct"}],"r":[{"kind":"digit","text":"14"},{"kind":"date","text":"14 oct"}]},
{"s":"14/10","c":"14/10","st":"14/10","tk":[],"er":[],"l":[{"kind":"digit","text":"14"},{"kind":"digit","text":"10"},{"kind":"date","text":"14/10"}],"r":[{"kind":"digit","text":"14"},{"kind":"digit","text":"10"},{"kind":"date","text":"14/10"}]},
{"s":"2026-10-14","c":"2026-10-14","st":"2026-10-14","tk":[],"er":[],"l":[{"kind":"digit","text":"2026"},{"kind":"digit","text":"10"},{"kind":"digit","text":"14"},{"kind":"date","text":"2026-10-14"}],"r":[{"kind":"digit","text":"2026"},{"kind":"digit","text":"10"},{"kind":"digit","text":"14"},{"kind":"date","text":"2026-10-14"}]},
{"s":".5%","c":".5%","st":".5%","tk":[],"er":[],"l":[{"kind":"digit","text":"5"},{"kind":"percent","text":"%"}],"r":[{"kind":"digit","text":"5"},{"kind":"percent","text":"%"}]},
{"s":"1,5","c":"1,5","st":"1,5","tk":[],"er":[],"l":[{"kind":"digit","text":"1"},{"kind":"digit","text":"5"}],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"5"}]},
{"s":"1 000","c":"1 000","st":"1 000","tk":[],"er":[],"l":[{"kind":"digit","text":"1"},{"kind":"digit","text":"000"}],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"000"}]},
{"s":"1\u00a0000","c":"1 000","st":"1 000","tk":[],"er":[],"l":[{"kind":"digit","text":"1"},{"kind":"digit","text":"000"}],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"000"}]},
{"s":"\uff11\uff12 games","c":"12 games","st":"12 games","tk":[],"er":[],"l":[{"kind":"digit","text":"12"}],"r":[{"kind":"digit","text":"12"}]},
{"s":"\u0661\u0662 games","c":"\u0661\u0662 games","st":"\u0661\u0662 games","tk":[],"er":[],"l":[{"kind":"digit","text":"\u0661\u0662"}],"r":[{"kind":"digit","text":"\u0661\u0662"}]},
{"s":"half of your games","c":"half of your games","st":"half of your games","tk":[],"er":[],"l":[{"kind":"number_word","text":"half"}],"r":[{"kind":"number_word","text":"half"}]},
{"s":"a quarter","c":"a quarter","st":"a quarter","tk":[],"er":[],"l":[{"kind":"number_word","text":"quarter"}],"r":[{"kind":"number_word","text":"quarter"}]},
{"s":"a dozen","c":"a dozen","st":"a dozen","tk":[],"er":[],"l":[{"kind":"number_word","text":"dozen"}],"r":[{"kind":"number_word","text":"dozen"}]},
{"s":"twice as often","c":"twice as often","st":"twice as often","tk":[],"er":[],"l":[{"kind":"number_word","text":"twice"}],"r":[{"kind":"number_word","text":"twice"}]},
{"s":"the second game","c":"the second game","st":"the second game","tk":[],"er":[],"l":[{"kind":"number_word","text":"second"}],"r":[{"kind":"number_word","text":"second"}]},
{"s":"ten","c":"ten","st":"ten","tk":[],"er":[],"l":[{"kind":"number_word","text":"ten"}],"r":[{"kind":"number_word","text":"ten"}]},
{"s":"\u04354","c":"\u04354","st":"\u04354","tk":[],"er":[],"l":[{"kind":"digit","text":"4"},{"kind":"square","text":"e4"}],"r":[{"kind":"digit","text":"4"},{"kind":"square","text":"e4"}]},
{"s":"N\u200bf\u200c3","c":"Nf3","st":"Nf3","tk":[],"er":[],"l":[{"kind":"digit","text":"3"},{"kind":"square","text":"f3"}],"r":[{"kind":"digit","text":"3"},{"kind":"square","text":"f3"}]},
{"s":"N\u3164f3","c":"Nf3","st":"Nf3","tk":[],"er":[],"l":[{"kind":"digit","text":"3"},{"kind":"square","text":"f3"}],"r":[{"kind":"digit","text":"3"},{"kind":"square","text":"f3"}]},
{"s":"\u265f","c":"\u265f","st":"\u265f","tk":[],"er":[],"l":[{"kind":"figurine","text":"\u265f"}],"r":[{"kind":"figurine","text":"\u265f"}]},
{"s":"\u2654","c":"\u2654","st":"\u2654","tk":[],"er":[],"l":[{"kind":"figurine","text":"\u2654"}],"r":[{"kind":"figurine","text":"\u2654"}]},
{"s":"\uff2e\uff46\uff13","c":"Nf3","st":"Nf3","tk":[],"er":[],"l":[{"kind":"digit","text":"3"},{"kind":"square","text":"f3"}],"r":[{"kind":"digit","text":"3"},{"kind":"square","text":"f3"}]},
{"s":"e\u03014","c":"\u00e94","st":"\u00e94","tk":[],"er":[],"l":[{"kind":"digit","text":"4"}],"r":[{"kind":"digit","text":"4"}]},
{"s":"Nf3\ufe0f","c":"Nf3","st":"Nf3","tk":[],"er":[],"l":[{"kind":"digit","text":"3"},{"kind":"square","text":"f3"}],"r":[{"kind":"digit","text":"3"},{"kind":"square","text":"f3"}]},
{"s":"twentieth move","c":"twentieth move","st":"twentieth move","tk":[],"er":[],"l":[{"kind":"number_word","text":"twentieth"}],"r":[{"kind":"number_word","text":"twentieth"}]},
{"s":"O\u2011O\u2011O","c":"O\u2010O\u2010O","st":"O\u2010O\u2010O","tk":[],"er":[],"l":[{"kind":"castling","text":"o\u2010o\u2010o"}],"r":[{"kind":"castling","text":"o\u2010o\u2010o"}]},
{"s":"you tend to rush in sharp positions","c":"you tend to rush in sharp positions","st":"you tend to rush in sharp positions","tk":[],"er":[],"l":[],"r":[]},
{"s":"the {{sq:e4}} square","c":"the {{sq:e4}} square","st":"the \u00a7 square","tk":[{"kind":"sq","square":"e4","raw":"{{sq:e4}}","index":4}],"er":[],"l":[],"r":[{"kind":"digit","text":"4"},{"kind":"square","text":"e4"}]},
{"s":"you lost the {{move:li-w1a:6}} knight too early; {{best:li-w1a:6}} was safer","c":"you lost the {{move:li-w1a:6}} knight too early; {{best:li-w1a:6}} was safer","st":"you lost the \u00a7 knight too early; \u00a7 was safer","tk":[{"kind":"move","gameId":"li-w1a","ply":6,"raw":"{{move:li-w1a:6}}","index":13},{"kind":"best","gameId":"li-w1a","ply":6,"raw":"{{best:li-w1a:6}}","index":49}],"er":[],"l":[],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"6"}]},
{"s":"your rate was {{num:weaknesses.W1.posteriorRate|pct0}} over {{num:basis.analysedGames|int}} games","c":"your rate was {{num:weaknesses.W1.posteriorRate|pct0}} over {{num:basis.analysedGames|int}} games","st":"your rate was \u00a7 over \u00a7 games","tk":[{"kind":"num","path":"weaknesses.W1.posteriorRate","fmt":"pct0","raw":"{{num:weaknesses.W1.posteriorRate|pct0}}","index":14},{"kind":"num","path":"basis.analysedGames","fmt":"int","raw":"{{num:basis.analysedGames|int}}","index":60}],"er":[],"l":[],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"0"}]},
{"s":"Pay attention to king safety and loose pieces. Castling early helps.","c":"Pay attention to king safety and loose pieces. Castling early helps.","st":"Pay attention to king safety and loose pieces. Castling early helps.","tk":[],"er":[],"l":[],"r":[]},
{"s":"a {{move:g1:6}} b {{best:g1:6}} c {{pv:g1:6:best:2}} d {{pv:g1:6:refutation:1}} e {{line:w-italian}} f {{line:w-italian:3}} g {{num:basis.analysedGames}} h {{num:weaknesses.W1.posteriorRate|pct1}} i {{date:basis.windowStart}} j {{sq:f7}}","c":"a {{move:g1:6}} b {{best:g1:6}} c {{pv:g1:6:best:2}} d {{pv:g1:6:refutation:1}} e {{line:w-italian}} f {{line:w-italian:3}} g {{num:basis.analysedGames}} h {{num:weaknesses.W1.posteriorRate|pct1}} i {{date:basis.windowStart}} j {{sq:f7}}","st":"a \u00a7 b \u00a7 c \u00a7 d \u00a7 e \u00a7 f \u00a7 g \u00a7 h \u00a7 i \u00a7 j \u00a7","tk":[{"kind":"move","gameId":"g1","ply":6,"raw":"{{move:g1:6}}","index":2},{"kind":"best","gameId":"g1","ply":6,"raw":"{{best:g1:6}}","index":18},{"kind":"pv","gameId":"g1","ply":6,"side":"best","n":2,"raw":"{{pv:g1:6:best:2}}","index":34},{"kind":"pv","gameId":"g1","ply":6,"side":"refutation","n":1,"raw":"{{pv:g1:6:refutation:1}}","index":55},{"kind":"line","lineKey":"w-italian","n":null,"raw":"{{line:w-italian}}","index":82},{"kind":"line","lineKey":"w-italian","n":3,"raw":"{{line:w-italian:3}}","index":103},{"kind":"num","path":"basis.analysedGames","fmt":null,"raw":"{{num:basis.analysedGames}}","index":126},{"kind":"num","path":"weaknesses.W1.posteriorRate","fmt":"pct1","raw":"{{num:weaknesses.W1.posteriorRate|pct1}}","index":156},{"kind":"date","path":"basis.windowStart","raw":"{{date:basis.windowStart}}","index":199},{"kind":"sq","square":"f7","raw":"{{sq:f7}}","index":228}],"er":[],"l":[],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"6"},{"kind":"digit","text":"2"},{"kind":"digit","text":"3"},{"kind":"digit","text":"7"},{"kind":"square","text":"g1"},{"kind":"square","text":"f7"}]},
{"s":"x {{move:g1:6}} y {{sq:e4}}","c":"x {{move:g1:6}} y {{sq:e4}}","st":"x \u00a7 y \u00a7","tk":[{"kind":"move","gameId":"g1","ply":6,"raw":"{{move:g1:6}}","index":2},{"kind":"sq","square":"e4","raw":"{{sq:e4}}","index":18}],"er":[],"l":[],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"6"},{"kind":"digit","text":"4"},{"kind":"square","text":"g1"},{"kind":"square","text":"e4"}]},
{"s":"see {{move:g1}} here","c":"see {{move:g1}} here","st":"see \u00a7 here","tk":[],"er":[{"message":"move token needs {{move:<gameId>:<ply>}}","raw":"{{move:g1}}","index":4}],"l":[],"r":[{"kind":"digit","text":"1"},{"kind":"square","text":"g1"}]},
{"s":"see {{move:g1:x}} here","c":"see {{move:g1:x}} here","st":"see \u00a7 here","tk":[],"er":[{"message":"move token needs {{move:<gameId>:<ply>}}","raw":"{{move:g1:x}}","index":4}],"l":[],"r":[{"kind":"digit","text":"1"},{"kind":"square","text":"g1"}]},
{"s":"see {{move:g1:0}} here","c":"see {{move:g1:0}} here","st":"see \u00a7 here","tk":[],"er":[{"message":"move token needs {{move:<gameId>:<ply>}}","raw":"{{move:g1:0}}","index":4}],"l":[],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"0"},{"kind":"square","text":"g1"}]},
{"s":"see {{pv:g1:6:middle:1}} here","c":"see {{pv:g1:6:middle:1}} here","st":"see \u00a7 here","tk":[],"er":[{"message":"pv token needs {{pv:<gameId>:<ply>:best|refutation:<n>}}","raw":"{{pv:g1:6:middle:1}}","index":4}],"l":[],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"6"},{"kind":"square","text":"g1"}]},
{"s":"see {{pv:g1:6:best:0}} here","c":"see {{pv:g1:6:best:0}} here","st":"see \u00a7 here","tk":[],"er":[{"message":"pv token needs {{pv:<gameId>:<ply>:best|refutation:<n>}}","raw":"{{pv:g1:6:best:0}}","index":4}],"l":[],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"6"},{"kind":"digit","text":"0"},{"kind":"square","text":"g1"}]},
{"s":"see {{num:a.b|bogus}} here","c":"see {{num:a.b|bogus}} here","st":"see \u00a7 here","tk":[],"er":[{"message":"unknown number format \"bogus\"","raw":"{{num:a.b|bogus}}","index":4}],"l":[],"r":[]},
{"s":"see {{num:}} here","c":"see {{num:}} here","st":"see \u00a7 here","tk":[],"er":[{"message":"num token needs {{num:<path>[|<fmt>]}}","raw":"{{num:}}","index":4}],"l":[],"r":[]},
{"s":"see {{sq:i9}} here","c":"see {{sq:i9}} here","st":"see \u00a7 here","tk":[],"er":[{"message":"sq token needs a square name a1-h8","raw":"{{sq:i9}}","index":4}],"l":[],"r":[{"kind":"digit","text":"9"}]},
{"s":"see {{sq:e44}} here","c":"see {{sq:e44}} here","st":"see \u00a7 here","tk":[],"er":[{"message":"sq token needs a square name a1-h8","raw":"{{sq:e44}}","index":4}],"l":[],"r":[{"kind":"digit","text":"44"},{"kind":"square","text":"e4"}]},
{"s":"see {{nope:x}} here","c":"see {{nope:x}} here","st":"see \u00a7 here","tk":[],"er":[{"message":"unknown token kind \"nope\"","raw":"{{nope:x}}","index":4}],"l":[],"r":[]},
{"s":"see {{move:g1:6 here","c":"see {{move:g1:6 here","st":"see {{move:g1:6 here","tk":[],"er":[{"message":"stray or unterminated brace","raw":"{{","index":4}],"l":[{"kind":"digit","text":"1"},{"kind":"digit","text":"6"},{"kind":"square","text":"g1"}],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"6"},{"kind":"square","text":"g1"}]},
{"s":"see move:g1:6}} here","c":"see move:g1:6}} here","st":"see move:g1:6}} here","tk":[],"er":[{"message":"stray or unterminated brace","raw":"}}","index":13}],"l":[{"kind":"digit","text":"1"},{"kind":"digit","text":"6"},{"kind":"square","text":"g1"}],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"6"},{"kind":"square","text":"g1"}]},
{"s":"see {{}} here","c":"see {{}} here","st":"see \u00a7 here","tk":[],"er":[{"message":"unknown token kind \"\"","raw":"{{}}","index":4}],"l":[],"r":[]},
{"s":"{{num:a.b|int}} {{num:a.b|pct0}} {{num:a.b|dec2}} {{num:a.b|signed1}}","c":"{{num:a.b|int}} {{num:a.b|pct0}} {{num:a.b|dec2}} {{num:a.b|signed1}}","st":"\u00a7 \u00a7 \u00a7 \u00a7","tk":[{"kind":"num","path":"a.b","fmt":"int","raw":"{{num:a.b|int}}","index":0},{"kind":"num","path":"a.b","fmt":"pct0","raw":"{{num:a.b|pct0}}","index":16},{"kind":"num","path":"a.b","fmt":"dec2","raw":"{{num:a.b|dec2}}","index":33},{"kind":"num","path":"a.b","fmt":"signed1","raw":"{{num:a.b|signed1}}","index":50}],"er":[],"l":[],"r":[{"kind":"digit","text":"0"},{"kind":"digit","text":"2"},{"kind":"digit","text":"1"},{"kind":"square","text":"c2"},{"kind":"square","text":"d1"}]},
{"s":"{{mo\u200bve:g1:\uff16}}","c":"{{move:g1:6}}","st":"\u00a7","tk":[{"kind":"move","gameId":"g1","ply":6,"raw":"{{move:g1:6}}","index":0}],"er":[],"l":[],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"6"},{"kind":"square","text":"g1"}]},
{"s":"play O-O","c":"play O-O","st":"play O-O","tk":[],"er":[],"l":[{"kind":"castling","text":"o-o"}],"r":[{"kind":"castling","text":"o-o"}]},
{"s":"play \u265e","c":"play \u265e","st":"play \u265e","tk":[],"er":[],"l":[{"kind":"figurine","text":"\u265e"}],"r":[{"kind":"figurine","text":"\u265e"}]},
{"s":"about 40","c":"about 40","st":"about 40","tk":[],"er":[],"l":[{"kind":"digit","text":"40"}],"r":[{"kind":"digit","text":"40"}]},
{"s":"about forty","c":"about forty","st":"about forty","tk":[],"er":[],"l":[{"kind":"number_word","text":"forty"}],"r":[{"kind":"number_word","text":"forty"}]},
{"s":"a 14 Oct date","c":"a 14 Oct date","st":"a 14 Oct date","tk":[],"er":[],"l":[{"kind":"digit","text":"14"},{"kind":"date","text":"14 oct"}],"r":[{"kind":"digit","text":"14"},{"kind":"date","text":"14 oct"}]},
{"s":"a {{move:g1}} {{num:a.b|}} {{nope:x}} } {{sq:e4","c":"a {{move:g1}} {{num:a.b|}} {{nope:x}} } {{sq:e4","st":"a \u00a7 \u00a7 \u00a7 } {{sq:e4","tk":[],"er":[{"message":"move token needs {{move:<gameId>:<ply>}}","raw":"{{move:g1}}","index":2},{"message":"unknown number format \"\"","raw":"{{num:a.b|}}","index":14},{"message":"unknown token kind \"nope\"","raw":"{{nope:x}}","index":27},{"message":"stray or unterminated brace","raw":"}","index":8},{"message":"stray or unterminated brace","raw":"{{","index":10}],"l":[{"kind":"digit","text":"4"},{"kind":"square","text":"e4"}],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"4"},{"kind":"square","text":"g1"},{"kind":"square","text":"e4"}]},
{"s":"{{line:w:italian:2}} {{line:w:italian:x}} {{num:a#1.b|int}}","c":"{{line:w:italian:2}} {{line:w:italian:x}} {{num:a#1.b|int}}","st":"\u00a7 \u00a7 \u00a7","tk":[{"kind":"line","lineKey":"w:italian","n":2,"raw":"{{line:w:italian:2}}","index":0},{"kind":"line","lineKey":"w:italian:x","n":null,"raw":"{{line:w:italian:x}}","index":21},{"kind":"num","path":"a#1.b","fmt":"int","raw":"{{num:a#1.b|int}}","index":42}],"er":[],"l":[],"r":[{"kind":"digit","text":"2"},{"kind":"digit","text":"1"}]},
{"s":"{{line:}} {{line}} {{num}} {{date:}} {{date:a..b}} {{sq:e4:x}} {{best:g 1:3}} {{move:g1:10000}}","c":"{{line:}} {{line}} {{num}} {{date:}} {{date:a..b}} {{sq:e4:x}} {{best:g 1:3}} {{move:g1:10000}}","st":"\u00a7 \u00a7 \u00a7 \u00a7 \u00a7 \u00a7 \u00a7 \u00a7","tk":[],"er":[{"message":"line token has an invalid line key","raw":"{{line:}}","index":0},{"message":"line token needs {{line:<lineKey>[:<n>]}}","raw":"{{line}}","index":10},{"message":"num token needs {{num:<path>[|<fmt>]}}","raw":"{{num}}","index":19},{"message":"date token needs {{date:<path>}}","raw":"{{date:}}","index":27},{"message":"date token needs {{date:<path>}}","raw":"{{date:a..b}}","index":37},{"message":"sq token needs a square name a1-h8","raw":"{{sq:e4:x}}","index":51},{"message":"best token needs {{best:<gameId>:<ply>}}","raw":"{{best:g 1:3}}","index":63},{"message":"move token needs {{move:<gameId>:<ply>}}","raw":"{{move:g1:10000}}","index":78}],"l":[],"r":[{"kind":"digit","text":"4"},{"kind":"digit","text":"1"},{"kind":"digit","text":"3"},{"kind":"digit","text":"10000"},{"kind":"square","text":"e4"},{"kind":"square","text":"g1"}]},
{"s":"{{pv:g1:6:best:1000}} {{pv:g1:6:best}} {{move:g1:0012}} {{num:a.b|int|pct0}} {{num:a b}}","c":"{{pv:g1:6:best:1000}} {{pv:g1:6:best}} {{move:g1:0012}} {{num:a.b|int|pct0}} {{num:a b}}","st":"\u00a7 \u00a7 \u00a7 \u00a7 \u00a7","tk":[],"er":[{"message":"pv token needs {{pv:<gameId>:<ply>:best|refutation:<n>}}","raw":"{{pv:g1:6:best:1000}}","index":0},{"message":"pv token needs {{pv:<gameId>:<ply>:best|refutation:<n>}}","raw":"{{pv:g1:6:best}}","index":22},{"message":"move token needs {{move:<gameId>:<ply>}}","raw":"{{move:g1:0012}}","index":39},{"message":"num token needs {{num:<path>[|<fmt>]}}","raw":"{{num:a.b|int|pct0}}","index":56},{"message":"num token needs {{num:<path>[|<fmt>]}}","raw":"{{num:a b}}","index":77}],"l":[],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"6"},{"kind":"digit","text":"1000"},{"kind":"digit","text":"0012"},{"kind":"digit","text":"0"},{"kind":"square","text":"g1"}]},
{"s":"{{{sq:e4}}} {{sq:e4}}} {{{{sq:e4}}}}","c":"{{{sq:e4}}} {{sq:e4}}} {{{{sq:e4}}}}","st":"{\u00a7} \u00a7} {{\u00a7}}","tk":[{"kind":"sq","square":"e4","raw":"{{sq:e4}}","index":1},{"kind":"sq","square":"e4","raw":"{{sq:e4}}","index":12},{"kind":"sq","square":"e4","raw":"{{sq:e4}}","index":25}],"er":[{"message":"stray or unterminated brace","raw":"{","index":0},{"message":"stray or unterminated brace","raw":"}","index":2},{"message":"stray or unterminated brace","raw":"}","index":5},{"message":"stray or unterminated brace","raw":"{{","index":7},{"message":"stray or unterminated brace","raw":"}}","index":10}],"l":[],"r":[{"kind":"digit","text":"4"},{"kind":"square","text":"e4"}]},
{"s":"\u00e9t\u00e9 {{sq:e4}} and\u200b {{move:g1:6}} \ud83d\ude00 {{sq:a1}} {","c":"\u00e9t\u00e9 {{sq:e4}} and {{move:g1:6}} \ud83d\ude00 {{sq:a1}} {","st":"\u00e9t\u00e9 \u00a7 and \u00a7 \ud83d\ude00 \u00a7 {","tk":[{"kind":"sq","square":"e4","raw":"{{sq:e4}}","index":4},{"kind":"move","gameId":"g1","ply":6,"raw":"{{move:g1:6}}","index":18},{"kind":"sq","square":"a1","raw":"{{sq:a1}}","index":35}],"er":[{"message":"stray or unterminated brace","raw":"{","index":17}],"l":[],"r":[{"kind":"digit","text":"4"},{"kind":"digit","text":"1"},{"kind":"digit","text":"6"},{"kind":"square","text":"e4"},{"kind":"square","text":"g1"},{"kind":"square","text":"a1"}]},
{"s":"{{move:g1:6\n}} {{sq:\uff45\uff14}} {{\u200bsq:h8}}","c":"{{move:g1:6\n}} {{sq:e4}} {{sq:h8}}","st":"\u00a7 \u00a7 \u00a7","tk":[{"kind":"sq","square":"e4","raw":"{{sq:e4}}","index":15},{"kind":"sq","square":"h8","raw":"{{sq:h8}}","index":25}],"er":[{"message":"move token needs {{move:<gameId>:<ply>}}","raw":"{{move:g1:6\n}}","index":0}],"l":[],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"6"},{"kind":"digit","text":"4"},{"kind":"digit","text":"8"},{"kind":"square","text":"g1"},{"kind":"square","text":"e4"},{"kind":"square","text":"h8"}]},
{"s":"tens tenses tenth tenths sevenths twenties forties nineties fifties","c":"tens tenses tenth tenths sevenths twenties forties nineties fifties","st":"tens tenses tenth tenths sevenths twenties forties nineties fifties","tk":[],"er":[],"l":[{"kind":"number_word","text":"tens"},{"kind":"number_word","text":"tenth"},{"kind":"number_word","text":"tenths"},{"kind":"number_word","text":"sevenths"},{"kind":"number_word","text":"twenties"},{"kind":"number_word","text":"forties"},{"kind":"number_word","text":"nineties"},{"kind":"number_word","text":"fifties"}],"r":[{"kind":"number_word","text":"tens"},{"kind":"number_word","text":"tenth"},{"kind":"number_word","text":"tenths"},{"kind":"number_word","text":"sevenths"},{"kind":"number_word","text":"twenties"},{"kind":"number_word","text":"forties"},{"kind":"number_word","text":"nineties"},{"kind":"number_word","text":"fifties"}]},
{"s":"often attention none alone bones someone","c":"often attention none alone bones someone","st":"often attention none alone bones someone","tk":[],"er":[],"l":[],"r":[]},
{"s":"per  cent percent percents percentage per\u2028cent per\u0085cent","c":"per  cent percent percents percentage per\u2028cent per\u0085cent","st":"per  cent percent percents percentage per\u2028cent per\u0085cent","tk":[],"er":[],"l":[{"kind":"number_word","text":"per  cent"},{"kind":"number_word","text":"percent"},{"kind":"number_word","text":"percents"},{"kind":"number_word","text":"percentage"},{"kind":"number_word","text":"per\u2028cent"}],"r":[{"kind":"number_word","text":"per  cent"},{"kind":"number_word","text":"percent"},{"kind":"number_word","text":"percents"},{"kind":"number_word","text":"percentage"},{"kind":"number_word","text":"per\u2028cent"}]},
{"s":"a couple of pairs, thrice, quadruple, halves","c":"a couple of pairs, thrice, quadruple, halves","st":"a couple of pairs, thrice, quadruple, halves","tk":[],"er":[],"l":[{"kind":"number_word","text":"couple"},{"kind":"number_word","text":"pairs"},{"kind":"number_word","text":"thrice"},{"kind":"number_word","text":"quadruple"},{"kind":"number_word","text":"halves"}],"r":[{"kind":"number_word","text":"couple"},{"kind":"number_word","text":"pairs"},{"kind":"number_word","text":"thrice"},{"kind":"number_word","text":"quadruple"},{"kind":"number_word","text":"halves"}]},
{"s":"TEN Twenty Million","c":"TEN Twenty Million","st":"TEN Twenty Million","tk":[],"er":[],"l":[{"kind":"number_word","text":"ten"},{"kind":"number_word","text":"twenty"},{"kind":"number_word","text":"million"}],"r":[{"kind":"number_word","text":"ten"},{"kind":"number_word","text":"twenty"},{"kind":"number_word","text":"million"}]},
{"s":"fourteenths and hundredths","c":"fourteenths and hundredths","st":"fourteenths and hundredths","tk":[],"er":[],"l":[{"kind":"number_word","text":"fourteenths"},{"kind":"number_word","text":"hundredths"}],"r":[{"kind":"number_word","text":"fourteenths"},{"kind":"number_word","text":"hundredths"}]},
{"s":"xten tenx 1ten ten1","c":"xten tenx 1ten ten1","st":"xten tenx 1ten ten1","tk":[],"er":[],"l":[{"kind":"digit","text":"1"},{"kind":"number_word","text":"ten"}],"r":[{"kind":"digit","text":"1"},{"kind":"number_word","text":"ten"}]},
{"s":"zeroes dozens","c":"zeroes dozens","st":"zeroes dozens","tk":[],"er":[],"l":[{"kind":"number_word","text":"zeroes"},{"kind":"number_word","text":"dozens"}],"r":[{"kind":"number_word","text":"zeroes"},{"kind":"number_word","text":"dozens"}]},
{"s":"e8 = q) and a1 (n)x and h8qa and b1r","c":"e8 = q) and a1 (n)x and h8qa and b1r","st":"e8 = q) and a1 (n)x and h8qa and b1r","tk":[],"er":[],"l":[{"kind":"digit","text":"8"},{"kind":"digit","text":"1"},{"kind":"square","text":"e8"},{"kind":"square","text":"a1"},{"kind":"square","text":"h8"},{"kind":"square","text":"b1"},{"kind":"san","text":"e8 = q)"},{"kind":"san","text":"a1 (n"},{"kind":"san","text":"b1r"}],"r":[{"kind":"digit","text":"8"},{"kind":"digit","text":"1"},{"kind":"square","text":"e8"},{"kind":"square","text":"a1"},{"kind":"square","text":"h8"},{"kind":"square","text":"b1"},{"kind":"san","text":"e8 = q)"},{"kind":"san","text":"a1 (n"},{"kind":"san","text":"b1r"}]},
{"s":"g8 ( Q ) or c1=b!","c":"g8 ( Q ) or c1=b!","st":"g8 ( Q ) or c1=b!","tk":[],"er":[],"l":[{"kind":"digit","text":"8"},{"kind":"digit","text":"1"},{"kind":"square","text":"g8"},{"kind":"square","text":"c1"},{"kind":"san","text":"g8 ( q"},{"kind":"san","text":"c1=b"}],"r":[{"kind":"digit","text":"8"},{"kind":"digit","text":"1"},{"kind":"square","text":"g8"},{"kind":"square","text":"c1"},{"kind":"san","text":"g8 ( q"},{"kind":"san","text":"c1=b"}]},
{"s":"ep. e.p e p. e. p. xe.p. e.p.x","c":"ep. e.p e p. e. p. xe.p. e.p.x","st":"ep. e.p e p. e. p. xe.p. e.p.x","tk":[],"er":[],"l":[{"kind":"san","text":"ep."},{"kind":"san","text":"e p."},{"kind":"san","text":"e. p."}],"r":[{"kind":"san","text":"ep."},{"kind":"san","text":"e p."},{"kind":"san","text":"e. p."}]},
{"s":"a1-b2 a1xb2 a1\u2014b2 a1\uff0db2 a1\u02d7b2","c":"a1-b2 a1xb2 a1\u2014b2 a1-b2 a1\u02d7b2","st":"a1-b2 a1xb2 a1\u2014b2 a1-b2 a1\u02d7b2","tk":[],"er":[],"l":[{"kind":"digit","text":"1"},{"kind":"digit","text":"2"},{"kind":"square","text":"a1"},{"kind":"square","text":"b2"},{"kind":"san","text":"a1-b2"},{"kind":"san","text":"a1xb2"},{"kind":"san","text":"a1\u2014b2"}],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"2"},{"kind":"square","text":"a1"},{"kind":"square","text":"b2"},{"kind":"san","text":"a1-b2"},{"kind":"san","text":"a1xb2"},{"kind":"san","text":"a1\u2014b2"}]},
{"s":"o-o-o-o 0-0x o - o ao-o o-o1 o--o 0\u2013 0","c":"o-o-o-o 0-0x o - o ao-o o-o1 o--o 0\u2013 0","st":"o-o-o-o 0-0x o - o ao-o o-o1 o--o 0\u2013 0","tk":[],"er":[],"l":[{"kind":"digit","text":"0"},{"kind":"digit","text":"1"},{"kind":"castling","text":"o-o-o"},{"kind":"castling","text":"o - o"},{"kind":"castling","text":"o--o"},{"kind":"castling","text":"0\u2013 0"}],"r":[{"kind":"digit","text":"0"},{"kind":"digit","text":"1"},{"kind":"castling","text":"o-o-o"},{"kind":"castling","text":"o - o"},{"kind":"castling","text":"o--o"},{"kind":"castling","text":"0\u2013 0"}]},
{"s":"O-O-O! and O-O+","c":"O-O-O! and O-O+","st":"O-O-O! and O-O+","tk":[],"er":[],"l":[{"kind":"castling","text":"o-o-o"},{"kind":"castling","text":"o-o"}],"r":[{"kind":"castling","text":"o-o-o"},{"kind":"castling","text":"o-o"}]},
{"s":"jan 5 janx 5 jan. the 3rd","c":"jan 5 janx 5 jan. the 3rd","st":"jan 5 janx 5 jan. the 3rd","tk":[],"er":[],"l":[{"kind":"digit","text":"5"},{"kind":"digit","text":"3"},{"kind":"date","text":"jan 5"},{"kind":"date","text":"5 jan"}],"r":[{"kind":"digit","text":"5"},{"kind":"digit","text":"3"},{"kind":"date","text":"jan 5"},{"kind":"date","text":"5 jan"}]},
{"s":"the 3rd of may","c":"the 3rd of may","st":"the 3rd of may","tk":[],"er":[],"l":[{"kind":"digit","text":"3"},{"kind":"date","text":"3rd of may"}],"r":[{"kind":"digit","text":"3"},{"kind":"date","text":"3rd of may"}]},
{"s":"3 of mayo","c":"3 of mayo","st":"3 of mayo","tk":[],"er":[],"l":[{"kind":"digit","text":"3"}],"r":[{"kind":"digit","text":"3"}]},
{"s":"end of march","c":"end of march","st":"end of march","tk":[],"er":[],"l":[{"kind":"date","text":"end of march"}],"r":[{"kind":"date","text":"end of march"}]},
{"s":"march the fifth","c":"march the fifth","st":"march the fifth","tk":[],"er":[],"l":[{"kind":"number_word","text":"fifth"},{"kind":"date","text":"march the"}],"r":[{"kind":"number_word","text":"fifth"},{"kind":"date","text":"march the"}]},
{"s":"14 junes","c":"14 junes","st":"14 junes","tk":[],"er":[],"l":[{"kind":"digit","text":"14"}],"r":[{"kind":"digit","text":"14"}]},
{"s":"1/2 12 . 3 2026 - 10 - 14","c":"1/2 12 . 3 2026 - 10 - 14","st":"1/2 12 . 3 2026 - 10 - 14","tk":[],"er":[],"l":[{"kind":"digit","text":"1"},{"kind":"digit","text":"2"},{"kind":"digit","text":"12"},{"kind":"digit","text":"3"},{"kind":"digit","text":"2026"},{"kind":"digit","text":"10"},{"kind":"digit","text":"14"},{"kind":"date","text":"1/2"},{"kind":"date","text":"12 . 3"},{"kind":"date","text":"2026 - 10 - 14"}],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"2"},{"kind":"digit","text":"12"},{"kind":"digit","text":"3"},{"kind":"digit","text":"2026"},{"kind":"digit","text":"10"},{"kind":"digit","text":"14"},{"kind":"date","text":"1/2"},{"kind":"date","text":"12 . 3"},{"kind":"date","text":"2026 - 10 - 14"}]},
{"s":"JAN 5","c":"JAN 5","st":"JAN 5","tk":[],"er":[],"l":[{"kind":"digit","text":"5"},{"kind":"date","text":"jan 5"}],"r":[{"kind":"digit","text":"5"},{"kind":"date","text":"jan 5"}]},
{"s":"jan\u00855","c":"jan\u00855","st":"jan\u00855","tk":[],"er":[],"l":[{"kind":"digit","text":"5"}],"r":[{"kind":"digit","text":"5"}]},
{"s":"sept 3 sep 3","c":"sept 3 sep 3","st":"sept 3 sep 3","tk":[],"er":[],"l":[{"kind":"digit","text":"3"},{"kind":"date","text":"sept 3"},{"kind":"date","text":"sep 3"}],"r":[{"kind":"digit","text":"3"},{"kind":"date","text":"sept 3"},{"kind":"date","text":"sep 3"}]},
{"s":"x12 of oct","c":"x12 of oct","st":"x12 of oct","tk":[],"er":[],"l":[{"kind":"digit","text":"12"},{"kind":"date","text":"x12 of oct"}],"r":[{"kind":"digit","text":"12"},{"kind":"date","text":"x12 of oct"}]},
{"s":"12 of oct","c":"12 of oct","st":"12 of oct","tk":[],"er":[],"l":[{"kind":"digit","text":"12"},{"kind":"date","text":"12 of oct"}],"r":[{"kind":"digit","text":"12"},{"kind":"date","text":"12 of oct"}]},
{"s":"5th of june, 2026","c":"5th of june, 2026","st":"5th of june, 2026","tk":[],"er":[],"l":[{"kind":"digit","text":"5"},{"kind":"digit","text":"2026"},{"kind":"date","text":"5th of june"}],"r":[{"kind":"digit","text":"5"},{"kind":"digit","text":"2026"},{"kind":"date","text":"5th of june"}]},
{"s":"\u041d\u0435\u0440\u043e","c":"\u041d\u0435\u0440\u043e","st":"\u041d\u0435\u0440\u043e","tk":[],"er":[],"l":[],"r":[]},
{"s":"\u0391\u039d\u039f","c":"\u0391\u039d\u039f","st":"\u0391\u039d\u039f","tk":[],"er":[],"l":[],"r":[]},
{"s":"\u03c0\u03ac\u03c4\u03b7\u03c3\u03b5","c":"\u03c0\u03ac\u03c4\u03b7\u03c3\u03b5","st":"\u03c0\u03ac\u03c4\u03b7\u03c3\u03b5","tk":[],"er":[],"l":[],"r":[]},
{"s":"\u0130stanbul","c":"\u0130stanbul","st":"\u0130stanbul","tk":[],"er":[],"l":[],"r":[]},
{"s":"\u03a3\u039f\u03a3","c":"\u03a3\u039f\u03a3","st":"\u03a3\u039f\u03a3","tk":[],"er":[],"l":[],"r":[]},
{"s":"\u0391-\u039f","c":"\u0391-\u039f","st":"\u0391-\u039f","tk":[],"er":[],"l":[],"r":[]},
{"s":"\u041e-\u041e","c":"\u041e-\u041e","st":"\u041e-\u041e","tk":[],"er":[],"l":[{"kind":"castling","text":"o-o"}],"r":[{"kind":"castling","text":"o-o"}]},
{"s":"\u2030 and \u066a","c":"\u2030 and \u066a","st":"\u2030 and \u066a","tk":[],"er":[],"l":[{"kind":"percent","text":"\u2030"},{"kind":"percent","text":"\u066a"}],"r":[{"kind":"percent","text":"\u2030"},{"kind":"percent","text":"\u066a"}]},
{"s":"\ud835\udfd9\ud835\udfda","c":"12","st":"12","tk":[],"er":[],"l":[{"kind":"digit","text":"12"}],"r":[{"kind":"digit","text":"12"}]},
{"s":"\u2460 \u00b2 \u216b","c":"1 2 XII","st":"1 2 XII","tk":[],"er":[],"l":[{"kind":"digit","text":"1"},{"kind":"digit","text":"2"}],"r":[{"kind":"digit","text":"1"},{"kind":"digit","text":"2"}]},
{"s":"\ud83e\ude00 \ud83e\ude6f \ud83e\ude70","c":"\ud83e\ude00 \ud83e\ude6f \ud83e\ude70","st":"\ud83e\ude00 \ud83e\ude6f \ud83e\ude70","tk":[],"er":[],"l":[{"kind":"figurine","text":"\ud83e\ude00"},{"kind":"figurine","text":"\ud83e\ude6f"}],"r":[{"kind":"figurine","text":"\ud83e\ude00"},{"kind":"figurine","text":"\ud83e\ude6f"}]},
{"s":"\u0663 games","c":"\u0663 games","st":"\u0663 games","tk":[],"er":[],"l":[{"kind":"digit","text":"\u0663"}],"r":[{"kind":"digit","text":"\u0663"}]},
{"s":"f\u03b9ve\u00e9 games","c":"f\u03b9ve\u00e9 games","st":"f\u03b9ve\u00e9 games","tk":[],"er":[],"l":[],"r":[]},
{"s":"\u0555-\u0555\u00e9","c":"\u0555-\u0555\u00e9","st":"\u0555-\u0555\u00e9","tk":[],"er":[],"l":[],"r":[]},
{"s":"t\u180fen games","c":"t\u180fen games","st":"t\u180fen games","tk":[],"er":[],"l":[],"r":[]},
{"s":"tw\u0301enty games","c":"t\u1e83enty games","st":"t\u1e83enty games","tk":[],"er":[],"l":[],"r":[]},
{"s":"O\u02d7O","c":"O\u02d7O","st":"O\u02d7O","tk":[],"er":[],"l":[],"r":[]},
{"s":"\u0585nce","c":"\u0585nce","st":"\u0585nce","tk":[],"er":[],"l":[],"r":[]},
{"s":"\u0431\u0430\u0437\u0430","c":"\u0431\u0430\u0437\u0430","st":"\u0431\u0430\u0437\u0430","tk":[],"er":[],"l":[],"r":[]},
{"s":"ten\u00e9","c":"ten\u00e9","st":"ten\u00e9","tk":[],"er":[],"l":[{"kind":"number_word","text":"ten"}],"r":[{"kind":"number_word","text":"ten"}]},
{"s":"ten\u180fx","c":"ten\u180fx","st":"ten\u180fx","tk":[],"er":[],"l":[{"kind":"number_word","text":"ten"}],"r":[{"kind":"number_word","text":"ten"}]},
{"s":"Nf3\u00e9","c":"Nf3\u00e9","st":"Nf3\u00e9","tk":[],"er":[],"l":[{"kind":"digit","text":"3"},{"kind":"square","text":"f3"}],"r":[{"kind":"digit","text":"3"},{"kind":"square","text":"f3"}]},
{"s":"O-O\u00e9","c":"O-O\u00e9","st":"O-O\u00e9","tk":[],"er":[],"l":[{"kind":"castling","text":"o-o"}],"r":[{"kind":"castling","text":"o-o"}]},
{"s":"e.p.\u00e9","c":"e.p.\u00e9","st":"e.p.\u00e9","tk":[],"er":[],"l":[{"kind":"san","text":"e.p."}],"r":[{"kind":"san","text":"e.p."}]},
{"s":"\u00e9one game","c":"\u00e9one game","st":"\u00e9one game","tk":[],"er":[],"l":[{"kind":"number_word","text":"one"}],"r":[{"kind":"number_word","text":"one"}]},
{"s":"three\u03b9","c":"three\u03b9","st":"three\u03b9","tk":[],"er":[],"l":[{"kind":"number_word","text":"three"}],"r":[{"kind":"number_word","text":"three"}]},
{"s":"Caf\u00e9 talk: often your pieces are loose.","c":"Caf\u00e9 talk: often your pieces are loose.","st":"Caf\u00e9 talk: often your pieces are loose.","tk":[],"er":[],"l":[],"r":[]},
{"s":"\u00c9t\u00e9 and na\u00efve plans","c":"\u00c9t\u00e9 and na\u00efve plans","st":"\u00c9t\u00e9 and na\u00efve plans","tk":[],"er":[],"l":[],"r":[]}
]"#;
    // END GENERATED
}
