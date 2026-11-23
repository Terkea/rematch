//! Port of `.worktrees/m3/packages/ai/test/validator/evidence.test.ts` (each `describe` is a module,
//! each `it` title is snake_case), plus the R3 additions and a differential check against the TS
//! index recorded in `backend/fixtures/ai/evidence/ts-evidence.json`
//! (`backend/tools/gen-evidence-golden.mjs`).

use std::collections::{BTreeMap, BTreeSet, HashSet};

use ai::input::types::{CoachingInput, MoveRef, Score};
use ai::testing::{fixtures_dir, input, load_fixture, make_input};
use ai::validator::evidence::{
    EngineEvidence, EngineLine, Evidence, PositionArgs, PvFrom, build_evidence, create_evidence,
};
use ai::validator::format::format_number;
use ai::validator::tokens::NumFormat;
use chess_core::Color;
use serde_json::{Value, json};

const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

fn has(ns: &[f64], x: f64, tol: f64) -> bool {
    ns.iter().any(|n| (n - x).abs() < tol)
}

fn cp(s: Score) -> i32 {
    match s {
        Score::Cp(c) => c,
        Score::Mate(_) => 0,
    }
}

mod build_evidence_on_b2_rapid {
    use super::*;

    struct Ctx {
        input: CoachingInput,
        ev: Evidence,
        ev420: Evidence,
    }

    fn ctx() -> Ctx {
        let input = input("b2-rapid");
        let ev = build_evidence(&input);
        let over =
            make_input(json!({"weaknesses": [{"exemplars": [{"evalAfter": {"cp": -420}}]}]}));
        let ev420 = build_evidence(&serde_json::from_value(over).expect("override input"));
        Ctx { input, ev, ev420 }
    }

    #[test]
    fn indexes_exemplar_positions_with_played_best_and_pv_moves() {
        let Ctx { input, ev, .. } = ctx();
        let w1ex0 = &input.weaknesses[0].exemplars[0];
        let p = &ev.positions[&format!("{}#{}", w1ex0.game_id, w1ex0.ply)];
        assert!(p.allowed_san.contains(&w1ex0.played.san));
        assert!(p.allowed_san.contains(&w1ex0.best.as_ref().unwrap().san));
        assert!(p.allowed_san.contains("Nxd4"), "refutationPv[1]");
        assert!(p.allowed_uci.contains(&w1ex0.played.uci));
        assert!(p.allowed_uci.contains("f3d4"));
        assert!(ev.game_ids.contains(&w1ex0.game_id));
    }

    #[test]
    fn indexes_opening_lines() {
        let Ctx { input, ev, .. } = ctx();
        let o = &input.openings[0];
        let line = &ev.lines[&o.line_key];
        for m in &o.moves_san {
            assert!(line.contains(m), "{m}");
        }
        if let Some(d) = &o.deviation_move_san {
            assert!(line.contains(d));
        }
        for a in &o.alternatives {
            for m in &a.moves_san {
                assert!(line.contains(m), "{m}");
            }
        }
    }

    #[test]
    fn collects_numbers() {
        let Ctx { input, ev, ev420 } = ctx();
        let w1ex0 = &input.weaknesses[0].exemplars[0];
        assert!(ev.numbers.contains(&214.0));
        assert!(ev.numbers.contains(&10.0), "600 s base / 60");
        let pawns = f64::from(cp(w1ex0.eval_after).abs()) / 100.0;
        assert!(has(&ev.numbers, pawns, 1e-9));
        assert!(has(&ev420.numbers, 4.2, 1e-9));
        assert!(has(&ev420.numbers, -4.2, 1e-9));
        assert!(ev.numbers.contains(&3.0), "ceil(6/2)");
        assert!(ev.numbers.contains(&(input.weaknesses.len() as f64)));
    }

    #[test]
    fn stores_rates_in_0_1_also_times_100() {
        let Ctx { input, ev, .. } = ctx();
        let r = input.weaknesses[0].posterior_rate;
        assert!(has(&ev.numbers, r * 100.0, 1e-6));
    }

