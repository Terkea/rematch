//! Port of `.worktrees/m3/packages/ai/test/validator/extract-gaps.test.ts`, plus the Rust-only
//! checks for `bounded_matches` (deviation 2, ADR-R3-2), linear time on hostile input, the
//! hardening deviations H1-H6 and the differential TS corpus.

use std::cell::Cell;
use std::time::{Duration, Instant};

use ai::validator::bounded::bounded_matches;
use ai::validator::extract::{
    ExtractedNumber, NumberKind, extract_dates, extract_moves, extract_numbers,
};
use ai::validator::normalize::{normalize_text, normalize_ts, strip_invisibles};
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

    /// `after_ok` is a post-check on the char after the match, which is only valid for patterns
    /// whose match end is fixed. `[a-c]+` has a variable end, so this result is NOT what JS
    /// `[a-c]+(?!\d)` gives: JS backtracks to "ab" in "abc1". A variable-length pattern must fold
    /// its lookahead into the regex instead (`(?P<m>…)(?:[^…]|$)`, ADR-R3-2).
    #[test]
    fn bounded_matches_checks_the_char_after_and_retries_from_start_plus_one() {
        let re = Regex::new("[a-c]+").unwrap();
        // "abc" is followed by a digit, so it is rejected. The retries from start + 1 find "bc"
        // and "c", which end at the same digit, so they are rejected too. Only the last "ab" passes.
        let not_digit = |c: Option<char>| !c.is_some_and(|c| c.is_ascii_digit());
        assert_eq!(
            bounded_matches(&re, "abc1 ab", |_| true, not_digit),
            vec![(5, 7)]
        );
    }

    /// The TS output for this input, recorded by `backend/tools/gen-validator-corpus.mjs`, is
    /// `extractNumbers("\u{663} games") == []`: JS `\d` is ASCII-only, and NFKC keeps U+0663. The
    /// Rust digit class is ASCII too (first assert). But hardening H1 (ADR-R3-2) folds every
    /// `\p{Nd}` digit to ASCII before extraction, so Rust deliberately reads 3.
    #[test]
    fn rust_digit_class_is_ascii_like_js() {
        let re = Regex::new("[0-9]+").unwrap();
        assert_eq!(bounded_matches(&re, "\u{663}", |_| true, |_| true), vec![]);
        assert_eq!(values(&extract_numbers("\u{663} games")), vec![3.0]);
        assert_eq!(
            values(&extract_numbers("\u{663}\u{661}\u{664} games")),
            vec![314.0]
        );
    }

    #[test]
    fn strip_invisibles_removes_the_tag_and_variation_planes() {
        assert_eq!(strip_invisibles("a\u{e0001}b\u{e0100}c\u{e01ef}d"), "abcd");
        // U+E01F0 is unassigned, but it is in the Default_Ignorable block range (H2).
        assert_eq!(strip_invisibles("a\u{e01f0}b"), "ab");
        assert_eq!(strip_invisibles("a\u{1f600}b"), "a\u{1f600}b");
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

    fn run_all(text: &str) {
        let _ = normalize_text(text);
        let _ = extract_moves(text);
        let _ = extract_numbers(text);
        let _ = extract_dates(text);
    }

    /// The fastest of three runs, to damp scheduler noise.
    fn cost(text: &str) -> Duration {
        (0..3)
            .map(|_| {
                let t0 = Instant::now();
                run_all(text);
                t0.elapsed()
            })
            .min()
            .unwrap()
    }

    type Shape = fn(usize) -> String;

    /// A timing cross-check of the deterministic byte-count test
    /// (`extract::tests::scanned_bytes_are_linear_on_hostile_shapes`, the binding one). Each shape
    /// runs at N and at 4N: linear work takes about 4x as long, quadratic about 16x; the bound is
    /// 10x plus 40 ms. Release builds only, because debug timings are noisy and slow.
    #[test]
    #[cfg_attr(debug_assertions, ignore)]
    fn extractors_are_linear_on_large_hostile_text() {
        const N: usize = 1 << 14;
        let shapes: Vec<(&str, Shape)> = vec![
            ("move, gap, rejected squares", |n| {
                format!("Nf3{}{}", " ".repeat(n / 2), "x e4 ".repeat(n / 10))
            }),
            ("move, punctuation gap, squares", |n| {
                format!("Nf3{}{}", "(.; ".repeat(n / 8), "the e4 ".repeat(n / 14))
            }),
            ("glued digits", |n| format!("x{}", "1".repeat(n))),
            ("glued thousands", |n| format!("x{}", "1,111".repeat(n / 5))),
            ("comma digits", |n| "1,".repeat(n / 2)),
            ("glued uci", |n| "e2e4".repeat(n / 4)),
            ("dangling long", |n| "e2-".repeat(n / 3)),
            ("castling chain", |n| "O-O-".repeat(n / 4)),
            ("castling glued", |n| "0-0-0x".repeat(n / 6)),
            ("glued san", |n| "Nf3".repeat(n / 3)),
            ("word run", |n| format!("x{}", "one ".repeat(n / 4))),
            ("glued words", |n| "xone ".repeat(n / 5)),
            ("articles", |n| "a ".repeat(n / 2)),
            ("month day", |n| "Oct 1".repeat(n / 5)),
            ("month and spaces", |n| {
                format!("Oct{}x ", " ".repeat(16)).repeat(n / 21)
            }),
            ("day month", |n| "14 Oct ".repeat(n / 7)),
            ("iso prefix", |n| "2026-10-1".repeat(n / 9)),
            ("half", |n| "half ".repeat(n / 5)),
            ("half and spaces", |n| format!("half{}", " ".repeat(n))),
            ("double", |n| "double ".repeat(n / 7)),
            ("spaces", |n| " ".repeat(n)),
            ("nbsp digits", |n| "\u{a0}1".repeat(n / 2)),
            ("move numbers", |n| "12. ".repeat(n / 4)),
            ("weakness ids", |n| "W1".repeat(n / 2)),
            ("zero width", |n| "\u{200b}".repeat(n)),
            ("minus digits", |n| "\u{2212}0".repeat(n / 2)),
            ("combining marks", |n| "e\u{301}".repeat(n / 2)),
            ("homoglyphs", |n| "\u{435}4 ".repeat(n / 3)),
        ];
        for (name, make) in shapes {
            let small = cost(&make(N));
            let big = cost(&make(4 * N));
            assert!(
                big <= small * 10 + Duration::from_millis(40),
                "{name}: {small:?} at N, {big:?} at 4N"
            );
        }
    }
}

