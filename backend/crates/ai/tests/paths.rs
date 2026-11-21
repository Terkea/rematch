//! Port of `.worktrees/m3/packages/ai/test/validator/paths.test.ts` (each `describe` is a module,
//! each `it` title is snake_case), plus the R3 `{{eng:}}` tokens, segment rendering and the
//! recorded `formatNumber` grid.

use ai::input::types::{CoachingInput, Score};
use ai::segments::{PvSide, Segment};
use ai::testing::{fixtures_dir, input};
use ai::validator::evidence::{
    EngineEvidence, EngineLine, Evidence, build_evidence, create_evidence, list_date_paths,
    list_number_paths, resolve_date_path, resolve_number_path,
};
use ai::validator::format::{format_number, resolve_token};
use ai::validator::segments_render::render_segments;
use ai::validator::tokens::{NumFormat, TokenBody, canonical_for_tokens, parse_tokens};
use chess_core::Color;

fn setup() -> (CoachingInput, Evidence) {
    let input = input("b2-rapid");
    let ev = build_evidence(&input);
    (input, ev)
}

fn mk(s: &str) -> TokenBody {
    let parsed = parse_tokens(s);
    assert!(parsed.errors.is_empty(), "{s}: {:?}", parsed.errors);
    parsed.tokens.into_iter().next().expect("no token").body
}

fn iso(d: time::Date) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())
}

fn cp(s: Score) -> f64 {
    match s {
        Score::Cp(c) => f64::from(c),
        Score::Mate(_) => 0.0,
    }
}

mod number_path_index {
    use super::*;

    #[test]
    fn resolves_basis_weakness_metric_and_exemplar_paths() {
        let (input, ev) = setup();
        let w1 = &input.weaknesses[0];
        let ex = &w1.exemplars[0];
        let n = |p: &str| resolve_number_path(&ev, p);
        assert_eq!(
            n("basis.analysedGames"),
            Some(f64::from(input.basis.analysed_games))
        );
        assert_eq!(
            n("basis.importedGames"),
            Some(f64::from(input.basis.imported_games))
        );
        assert_eq!(n("weaknesses.W1.posteriorRate"), Some(w1.posterior_rate));
        assert_eq!(n("weaknesses.W1.events"), Some(f64::from(w1.events)));
        let m = &input.metrics[0];
        assert_eq!(n(&format!("metrics.{}.value", m.key)), Some(m.value));
        assert_eq!(n("timeControl.baseMinutes"), Some(10.0));
        assert_eq!(
            n("weaknesses.W1.exemplars.0.moveNumber"),
            Some(f64::from(ex.ply.div_ceil(2)))
        );
        let pawns = n("weaknesses.W1.exemplars.0.evalAfter.pawns").unwrap();
        assert!((pawns - cp(ex.eval_after) / 100.0).abs() < 1e-9);
        assert_eq!(n("weaknesses.W1.exemplars.0.winAfter"), Some(ex.win_after));
    }

    #[test]
    fn resolves_changes_by_series_key_and_counts() {
        let (input, ev) = setup();
        let c = &input.changes[0];
        let path = format!("changes.{}.rateAfter", c.series_key.replace('.', "_"));
        assert_eq!(resolve_number_path(&ev, &path), Some(c.rate_after));
        assert_eq!(
            resolve_number_path(&ev, "weaknesses.count"),
            Some(input.weaknesses.len() as f64)
        );
    }

    #[test]
    fn returns_undefined_for_unknown_paths() {
        let (_, ev) = setup();
        assert_eq!(
            resolve_number_path(&ev, "weaknesses.W99.posteriorRate"),
            None
        );
        assert_eq!(resolve_number_path(&ev, "nope"), None);
    }