    #[test]
    fn collects_ratios_for_changes_both_ways() {
        let Ctx { input, ev, .. } = ctx();
        let c = &input.changes[0];
        assert!(has(&ev.ratios, c.rate_after / c.rate_before, 1e-9));
        assert!(has(&ev.ratios, c.rate_before / c.rate_after, 1e-9));
    }

    #[test]
    fn collects_dates_as_yyyy_mm_dd_and_mm_dd_plus_their_parts() {
        let Ctx { input, ev, .. } = ctx();
        let d = input.changes[0].onset_date;
        let iso = format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day());
        assert!(ev.dates.contains(&iso));
        assert!(ev.dates.contains(&iso[5..]));
        assert!(ev.numbers.contains(&f64::from(d.year())));
        assert!(ev.numbers.contains(&f64::from(d.day())));
    }

    #[test]
    fn indexes_change_examples_and_onset_games() {
        let Ctx { input, ev, .. } = ctx();
        let c = &input.changes[0];
        assert!(ev.game_ids.contains(&c.onset_game_id));
        if let Some(e) = c.examples.first() {
            let key = format!("{}#{}", e.position.game_id, e.position.ply);
            assert!(ev.positions.contains_key(&key));
        }
    }
}

mod add_numbers_from {
    use super::*;

    #[test]
    fn never_mines_uuid_strings_even_under_mined_keys() {
        let mut ev = create_evidence();
        ev.add_numbers_from(&json!({
            "gameId": "4a3f9c1e-2b7d-4e8a-9f10-3c5d6e7f8a9b",
            "name": "4a3f9c1e-2b7d-4e8a-9f10-3c5d6e7f8a9b",
            "label": "4A3F9C1E-2B7D-4E8A-9F10-3C5D6E7F8A9B",
        }));
        assert_eq!(ev.numbers, Vec::<f64>::new());
    }

    #[test]
    fn mines_strings_only_under_label_description_name_before_after() {
        let mut ev = create_evidence();
        ev.add_numbers_from(
            &json!({"label": "7 blunders", "other": "99 problems", "before": "5 games"}),
        );
        assert!(ev.numbers.contains(&7.0));
        assert!(ev.numbers.contains(&5.0));
        assert!(!ev.numbers.contains(&99.0));
    }

    #[test]
    fn stores_score_cp_both_signs_and_mate() {
        let mut ev = create_evidence();
        ev.add_numbers_from(&json!({"evalBefore": {"cp": 150}, "evalAfter": {"mate": -3}}));
        for x in [1.5, -1.5, 3.0, -3.0] {
            assert!(ev.numbers.contains(&x), "{x}");
        }
    }

    #[test]
    fn adds_ply_as_full_move_number_and_array_lengths() {
        let mut ev = create_evidence();
        ev.add_numbers_from(&json!({"ply": 7, "items": [1, 2, 3, 4, 5]}));
        assert!(ev.numbers.contains(&4.0));
        assert!(ev.numbers.contains(&5.0));
    }

    /// JS `Set` uses SameValueZero: 0 and -0 are one value, and each number is listed once.
    #[test]
    fn numbers_are_unique_and_zero_is_one_value() {
        let mut ev = create_evidence();
        ev.add_numbers_from(&json!({"cp": 0, "x": [0, 0], "y": -0.0}));
        assert_eq!(ev.numbers.iter().filter(|n| **n == 0.0).count(), 1);
        let mut seen = HashSet::new();
        assert!(ev.numbers.iter().all(|n| seen.insert(n.to_bits())));
    }

    /// ISO dates are mined from any string; dates inside mined text through `extract_dates`.
    #[test]
    fn mines_iso_prefixes_and_dates_in_mined_text() {
        let mut ev = create_evidence();
        ev.add_numbers_from(&json!({"at": "2026-03-04T10:00:00Z", "label": "since Oct 14"}));
        assert!(ev.dates.contains("2026-03-04"));
        assert!(ev.dates.contains("03-04"));
        assert!(ev.dates.contains("10-14"));
        for x in [2026.0, 3.0, 4.0, 10.0, 14.0] {
            assert!(ev.numbers.contains(&x), "{x}");
        }
    }
}