/// Hardening beyond TS, ruled in fix round 1 and recorded as deviations H1-H6 in ADR-R3-2.
mod hardening {
    use super::*;

    #[test]
    fn h1_folds_every_decimal_digit_to_ascii() {
        assert_eq!(
            normalize_text("\u{663}\u{966}\u{96f}\u{e53}\u{1d7d8}"),
            "30930"
        );
        assert_eq!(values(&extract_numbers("\u{967}\u{968} games")), vec![12.0]);
        assert_eq!(san("\u{967}\u{968}. e\u{666}"), s(&["e6"]));
    }

    #[test]
    fn h2_strips_every_default_ignorable_code_point() {
        for c in [
            '\u{180f}',
            '\u{206a}',
            '\u{fff0}',
            '\u{1bca0}',
            '\u{1d173}',
            '\u{e0fff}',
        ] {
            assert_eq!(san(&format!("N{c}f3")), s(&["Nf3"]), "{c:?}");
        }
    }

    #[test]
    fn h3_removes_combining_marks() {
        assert_eq!(normalize_text("N\u{301}f3"), "Nf3");
        assert_eq!(normalize_text("e\u{308}4 \u{e9}"), "e4 e");
        assert_eq!(san("N\u{301}f\u{20d7}3"), s(&["Nf3"]));
        // The union: the TS path reads "4" and "2" either side of the mark, then the hardened path
        // adds 42.
        assert_eq!(
            values(&extract_numbers("4\u{301}2 games")),
            vec![4.0, 2.0, 42.0]
        );
    }

