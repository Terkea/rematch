//! Port of `.worktrees/m3/packages/ai/test/validator/extract-gaps.test.ts`, plus the Rust-only
//! checks for `bounded_matches` (deviation 2, ADR-R3-2) and linear time on hostile input.

use std::cell::Cell;
use std::time::{Duration, Instant};

use ai::validator::bounded::bounded_matches;
use ai::validator::extract::{
    ExtractedNumber, NumberKind, extract_dates, extract_moves, extract_numbers,
};
use ai::validator::normalize::{normalize_text, strip_invisibles};
use regex::Regex;

fn values(ns: &[ExtractedNumber]) -> Vec<f64> {
    ns.iter().map(|n| n.value).collect()
}

fn san(text: &str) -> Vec<String> {
    extract_moves(text).san
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| (*x).to_owned()).collect()
}

fn one(text: &str) -> ExtractedNumber {
    let r = extract_numbers(text);
    assert_eq!(r.len(), 1, "{text}: {r:?}");
    r[0].clone()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 5e-10
}

mod extract_moves_gaps {
    use super::*;

    #[test]
    fn counts_bare_pawn_moves_adjacent_to_a_san_move_or_move_number() {
        assert_eq!(san("12. Nf3 d5"), s(&["Nf3", "d5"]));
        assert_eq!(san("1. e4 e5 2. Nf3 Nc6"), s(&["e4", "e5", "Nf3", "Nc6"]));
        assert_eq!(san("Nxe5 d6 (e4)"), s(&["Nxe5", "d6", "e4"]));
        assert_eq!(san("...d6"), s(&["d6"]));
        assert_eq!(san("Nxe5 ...d6"), s(&["Nxe5", "d6"]));
    }

    #[test]
    fn still_ignores_squares_in_prose() {
        assert_eq!(san("the knight on f3 guards e5 and d4"), s(&[]));
        assert_eq!(san("after Nf3 and then the pawn on e5"), s(&["Nf3"]));
    }

    #[test]
    fn reads_long_algebraic_as_uci() {
        assert_eq!(extract_moves("e2-e4").uci, s(&["e2e4"]));
        assert_eq!(extract_moves("Ng1-f3").uci, s(&["g1f3"]));
        assert_eq!(
            extract_moves("e2xd5 and Qd1\u{2013}h5").uci,
            s(&["e2d5", "d1h5"])
        );
    }

    #[test]
    fn reads_promotion_without_equals_sign() {
        assert_eq!(san("e8Q"), s(&["e8=Q"]));
        assert_eq!(san("e8(Q)"), s(&["e8=Q"]));
        assert_eq!(san("exd8N+"), s(&["exd8=N"]));
    }

    #[test]
    fn reads_castling_with_any_dash_variant() {
        for d in [
            "-", "\u{2013}", "\u{2014}", "\u{2011}", "\u{2212}", "\u{2010}",
        ] {
            assert_eq!(san(&format!("O{d}O")), s(&["O-O"]), "{d:?}");
            assert_eq!(san(&format!("0{d}0{d}0")), s(&["O-O-O"]), "{d:?}");
            assert_eq!(san(&format!("O{d}O{d}O")), s(&["O-O-O"]), "{d:?}");
        }
    }

    #[test]
    fn strips_the_full_invisible_set_inside_moves() {
        let set = [
            '\u{200b}', '\u{200c}', '\u{200d}', '\u{feff}', '\u{2060}', '\u{ad}', '\u{34f}',
            '\u{2066}', '\u{2069}', '\u{202a}', '\u{202e}', '\u{fe0f}', '\u{fe00}', '\u{3164}',
            '\u{115f}', '\u{17b4}', '\u{17b5}',
        ];
        assert_eq!(set.len(), 17);
        for c in set {
            assert_eq!(san(&format!("N{c}f3")), s(&["Nf3"]), "{c:?}");
        }
    }
}

mod extract_numbers_gaps {
    use super::*;

    #[test]
    fn reads_number_words_beyond_twenty_with_singular_and_plural_units() {
        assert_eq!(one("thirty games").value, 30.0);
        assert_eq!(one("forty-two moves").value, 42.0);
        assert_eq!(one("twenty one blunders").value, 21.0);
        assert_eq!(one("one game").value, 1.0);
        assert_eq!(one("a hundred games").value, 100.0);
        assert_eq!(one("two hundred and five games").value, 205.0);
        assert_eq!(one("three openings").value, 3.0);
        assert_eq!(extract_numbers("one of the games"), vec![]);
    }