mod add_position {
    use super::*;

    #[test]
    fn never_throws_on_bad_pvs() {
        let mut ev = create_evidence();
        ev.add_position(PositionArgs {
            game_id: "g".into(),
            ply: 1,
            fen: START_FEN.into(),
            played: None,
            best: None,
            pvs: vec![PvFrom {
                from_fen: "bad".into(),
                uci: vec!["e2e4".into()],
            }],
            ..Default::default()
        });
        assert_eq!(ev.positions["g#1"].allowed_san.len(), 0);
    }

    #[test]
    fn keeps_the_first_entry_and_unions_allowed_moves() {
        let mut ev = create_evidence();
        let mv = |san: &str, uci: &str| {
            Some(MoveRef {
                san: san.into(),
                uci: uci.into(),
            })
        };
        ev.add_position(PositionArgs {
            game_id: "g".into(),
            ply: 1,
            fen: START_FEN.into(),
            played: mv("e4", "e2e4"),
            ..Default::default()
        });
        ev.add_position(PositionArgs {
            game_id: "g".into(),
            ply: 1,
            fen: START_FEN.into(),
            played: mv("Nf3+", "g1f3"),
            ..Default::default()
        });
        let p = &ev.positions["g#1"];
        assert_eq!(p.played.as_ref().unwrap().san, "e4");
        for s in ["e4", "Nf3+", "Nf3"] {
            assert!(p.allowed_san.contains(s), "{s}");
        }
        assert!(p.allowed_uci.contains("g1f3"));
        assert!(ev.game_ids.contains("g"));
    }
}

mod r3_additions {
    use super::*;

    #[test]
    fn add_line_indexes_san_and_keeps_the_order() {
        let mut ev = create_evidence();
        ev.add_line("k", vec!["e4".into(), "e5".into(), "Qh5+".into()]);
        assert_eq!(ev.line_moves["k"], vec!["e4", "e5", "Qh5+"]);
        for s in ["e4", "e5", "Qh5+", "Qh5"] {
            assert!(ev.lines["k"].contains(s), "{s}");
        }
    }

    #[test]
    fn put_paths_keep_the_first_finite_value() {
        let mut ev = create_evidence();
        ev.put_number_path("a.b", 1.0);
        ev.put_number_path("a.b", 2.0);
        ev.put_number_path("nan", f64::NAN);
        ev.put_date_path("d", "2026-01-02".into());
        ev.put_date_path("d", "2026-01-03".into());
        assert_eq!(ev.number_paths.get("a.b"), Some(&1.0));
        assert!(!ev.number_paths.contains_key("nan"));
        assert_eq!(ev.date_paths["d"], "2026-01-02");
    }

    #[test]
    fn add_engine_eval_and_add_ratio() {
        let mut ev = create_evidence();
        ev.add_engine_eval(
            "a".repeat(32),
            EngineEvidence {
                tried_san: vec!["Nf3".into()],
                lines: vec![EngineLine {
                    score: Score::Cp(130),
                    side_to_move: Color::White,
                    san: vec!["e5".into()],
                }],
            },
        );
        assert_eq!(ev.engine_evals[&"a".repeat(32)].tried_san, vec!["Nf3"]);
        ev.add_ratio(2.0);
        ev.add_ratio(2.0);
        ev.add_ratio(f64::INFINITY);
        assert_eq!(ev.ratios, vec![2.0]);
    }

