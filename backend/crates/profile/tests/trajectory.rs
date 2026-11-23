//! The coach's trajectory is the web's (ADR 0088): every case of the web-written fixture
//! (`backend/fixtures/trajectory/cases.json`, `apps/web/test/trajectory/parity.test.ts`) is
//! replayed here and must give the same windows, verdicts and numbers. Then the contract mapping:
//! units, insufficient sides and the months.

use std::collections::HashMap;

use coaching_input::{ChangeVerdict, MetricUnit, TrajectoryKind, WindowBasis};
use profile::coaching::trajectory::{
    Computed, Estimate, GameResult, Input, Prior, RatedGame, Row, RowKey, StatRow, WeaknessInput,
    compute, contract, fnv1a, pre_game_ratings, prior_of,
};
use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

fn fixture() -> Value {
    let path = format!(
        "{}/../../fixtures/trajectory/cases.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

fn at(s: &Value) -> OffsetDateTime {
    OffsetDateTime::parse(s.as_str().unwrap(), &Rfc3339).unwrap()
}

fn input_of(c: &Value) -> Input {
    let games = c["games"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| RatedGame {
            id: g["id"].as_str().unwrap().to_owned(),
            played_at: at(&g["playedAt"]),
            rating: g["rating"].as_f64().unwrap(),
        })
        .collect();
    let current = (!c["current"].is_null())
        .then(|| (c["current"]["rating"].as_f64().unwrap(), at(&c["current"]["at"])));
    let stats = c["stats"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| StatRow {
            id: s["id"].as_str().unwrap().to_owned(),
            result: match s["result"].as_str().unwrap() {
                "win" => GameResult::Win,
                "draw" => GameResult::Draw,
                "loss" => GameResult::Loss,
                _ => GameResult::Unfinished,
            },
            analysed: s["analysed"].as_bool().unwrap(),
            accuracy: s["accuracy"].as_f64(),
            blunders: s["blunders"].as_f64(),
        })
        .collect();
    let weaknesses = c["weaknesses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| WeaknessInput {
            series_key: w["seriesKey"].as_str().unwrap().to_owned(),
            prior: Prior {
                mu: w["prior"]["mu"].as_f64().unwrap(),
                n0: w["prior"]["n0"].as_f64().unwrap(),
            },
            points: w["points"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| {
                    (
                        p[0].as_str().unwrap().to_owned(),
                        (p[1].as_u64().unwrap() as u32, p[2].as_u64().unwrap() as u32),
                    )
                })
                .collect::<HashMap<_, _>>(),
        })
        .collect();
    Input {
        games,
        current,
        stats,
        weaknesses,
        seed: c["seed"].as_str().unwrap().to_owned(),
    }
}

fn close(name: &str, got: Option<f64>, want: &Value) {
    match (got, want.as_f64()) {
        (None, None) => {}
        (Some(g), Some(w)) => assert!((g - w).abs() <= 1e-9 * w.abs().max(1.0), "{name}: {g} != {w}"),
        _ => panic!("{name}: {got:?} != {want}"),
    }
}

fn estimate(name: &str, e: &Estimate, want: &Value) {
    match e {
        Estimate::Estimate { value, interval, n } => {
            close(&format!("{name}.value"), Some(*value), &want["value"]);
            close(&format!("{name}.low"), interval.map(|i| i.low), &want["low"]);
            close(&format!("{name}.high"), interval.map(|i| i.high), &want["high"]);
            assert_eq!(want["n"].as_u64(), Some(*n as u64), "{name}.n");
        }
        Estimate::Insufficient { n, .. } => {
            assert!(want["value"].is_null(), "{name}: insufficient here, {want} on the web");
            assert_eq!(want["n"].as_u64(), Some(*n as u64), "{name}.n");
        }
    }
}

fn verdict(v: profile::coaching::trajectory::Verdict) -> &'static str {
    use profile::coaching::trajectory::Verdict::*;
    match v {
        Better => "better",
        Worse => "worse",
        NoClearChange => "no_clear_change",
        NotEnoughGames => "not_enough_games",
    }
}

#[test]
fn fnv1a_hashes_like_the_web() {
    // TS `fnv1a("")` and `fnv1a("a")`.
    assert_eq!(fnv1a(""), 0x811c_9dc5);
    assert_eq!(fnv1a("a"), 0xe40c_292c);
}