    #[test]
    fn h4_folds_cyrillic_and_greek_homoglyphs() {
        assert_eq!(normalize_text("\u{435}4"), "e4");
        assert_eq!(san("\u{39d}f3 \u{41a}\u{430}4"), s(&["Nf3", "Ka4"]));
        assert_eq!(san("\u{41e}-\u{41e}-\u{41e}"), s(&["O-O-O"]));
        assert_eq!(extract_moves("\u{441}7\u{441}5").uci, s(&["c7c5"]));
        assert_eq!(values(&extract_numbers("\u{417} games")), vec![3.0]);
        // One table: each entry maps to a single ASCII char, and no code point appears twice.
        let table = ai::validator::normalize::HOMOGLYPHS;
        let mut keys: Vec<char> = table.iter().map(|(k, _)| *k).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), table.len());
        assert!(table.iter().all(|(k, v)| !k.is_ascii() && v.is_ascii()));
    }

    #[test]
    fn h5_extends_the_shared_dash_set() {
        for d in [
            "\u{2796}", "\u{2d7}", "\u{2e3a}", "\u{2e3b}", "\u{58a}", "\u{30fc}",
        ] {
            assert_eq!(san(&format!("O{d}O{d}O")), s(&["O-O-O"]), "{d:?}");
            assert_eq!(
                extract_moves(&format!("e2{d}e4")).uci,
                s(&["e2e4"]),
                "{d:?}"
            );
            // TS's own dashes (U+2796) give -3 on both paths; a new dash gives the TS-path 3, then
            // the hardened -3.
            let v = values(&extract_numbers(&format!("{d}3 eval")));
            let want = if d == "\u{2796}" {
                vec![-3.0]
            } else {
                vec![3.0, -3.0]
            };
            assert_eq!(v, want, "{d:?}");
        }
    }

    #[test]
    fn h6_treats_more_spaces_as_thousands_separators() {
        for sp in ['\u{2006}', '\u{2008}', '\u{200a}'] {
            assert_eq!(normalize_text(&format!("1{sp}000")), "1,000", "{sp:?}");
            // The TS path reads 1 and 0 (000); the hardened path adds 1000.
            assert_eq!(
                values(&extract_numbers(&format!("1{sp}000 games"))),
                vec![1.0, 0.0, 1000.0],
                "{sp:?}"
            );
        }
    }
}

/// Fix round 2: hardening folds a neighbour to ASCII, which can glue it onto a move or date
/// (`Nf3é` becomes `Nf3e`). The union keeps every result the TS-exact path finds.
mod union {
    use super::*;
    use ai::validator::extract::ExtractedDate;

    #[test]
    fn a_folded_neighbour_never_hides_a_ts_result() {
        for (text, want) in [
            ("Nf3\u{e9}", "Nf3"),
            ("\u{e9}Nf3", "Nf3"),
            ("Nf3\u{430}", "Nf3"),
            ("Nf3\u{663}", "Nf3"),
            ("Nf3\u{3bd}", "Nf3"),
            ("Qxe5\u{e9} wins", "Qxe5"),
            ("O-O\u{e9}", "O-O"),
        ] {
            assert!(
                san(text).contains(&want.to_owned()),
                "{text:?}: {:?}",
                san(text)
            );
        }
        assert_eq!(san("\u{e8}Rd1 and Rd1\u{e8}"), s(&["Rd1", "Rd1"]));
        assert_eq!(extract_moves("e2e4\u{e9}").uci, s(&["e2e4"]));
        assert_eq!(extract_moves("e2-e4\u{430}").uci, s(&["e2e4"]));
        assert_eq!(
            extract_dates("14 Oct\u{e9}"),
            vec![ExtractedDate {
                year: None,
                month: 10,
                day: 14
            }]
        );
    }