    #[test]
    fn lists_paths() {
        let (_, ev) = setup();
        let paths = list_number_paths(&ev);
        assert!(paths.iter().any(|p| p == "basis.analysedGames"));
        assert!(paths.iter().any(|p| p == "weaknesses.W1.posteriorRate"));
        let mut sorted = paths.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(paths, sorted, "sorted and unique");
        let dates = list_date_paths(&ev);
        assert!(dates.iter().any(|p| p == "basis.windowStart"));
        assert!(dates.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn resolves_date_paths() {
        let (input, ev) = setup();
        assert_eq!(
            resolve_date_path(&ev, "basis.windowStart"),
            Some(iso(input.basis.window_start).as_str())
        );
        assert_eq!(resolve_date_path(&ev, "nope"), None);
    }
}

mod format_number {
    use super::*;

    #[test]
    fn formats_every_fmt() {
        assert_eq!(format_number(214.4, Some(NumFormat::Int)), "214");
        assert_eq!(format_number(0.3149, Some(NumFormat::Pct0)), "31%");
        assert_eq!(format_number(0.3149, Some(NumFormat::Pct1)), "31.5%");
        assert_eq!(format_number(1.234, Some(NumFormat::Dec1)), "1.2");
        assert_eq!(format_number(1.235, Some(NumFormat::Dec2)), "1.24");
        assert_eq!(format_number(1.5, Some(NumFormat::Signed1)), "+1.5");
        assert_eq!(format_number(-1.5, Some(NumFormat::Signed1)), "-1.5");
        assert_eq!(format_number(0.0, Some(NumFormat::Signed1)), "0.0");
        assert_eq!(format_number(3912.0, Some(NumFormat::Int)), "3,912");
    }

    /// JS `Math.round` rounds half up (towards +inf): 2.5 is 3 and -2.5 is -2. `round(v, d)` adds
    /// `EPSILON * sign(v)` first, so -1.25 at one decimal is -1.3 in TS (recorded with Node 22),
    /// not the brief's "-1.2"; the TS behaviour wins (task-7-report.md).
    #[test]
    fn format_number_rounds_half_up_like_js() {
        assert_eq!(format_number(2.5, Some(NumFormat::Int)), "3");
        assert_eq!(format_number(-2.5, Some(NumFormat::Int)), "-2");
        assert_eq!(format_number(-1.25, Some(NumFormat::Dec1)), "-1.3");
        assert_eq!(format_number(1.25, Some(NumFormat::Dec1)), "1.3");
        assert_eq!(format_number(-0.04, Some(NumFormat::Dec1)), "0.0", "no -0");
        assert_eq!(format_number(-0.4, Some(NumFormat::Int)), "0", "no -0");
        assert_eq!(format_number(1234567.0, None), "1,234,567");
        assert_eq!(format_number(-3912.0, None), "-3,912");
        assert_eq!(format_number(1.0 / 3.0, None), "0.33");
    }

    /// Every row of `formatNumber` recorded from TS (values × formats).
    #[test]
    fn rust_matches_the_recorded_ts_format_grid() {
        let path = fixtures_dir().join("evidence/ts-evidence.json");
        let g: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let rows = g["format"].as_array().unwrap();
        assert!(rows.len() > 200);
        let mut bad = Vec::new();
        for r in rows {
            let v: f64 = r[0].as_str().unwrap().parse().unwrap();
            let fmt = r[1].as_str().map(|f| NumFormat::parse(f).unwrap());
            let want = r[2].as_str().unwrap();
            let got = format_number(v, fmt);
            if got != want {
                bad.push(format!("{v} {fmt:?}: got {got}, want {want}"));
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }
}

mod resolve_token {
    use super::*;

    #[test]
    fn renders_move_best_and_pv_tokens() {
        let (input, ev) = setup();
        let ex = &input.weaknesses[0].exemplars[0];
        let id = format!("{}:{}", ex.game_id, ex.ply);
        let r = |s: String| resolve_token(&ev, &mk(&s));
        assert_eq!(r(format!("{{{{move:{id}}}}}")), Some(ex.played.san.clone()));
        assert_eq!(
            r(format!("{{{{best:{id}}}}}")),
            ex.best.as_ref().map(|b| b.san.clone())
        );
        assert_eq!(
            r(format!("{{{{pv:{id}:best:2}}}}")).as_deref(),
            Some("dxe5")
        );
        assert_eq!(
            r(format!("{{{{pv:{id}:refutation:2}}}}")).as_deref(),
            Some("Nxd4")
        );
        assert_eq!(r(format!("{{{{pv:{id}:best:99}}}}")), None);
        assert_eq!(r("{{move:nope:1}}".into()), None);
    }

    #[test]
    fn renders_line_num_date_and_sq_tokens() {
        let (input, ev) = setup();
        let o = &input.openings[0];
        let r = |s: &str| resolve_token(&ev, &mk(s));
        assert_eq!(
            r(&format!("{{{{line:{}}}}}", o.line_key)),
            Some(o.moves_san.join(" "))
        );
        assert_eq!(
            r(&format!("{{{{line:{}:2}}}}", o.line_key)),
            Some(o.moves_san[1].clone())
        );
        assert_eq!(r("{{num:basis.analysedGames|int}}").as_deref(), Some("214"));
        assert_eq!(
            r("{{date:basis.windowStart}}"),
            Some(iso(input.basis.window_start))
        );
        assert_eq!(r("{{sq:f7}}").as_deref(), Some("f7"));
    }

    const ID: &str = "0123456789abcdef0123456789abcdef";

    fn eng_ev(score: Score) -> Evidence {
        let mut ev = create_evidence();
        ev.add_engine_eval(
            ID.into(),
            EngineEvidence {
                tried_san: vec!["Nf3".into()],
                lines: vec![
                    EngineLine {
                        score: Score::Cp(130),
                        side_to_move: Color::Black,
                        san: vec!["e5".into(), "Nxe5".into()],
                    },
                    EngineLine {
                        score,
                        side_to_move: Color::Black,
                        san: vec!["d5".into()],
                    },
                ],
            },
        );
        ev
    }

    #[test]
    fn resolves_eng_tokens() {
        let ev = eng_ev(Score::Mate(3));
        let r = |ev: &Evidence, s: &str| resolve_token(ev, &mk(s));
        assert_eq!(
            r(&ev, &format!("{{{{eng:{ID}:tried}}}}")).as_deref(),
            Some("Nf3")
        );
        assert_eq!(
            r(&ev, &format!("{{{{eng:{ID}:1}}}}")).as_deref(),
            Some("+1.3")
        );
        assert_eq!(
            r(&ev, &format!("{{{{eng:{ID}:2}}}}")).as_deref(),
            Some("mate in 3")
        );
        let ev2 = eng_ev(Score::Mate(-2));
        assert_eq!(
            r(&ev2, &format!("{{{{eng:{ID}:2}}}}")).as_deref(),
            Some("mated in 2")
        );
        let ev3 = eng_ev(Score::Cp(-45));
        assert_eq!(
            r(&ev3, &format!("{{{{eng:{ID}:2}}}}")).as_deref(),
            Some("-0.5")
        );
        assert_eq!(
            r(&ev, &format!("{{{{eng:{ID}:1:2}}}}")).as_deref(),
            Some("Nxe5")
        );
        assert_eq!(r(&ev, &format!("{{{{eng:{ID}:1:3}}}}")), None);
        assert_eq!(r(&ev, &format!("{{{{eng:{ID}:3}}}}")), None, "missing line");
        let other = "f".repeat(32);
        assert_eq!(
            r(&ev, &format!("{{{{eng:{other}:1}}}}")),
            None,
            "unknown id"
        );
        assert_eq!(r(&ev, &format!("{{{{eng:{other}:tried}}}}")), None);
        assert_eq!(r(&ev, "{{offtopic}}"), None, "the chat engine handles it");
    }
}

mod render_segments {
    use super::*;

    fn render(ev: &Evidence, text: &str) -> Option<Vec<Segment>> {
        let canonical = canonical_for_tokens(text);
        let parsed = parse_tokens(text);
        render_segments(&canonical, &parsed.tokens, ev)
    }

    #[test]
    fn render_segments_in_order() {
        let (input, ev) = setup();
        let ex = &input.weaknesses[0].exemplars[0];
        let text = format!(
            "you played {{{{move:{}:{}}}}} at {{{{sq:f7}}}}",
            ex.game_id, ex.ply
        );
        assert_eq!(
            render(&ev, &text),
            Some(vec![
                Segment::Text {
                    text: "you played ".into()
                },
                Segment::Move {
                    game_id: ex.game_id.clone(),
                    ply: ex.ply,
                    san: ex.played.san.clone()
                },
                Segment::Text {
                    text: " at ".into()
                },
                Segment::Square {
                    square: "f7".into()
                },
            ])
        );
        assert_eq!(render(&ev, "see {{move:nope:1}} here"), None);
        assert_eq!(render(&ev, "{{offtopic}}"), None);
    }

    #[test]
    fn renders_every_kind_with_its_fields() {
        let (input, mut ev) = setup();
        let ex = &input.weaknesses[0].exemplars[0];
        let o = &input.openings[0];
        ev.add_engine_eval(
            "a".repeat(32),
            EngineEvidence {
                tried_san: vec!["Nf3".into(), "d4".into()],
                lines: vec![EngineLine {
                    score: Score::Cp(-130),
                    side_to_move: Color::White,
                    san: vec!["e5".into()],
                }],
            },
        );
        let (g, p) = (&ex.game_id, ex.ply);
        let a = "a".repeat(32);
        let text = format!(
            "{{{{best:{g}:{p}}}}}{{{{pv:{g}:{p}:refutation:2}}}}{{{{line:{k}:2}}}}\
             {{{{num:basis.analysedGames}}}}{{{{num:weaknesses.W1.posteriorRate|pct0}}}}\
             {{{{date:basis.windowStart}}}}{{{{eng:{a}:tried}}}}{{{{eng:{a}:1}}}}{{{{eng:{a}:1:1}}}}.",
            k = o.line_key
        );
        let rate = input.weaknesses[0].posterior_rate;
        let want = vec![
            Segment::Best {
                game_id: g.clone(),
                ply: p,
                san: ex.best.as_ref().unwrap().san.clone(),
            },
            Segment::Pv {
                game_id: g.clone(),
                ply: p,
                side: PvSide::Refutation,
                n: 2,
                san: "Nxd4".into(),
            },
            Segment::Line {
                line_key: o.line_key.clone(),
                n: Some(2),
                san: o.moves_san[1].clone(),
            },
            Segment::Num {
                path: "basis.analysedGames".into(),
                value: 214.0,
                display: "214".into(),
            },
            Segment::Num {
                path: "weaknesses.W1.posteriorRate".into(),
                value: rate,
                display: format_number(rate, Some(NumFormat::Pct0)),
            },
            Segment::Date {
                path: "basis.windowStart".into(),
                date: iso(input.basis.window_start),
            },
            Segment::Eng {
                eval_id: a.clone(),
                line: None,
                n: None,
                text: "Nf3 d4".into(),
            },
            Segment::Eng {
                eval_id: a.clone(),
                line: Some(1),
                n: None,
                text: "-1.3".into(),
            },
            Segment::Eng {
                eval_id: a.clone(),
                line: Some(1),
                n: Some(1),
                text: "e5".into(),
            },
            Segment::Text { text: ".".into() },
        ];
        assert_eq!(render(&ev, &text), Some(want));
    }

    #[test]
    fn text_without_tokens_is_one_run_and_empty_text_is_none_of_them() {
        let ev = create_evidence();
        assert_eq!(
            render(&ev, "plain words"),
            Some(vec![Segment::Text {
                text: "plain words".into()
            }])
        );
        assert_eq!(render(&ev, ""), Some(vec![]));
    }

    /// Tokens whose index or raw text does not match `canonical` never panic: they fail.
    #[test]
    fn mismatched_tokens_fail_without_panicking() {
        let ev = create_evidence();
        let parsed = parse_tokens("ab {{sq:f7}}");
        assert_eq!(render_segments("ab", &parsed.tokens, &ev), None);
        assert_eq!(render_segments("ab é{{sq:f7}}", &parsed.tokens, &ev), None);
        let mut twice = parsed.tokens.clone();
        twice.extend(parsed.tokens.clone());
        assert_eq!(
            render_segments("ab {{sq:f7}}", &twice, &ev),
            None,
            "overlap"
        );
    }
}
