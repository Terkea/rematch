//! Port of `packages/ai/test/input/schema.test.ts` (m3 worktree) against R2's
//! `coaching_input::validate()`, via `ai::input::parse::parse_coaching_input`.

use ai::input::parse::{CoachingInputIssue, is_weakness_id, parse_coaching_input};
use ai::testing::{load_fixture, make_input};
use chess_core::board::Position;
use serde_json::{Value, json};

const FIXTURES: [&str; 3] = ["b2-rapid", "b0-blitz", "early-read"];

fn w1() -> Value {
    load_fixture("b2-rapid")["weaknesses"][0].clone()
}

fn ok(v: &Value) -> bool {
    parse_coaching_input(v).is_ok()
}

fn issues(v: &Value) -> Vec<CoachingInputIssue> {
    parse_coaching_input(v).err().unwrap_or_default()
}

fn paths(v: &Value) -> Vec<String> {
    issues(v).into_iter().map(|i| i.path).collect()
}

fn has(v: &Value, path: &str) -> bool {
    let p = paths(v);
    assert!(p.iter().any(|x| x == path), "{path} not in {p:?}");
    true
}

fn with_exemplar(patch: Value) -> Value {
    let mut f = load_fixture("b2-rapid");
    let ex = f["weaknesses"][0]["exemplars"][0].as_object_mut().unwrap();
    for (k, v) in patch.as_object().unwrap() {
        ex.insert(k.clone(), v.clone());
    }
    f
}

fn raw() -> Value {
    load_fixture("b2-rapid")
}

fn with_w1(patch: Value) -> Value {
    let mut w = w1();
    for (k, v) in patch.as_object().unwrap() {
        w[k] = v.clone();
    }
    make_input(json!({ "weaknesses": [w] }))
}

// ---- parseCoachingInput ------------------------------------------------------------------

#[test]
fn accepts_fixtures() {
    for name in FIXTURES {
        let r = parse_coaching_input(&load_fixture(name));
        assert!(r.is_ok(), "{name}: {:?}", r.err());
    }
}

#[test]
fn validates_the_weakness_id_pattern() {
    assert!(!ok(&with_w1(json!({"id": "W13"}))));
    assert!(ok(&with_w1(json!({"id": "W9b"}))));
    assert!(!ok(&with_w1(json!({"id": "W0"}))));
    assert!(ok(&with_w1(json!({"id": "W12d"}))));
    for s in ["W1", "W12", "W12d", "W9b"] {
        assert!(is_weakness_id(s), "{s}");
    }
    for s in [
        "W13", "W0", "", "W", "W1e", "W12dd", "w1", "W1 ", "W+1", "W01",
    ] {
        assert!(!is_weakness_id(s), "{s}");
    }
}

#[test]
fn requires_every_area_exactly_once() {
    let b2 = raw();
    let areas = b2["areaScores"].as_array().unwrap().clone();
    let mut missing = b2.clone();
    missing["areaScores"] = Value::Array(areas[1..].to_vec());
    let is = issues(&missing);
    assert_eq!(is.len(), 1, "{is:?}");
    assert_eq!(is[0].path, "areaScores");
    let mut dup = b2.clone();
    let mut v = areas[1..].to_vec();
    v.push(areas[1].clone());
    dup["areaScores"] = Value::Array(v);
    assert!(!ok(&dup));
}

#[test]
fn rejects_unknown_keys_and_names_them() {
    let mut d = raw();
    d["focus"] = json!([]);
    let is = issues(&d);
    assert!(!is.is_empty());
    assert!(serde_json::to_string(&is).unwrap().contains("focus"));
}

#[test]
fn rejects_an_invalid_fen_a_bad_uci_and_an_out_of_range_confidence() {
    assert!(!ok(&with_exemplar(json!({"fen": "not a fen"}))));
    let mut played = w1()["exemplars"][0]["played"].clone();
    played["uci"] = json!("e2e9");
    assert!(!ok(&with_exemplar(json!({"played": played}))));
    assert!(!ok(&with_w1(json!({"confidence": 1.2}))));
}

