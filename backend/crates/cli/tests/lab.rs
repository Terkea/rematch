//! The lab bridge (ADR 0106): `cli lab profile` and `cli lab rank` on the committed fixture give
//! the committed golden files byte for byte, every message validates against
//! `lab/schemas/lab-v1.schema.json`, and `cli lab analyse` gives the same facts twice and facts
//! whose profile is the one the production build stored.
//!
//! `REPLAY_UPDATE=1` rewrites the golden files (after a deliberate algorithm change, ADR 0104).

use std::path::PathBuf;
use std::sync::Arc;

use bench::generate::generate_game;
use bench::lab::{AnalyseOptions, AnalysePlayer, AnalyseRequest, LabGame, SCHEMA_VERSION, analyse};
use bench::memo::MemoAnalyser;
use bench::suite::{PlayerDef, game_specs};
use cli::lab_cmd::{algorithm_id, run_pure, to_output};
use engine::Analyser;
use engine::pool::{EnginePool, PoolOptions};
use serde_json::{Value, json};

/// Golden files store this in place of the id, so a manifest bump alone does not move them.
const ID_PLACEHOLDER: &str = "ALGORITHM_ID";

fn lab_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../lab")
}

fn read(rel: &str) -> String {
    let p = lab_dir().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn with_placeholder(out: &str) -> String {
    out.replace(
        &format!("\"algorithm_id\": \"{}\"", algorithm_id()),
        &format!("\"algorithm_id\": \"{ID_PLACEHOLDER}\""),
    )
}

/// Compares `out` with the golden file `rel`, or rewrites it with `REPLAY_UPDATE=1`.
fn golden(rel: &str, out: &str) {
    let out = with_placeholder(out);
    let path = lab_dir().join(rel);
    if std::env::var("REPLAY_UPDATE").as_deref() == Ok("1") {
        std::fs::write(&path, &out).expect("write golden");
        return;
    }
    let want = read(rel);
    assert!(
        out == want,
        "{rel} differs from the bridge's output; if the algorithm changed on purpose, rerun with \
         REPLAY_UPDATE=1 and bump the algorithm version.\n--- got ---\n{out}"
    );
}

fn validator(message: &str) -> jsonschema::Validator {
    let mut schema: Value =
        serde_json::from_str(&read("schemas/lab-v1.schema.json")).expect("schema JSON");
    schema["$ref"] = json!(format!("#/$defs/{message}"));
    jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect("the lab schema compiles")
}

fn assert_valid(message: &str, doc: &str) {
    let v: Value = serde_json::from_str(doc).expect("one JSON document");
    let errors: Vec<String> = validator(message)
        .iter_errors(&v)
        .map(|e| format!("{e} at {}", e.instance_path()))
        .collect();
    assert!(
        errors.is_empty(),
        "{message} rejects the document: {errors:?}"
    );
}

#[test]
fn profile_and_rank_match_the_golden_files() {
    let request = read("fixtures/profile-request.json");
    assert_valid("ProfileRequest", &request);
    let profile = run_pure("profile", &request).expect("profile");
    assert_valid("ProfileResponse", &profile);
    golden("fixtures/golden/profile-response.json", &profile);

    // A profile response is a rank request.
    assert_valid("RankRequest", &profile);
    let rank = run_pure("rank", &profile).expect("rank");
    assert_valid("RankResponse", &rank);
    golden("fixtures/golden/rank-response.json", &rank);

    let doc: Value = serde_json::from_str(&rank).unwrap();
    assert_eq!(doc["schema_version"], json!(SCHEMA_VERSION));
    assert_eq!(doc["algorithm_id"], json!(algorithm_id()));
    // The fixture's design (lab/fixtures/make-profile-request.mjs): the hanging player's W1 is
    // ranked and is the focus; the clean player has nothing ranked and the general focus.
    assert_eq!(doc["players"][0]["ranked"][0]["id"], json!("W1"));
    assert_eq!(doc["players"][0]["focuses"], json!(["W1"]));
    assert_eq!(doc["players"][1]["ranked"], json!([]));
    assert_eq!(doc["players"][1]["focuses"], json!(["general"]));
    assert_eq!(
        doc["players"][1]["focus_drills"]["general"],
        json!(["own_game_analysis"])
    );
}

#[test]
fn the_same_request_gives_the_same_bytes() {
    let request = read("fixtures/profile-request.json");
    let a = run_pure("profile", &request).unwrap();
    let b = run_pure("profile", &request).unwrap();
    assert_eq!(a, b);
    assert_eq!(run_pure("rank", &a).unwrap(), run_pure("rank", &b).unwrap());
}

#[test]
fn bad_requests_are_refused_with_a_reason() {
    let err = run_pure("profile", r#"{"schema_version":"lab-v0","players":[]}"#).unwrap_err();
    assert!(err.to_string().contains("lab-v0"), "{err}");
    let err = run_pure(
        "profile",
        r#"{"schema_version":"lab-v1","players":[{"id":"x","now":"2026-11-01T00:00:00Z","band":"B2","games_in_window":30,"series":{"W9d":[]}}]}"#,
    )
    .unwrap_err();
    assert!(err.to_string().contains("not a binomial"), "{err}");
    let err = run_pure(
        "profile",
        r#"{"schema_version":"lab-v1","players":[{"id":"x","now":"2026-11-01T00:00:00Z","band":"B2","games_in_window":30,"serie":{}}]}"#,
    )
    .unwrap_err();
    assert!(err.to_string().contains("serie"), "{err}");
    assert!(run_pure("bench", "{}").is_err());
}

#[test]
fn the_schema_is_not_vacuous() {
    let v = validator("RankResponse");
    assert!(!v.is_valid(&json!({})));
    assert!(!v.is_valid(&json!({"schema_version": "lab-v1", "algorithm_id": "v1", "players": []})));
    assert!(v.is_valid(
        &json!({"schema_version": "lab-v1", "algorithm_id": "algo-1.1.0+0123456789ab", "players": []})
    ));
}

/// `tag-motifs` (EXP-0007): the fixture gives the golden file, and each line's `best` is exactly
/// what the production tagger gives for that position and line (`tag_line` at
/// `LINE_MAX_PLIES`), so the bridge adds nothing of its own.
#[test]
fn tag_motifs_is_the_production_tagger() {
    use chess_core::Color;
    use profile_core::features::derive::LINE_MAX_PLIES;
    use profile_core::motifs::tag_line;

    let request = read("fixtures/tag-motifs-request.json");
    assert_valid("TagMotifsRequest", &request);
    let out = run_pure("tag-motifs", &request).expect("tag-motifs");
    assert_valid("TagMotifsResponse", &out);
    golden("fixtures/golden/tag-motifs-response.json", &out);
    assert_eq!(run_pure("tag-motifs", &request).unwrap(), out);

    let doc: Value = serde_json::from_str(&out).unwrap();
    let lines = doc["lines"].as_array().expect("lines");
    let by_id = |id: &str| {
        lines
            .iter()
            .find(|l| l["id"] == json!(id))
            .unwrap_or_else(|| panic!("no line {id}"))
    };
    // The puzzle's first move is the opponent's: the tagged position is after it, White to move.
    let puzzle = by_id("puzzle-00008");
    assert_eq!(puzzle["pov"], json!("white"));
    let fen = puzzle["fen"].as_str().unwrap();
    let pv: Vec<String> = ["e6e7", "b2b1", "b3c1", "b1c1", "h6c1"]
        .map(String::from)
        .to_vec();
    let direct = tag_line(fen, &pv, Color::White, LINE_MAX_PLIES);
    assert_eq!(puzzle["best"], serde_json::to_value(&direct).unwrap());

    let tags = |id: &str| by_id(id)["best"]["tags"].clone();
    assert!(tags("fork").as_array().unwrap().contains(&json!("fork")));
    assert!(
        tags("smothered")
            .as_array()
            .unwrap()
            .contains(&json!("smotheredMate"))
    );
    assert!(
        tags("back-rank")
            .as_array()
            .unwrap()
            .contains(&json!("backRankMate"))
    );
    // No line, no analysis, as production stores none.
    for id in ["illegal-line", "empty-line"] {
        assert_eq!(by_id(id)["best"], Value::Null, "{id}");
        assert_eq!(by_id(id)["pov"], json!("white"), "{id}");
    }
    for id in ["illegal-setup", "bad-fen"] {
        assert_eq!(by_id(id)["fen"], Value::Null, "{id}");
        assert_eq!(by_id(id)["best"], Value::Null, "{id}");
    }

    let err = run_pure("tag-motifs", r#"{"schema_version":"lab-v0","lines":[]}"#).unwrap_err();
    assert!(err.to_string().contains("lab-v0"), "{err}");
    let err = run_pure(
        "tag-motifs",
        r#"{"schema_version":"lab-v1","lines":[{"id":"x","fen":"8/8/8/8/8/8/8/8 w - - 0 1","pv":[],"moves":[]}]}"#,
    )
    .unwrap_err();
    assert!(err.to_string().contains("moves"), "{err}");
}

fn lab_game(g: &bench::generate::GeneratedGame) -> LabGame {
    LabGame {
        id: g.platform_game_id.clone(),
        pgn: g.pgn.clone(),
        played_at: g.played_at,
        player_color: g.player_color.clone(),
        white_name: g.white_name.clone(),
        black_name: g.black_name.clone(),
        white_rating: g.white_rating,
        black_rating: g.black_rating,
        result: g.result.clone(),
        termination: g.termination.clone(),
        base_seconds: bench::generate::BASE_SECONDS as i32,
        increment_seconds: 0,
    }
}

/// `analyse` twice on the same games: the same facts (byte for byte), and the profile of those
/// facts equals the estimates the production build stored.
#[tokio::test]
async fn analyse_is_deterministic_and_its_facts_reproduce_the_build() {
    let Some(stockfish) = testkit::stockfish() else {
        return;
    };
    let Some(database_url) = std::env::var("DATABASE_URL").ok().filter(|u| !u.is_empty()) else {
        assert!(
            !testkit::in_docker(),
            "DATABASE_URL must be set when IN_DOCKER=1"
        );
        return;
    };
    const NODES: u64 = 1_500;
    let engines = EnginePool::create(PoolOptions {
        size: 2,
        path: stockfish.clone(),
        hash_mb: Some(16),
    })
    .await
    .expect("engine pool");
    let inner: Arc<dyn Analyser> = engines.clone();
    let memo = MemoAnalyser::new(inner, NODES);
    let def = PlayerDef {
        name: "lab-t".to_owned(),
        flaw: None,
        games: 3,
        seed: 11,
    };
    let mut games = Vec::new();
    for spec in game_specs(&def, 3).iter().take(3) {
        let (g, _) = generate_game(&memo, spec, None).await.expect("game");
        games.push(lab_game(&g));
    }
    EnginePool::close(&engines).await;

    let req = AnalyseRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        nodes: Some(NODES),
        engines: Some(2),
        players: vec![AnalysePlayer {
            id: "p1".to_owned(),
            now: None,
            games,
        }],
    };
    let request_json = serde_json::to_string(&req).unwrap();
    assert_valid("AnalyseRequest", &request_json);
    let opts = AnalyseOptions {
        database_url,
        stockfish,
    };
    let run = || async {
        let r = analyse(&req, &opts, algorithm_id(), Box::new(|_| {}))
            .await
            .expect("analyse");
        to_output(&r).unwrap()
    };
    let a = run().await;
    let b = run().await;
    assert_eq!(a, b, "the same games gave different facts");
    assert_valid("AnalyseResponse", &a);
    let doc: Value = serde_json::from_str(&a).unwrap();
    assert_eq!(doc["checks"][0]["analysed_games"], json!(3));
    assert_eq!(
        doc["checks"][0]["profile_matches_build"],
        json!(true),
        "profile(facts) differs from the build: {}",
        doc["checks"][0]["mismatches"]
    );
    // The facts are a profile request.
    assert_valid("ProfileRequest", &a);
    let profile = run_pure("profile", &a).expect("profile of analyse");
    assert_valid("ProfileResponse", &profile);
}

#[tokio::test]
async fn an_unknown_algo_variant_is_refused_before_anything_runs() {
    let errors = std::sync::Mutex::new(Vec::<String>::new());
    let argv = ["lab".to_owned(), "profile".to_owned()];
    let code = cli::lab_cmd::run_lab(
        &argv,
        &|k| (k == "ALGO_VARIANT").then(|| "lighter".to_owned()),
        &|_| {},
        &|e| errors.lock().unwrap().push(e.to_owned()),
    )
    .await;
    assert_eq!(code, 1);
    assert!(errors.lock().unwrap()[0].starts_with("ALGO_VARIANT: unknown algorithm variant"));
}
