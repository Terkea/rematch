//! `cli lab observe` (EXP-0009): a Lichess game's weakness observations from Lichess's own
//! evaluations, through the production feature path with no enrichment. Only the keys whose
//! detectors need no enrichment input come back, a game that cannot be observed carries its error
//! without stopping the batch, and the response is the schema's.

use std::path::PathBuf;

use bench::depth::GivenSearch;
use bench::lab::SCHEMA_VERSION;
use bench::observe::{EVAL_ONLY_KEYS, ObserveGame, ObserveRequest, observe};
use chess_core::{Color, EngineScore};
use cli::lab_cmd::{algorithm_id, run_depth, to_output};
use serde_json::{Value, json};

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// Scholar's mate with clocks: Black's 3...Nf6 allows mate in one, White's 4.Qxf7# takes it.
const MATE: &str = "[Event \"t\"]\n[Result \"1-0\"]\n\n1. e4 { [%clk 0:10:00] } e5 { [%clk 0:10:00] } \
2. Bc4 { [%clk 0:09:58] } Nc6 { [%clk 0:09:57] } 3. Qh5 { [%clk 0:09:55] } Nf6 { [%clk 0:09:50] } \
4. Qxf7# { [%clk 0:09:53] } 1-0\n";

/// A short game that does not end on the board; Lichess scores 3...Nd4 as losing for Black.
const OPEN: &str = "[Event \"t\"]\n[Result \"*\"]\n\n1. e4 { [%clk 0:10:00] } e5 { [%clk 0:10:00] } \
2. Nf3 { [%clk 0:09:58] } Nc6 { [%clk 0:09:57] } 3. Bc4 { [%clk 0:09:55] } Nd4 { [%clk 0:09:50] } *\n";

fn cp(v: i32) -> Option<EngineScore> {
    Some(EngineScore::Cp(v))
}

fn game(id: &str, pgn: &str, evals: Vec<Option<EngineScore>>, sides: Vec<Color>) -> ObserveGame {
    ObserveGame {
        id: id.to_owned(),
        pgn: pgn.to_owned(),
        base_seconds: 600,
        increment_seconds: 0,
        lichess_evals: evals,
        sides,
        white_rating: Some(1500),
        black_rating: Some(1500),
        termination: Some("Normal".to_owned()),
    }
}

fn request(games: Vec<ObserveGame>) -> ObserveRequest {
    ObserveRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        games,
        searches: vec![GivenSearch {
            fen: START.to_owned(),
            score: EngineScore::Cp(20),
            best_uci: Some("e2e4".to_owned()),
        }],
    }
}

fn schema_check(message: &str, doc: &str) {
    let lab = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../lab");
    let mut schema: Value = serde_json::from_str(
        &std::fs::read_to_string(lab.join("schemas/lab-v1.schema.json")).expect("schema"),
    )
    .expect("schema JSON");
    schema["$ref"] = json!(format!("#/$defs/{message}"));
    let v = jsonschema::options().build(&schema).expect("compiles");
    let doc: Value = serde_json::from_str(doc).expect("JSON");
    let errors: Vec<String> = v.iter_errors(&doc).map(|e| e.to_string()).collect();
    assert!(errors.is_empty(), "{message}: {errors:?}");
}

#[tokio::test]
async fn a_mate_allowed_and_an_early_blunder_are_counted_from_lichess_evaluations() {
    // The final position is mate: production scores it without a search, so Lichess gives none.
    let mate_evals = vec![
        cp(20),
        cp(20),
        cp(20),
        cp(20),
        cp(0),
        Some(EngineScore::Mate(1)),
        None,
    ];
    let open_evals = vec![cp(20), cp(20), cp(20), cp(20), cp(20), cp(400)];
    let req = request(vec![
        game("mate", MATE, mate_evals, vec![Color::White, Color::Black]),
        game("open", OPEN, open_evals, vec![Color::Black]),
    ]);
    let out = observe(&req, algorithm_id()).await.expect("observe");
    let mate = &out.games[0];
    assert_eq!(mate.error, None);
    let (white, black) = (&mate.sides[0], &mate.sides[1]);
    assert_eq!((white.color, black.color), (Color::White, Color::Black));
    let allowed = &black.keys["W4.allowed"];
    assert_eq!(allowed.events, 1, "3...Nf6 allows mate in one");
    let missed = &white.keys["W4.missed"];
    assert_eq!(
        (missed.events, missed.opportunities),
        (0, 1),
        "4.Qxf7# takes it"
    );
    assert_eq!(white.moves, 4);
    let traps = &out.games[1].sides[0].keys["W7.traps"];
    assert_eq!(
        (traps.events, traps.opportunities),
        (1, 1),
        "3...Nd4 by ply 20"
    );
    assert_eq!(out.games[1].sides[0].blunders, 1);
    let names: Vec<&str> = EVAL_ONLY_KEYS.iter().map(|k| k.as_str()).collect();
    for g in &out.games {
        for s in &g.sides {
            for k in s.keys.keys() {
                assert!(names.contains(&k.as_str()), "{k} needs the enrichment");
            }
        }
    }
    schema_check("ObserveResponse", &to_output(&out).expect("json"));
}

#[tokio::test]
async fn a_game_that_cannot_be_observed_carries_its_error_and_the_batch_goes_on() {
    // No evaluation after 2...Nc6 and no search given for it.
    let gap = vec![cp(20), cp(20), cp(20), None, cp(20), cp(400)];
    let ok = vec![cp(20), cp(20), cp(20), cp(20), cp(20), cp(400)];
    let req = request(vec![
        game("gap", OPEN, gap, vec![Color::White]),
        game("ok", OPEN, ok, vec![Color::White]),
    ]);
    let input = serde_json::to_string(&req).expect("json");
    schema_check("ObserveRequest", &input);
    let out: Value =
        serde_json::from_str(&run_depth("observe", &input).await.expect("observe")).expect("JSON");
    assert!(out["games"][0]["error"].is_string());
    assert_eq!(out["games"][0]["sides"], json!([]));
    assert!(out["games"][1]["error"].is_null());
    assert_eq!(out["games"][1]["sides"][0]["color"], "white");
}

#[tokio::test]
async fn an_evaluation_list_of_the_wrong_length_is_an_error_not_a_shift() {
    let short = vec![cp(20), cp(20), cp(20), cp(20), cp(400)];
    let out = observe(
        &request(vec![game("short", OPEN, short, vec![Color::White])]),
        algorithm_id(),
    )
    .await
    .expect("observe");
    assert_eq!(
        out.games[0].error.as_deref(),
        Some("5 evaluations for 6 plies")
    );
}