    #[test]
    fn ts_results_come_first_then_hardened_extras_without_duplicates() {
        // Both paths find Nf3; the hardened path adds e4 (from Cyrillic е4) once.
        assert_eq!(san("Nf3 \u{435}4 and Nf3"), s(&["Nf3", "Nf3", "e4"]));
        // Identical paths add nothing.
        assert_eq!(
            values(&extract_numbers("5 games and 5 moves")),
            vec![5.0, 5.0]
        );
    }
}

/// Differential parity with the TS reference on edge cases where an exact port matters:
/// - lookaheads that the JS engine satisfies by backtracking (`0-0-0x` normalises to `O-O-0x`);
/// - inner lookaheads (`1,234,5678`, `Oct 14th:`);
/// - UTF-16 window sizes (astral chars across the 12-, 16- and 20-unit windows);
/// - JS `\s` (U+0085 is not a member, U+1680 and U+2029 are);
/// - the masks.
///
/// `TS_CORPUS` is generated by `backend/tools/gen-validator-corpus.mjs`, which records m3's
/// `normalizeText`, `extractNumbers`, `extractMoves` and `extractDates` for each input. Inputs in
/// `HARDENED` deliberately differ (H1-H6): for those the test asserts that Rust does differ, so the
/// list cannot go stale; their Rust values are pinned in `mod hardening`.
mod ts_parity {
    use super::*;
    use ai::validator::extract::ExtractedDate;
    use serde_json::Value;

    const HARDENED: &[&str] = &[
        "\u{661}\u{a0}000",
        "\u{663}\u{661}\u{664}",
        "Q\u{e9}e5",
        "\u{e9}e4",
        "e4\u{e9}",
        "\u{663} games",
        "\u{966}\u{96f} games",
        "\u{435}4 and \u{41d}\u{435}4",
        "\u{39d}f3 \u{41a}\u{430}4",
        "N\u{301}f3",
        "e\u{308}4",
        "N\u{180f}f3 and N\u{1d173}f3 and N\u{fff0}f3",
        "O\u{2d7}O and 0\u{2e3a}0\u{2e3b}0 and O\u{58a}O\u{30fc}O",
        "e2\u{2d7}e4 and Ng1\u{2e3a}f3",
        "1\u{2006}000 and 1\u{2008}000 and 1\u{200a}000 games",
        // Fix round 2: the hardened text glues a folded neighbour on, so only the normalisation
        // differs; the union must still keep every TS result (checked as a prefix).
        "Nf3\u{e9}",
        "\u{e9}Nf3",
        "Nf3\u{430}",
        "Nf3\u{663}",
        "Nf3\u{3bd}",
        "Qxe5\u{e9} wins",
        "e2e4\u{e9}",
        "e2-e4\u{430}",
        "O-O\u{e9}",
        "\u{e8}Rd1 and Rd1\u{e8}",
        "14 Oct\u{e9}",
        "5\u{e9} games and 7\u{663} games",
    ];

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

    /// The recorded TS output for one corpus row.
    struct Ts {
        t: String,
        numbers: Vec<ExtractedNumber>,
        san: Vec<String>,
        uci: Vec<String>,
        dates: Vec<ExtractedDate>,
    }

    fn ts_row(case: &Value) -> Ts {
        Ts {
            t: case["t"].as_str().unwrap().to_owned(),
            numbers: case["n"]
                .as_array()
                .unwrap()
                .iter()
                .map(ts_number)
                .collect(),
            san: strings(&case["m"]["san"]),
            uci: strings(&case["m"]["uci"]),
            dates: case["d"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| ExtractedDate {
                    year: x["year"].as_i64().map(|y| i32::try_from(y).unwrap()),
                    month: u32::try_from(x["month"].as_u64().unwrap()).unwrap(),
                    day: u32::try_from(x["day"].as_u64().unwrap()).unwrap(),
                })
                .collect(),
        }
    }