    #[test]
    fn reads_fractions_as_ratios() {
        assert_eq!(one("a third of games").kind, NumberKind::Ratio);
        assert!(close(
            extract_numbers("a third of games")[0].value,
            1.0 / 3.0
        ));
        assert!(close(extract_numbers("a quarter of games")[0].value, 0.25));
        assert!(close(extract_numbers("two thirds")[0].value, 2.0 / 3.0));
        let h = one("half of your games");
        assert!(h.value == 0.5 && h.kind == NumberKind::Ratio);
    }

    #[test]
    fn only_counts_half_in_quantitative_uses() {
        assert_eq!(extract_numbers("in the second half of the game"), vec![]);
        assert_eq!(extract_numbers("a half-open file"), vec![]);
        assert_eq!(extract_numbers("the first half"), vec![]);
    }

    #[test]
    fn types_digit_ratios() {
        let r = one("2.5x");
        assert!(r.value == 2.5 && r.kind == NumberKind::Ratio && r.decimals == 1);
        let r = one("3 times as often");
        assert!(r.value == 3.0 && r.kind == NumberKind::Ratio);
        let r = one("2\u{d7} more");
        assert!(r.value == 2.0 && r.kind == NumberKind::Ratio);
        assert_eq!(extract_numbers("31% of games")[0].kind, NumberKind::Plain);
    }

    #[test]
    fn parses_a_leading_dot_decimal() {
        let r = one(".5%");
        assert!(r.value == 0.5 && r.percent && r.decimals == 1);
    }

    #[test]
    fn treats_a_percent_range_as_a_range_not_a_negative() {
        assert_eq!(values(&extract_numbers("50%-60%")), vec![50.0, 60.0]);
        assert_eq!(
            values(&extract_numbers("50 %\u{2013}60 %")),
            vec![50.0, 60.0]
        );
    }

    #[test]
    fn flags_comma_decimals_as_ambiguous_instead_of_splitting() {
        let r = extract_numbers("about 1,5 games");
        assert_eq!(r.len(), 1);
        assert!(r[0].ambiguous);
        assert!(!extract_numbers("3,912 games")[0].ambiguous);
    }

    #[test]
    fn reads_a_space_grouped_thousand_nbsp() {
        assert_eq!(values(&extract_numbers("1\u{a0}000 games")), vec![1000.0]);
        assert_eq!(values(&extract_numbers("1\u{202f}000 games")), vec![1000.0]);
        assert_eq!(normalize_text("1\u{a0}000"), "1,000");
    }
}

mod rust_only {
    use super::*;

    #[test]
    fn bounded_matches_retries_after_a_rejected_boundary() {
        let re = Regex::new("ab").unwrap();
        let not_letter = |c: Option<char>| !c.is_some_and(|c| c.is_ascii_alphabetic());
        assert_eq!(
            bounded_matches(&re, "xab ab", not_letter, |_| true),
            vec![(4, 6)]
        );
    }

    #[test]
    fn bounded_matches_checks_the_char_after_and_retries_from_start_plus_one() {
        let re = Regex::new("[a-c]+").unwrap();
        // "abc" is followed by a digit, so it is rejected; the retry from start + 1 finds "bc",
        // whose end is the same, so it is rejected too, and so on: nothing matches.
        let not_digit = |c: Option<char>| !c.is_some_and(|c| c.is_ascii_digit());
        assert_eq!(
            bounded_matches(&re, "abc1 ab", |_| true, not_digit),
            vec![(5, 7)]
        );
    }

    /// The TS output for the same input, recorded with Node 22 on m3's `extract.ts`:
    /// `extractNumbers("٣ games")` is `[]` (JS `\d` is ASCII-only and NFKC keeps U+0663).
    #[test]
    fn rust_digit_class_is_ascii_like_js() {
        assert_eq!(extract_numbers("\u{663} games"), vec![]);
        assert_eq!(extract_numbers("\u{663}\u{661}\u{664} games"), vec![]);
    }

    #[test]
    fn strip_invisibles_removes_the_tag_and_variation_planes() {
        assert_eq!(strip_invisibles("a\u{e0001}b\u{e0100}c\u{e01ef}d"), "abcd");
        assert_eq!(strip_invisibles("a\u{e01f0}b"), "a\u{e01f0}b");
    }