    /// The Rust walk reads `CoachingInput::to_json`; it must see the same numbers as the raw
    /// fixture JSON that TS walks.
    #[test]
    fn the_typed_input_walks_like_the_raw_fixture_json() {
        for name in ["b2-rapid", "b0-blitz", "early-read"] {
            let mut raw = create_evidence();
            raw.add_numbers_from(&load_fixture(name));
            let ev = build_evidence(&input(name));
            let a: HashSet<u64> = raw.numbers.iter().map(|n| n.to_bits()).collect();
            let b: HashSet<u64> = ev.numbers.iter().map(|n| n.to_bits()).collect();
            assert_eq!(a, b, "{name}");
            assert_eq!(raw.dates, ev.dates, "{name}");
        }
    }
}

mod ts_parity {
    use super::*;

    fn golden() -> Value {
        let path = fixtures_dir().join("evidence/ts-evidence.json");
        let text = std::fs::read_to_string(&path).expect("golden file");
        serde_json::from_str(&text).expect("golden json")
    }

    fn f(v: &Value) -> f64 {
        v.as_str()
            .expect("numbers are recorded as strings")
            .parse()
            .expect("a JS number")
    }

    fn strs(v: &Value) -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_owned())
            .collect()
    }

    fn sorted<'a>(it: impl IntoIterator<Item = &'a String>) -> Vec<String> {
        let mut v: Vec<String> = it.into_iter().cloned().collect();
        v.sort();
        v
    }

    fn nums(v: &[f64]) -> Vec<f64> {
        let mut v = v.to_vec();
        v.sort_by(f64::total_cmp);
        v.into_iter()
            .map(|n| if n == 0.0 { 0.0 } else { n })
            .collect()
    }

    /// A secondary check on the values: at most `MAX_ULPS` representable doubles apart (the
    /// primary check is the exact `formatNumber` strings below). With `serde_json`'s
    /// `float_roundtrip` the fixtures read exactly, so the index normally matches bit for bit.
    const MAX_ULPS: u64 = 1;

    /// Maps a double onto a line where adjacent doubles differ by 1 (-0 and 0 are both 0).
    fn ordered(x: f64) -> i64 {
        let b = x.to_bits();
        let mag = (b & !(1u64 << 63)) as i64;
        if b >> 63 == 1 { -mag } else { mag }
    }

    fn close(a: f64, b: f64) -> bool {
        a == b || ordered(a).abs_diff(ordered(b)) <= MAX_ULPS
    }

    fn all_close(a: &[f64], b: &[f64]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| close(*x, *y))
    }

    #[test]
    fn rust_matches_the_recorded_ts_evidence_index() {
        let g = golden();
        for (name, want) in g["fixtures"].as_object().unwrap() {
            let ev = build_evidence(&input(name));
            let paths: BTreeMap<String, f64> = want["numberPaths"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), f(v)))
                .collect();
            for (k, v) in &paths {
                let got = ev.number_paths.get(k).copied();
                assert!(
                    got.is_some_and(|g| close(g, *v)),
                    "{name}: number path {k}: {got:?} vs {v}"
                );
            }
            assert_eq!(
                ev.number_paths.keys().collect::<Vec<_>>(),
                paths.keys().collect::<Vec<_>>(),
                "{name}: number paths"
            );
            // Primary: TS's `formatNumber` of every path in every format, as exact strings.
            let formats: Vec<Option<NumFormat>> = std::iter::once(None)
                .chain(NumFormat::ALL.map(Some))
                .collect();
            let mut bad = Vec::new();
            let mut cases = 0;
            for (k, row) in want["numberFormats"].as_object().unwrap() {
                let v = ev.number_paths[k];
                for (fmt, w) in formats.iter().zip(row.as_array().unwrap()) {
                    cases += 1;
                    let got = format_number(v, *fmt);
                    if got.as_deref() != w.as_str() {
                        bad.push(format!("{k} {fmt:?}: {got:?} vs {w}"));
                    }
                }
            }
            assert!(cases >= 7 * paths.len(), "{name}: {cases} format cases");
            assert!(bad.is_empty(), "{name}: {bad:#?}");
            let dates: BTreeMap<String, String> =
                serde_json::from_value(want["datePaths"].clone()).unwrap();
            assert_eq!(ev.date_paths, dates, "{name}: date paths");
            let want_nums: Vec<f64> = want["numbers"].as_array().unwrap().iter().map(f).collect();
            let (a, b) = (nums(&ev.numbers), nums(&want_nums));
            let only_rust: Vec<_> = a
                .iter()
                .filter(|x| !b.iter().any(|y| close(**x, *y)))
                .collect();
            let only_ts: Vec<_> = b
                .iter()
                .filter(|x| !a.iter().any(|y| close(**x, *y)))
                .collect();
            // Set cover, not lengths: TS can hold two values one ulp apart that Rust read as one.
            assert!(
                only_rust.is_empty() && only_ts.is_empty(),
                "{name}: numbers: only rust {only_rust:?}, only ts {only_ts:?}"
            );
            let want_ratios: Vec<f64> = want["ratios"].as_array().unwrap().iter().map(f).collect();
            assert!(
                all_close(&nums(&ev.ratios), &nums(&want_ratios)),
                "{name}: ratios"
            );
            assert_eq!(sorted(&ev.dates), strs(&want["dates"]), "{name}: dates");
            assert_eq!(
                sorted(&ev.game_ids),
                strs(&want["gameIds"]),
                "{name}: game ids"
            );

            let positions = want["positions"].as_object().unwrap();
            assert_eq!(
                sorted(ev.positions.keys()),
                sorted(positions.keys()),
                "{name}: position keys"
            );
            for (key, p) in positions {
                let got = &ev.positions[key];
                let ctx = format!("{name} {key}");
                assert_eq!(got.game_id, p["gameId"], "{ctx}");
                assert_eq!(u64::from(got.ply), p["ply"].as_u64().unwrap(), "{ctx}");
                assert_eq!(got.fen, p["fen"], "{ctx}");
                assert_eq!(
                    serde_json::to_value(&got.played).unwrap(),
                    p["played"],
                    "{ctx}"
                );
                assert_eq!(serde_json::to_value(&got.best).unwrap(), p["best"], "{ctx}");
                assert_eq!(
                    sorted(&got.allowed_san),
                    strs(&p["allowedSan"]),
                    "{ctx} san"
                );
                assert_eq!(
                    sorted(&got.allowed_uci),
                    strs(&p["allowedUci"]),
                    "{ctx} uci"
                );
                assert_eq!(got.best_pv_san, strs(&p["bestPvSan"]), "{ctx} best pv");
                assert_eq!(
                    got.refutation_pv_san,
                    strs(&p["refutationPvSan"]),
                    "{ctx} refutation pv"
                );
            }
            let lines = want["lines"].as_object().unwrap();
            assert_eq!(ev.lines.len(), lines.len(), "{name}: lines");
            for (k, l) in lines {
                assert_eq!(sorted(&ev.lines[k]), strs(l), "{name}: line {k}");
                assert_eq!(
                    ev.line_moves[k],
                    strs(&want["lineMoves"][k]),
                    "{name}: moves {k}"
                );
            }
        }
    }
}