    /// Checks one row. Every row: the TS-exact normalisation equals TS's, and each Rust result list
    /// starts with TS's list unchanged (so it is a superset, in TS order). Non-hardened rows: the
    /// hardened normalisation and every list equal TS exactly. Hardened rows must differ somewhere
    /// (`hardened_differs`), or the `HARDENED` entry is stale.
    fn check(case: &Value, hardened: bool, bad: &mut Vec<String>) {
        let s = case["s"].as_str().unwrap();
        let ts = ts_row(case);
        if normalize_ts(s) != ts.t {
            bad.push(format!("normalize_ts {s:?}: {:?}", normalize_ts(s)));
        }
        let n = extract_numbers(s);
        let m = extract_moves(s);
        let d = extract_dates(s);
        let lists = [
            (
                "numbers",
                n.starts_with(&ts.numbers),
                n.len() == ts.numbers.len(),
            ),
            (
                "san",
                m.san.starts_with(&ts.san),
                m.san.len() == ts.san.len(),
            ),
            (
                "uci",
                m.uci.starts_with(&ts.uci),
                m.uci.len() == ts.uci.len(),
            ),
            ("dates", d.starts_with(&ts.dates), d.len() == ts.dates.len()),
        ];
        let detail = || format!("rust {n:?} {m:?} {d:?}");
        for (what, prefix, _) in &lists {
            if !prefix {
                bad.push(format!("{what} {s:?} lost a TS result: {}", detail()));
            }
        }
        let same_norm = normalize_text(s) == ts.t;
        let all_equal = lists.iter().all(|(_, p, l)| *p && *l);
        if hardened {
            if same_norm && all_equal {
                bad.push(format!("hardened {s:?} no longer differs from TS"));
            }
        } else {
            if !same_norm {
                bad.push(format!("normalize {s:?}: {:?}", normalize_text(s)));
            }
            for (what, prefix, len) in &lists {
                if *prefix && !len {
                    bad.push(format!("{what} {s:?} has extras: {}", detail()));
                }
            }
        }
    }