    /// Before-check rejections must not rescan: the predicate runs O(n) times.
    #[test]
    fn bounded_matches_is_linear_in_rejections() {
        let n = 200_000;
        let text = format!("x{}", "1".repeat(n));
        let re = Regex::new("[0-9]+").unwrap();
        let calls = Cell::new(0usize);
        let not_alnum = |c: Option<char>| {
            calls.set(calls.get() + 1);
            !c.is_some_and(|c| c.is_ascii_alphanumeric())
        };
        assert_eq!(bounded_matches(&re, &text, not_alnum, |_| true), vec![]);
        assert!(calls.get() <= n + 2, "{} predicate calls", calls.get());
    }

    /// Hostile text for every extractor: long runs that the lookaround emulation rejects again and
    /// again. A quadratic retry would take minutes at this size; the bound is generous.
    #[test]
    fn extractors_are_linear_on_large_hostile_text() {
        const N: usize = 1 << 17;
        let hostile = [
            format!("x{}", "1".repeat(N)),
            format!("x{}", "1,111".repeat(N / 5)),
            "1,".repeat(N / 2),
            "e2e4".repeat(N / 4),
            "e2-".repeat(N / 3),
            "O-O-".repeat(N / 4),
            "0-0-0x".repeat(N / 6),
            "Nf3".repeat(N / 3),
            format!("x{}", "one ".repeat(N / 4)),
            "xone ".repeat(N / 5),
            "a ".repeat(N / 2),
            "Oct 1".repeat(N / 5),
            "14 Oct ".repeat(N / 7),
            "2026-10-1".repeat(N / 9),
            "half ".repeat(N / 5),
            "double ".repeat(N / 7),
            " ".repeat(N),
            "\u{a0}1".repeat(N / 2),
            "12. ".repeat(N / 4),
            "W1".repeat(N / 2),
            "\u{200b}".repeat(N),
            "\u{2212}0".repeat(N / 2),
        ];
        let started = Instant::now();
        for text in &hostile {
            let t0 = Instant::now();
            let _ = normalize_text(text);
            let _ = extract_moves(text);
            let _ = extract_numbers(text);
            let _ = extract_dates(text);
            let took = t0.elapsed();
            assert!(
                took < Duration::from_secs(20),
                "{took:?} on {:?}…",
                text.chars().take(12).collect::<String>()
            );
        }
        assert!(started.elapsed() < Duration::from_secs(120));
    }
}

/// Differential parity with the TS reference on edge cases where an exact port matters: lookaheads
/// that the JS engine satisfies by backtracking (`0-0-0x` normalises to `O-O-0x`), inner lookaheads
/// (`1,234,5678`, `Oct 14th:`), UTF-16 window sizes (astral chars before a hedge word), JS `\s`,
/// and the masks. `TS_CORPUS` is the output of m3's `normalizeText`, `extractNumbers`,
/// `extractMoves` and `extractDates` for each input, recorded with Node 22 type stripping.
mod ts_parity {
    use super::*;
    use ai::validator::extract::ExtractedDate;
    use serde_json::Value;

    fn ts_number(v: &Value) -> ExtractedNumber {
        ExtractedNumber {
            value: v["value"].as_f64().unwrap(),
            decimals: u32::try_from(v["decimals"].as_u64().unwrap()).unwrap(),
            percent: v["percent"].as_bool().unwrap(),
            hedged: v["hedged"].as_bool().unwrap(),
            kind: match v["kind"].as_str().unwrap() {
                "ratio" => NumberKind::Ratio,
                _ => NumberKind::Plain,
            },
            ambiguous: v["ambiguous"].as_bool().unwrap(),
        }
    }