#[test]
fn every_web_case_replays_exactly() {
    let f = fixture();
    let cases = f["cases"].as_array().unwrap();
    assert!(cases.len() >= 5);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let want = &c["expected"];
        let (_, got) = compute(&input_of(&c["input"]));
        let r = match got {
            Computed::NotEnoughGames { games, needed } => {
                assert_eq!(want["kind"], "not_enough_games", "{name}");
                assert_eq!(want["games"].as_u64(), Some(games as u64), "{name}");
                assert_eq!(want["needed"].as_u64(), Some(needed as u64), "{name}");
                continue;
            }
            Computed::Ready(r) => r,
        };
        let kind = match r.kind {
            TrajectoryKind::Falling => "falling",
            TrajectoryKind::Rising => "rising",
            TrajectoryKind::Flat => "flat",
        };
        assert_eq!(want["kind"], kind, "{name}");
        assert_eq!(want["games"].as_u64(), Some(r.games as u64), "{name}");
        close(&format!("{name}.peak"), Some(r.peak.0), &want["peak"]["rating"]);
        assert_eq!(want["peak"]["gameId"].as_str(), r.peak.2.as_deref(), "{name}");
        close(&format!("{name}.current"), Some(r.current.0), &want["current"]);
        close(&format!("{name}.change"), Some(r.change), &want["change"]);
        for (w, side) in [(&r.then, &want["then"]), (&r.now, &want["now"])] {
            let basis = match w.basis {
                WindowBasis::Peak => "peak",
                WindowBasis::Start => "start",
                WindowBasis::Last => "last",
            };
            assert_eq!(side["basis"], basis, "{name}");
            assert_eq!(side["games"].as_u64(), Some(w.games as u64), "{name}");
            assert_eq!(side["firstId"], w.first_id.as_str(), "{name}");
            assert_eq!(side["lastId"], w.last_id.as_str(), "{name}");
            close(&format!("{name}.meanRating"), Some(w.mean_rating), &side["meanRating"]);
        }
        assert_eq!(want["rating"]["verdict"], verdict(r.rating.verdict), "{name}");
        estimate(&format!("{name}.rating.a"), &r.rating_sides.0, &want["rating"]["a"]);
        estimate(&format!("{name}.rating.b"), &r.rating_sides.1, &want["rating"]["b"]);
        if let Some((d, i)) = r.rating.diff {
            close(&format!("{name}.rating.diff"), Some(d), &want["rating"]["diff"]["value"]);
            close("low", Some(i.low), &want["rating"]["diff"]["interval"]["low"]);
            close("high", Some(i.high), &want["rating"]["diff"]["interval"]["high"]);
        }
        let rows = want["rows"].as_array().unwrap();
        assert_eq!(rows.len(), r.rows.len(), "{name} rows");
        for (row, w) in r.rows.iter().zip(rows) {
            let label = format!("{name}.{}", w["key"].as_str().unwrap());
            match row {
                Row::Mean { key, a, b, result } => {
                    let k = match key {
                        RowKey::Accuracy => "accuracy",
                        RowKey::Blunders => "blunders",
                        RowKey::Score => "score",
                        RowKey::Weakness(_) => unreachable!(),
                    };
                    assert_eq!(w["key"], k, "{label}");
                    assert_eq!(w["verdict"], verdict(result.verdict), "{label}");
                    estimate(&format!("{label}.a"), a, &w["a"]);
                    estimate(&format!("{label}.b"), b, &w["b"]);
                    match result.diff {
                        Some((d, i)) => {
                            close(&label, Some(d), &w["diff"]["value"]);
                            close(&label, Some(i.low), &w["diff"]["interval"]["low"]);
                            close(&label, Some(i.high), &w["diff"]["interval"]["high"]);
                        }
                        None => assert!(w["diff"].is_null(), "{label}"),
                    }
                }
                Row::Rate {
                    key: RowKey::Weakness(k),
                    counts,
                    a,
                    b,
                    result,
                } => {
                    assert_eq!(w["key"], format!("weakness:{k}"), "{label}");
                    assert_eq!(w["verdict"], verdict(result.verdict), "{label}");
                    for (side, c) in [("a", counts.0), ("b", counts.1)] {
                        assert_eq!(w["counts"][side]["events"], c.events, "{label}");
                        assert_eq!(w["counts"][side]["opportunities"], c.opportunities, "{label}");
                        assert_eq!(w["counts"][side]["games"].as_u64(), Some(c.games as u64), "{label}");
                    }
                    for (side, p) in [("a", a), ("b", b)] {
                        match p {
                            Some(p) => {
                                close(&label, Some(p.mean), &w[side]["value"]);
                                close(&label, Some(p.interval.low), &w[side]["low"]);
                                close(&label, Some(p.interval.high), &w[side]["high"]);
                            }
                            None => assert!(w[side].is_null(), "{label}.{side}"),
                        }
                    }
                    close(&format!("{label}.pBetter"), result.p_better, &w["pBetter"]);
                    close(&format!("{label}.pWorse"), result.p_worse, &w["pWorse"]);
                }
                Row::Rate { .. } => unreachable!(),
            }
        }
    }
}