    #[test]
    fn rust_matches_the_recorded_ts_output_on_the_edge_corpus() {
        let corpus: Vec<Value> = serde_json::from_str(TS_CORPUS).unwrap();
        assert!(corpus.len() > 163, "{} rows", corpus.len());
        let inputs: Vec<&str> = corpus.iter().map(|c| c["s"].as_str().unwrap()).collect();
        for h in HARDENED {
            assert!(
                inputs.contains(h),
                "HARDENED input {h:?} is not in the corpus"
            );
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
    // node 22.22.0, icu 77.1, unicode 16.0, m3 4ff8220aed41afb2dd23e04314511f6ebcb1f8e7, 175 inputs
    const TS_CORPUS: &str = r#"[
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
{"s":"-0 games","t":"-0 games","n":[{"value":0,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"12.\u0085e4","t":"12.\u0085e4","n":[{"value":12,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"about\u00855 games","t":"about\u00855 games","n":[{"value":5,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"three\u0085games","t":"three\u0085games","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"you played\u1680h4","t":"you played\u1680h4","n":[],"m":{"san":["h4"],"uci":[]},"d":[]},
{"s":"12.\te4 and 13.\u000bd4","t":"12.\te4 and 13.\u000bd4","n":[],"m":{"san":["e4","d4"],"uci":[]},"d":[]},
{"s":"about\u20295 games","t":"about\u20295 games","n":[{"value":5,"decimals":0,"percent":false,"hedged":true,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"five\u2029games","t":"five\u2029games","n":[{"value":5,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"Oct 14th, 2026","t":"Oct 14th, 2026","n":[],"m":{"san":[],"uci":[]},"d":[{"year":2026,"month":10,"day":14}]},
{"s":"Oct\u000b14th,\u00852026","t":"Oct\u000b14th,\u00852026","n":[{"value":2026,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[{"year":null,"month":10,"day":14}]},
{"s":"x\ud83d\ude001.           e4","t":"x\ud83d\ude001.           e4","n":[],"m":{"san":["e4"],"uci":[]},"d":[]},
{"s":"\ud83d\ude00\ud83d\ude001.           e4","t":"\ud83d\ude00\ud83d\ude001.           e4","n":[],"m":{"san":["e4"],"uci":[]},"d":[]},
{"s":"\ud83d\ude00...           d5","t":"\ud83d\ude00...           d5","n":[],"m":{"san":["d5"],"uci":[]},"d":[]},
{"s":"x\ud83d\ude001.            e4","t":"x\ud83d\ude001.            e4","n":[],"m":{"san":["e4"],"uci":[]},"d":[]},
{"s":"\ud83d\ude00\ud83d\ude001.            e4","t":"\ud83d\ude00\ud83d\ude001.            e4","n":[],"m":{"san":["e4"],"uci":[]},"d":[]},
{"s":"\ud83d\ude00...            d5","t":"\ud83d\ude00...            d5","n":[],"m":{"san":["d5"],"uci":[]},"d":[]},
{"s":"x\ud83d\ude001.             e4","t":"x\ud83d\ude001.             e4","n":[],"m":{"san":["e4"],"uci":[]},"d":[]},
{"s":"\ud83d\ude00\ud83d\ude001.             e4","t":"\ud83d\ude00\ud83d\ude001.             e4","n":[],"m":{"san":["e4"],"uci":[]},"d":[]},
{"s":"\ud83d\ude00...             d5","t":"\ud83d\ude00...             d5","n":[],"m":{"san":["d5"],"uci":[]},"d":[]},
{"s":"x\ud83d\ude001.              e4","t":"x\ud83d\ude001.              e4","n":[],"m":{"san":["e4"],"uci":[]},"d":[]},
{"s":"\ud83d\ude00\ud83d\ude001.              e4","t":"\ud83d\ude00\ud83d\ude001.              e4","n":[],"m":{"san":["e4"],"uci":[]},"d":[]},
{"s":"\ud83d\ude00...              d5","t":"\ud83d\ude00...              d5","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"x\ud83d\ude001.               e4","t":"x\ud83d\ude001.               e4","n":[{"value":1,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00\ud83d\ude001.               e4","t":"\ud83d\ude00\ud83d\ude001.               e4","n":[{"value":1,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00...               d5","t":"\ud83d\ude00...               d5","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00first    half of games","t":"\ud83d\ude00first    half of games","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00last    half the games","t":"\ud83d\ude00last    half the games","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00first     half of games","t":"\ud83d\ude00first     half of games","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00last     half the games","t":"\ud83d\ude00last     half the games","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00first      half of games","t":"\ud83d\ude00first      half of games","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00last      half the games","t":"\ud83d\ude00last      half the games","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00first       half of games","t":"\ud83d\ude00first       half of games","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00last       half the games","t":"\ud83d\ude00last       half the games","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00first        half of games","t":"\ud83d\ude00first        half of games","n":[{"value":0.5,"decimals":1,"percent":false,"hedged":false,"kind":"ratio","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00last        half the games","t":"\ud83d\ude00last        half the games","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00about             5 games","t":"\ud83d\ude00about             5 games","n":[{"value":5,"decimals":0,"percent":false,"hedged":true,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00\ud83d\ude00more than         7 games","t":"\ud83d\ude00\ud83d\ude00more than         7 games","n":[{"value":7,"decimals":0,"percent":false,"hedged":true,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00about              5 games","t":"\ud83d\ude00about              5 games","n":[{"value":5,"decimals":0,"percent":false,"hedged":true,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00\ud83d\ude00more than          7 games","t":"\ud83d\ude00\ud83d\ude00more than          7 games","n":[{"value":7,"decimals":0,"percent":false,"hedged":true,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00about               5 games","t":"\ud83d\ude00about               5 games","n":[{"value":5,"decimals":0,"percent":false,"hedged":true,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00\ud83d\ude00more than           7 games","t":"\ud83d\ude00\ud83d\ude00more than           7 games","n":[{"value":7,"decimals":0,"percent":false,"hedged":true,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00about                5 games","t":"\ud83d\ude00about                5 games","n":[{"value":5,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\ud83d\ude00\ud83d\ude00more than            7 games","t":"\ud83d\ude00\ud83d\ude00more than            7 games","n":[{"value":7,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"Nf3                                        x e4 x e4 x e4 x e4 x e4 x e4 x e4 x e4 ","t":"Nf3                                        x e4 x e4 x e4 x e4 x e4 x e4 x e4 x e4 ","n":[],"m":{"san":["Nf3"],"uci":[]},"d":[]},
{"s":"\u0663 games","t":"\u0663 games","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\u0966\u096f games","t":"\u0966\u096f games","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\u04354 and \u041d\u04354","t":"\u04354 and \u041d\u04354","n":[{"value":4,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false},{"value":4,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"\u039df3 \u041a\u04304","t":"\u039df3 \u041a\u04304","n":[{"value":4,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"N\u0301f3","t":"\u0143f3","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"e\u03084","t":"\u00eb4","n":[{"value":4,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"N\u180ff3 and N\ud834\udd73f3 and N\ufff0f3","t":"N\u180ff3 and N\ud834\udd73f3 and N\ufff0f3","n":[],"m":{"san":[],"uci":[]},"d":[]},
{"s":"O\u02d7O and 0\u2e3a0\u2e3b0 and O\u058aO\u30fcO","t":"O\u02d7O and 0\u2e3a0\u2e3b0 and O\u058aO\u30fcO","n":[{"value":0,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false},{"value":0,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false},{"value":0,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"e2\u02d7e4 and Ng1\u2e3af3","t":"e2\u02d7e4 and Ng1\u2e3af3","n":[],"m":{"san":["Ng1"],"uci":[]},"d":[]},
{"s":"1\u2006000 and 1\u2008000 and 1\u200a000 games","t":"1 000 and 1 000 and 1 000 games","n":[{"value":1,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false},{"value":0,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false},{"value":1,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false},{"value":0,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false},{"value":1,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false},{"value":0,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]},
{"s":"Nf3\u00e9","t":"Nf3\u00e9","n":[],"m":{"san":["Nf3"],"uci":[]},"d":[]},
{"s":"\u00e9Nf3","t":"\u00e9Nf3","n":[],"m":{"san":["Nf3"],"uci":[]},"d":[]},
{"s":"Nf3\u0430","t":"Nf3\u0430","n":[],"m":{"san":["Nf3"],"uci":[]},"d":[]},
{"s":"Nf3\u0663","t":"Nf3\u0663","n":[],"m":{"san":["Nf3"],"uci":[]},"d":[]},
{"s":"Nf3\u03bd","t":"Nf3\u03bd","n":[],"m":{"san":["Nf3"],"uci":[]},"d":[]},
{"s":"Qxe5\u00e9 wins","t":"Qxe5\u00e9 wins","n":[],"m":{"san":["Qxe5"],"uci":[]},"d":[]},
{"s":"e2e4\u00e9","t":"e2e4\u00e9","n":[],"m":{"san":[],"uci":["e2e4"]},"d":[]},
{"s":"e2-e4\u0430","t":"e2-e4\u0430","n":[],"m":{"san":[],"uci":["e2e4"]},"d":[]},
{"s":"O-O\u00e9","t":"O-O\u00e9","n":[],"m":{"san":["O-O"],"uci":[]},"d":[]},
{"s":"\u00e8Rd1 and Rd1\u00e8","t":"\u00e8Rd1 and Rd1\u00e8","n":[],"m":{"san":["Rd1","Rd1"],"uci":[]},"d":[]},
{"s":"14 Oct\u00e9","t":"14 Oct\u00e9","n":[],"m":{"san":[],"uci":[]},"d":[{"year":null,"month":10,"day":14}]},
{"s":"5\u00e9 games and 7\u0663 games","t":"5\u00e9 games and 7\u0663 games","n":[{"value":5,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false},{"value":7,"decimals":0,"percent":false,"hedged":false,"kind":"plain","ambiguous":false}],"m":{"san":[],"uci":[]},"d":[]}
]"#;
    // END GENERATED
}