    fn strings(v: &Value) -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn rust_matches_the_recorded_ts_output_on_the_edge_corpus() {
        let corpus: Vec<Value> = serde_json::from_str(TS_CORPUS).unwrap();
        assert_eq!(corpus.len(), 110);
        let mut bad = Vec::new();
        for case in &corpus {
            let s = case["s"].as_str().unwrap();
            if normalize_text(s) != case["t"].as_str().unwrap() {
                bad.push(format!("normalize {s:?}: {:?}", normalize_text(s)));
            }
            let n: Vec<ExtractedNumber> = case["n"]
                .as_array()
                .unwrap()
                .iter()
                .map(ts_number)
                .collect();
            let got = extract_numbers(s);
            let same = got.len() == n.len()
                && got.iter().zip(&n).all(|(a, b)| {
                    a.value == b.value
                        && a.decimals == b.decimals
                        && a.percent == b.percent
                        && a.hedged == b.hedged
                        && a.kind == b.kind
                        && a.ambiguous == b.ambiguous
                });
            if !same {
                bad.push(format!("numbers {s:?}: rust {got:?} ts {n:?}"));
            }
            let m = extract_moves(s);
            if m.san != strings(&case["m"]["san"]) || m.uci != strings(&case["m"]["uci"]) {
                bad.push(format!("moves {s:?}: {m:?}"));
            }
            let d: Vec<ExtractedDate> = case["d"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| ExtractedDate {
                    year: x["year"].as_i64().map(|y| i32::try_from(y).unwrap()),
                    month: u32::try_from(x["month"].as_u64().unwrap()).unwrap(),
                    day: u32::try_from(x["day"].as_u64().unwrap()).unwrap(),
                })
                .collect();
            if extract_dates(s) != d {
                bad.push(format!("dates {s:?}: {:?}", extract_dates(s)));
            }
        }
        assert!(
            bad.is_empty(),
            "{} mismatches:\n{}",
            bad.len(),
            bad.join("\n")
        );
    }