/// coaching-input-v2 (ADR 0085): the colour split and the benchmark are citable, with units.
mod colour_split {
    use ai::segments::Segment;
    use ai::testing::v2_input;
    use ai::validator::evidence::NumUnit;
    use ai::validator::format::format_num_at;
    use ai::validator::text::{TextRules, validate_text};

    use super::*;

    fn ev() -> Evidence {
        build_evidence(&v2_input("b2-rapid"))
    }

    #[test]
    fn indexes_every_colour_number_by_path() {
        let ev = ev();
        let n = |p: &str| ev.number_paths.get(p).copied();
        assert_eq!(n("byColor.white.games"), Some(42.0));
        assert_eq!(n("byColor.white.wins"), Some(20.0));
        assert_eq!(n("byColor.white.score.value"), Some(0.524));
        assert_eq!(n("byColor.white.accuracy.high"), Some(79.8));
        assert_eq!(n("byColor.white.blundersPerGame.value"), Some(0.9));
        // Weakness rows are keyed by their series key.
        assert_eq!(n("byColor.white.weaknesses.W4_missed.rate"), Some(0.14));
        assert_eq!(
            n("byColor.white.weaknesses.W4_missed.peerRate"),
            Some(0.096)
        );
        assert_eq!(n("byColor.white.weaknesses.W1.opportunities"), Some(240.0));
        assert_eq!(n("benchmark.ratingLow"), Some(675.0));
        assert_eq!(n("benchmark.players"), Some(25.0));
        // An insufficient number is null: there is no path to cite.
        assert_eq!(n("byColor.black.score.value"), None);
        assert_eq!(n("byColor.black.accuracy.value"), None);
        assert_eq!(n("byColor.black.games"), Some(7.0));
        assert_eq!(n("byColor.black.needMore"), Some(3.0));
    }