#[test]
fn the_contract_states_units_shortfalls_and_months() {
    let f = fixture();
    let c = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "falling")
        .unwrap();
    let (series, computed) = compute(&input_of(&c["input"]));
    let t = contract(&series, &computed);
    assert!(t.sufficient);
    assert_eq!(t.kind, Some(TrajectoryKind::Falling));
    let peak = t.peak.as_ref().unwrap();
    assert!(peak.rating >= t.current.as_ref().unwrap().rating);
    assert_eq!(t.current.as_ref().unwrap().rating, 816);
    let v = t.then_vs_now.as_ref().unwrap();
    assert_eq!(v.then.basis, WindowBasis::Peak);
    assert_eq!(v.rating.then.unit, MetricUnit::Elo);
    assert_eq!(v.accuracy.then.unit, MetricUnit::Pct);
    assert_eq!(v.blunders_per_game.then.unit, MetricUnit::PerGame);
    assert_eq!(v.score.then.unit, MetricUnit::Rate);
    assert_eq!(v.accuracy.verdict, ChangeVerdict::Worse);
    // The two-chance weakness has too few games on the "then" side: no number, its shortfall.
    let few = v.weaknesses.iter().find(|w| w.series_key == "W5").unwrap();
    assert_eq!(few.verdict, ChangeVerdict::NotEnoughGames);
    assert!(few.then.value.is_none() && few.then.need_more.is_some());
    assert_eq!((few.id.as_str(), few.variant.as_deref()), ("W5", None));
    // 140 daily games from March 2025: five calendar months, oldest first.
    let months: Vec<&str> = t.months.iter().map(|m| m.month.as_str()).collect();
    assert_eq!(months.first(), Some(&"2025-03"));
    assert!(months.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(t.months.iter().map(|m| m.games).sum::<u32>(), 140);

    let short = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "few")
        .unwrap();
    let (series, computed) = compute(&input_of(&short["input"]));
    let t = contract(&series, &computed);
    assert!(!t.sufficient);
    assert_eq!(t.need_more, Some(8));
    assert!(t.peak.is_none() && t.then_vs_now.is_none() && t.change.is_none());
}

#[test]
fn pre_game_ratings_shift_chesscom_by_one_game() {
    let t = |d: i64| OffsetDateTime::UNIX_EPOCH + time::Duration::days(d);
    let rows = vec![
        ("a".to_owned(), t(1), Some(1000)),
        ("b".to_owned(), t(2), Some(1010)),
        ("c".to_owned(), t(3), None),
        ("d".to_owned(), t(4), Some(990)),
    ];
    let chesscom: Vec<f64> = pre_game_ratings(true, &rows).iter().map(|g| g.rating).collect();
    // "d" follows an unrated-looking row with no rating: it keeps its own, as the web does.
    assert_eq!(chesscom, [1000.0, 1000.0, 1010.0, 990.0]);
    let lichess: Vec<f64> = pre_game_ratings(false, &rows).iter().map(|g| g.rating).collect();
    assert_eq!(lichess, [1000.0, 1010.0, 990.0]);
}

#[test]
fn the_prior_comes_back_out_of_the_posterior() {
    let p = prior_of(30.0, 100.0, 42.0, 118.0);
    assert!((p.n0 - 60.0).abs() < 1e-9 && (p.mu - 0.2).abs() < 1e-9);
    assert_eq!(prior_of(5.0, 10.0, 5.0, 5.0), Prior { mu: 0.5, n0: 2.0 });
}