#[test]
fn carries_exemplars_on_strengths() {
    let b2 = raw();
    assert!(
        !b2["strengths"][0]["exemplars"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let bad = make_input(json!({"strengths": [{"exemplars": []}]}));
    assert!(ok(&bad));
}

#[test]
fn limits_exemplars_to_5_per_weakness() {
    let mut f = raw();
    let e = f["weaknesses"][0]["exemplars"][0].clone();
    f["weaknesses"][0]["exemplars"] = Value::Array(vec![e; 6]);
    assert!(!ok(&f));
}

// ---- fixtures ----------------------------------------------------------------------------

#[test]
fn fixtures_match_the_briefs_shape() {
    let b2 = raw();
    let ws = b2["weaknesses"].as_array().unwrap();
    assert_eq!(ws.len(), 6);
    let by_id = |id: &str| ws.iter().find(|w| w["id"] == id).unwrap();
    assert_eq!(by_id("W1")["exemplars"].as_array().unwrap().len(), 3);
    assert_eq!(by_id("W3")["variant"], "fork");
    assert_eq!(by_id("W3")["exemplars"].as_array().unwrap().len(), 3);
    let openings = b2["openings"].as_array().unwrap();
    assert_eq!(openings.len(), 3);
    assert!(
        openings
            .iter()
            .any(|o| o["verdict"] == "fix" && !o["deviationPly"].is_null())
    );
    assert_eq!(b2["changes"].as_array().unwrap().len(), 2);
    assert_eq!(b2["basis"]["analysedGames"], 214);
    assert_eq!(b2["basis"]["importedGames"], 3912);
    assert_eq!(
        b2["timeControl"],
        json!({"baseSeconds": 600, "incrementSeconds": 5})
    );
    assert_eq!(b2["rating"], 1450);
    assert_eq!(load_fixture("b0-blitz")["band"], "B0");
    let er = load_fixture("early-read");
    assert_eq!(er["basis"]["analysedGames"], 30);
    let insufficient = er["areaScores"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["sufficient"] == false && !a["needMore"].is_null())
        .count();
    assert_eq!(insufficient, 3);
    assert!(
        er["weaknesses"]
            .as_array()
            .unwrap()
            .iter()
            .all(|w| w["confidence"].as_f64().unwrap() < 0.7)
    );
    assert_eq!(er["openings"], json!([]));
}

#[test]
fn make_input_deep_merges_onto_b2_rapid() {
    let m = make_input(json!({"band": "B3", "basis": {"analysedGames": 99}}));
    assert_eq!(m["band"], "B3");
    assert_eq!(m["basis"]["analysedGames"], 99);
    assert_eq!(m["basis"]["importedGames"], 3912);
    assert_eq!(m["weaknesses"].as_array().unwrap().len(), 6);
}

fn all_exemplars(f: &Value) -> Vec<Value> {
    let mut out = vec![];
    for (outer, inner) in [
        ("weaknesses", "exemplars"),
        ("strengths", "exemplars"),
        ("openings", "examples"),
        ("changes", "examples"),
    ] {
        for item in f[outer].as_array().unwrap() {
            out.extend(item[inner].as_array().unwrap().iter().cloned());
        }
    }
    out
}

#[test]
fn every_pv_replays_legally() {
    for name in FIXTURES {
        let f = load_fixture(name);
        let exemplars = all_exemplars(&f);
        assert!(!exemplars.is_empty());
        for e in exemplars {
            let fen = e["fen"].as_str().unwrap();
            let id = e["gameId"].as_str().unwrap();
            let start = Position::from_fen(fen).unwrap();
            let mut best = start.clone();
            for u in e["bestPv"].as_array().unwrap() {
                best.push_uci(u.as_str().unwrap())
                    .unwrap_or_else(|err| panic!("{name} {id} bestPv {u}: {err:?}"));
            }
            let mut refd = start.clone();
            let played = refd
                .push_uci(e["played"]["uci"].as_str().unwrap())
                .unwrap_or_else(|err| panic!("{name} {id} played: {err:?}"));
            assert_eq!(played.san, e["played"]["san"].as_str().unwrap(), "{id}");
            for u in e["refutationPv"].as_array().unwrap() {
                refd.push_uci(u.as_str().unwrap())
                    .unwrap_or_else(|err| panic!("{name} {id} refutation {u}: {err:?}"));
            }
            if !e["best"].is_null() {
                let san = e["best"]["san"].as_str().unwrap();
                let mut p = start.clone();
                assert_eq!(p.play_san(san).unwrap().san, san, "{id} best");
            }
        }
    }
}

// ---- prompt-size bounds ------------------------------------------------------------------

#[test]
fn prompt_size_bounds_reject_with_a_named_issue() {
    type Mutate = fn(&mut Value);
    let cases: Vec<(&str, Mutate, &str)> = vec![
        (
            "oversize PV",
            |d| d["weaknesses"][0]["exemplars"][0]["bestPv"] = json!(vec!["e2e4"; 17]),
            "weaknesses.0.exemplars.0.bestPv",
        ),
        (
            "too many tags",
            |d| d["weaknesses"][0]["exemplars"][0]["tags"] = json!(vec!["t"; 13]),
            "weaknesses.0.exemplars.0.tags",
        ),
        (
            "oversize tag",
            |d| d["weaknesses"][0]["exemplars"][0]["tags"] = json!(["x".repeat(41)]),
            "weaknesses.0.exemplars.0.tags.0",
        ),
        (
            "too many metrics",
            |d| d["metrics"] = Value::Array(vec![d["metrics"][0].clone(); 81]),
            "metrics",
        ),
        (
            "too many weaknesses",
            |d| d["weaknesses"] = Value::Array(vec![d["weaknesses"][0].clone(); 25]),
            "weaknesses",
        ),
        (
            "too many openings",
            |d| d["openings"] = Value::Array(vec![d["openings"][0].clone(); 31]),
            "openings",
        ),
        (
            "too many changes",
            |d| d["changes"] = Value::Array(vec![d["changes"][0].clone(); 21]),
            "changes",
        ),
        (
            "too many alternatives",
            |d| {
                d["openings"][1]["alternatives"] =
                    Value::Array(vec![d["openings"][1]["alternatives"][0].clone(); 6])
            },
            "openings.1.alternatives",
        ),
        (
            "too many causes",
            |d| {
                d["changes"][0]["candidateCauses"] =
                    Value::Array(vec![d["changes"][0]["candidateCauses"][0].clone(); 7])
            },
            "changes.0.candidateCauses",
        ),
        (
            "too many moves",
            |d| d["openings"][0]["movesSan"] = json!(vec!["e4"; 41]),
            "openings.0.movesSan",
        ),
        (
            "oversize label",
            |d| d["weaknesses"][0]["label"] = json!("x".repeat(121)),
            "weaknesses.0.label",
        ),
        (
            "oversize description",
            |d| d["changes"][0]["candidateCauses"][0]["description"] = json!("x".repeat(601)),
            "changes.0.candidateCauses.0.description",
        ),
        (
            "oversize rank",
            |d| d["weaknesses"][0]["rank"] = json!(25),
            "weaknesses.0.rank",
        ),
        (
            "oversize priority",
            |d| d["weaknesses"][0]["priority"] = json!(1e9),
            "weaknesses.0.priority",
        ),
    ];
    for (name, mutate, path) in cases {
        let mut d = raw();
        mutate(&mut d);
        let p = paths(&d);
        assert!(p.contains(&path.to_string()), "{name}: {path} not in {p:?}");
    }
}

// ---- documented ranges and cross-field rules ----------------------------------------------

#[test]
fn rejects_a_per_game_value_declared_as_a_rate() {
    let mut d = raw();
    d["metrics"][0]["unit"] = json!("rate");
    d["metrics"][0]["value"] = json!(3.1);
    assert!(has(&d, "metrics.0.value"));
}

#[test]
fn rejects_old_unit_names() {
    let mut d = raw();
    d["metrics"][0]["unit"] = json!("percent");
    assert!(!ok(&d));
}

#[test]
fn rejects_pct_above_100_and_interval_fields_out_of_0_1() {
    let mut d = raw();
    d["metrics"][3]["value"] = json!(101);
    d["weaknesses"][0]["intervalHigh"] = json!(1.5);
    assert!(has(&d, "metrics.3.value"));
    assert!(has(&d, "weaknesses.0.intervalHigh"));
}

#[test]
fn requires_metric_low_le_high() {
    let mut d = raw();
    d["metrics"][0]["low"] = json!(2);
    d["metrics"][0]["high"] = json!(1);
    assert!(has(&d, "metrics.0.low"));
}

#[test]
fn requires_interval_low_le_posterior_le_interval_high() {
    let mut d = raw();
    d["weaknesses"][0]["posteriorRate"] = json!(0.5);
    assert!(has(&d, "weaknesses.0.posteriorRate"));
    let mut e = raw();
    e["weaknesses"][0]["intervalLow"] = json!(0.3);
    assert!(has(&e, "weaknesses.0.intervalLow"));
}

#[test]
fn requires_unique_weakness_ranks() {
    let mut d = raw();
    d["weaknesses"][1]["rank"] = d["weaknesses"][0]["rank"].clone();
    assert!(has(&d, "weaknesses.1.rank"));
}

#[test]
fn requires_fen_side_to_move_to_match_ply_parity() {
    let mut d = raw();
    let ply = d["weaknesses"][0]["exemplars"][0]["ply"].as_u64().unwrap();
    d["weaknesses"][0]["exemplars"][0]["ply"] = json!(ply + 1);
    assert!(has(&d, "weaknesses.0.exemplars.0.fen"));
    let mut c = raw();
    let ply = c["changes"][0]["examples"][0]["ply"].as_u64().unwrap();
    c["changes"][0]["examples"][0]["ply"] = json!(ply + 1);
    assert!(has(&c, "changes.0.examples.0.fen"));
}

#[test]
fn requires_events_le_opportunities_onset_ordering_analysed_le_imported() {
    let mut d = raw();
    let opp = d["weaknesses"][0]["opportunities"].as_u64().unwrap();
    d["weaknesses"][0]["events"] = json!(opp + 1);
    d["changes"][0]["onsetFrom"] = json!("2026-12-01");
    let imported = d["basis"]["importedGames"].as_u64().unwrap();
    d["basis"]["analysedGames"] = json!(imported + 1);
    assert!(has(&d, "weaknesses.0.events"));
    assert!(has(&d, "changes.0.onsetDate"));
    assert!(has(&d, "basis.analysedGames"));
}

// ---- strictness at depth and change examples ----------------------------------------------

#[test]
fn rejects_nested_unknown_keys_and_names_them() {
    type Mutate = fn(&mut Value);
    let mutations: [Mutate; 6] = [
        |d| d["basis"]["extra"] = json!(1),
        |d| d["weaknesses"][0]["exemplars"][0]["played"]["extra"] = json!(1),
        |d| d["weaknesses"][0]["exemplars"][0]["evalBefore"]["extra"] = json!(1),
        |d| d["changes"][0]["candidateCauses"][0]["extra"] = json!(1),
        |d| d["openings"][1]["alternatives"][0]["extra"] = json!(1),
        |d| d["changes"][0]["examples"][0]["extra"] = json!(1),
    ];
    for (i, mutate) in mutations.iter().enumerate() {
        let mut d = raw();
        mutate(&mut d);
        let is = issues(&d);
        assert!(!is.is_empty(), "case {i}: accepted");
        assert!(
            is.iter().any(|x| x.message.contains("extra")),
            "case {i}: {is:?}"
        );
    }
}

#[test]
fn requires_a_valid_side_on_change_examples_and_keeps_2_before_2_after() {
    let b2 = raw();
    let sides: Vec<&str> = b2["changes"][0]["examples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["side"].as_str().unwrap())
        .collect();
    assert_eq!(sides, ["before", "before", "after", "after"]);
    let mut d = raw();
    d["changes"][0]["examples"][0]["side"] = json!("during");
    assert!(has(&d, "changes.0.examples.0.side"));
    let mut m = raw();
    m["changes"][0]["examples"][0]
        .as_object_mut()
        .unwrap()
        .remove("side");
    assert!(!ok(&m));
}

// ---- fixture consistency ------------------------------------------------------------------

fn b2_weakness_exemplars() -> Vec<Value> {
    let f = raw();
    f["weaknesses"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|w| w["exemplars"].as_array().unwrap().clone())
        .collect()
}

#[test]
fn uses_distinct_positions_across_weaknesses() {
    let fens: Vec<String> = b2_weakness_exemplars()
        .iter()
        .map(|e| e["fen"].as_str().unwrap().to_string())
        .collect();
    let set: std::collections::HashSet<_> = fens.iter().collect();
    assert_eq!(set.len(), fens.len());
}

#[test]
fn derives_win_percentages_from_evals_for_the_mover() {
    let win = |cp: f64| 50.0 + 50.0 * (2.0 / (1.0 + (-0.00368208 * cp).exp()) - 1.0);
    for e in b2_weakness_exemplars() {
        let white = e["ply"].as_u64().unwrap() % 2 == 1;
        let cb = e["evalBefore"]["cp"].as_f64().unwrap();
        let ca = e["evalAfter"]["cp"].as_f64().unwrap();
        let sign = if white { 1.0 } else { -1.0 };
        let (wb, wa) = (
            e["winBefore"].as_f64().unwrap(),
            e["winAfter"].as_f64().unwrap(),
        );
        // toBeCloseTo(x, 0): |diff| < 0.5.
        assert!((wb - win(sign * cb)).abs() < 0.5, "{e}");
        assert!((wa - win(sign * ca)).abs() < 0.5, "{e}");
        assert!(wa < wb);
    }
}

#[test]
fn b0_and_b2_have_different_area_scores_and_metrics() {
    let b2 = raw();
    let b0 = load_fixture("b0-blitz");
    let scores = |f: &Value| -> Vec<Value> {
        f["areaScores"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["score"].clone())
            .collect()
    };
    assert_ne!(scores(&b0), scores(&b2));
    assert_ne!(b0["metrics"][0]["value"], b2["metrics"][0]["value"]);
}