    #[test]
    fn colour_paths_carry_their_units() {
        let ev = ev();
        assert_eq!(
            ev.unit_of("byColor.white.accuracy.value"),
            Some(NumUnit::Percent)
        );
        assert_eq!(ev.unit_of("byColor.white.blundersPerGame.value"), None);
        assert_eq!(ev.unit_of("byColor.white.score.value"), None);
        for p in [
            "byColor.white.score.value",
            "byColor.white.score.low",
            "byColor.white.weaknesses.W4_missed.rate",
            "byColor.white.weaknesses.W4_missed.peerRate",
        ] {
            assert!(ev.rate_paths.contains(p), "{p} is a rate");
        }
        for p in ["byColor.white.accuracy.value", "byColor.white.games"] {
            assert!(!ev.rate_paths.contains(p), "{p} is not a rate");
        }
        let f = |p: &str, fmt| format_num_at(&ev, p, fmt).unwrap();
        assert_eq!(
            f("byColor.white.accuracy.value", Some(NumFormat::Dec1)),
            "78.4%"
        );
        assert_eq!(f("byColor.white.score.value", Some(NumFormat::Pct0)), "52%");
        assert_eq!(
            f("byColor.white.blundersPerGame.value", Some(NumFormat::Dec1)),
            "0.9"
        );
        assert_eq!(f("byColor.white.games", Some(NumFormat::Int)), "42");
        assert_eq!(f("benchmark.ratingHigh", None), "975");
    }