    const TS_CORPUS: &str = r#"
[
{"s":"0-0-0x","t":"O-O-0x","n":[{"value":0,"decimals":0,"percent":false,"hedged":false,"kind":"ratio","ambiguous":false}],"m":{"san":["O-O"],"uci":[]},"d":[]},
{"s":"O-O-Ox","t":"O-O-Ox","n":[],"m":{"san":["O-O"],"uci":[]},"d":[]},
{"s":"O-O-O-O","t":"O-O-O-O","n":[],"m":{"san":["O-O-O"],"uci":[]},"d":[]},
{"s":"e7-e8=Qx","t":"e7-e8=Qx","n":[],"m":{"san":[],"uci":["e7e8"]},"d":[]},
{"s":"e8=Qx","t":"e8=Qx","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"e8(Q)x and e8(Qa","t":"e8(Q)x and e8(Qa","n":[],"m":{"san":["e8=Q"],"uci":[]},"d":[]},
{"s":"a7a8qz","t":"a7a8qz","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"g1f3e7e5","t":"g1f3e7e5","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"Ng1\u2013f3 then Kxe1","t":"Ng1\u2013f3 then Kxe1","n":[],"m":{"san":["Kxe1"],"uci":["g1f3"]},"d":[]},
{"s":"O\u2013O and 0\u20140","t":"O-O and O-O","n":[],"m":{"san":["O-O","O-O"],"uci":[]},"d":[]},
{"s":"o-o and oo","t":"o-o and oo","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"1,234,5678 games","t":"1,234,5678 games","n":[{"value":1234,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false},{"value":5678,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"1,2345 games","t":"1,2345 games","n":[{"value":1.2345,"decimals":4,"percent":false,"hedged":false,"kind":"plain","ambiguous":true}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"12,345.5 points","t":"12,345.5 points","n":[{"value":12345.5,"decimals":1,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"1,5.25","t":"1,5.25","n":[{"value":1.525,"decimals":3,"percent":false,"hedged":false,"kind":"plain","ambiguous":true}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"Oct 14th: game","t":"Oct 14th: game","n":[],"m":{"san":[],"uci":[]},"d":[{"year":null,"month":10,"day":14}]},
{"s":"Oct 14:30","t":"Oct 14:30","n":[{"value":14,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false},{"value":30,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"June 2026","t":"June 2026","n":[{"value":2026,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"June 5 2026","t":"June 5 2026","n":[],"m":{"san":[],"uci":[]},"d":[{"year":2026,"month":6,"day":5}]},
{"s":"14 Oct, 2026x","t":"14 Oct, 2026x","n":[{"value":2026,"decimals":0,"percent":false,"hedged":false,"kind":"ratio","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[{"year":null,"month":10,"day":14}]},
{"s":"14 Oct 2026","t":"14 Oct 2026","n":[],"m":{"san":[],"uci":[]},"d":[{"year":2026,"month":10,"day":14}]},
{"s":"21st May 2026","t":"21st May 2026","n":[],"m":{"san":[],"uci":[]},"d":[{"year":2026,"month":5,"day":21}]},
{"s":"May 5","t":"May 5","n":[],"m":{"san":[],"uci":[]},"d":[{"year":null,"month":5,"day":5}]},
{"s":"MAY 5","t":"MAY 5","n":[{"value":5,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"may 5","t":"may 5","n":[{"value":5,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"Sept 3","t":"Sept 3","n":[],"m":{"san":[],"uci":[]},"d":[{"year":null,"month":9,"day":3}]},
{"s":"Sep. 3, 2026","t":"Sep. 3, 2026","n":[],"m":{"san":[],"uci":[]},"d":[{"year":2026,"month":9,"day":3}]},
{"s":"2026-10-14T","t":"2026-10-14T","n":[],"m":{"san":[],"uci":[]},"d":[{"year":2026,"month":10,"day":14}]},
{"s":"12-2026-10-14","t":"12-2026-10-14","n":[{"value":12,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false},{"value":2026,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false},{"value":10,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false},{"value":14,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"3 Septembers","t":"3 Septembers","n":[{"value":3,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"Decx 5","t":"Decx 5","n":[{"value":5,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"5 Mayor","t":"5 Mayor","n":[{"value":5,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"a half-open file","t":"a half-open file","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"double check","t":"double check","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"double-check","t":"double-check","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"double checkmate","t":"double checkmate","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"twice as many","t":"twice as many","n":[{"value":2,"decimals":0,"percent":false,"hedged":false,"kind":"ratio","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"half the games","t":"half the games","n":[{"value":0.5,"decimals":1,"percent":false,"hedged":false,"kind":"ratio","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"the second half","t":"the second half","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"half again as much","t":"half again as much","n":[{"value":0.5,"decimals":1,"percent":false,"hedged":false,"kind":"ratio","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"half your moves","t":"half your moves","n":[{"value":0.5,"decimals":1,"percent":false,"hedged":false,"kind":"ratio","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"one hundred thousand games","t":"one hundred thousand games","n":[{"value":100000,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"thousand thousand games","t":"thousand thousand games","n":[{"value":2000,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"twenty-one games","t":"twenty-one games","n":[{"value":21,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"a dozen games","t":"a dozen games","n":[{"value":12,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"two dozen moves","t":"two dozen moves","n":[{"value":24,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"three-quarters of games","t":"three-quarters of games","n":[{"value":0.75,"decimals":2,"percent":false,"hedged":false,"kind":"ratio","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"an eighth of the games","t":"an eighth of the games","n":[{"value":0.125,"decimals":2,"percent":false,"hedged":false,"kind":"ratio","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"two hundred twelve games","t":"two hundred twelve games","n":[{"value":212,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"ninety nine games","t":"ninety nine games","n":[{"value":99,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"nineteen ninety games","t":"nineteen ninety games","n":[{"value":90,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"one two three games","t":"one two three games","n":[{"value":3,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"a sixteenth","t":"a sixteenth","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"fourty games","t":"fourty games","n":[{"value":40,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"three times as many","t":"three times as many","n":[{"value":3,"decimals":0,"percent":false,"hedged":false,"kind":"ratio","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"eleven times","t":"eleven times","n":[{"value":11,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"2x","t":"2x","n":[{"value":2,"decimals":0,"percent":false,"hedged":false,"kind":"ratio","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"2X faster","t":"2X faster","n":[{"value":2,"decimals":0,"percent":false,"hedged":false,"kind":"ratio","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"2xy","t":"2xy","n":[{"value":2,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"5 \u00d7 3","t":"5 \u00d7 3","n":[{"value":5,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false},{"value":3,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\u22123","t":"-3","n":[{"value":-3,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"+-5","t":"+-5","n":[{"value":-5,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"--5","t":"--5","n":[{"value":-5,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"5%-6%","t":"5%-6%","n":[{"value":5,"decimals":0,"percent":true,"hedged":false,"kind":"plain","ambiguous":false},{"value":6,"decimals":0,"percent":true,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"1.2.3","t":"1.2.3","n":[{"value":1.2,"decimals":1,"percent":false,"hedged":false,"kind":"plain","ambiguous":false},{"value":3,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":".5.5","t":".5.5","n":[{"value":0.5,"decimals":1,"percent":false,"hedged":false,"kind":"plain","ambiguous":false},{"value":5,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"50 % \u2013 60 %","t":"50 % \u2013 60 %","n":[{"value":50,"decimals":0,"percent":true,"hedged":false,"kind":"plain","ambiguous":false},{"value":60,"decimals":0,"percent":true,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"about\u20285 games","t":"about\u20285 games","n":[{"value":5,"decimals":0,"percent":false,"hedged":true,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00 about 5 games","t":"\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00 about 5 games","n":[{"value":5,"decimals":0,"percent":false,"hedged":true,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"roughly                    5 games","t":"roughly                    5 games","n":[{"value":5,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"at   least 5","t":"at   least 5","n":[{"value":5,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"approx. 7 games","t":"approx. 7 games","n":[{"value":7,"decimals":0,"percent":false,"hedged":true,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\u223c9 games","t":"\u223c9 games","n":[{"value":9,"decimals":0,"percent":false,"hedged":true,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"per cent","t":"per cent","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"10 per  cent of games","t":"10 per  cent of games","n":[{"value":10,"decimals":0,"percent":true,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"W12a W1e W123","t":"W12a W1e W123","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"B6 B5","t":"B6 B5","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"1/2-1/2 and \u00bd-\u00bd","t":"1/2-1/2 and 1\u20442-1\u20442","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"about \u00bd of games","t":"about 1\u20442 of games","n":[{"value":1,"decimals":0,"percent":false,"hedged":true,"kind":"plain","ambiguous":false},{"value":2,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"3rd 4TH 21st","t":"3rd 4TH 21st","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"Nxe5?? 12...Qh4+ 13. g3","t":"Nxe5?? 12...Qh4+ 13. g3","n":[],"m":{"san":["Nxe5","Qh4","g3"],"uci":[]},"d":[]},
{"s":"1. e4 e5 2. Nf3","t":"1. e4 e5 2. Nf3","n":[],"m":{"san":["e4","e5","Nf3"],"uci":[]},"d":[]},
{"s":"the e-pawn","t":"the e-pawn","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"h8=Q","t":"h8=Q","n":[],"m":{"san":["h8=Q"],"uci":[]},"d":[]},
{"s":"exd8=Q+","t":"exd8=Q+","n":[],"m":{"san":["exd8=Q"],"uci":[]},"d":[]},
{"s":"Rxe6 then d5 (e4) ...","t":"Rxe6 then d5 (e4) ...","n":[],"m":{"san":["Rxe6"],"uci":[]},"d":[]},
{"s":"12 . e4","t":"12 . e4","n":[],"m":{"san":["e4"],"uci":[]},"d":[]},
{"s":"12.\u2028e4","t":"12.\u2028e4","n":[],"m":{"san":["e4"],"uci":[]},"d":[]},
{"s":"play\u2028e4","t":"play\u2028e4","n":[],"m":{"san":["e4"],"uci":[]},"d":[]},
{"s":"you PLAYED to e4","t":"you PLAYED to e4","n":[],"m":{"san":["e4"],"uci":[]},"d":[]},
{"s":"pushed  to  d4","t":"pushed  to  d4","n":[],"m":{"san":["d4"],"uci":[]},"d":[]},
{"s":"Q\u00e9e5","t":"Q\u00e9e5","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\u00e9e4","t":"\u00e9e4","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"e4\u00e9","t":"e4\u00e9","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"1\u2009000\u2009000 games","t":"1,000,000 games","n":[{"value":1000000,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"1\u00a0000 0","t":"1,000 0","n":[{"value":1000,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false},{"value":0,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\u0661\u00a0000","t":"\u0661,000","n":[{"value":0,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\u0663\u0661\u0664","t":"\u0663\u0661\u0664","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\uff11\uff12\uff13 games","t":"123 games","n":[{"value":123,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"12\u200b34","t":"1234","n":[{"value":1234,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"[[g:abc:2026-10-14]] 5 games","t":"[[g:abc:2026-10-14]] 5 games","n":[{"value":5,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[{"year":2026,"month":10,"day":14}]},
{"s":"since 2026-10-14 vs 2026-10-15","t":"since 2026-10-14 vs 2026-10-15","n":[],"m":{"san":[],"uci":[]},"d":[{"year":2026,"month":10,"day":14},{"year":2026,"month":10,"day":15}]},
{"s":"2026-02-31","t":"2026-02-31","n":[],"m":{"san":[],"uci":[]},"d":[{"year":2026,"month":2,"day":31}]},
{"s":"on 31 Feb","t":"on 31 Feb","n":[],"m":{"san":[],"uci":[]},"d":[{"year":null,"month":2,"day":31}]},
{"s":"0 games","t":"0 games","n":[{"value":0,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"00 games","t":"00 games","n":[{"value":0,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"007 moves","t":"007 moves","n":[{"value":7,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"1e5 games","t":"1e5 games","n":[{"value":1,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"3.0 games","t":"3.0 games","n":[{"value":3,"decimals":1,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"-0 games","t":"-0 games","n":[{"value":0,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]}
]
"#;
}