    #[test]
    fn a_paragraph_citing_the_split_validates() {
        let ev = ev();
        let text = "As White you scored {{num:byColor.white.score.value|pct0}} over \
                    {{num:byColor.white.games|int}} games, with an accuracy of \
                    {{num:byColor.white.accuracy.value|dec1}}. Players you have faced, rated \
                    about {{num:benchmark.ratingLow}} to {{num:benchmark.ratingHigh}}, miss \
                    these chances at {{num:byColor.white.weaknesses.W4_missed.peerRate|pct1}}.";
        let out = validate_text(text, &[], &ev, &TextRules::default(), "x").expect("valid");
        let nums: Vec<(String, String)> = out
            .iter()
            .filter_map(|s| match s {
                Segment::Num { path, display, .. } => Some((path.clone(), display.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            nums.iter().map(|(_, d)| d.as_str()).collect::<Vec<_>>(),
            ["52%", "42", "78.4%", "675", "975", "9.6%"]
        );
        // A null (insufficient) number cannot be cited.
        let bad = "As Black you scored {{num:byColor.black.score.value|pct0}}.";
        assert!(validate_text(bad, &[], &ev, &TextRules::default(), "x").is_err());
        // A percentage format on a non-rate path is refused.
        let bad = "{{num:byColor.white.accuracy.value|pct0}} accuracy.";
        assert!(validate_text(bad, &[], &ev, &TextRules::default(), "x").is_err());
    }

    #[test]
    fn a_fraction_reads_as_a_percentage_in_any_format() {
        // Q9: "As White you have 50 games and score 0.45, with a range of 0.36 to 0.54". A
        // score or rate is stored as a fraction; users read percentages.
        let ev = ev();
        let f = |p: &str, fmt| format_num_at(&ev, p, fmt).unwrap();
        assert_eq!(f("byColor.white.score.value", Some(NumFormat::Dec2)), "52%");
        assert_eq!(f("byColor.white.score.low", Some(NumFormat::Dec2)), "44%");
        assert_eq!(f("byColor.white.score.high", Some(NumFormat::Dec1)), "60%");
        assert_eq!(
            f("byColor.white.weaknesses.W4_missed.rate", Some(NumFormat::Dec2)),
            "14%"
        );
        // Below 10% one decimal is kept (a whole one would read 0% for a rare event).
        assert_eq!(
            f(
                "byColor.white.weaknesses.W4_missed.peerRate",
                Some(NumFormat::Dec2)
            ),
            "9.6%"
        );
        assert_eq!(f("metrics.hung_piece_rate.value", Some(NumFormat::Dec2)), "6%");
        assert_eq!(f("weaknesses.W9b.pImp", Some(NumFormat::Dec2)), "40%");
        // A score difference reads as one too.
        assert_eq!(
            f(
                "openings.b-sicilian-najdorf.performanceVsExpected",
                Some(NumFormat::Dec2)
            ),
            "-9%"
        );
        // pct0/pct1 keep their precision; non-fractions keep their own units.
        assert_eq!(f("byColor.white.score.value", Some(NumFormat::Pct1)), "52.4%");
        assert_eq!(
            f("byColor.white.blundersPerGame.value", Some(NumFormat::Dec1)),
            "0.9"
        );

        let text = "As White you have {{num:byColor.white.games|int}} games and score \
                    {{num:byColor.white.score.value|dec2}}, with a range of \
                    {{num:byColor.white.score.low|dec2}} to \
                    {{num:byColor.white.score.high|dec2}}.";
        let out = validate_text(text, &[], &ev, &TextRules::default(), "x").expect("valid");
        let shown: Vec<String> = out
            .iter()
            .filter_map(|s| match s {
                Segment::Num { display, .. } => Some(display.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(shown, ["42", "52%", "44%", "60%"]);
    }

    /// Every number leaf of the CoachingInput schema, as a pattern (`[]` for an array item),
    /// with its `minimum` and `maximum`.
    fn schema_leaves() -> BTreeMap<String, (Option<f64>, Option<f64>)> {
        fn walk(
            root: &Value,
            s: &Value,
            path: &str,
            out: &mut BTreeMap<String, (Option<f64>, Option<f64>)>,
            depth: u32,
        ) {
            assert!(depth < 64, "schema too deep at {path}");
            if let Some(r) = s.get("$ref").and_then(Value::as_str) {
                let name = r.rsplit('/').next().unwrap();
                let def = root["$defs"]
                    .get(name)
                    .or_else(|| root["definitions"].get(name))
                    .unwrap_or_else(|| panic!("unresolved {r}"));
                walk(root, def, path, out, depth + 1);
            }
            for k in ["anyOf", "oneOf", "allOf"] {
                for x in s.get(k).and_then(Value::as_array).into_iter().flatten() {
                    walk(root, x, path, out, depth + 1);
                }
            }
            if let Some(p) = s.get("properties").and_then(Value::as_object) {
                for (k, x) in p {
                    let child = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    walk(root, x, &child, out, depth + 1);
                }
            }
            if let Some(items) = s.get("items") {
                walk(root, items, &format!("{path}[]"), out, depth + 1);
            }
            let is_num = |t: &Value| matches!(t.as_str(), Some("number" | "integer"));
            let num = match s.get("type") {
                Some(Value::Array(ts)) => ts.iter().any(is_num),
                Some(t) => is_num(t),
                None => false,
            };
            if num {
                let range = (
                    s.get("minimum").and_then(Value::as_f64),
                    s.get("maximum").and_then(Value::as_f64),
                );
                out.insert(path.to_owned(), range);
            }
        }
        let root = ai::input::types::json_schema();
        let mut out = BTreeMap::new();
        walk(&root, &root, "", &mut out, 0);
        out
    }

    /// The schema pattern of an evidence path, and the object holding its number. `None` for
    /// a derived path (`count`, `pawns`, `moveNumber`, a ratio).
    fn pattern_of<'a>(json: &'a Value, path: &str) -> Option<(String, &'a Value)> {
        let mut cur = json;
        let mut parent = json;
        let mut pat = String::new();
        for s in path.split('.') {
            parent = cur;
            match cur {
                Value::Object(o) => {
                    cur = o.get(s)?;
                    if !pat.is_empty() {
                        pat.push('.');
                    }
                    pat.push_str(s);
                }
                Value::Array(xs) => {
                    cur = match s.parse::<usize>() {
                        Ok(i) => xs.get(i)?,
                        Err(_) => xs.iter().find(|x| {
                            x.as_object().is_some_and(|o| {
                                o.values().any(|v| {
                                    v.as_str().is_some_and(|t| {
                                        ai::validator::evidence::path_segment(t) == s
                                    })
                                })
                            })
                        })?,
                    };
                    pat.push_str("[]");
                }
                _ => return None,
            }
        }
        cur.is_number().then_some((pat, parent))
    }

    /// Q9: no number stored as a fraction renders bare. The schema says which leaves are
    /// fractions (`0..1`, or `-1..1` for a score difference); a metric's own `unit: "rate"`
    /// says it for the unit-dependent `value`/`low`/`high`/`bandMedian`. A new fraction field
    /// fails here until the fixture covers it and the evidence marks it.
    #[test]
    fn every_fraction_path_of_the_schema_renders_as_a_percentage() {
        let leaves = schema_leaves();
        assert!(leaves.contains_key("byColor.white.score.value"), "{leaves:#?}");
        assert!(leaves.contains_key("weaknesses[].posteriorRate"), "{leaves:#?}");
        let fraction_leaf = |p: &str| {
            matches!(leaves.get(p), Some((Some(lo), Some(hi))) if *hi == 1.0 && (*lo == 0.0 || *lo == -1.0))
        };
        let wanted: BTreeSet<&str> = leaves
            .keys()
            .map(String::as_str)
            .filter(|p| fraction_leaf(p))
            .collect();

        let input = v2_input("b2-rapid");
        let json = input.to_json();
        let ev = build_evidence(&input);
        let mut hit = BTreeSet::new();
        let mut by_unit = 0;
        let mut bad = Vec::new();
        for path in ev.number_paths.keys() {
            let Some((pat, parent)) = pattern_of(&json, path) else {
                continue;
            };
            let field = path.rsplit('.').next().unwrap();
            let rate_unit = parent.get("unit").and_then(Value::as_str) == Some("rate")
                && ["value", "low", "high", "bandMedian"].contains(&field);
            if !(fraction_leaf(&pat) || rate_unit) {
                continue;
            }
            hit.insert(pat.clone());
            by_unit += usize::from(rate_unit);
            if !ev.rate_paths.contains(path) {
                bad.push(format!("{path}: not a rate path"));
            }
            for fmt in [Some(NumFormat::Dec1), Some(NumFormat::Dec2), None] {
                let shown = format_num_at(&ev, path, fmt).unwrap();
                if !shown.ends_with('%') {
                    bad.push(format!("{path} ({fmt:?}) renders bare: {shown}"));
                }
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
        let missing: Vec<_> = wanted.iter().filter(|p| !hit.contains(**p)).collect();
        assert!(missing.is_empty(), "fixture covers no {missing:?}");
        assert!(by_unit >= 4, "rate metrics and the colour score: {by_unit}");
    }

    #[test]
    fn a_v1_input_has_no_colour_paths() {
        let ev = build_evidence(&input("b2-rapid"));
        assert!(ev.number_paths.keys().all(|p| !p.starts_with("byColor")));
        assert!(ev.number_paths.keys().all(|p| !p.starts_with("benchmark")));
    }
}
