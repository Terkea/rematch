//! `cli lab positions|evaluate|judge` (EXP-0008): the positions are the ones `evaluate_game`
//! asks the engine for, `judge` gives exactly the move records and accuracy of the production
//! functions on the same evaluations, Lichess evaluations go in where production puts them, and
//! `evaluate` returns what the production engine pool returns for the same search.

use std::path::PathBuf;

use bench::depth::{
    DepthGame, EvaluateOptions, EvaluateRequest, GivenSearch, JudgeRequest, PositionsRequest,
    evaluate, judge, positions,
};
use bench::lab::SCHEMA_VERSION;
use chess_core::game_accuracy::game_accuracy;
use chess_core::move_record::build_move_records;
use chess_core::pgn::parse_pgn;
use chess_core::score::{flip_score, win_percent};
use chess_core::{Color, EngineScore, PositionEval};
use cli::lab_cmd::{algorithm_id, run_depth, to_output};
use engine::pool::{EnginePool, PoolOptions};
use engine::{AnalyseOptions, DEFAULT_ANALYSIS_TIMEOUT};
use serde_json::{Value, json};

/// Scholar's mate: ends in checkmate, so the final position is not searched.
const MATE: &str =
    "[Event \"t\"]\n[Result \"1-0\"]\n\n1. e4 e5 2. Bc4 Nc6 3. Qh5 Nf6 4. Qxf7# 1-0\n";
/// A short game that does not end on the board; the test scores 3...Nd4 as a blunder.
const OPEN: &str = "[Event \"t\"]\n[Result \"*\"]\n\n1. e4 e5 2. Nf3 Nc6 3. Bc4 Nd4 *\n";

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

fn game(id: &str, pgn: &str) -> DepthGame {
    DepthGame {
        id: id.to_owned(),
        pgn: pgn.to_owned(),
        base_seconds: None,
        increment_seconds: None,
        lichess_evals: None,
    }
}

#[tokio::test]
async fn positions_are_every_searched_position_once_in_order() {
    let req = PositionsRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        games: vec![game("mate", MATE), game("open", OPEN)],
    };
    let out = positions(&req, algorithm_id()).await.expect("positions");
    let mate = &out.games[0];
    assert_eq!((mate.plies, mate.terminal), (7, true));
    // The start and the position after each ply but the mating one.
    assert_eq!(mate.fens.len(), 7);
    let parsed = parse_pgn(MATE).unwrap();
    assert_eq!(mate.fens[0], parsed.start_fen);
    assert_eq!(mate.fens[6], parsed.plies[5].fen_after);
    let open = &out.games[1];
    assert_eq!((open.plies, open.terminal), (6, false));
    assert_eq!(open.fens.len(), 7);
    let text = to_output(&out).unwrap();
    schema_check("PositionsResponse", &text);
    schema_check("PositionsRequest", &serde_json::to_string(&req).unwrap());
}

/// Scores from the side to move's point of view, as an engine reports them, that make 3...Nd4 a
/// blunder for Black: the position after it is +4 for White.
fn searches(fens: &[String]) -> Vec<GivenSearch> {
    fens.iter()
        .enumerate()
        .map(|(i, fen)| GivenSearch {
            fen: fen.clone(),
            // After ply 6 (index 6) White is to move and +400; otherwise level.
            score: EngineScore::Cp(if i == 6 { 400 } else { 10 }),
            best_uci: Some("e2e4".to_owned()),
        })
        .collect()
}

#[tokio::test]
async fn judge_is_the_production_move_records_and_accuracy() {
    let games = vec![game("open", OPEN), game("mate", MATE)];
    let preq = PositionsRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        games: games.clone(),
    };
    let asked = positions(&preq, algorithm_id()).await.unwrap();
    let all: Vec<String> = asked.games.iter().flat_map(|g| g.fens.clone()).collect();
    let req = JudgeRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        games,
        searches: searches(&all),
    };
    let out = judge(&req, algorithm_id()).await.expect("judge");
    let text = to_output(&out).unwrap();
    schema_check("JudgeRequest", &serde_json::to_string(&req).unwrap());
    schema_check("JudgeResponse", &text);
    assert_eq!(
        run_depth("judge", &serde_json::to_string(&req).unwrap())
            .await
            .unwrap(),
        text
    );

    // The same evaluations straight through the production functions.
    let parsed = parse_pgn(OPEN).unwrap();
    let by_fen: std::collections::HashMap<_, _> = req
        .searches
        .iter()
        .map(|s| (s.fen.clone(), s.score))
        .collect();
    let fens: Vec<String> = std::iter::once(parsed.start_fen.clone())
        .chain(parsed.plies.iter().map(|p| p.fen_after.clone()))
        .collect();
    let evals: Vec<PositionEval> = fens
        .iter()
        .map(|f| {
            let s = by_fen[f];
            PositionEval {
                score: if f.split(' ').nth(1) == Some("b") {
                    flip_score(s)
                } else {
                    s
                },
                best_uci: Some("e2e4".to_owned()),
                pv: vec!["e2e4".to_owned()],
            }
        })
        .collect();
    let records = build_move_records(&parsed, &evals, None).unwrap();
    let judged = &out.games[0];
    assert_eq!(judged.moves.len(), records.len());
    for (m, r) in judged.moves.iter().zip(&records) {
        assert_eq!(m.judgement, r.judgement.map(|j| j.as_str().to_owned()));
        assert_eq!(
            (m.win_before, m.win_after, m.accuracy),
            (r.win_before, r.win_after, r.accuracy)
        );
        assert_eq!(m.ply, r.ply);
    }
    assert_eq!(judged.moves[5].judgement.as_deref(), Some("blunder"));
    let wins: Vec<f64> = evals.iter().map(|e| win_percent(e.score)).collect();
    assert_eq!(judged.accuracy_white, game_accuracy(&wins, Color::White));
    assert_eq!(judged.accuracy_black, game_accuracy(&wins, Color::Black));
    // The mating move is scored as a delivered mate, with no search of the final position.
    assert_eq!(out.games[1].moves.len(), 7);
}

#[tokio::test]
async fn lichess_evaluations_go_in_before_the_engine() {
    let mut g = game("open", OPEN);
    g.lichess_evals = Some(vec![Some(EngineScore::Cp(20)); 6]);
    let parsed = parse_pgn(OPEN).unwrap();
    // Only the start position has no Lichess evaluation, as in production.
    let req = JudgeRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        games: vec![g],
        searches: vec![GivenSearch {
            fen: parsed.start_fen.clone(),
            score: EngineScore::Cp(20),
            best_uci: None,
        }],
    };
    let out = judge(&req, algorithm_id()).await.expect("judge");
    assert_eq!(out.games[0].lichess_used, 6);
    assert!(out.games[0].moves.iter().all(|m| m.judgement.is_none()));
    // A position with neither is an error, never a guess.
    let mut bare = req.clone();
    bare.searches.clear();
    assert!(judge(&bare, algorithm_id()).await.is_err());
}

/// `evaluate` at N nodes is the production pool's search at N nodes, and the same twice.
#[tokio::test]
async fn evaluate_is_the_production_search() {
    let Some(stockfish) = testkit::stockfish() else {
        return;
    };
    const NODES: u64 = 20_000;
    let fens = vec![
        "r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R b KQkq - 3 3".to_owned(),
        "8/8/4k3/8/2K5/8/3P4/8 w - - 0 1".to_owned(),
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".to_owned(),
    ];
    let req = EvaluateRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        nodes: NODES,
        hash_mb: None,
        syzygy: false,
        engines: Some(2),
        fens: fens.clone(),
    };
    let opts = EvaluateOptions {
        stockfish: stockfish.clone(),
        syzygy_path: None,
    };
    let a = evaluate(&req, &opts, algorithm_id())
        .await
        .expect("evaluate");
    let b = evaluate(&req, &opts, algorithm_id())
        .await
        .expect("evaluate");
    let strip = |r: &bench::depth::EvaluateResponse| {
        r.searches
            .iter()
            .map(|s| (s.fen.clone(), s.score, s.best_uci.clone(), s.depth, s.nodes))
            .collect::<Vec<_>>()
    };
    assert_eq!(strip(&a), strip(&b));
    assert_eq!(a.hash_mb, 16);
    schema_check("EvaluateResponse", &to_output(&a).unwrap());
    schema_check("EvaluateRequest", &serde_json::to_string(&req).unwrap());

    let pool = EnginePool::create(PoolOptions {
        size: 1,
        path: stockfish,
        hash_mb: None,
    })
    .await
    .expect("pool");
    for (s, fen) in a.searches.iter().zip(&fens) {
        assert_eq!(&s.fen, fen);
        let r = pool
            .analyse(
                fen,
                AnalyseOptions {
                    nodes: NODES,
                    multipv: 1,
                    timeout: DEFAULT_ANALYSIS_TIMEOUT,
                },
            )
            .await
            .expect("pool search");
        let line = r.lines.into_iter().next().expect("a line");
        assert_eq!(
            (s.score, s.best_uci.clone(), s.depth, s.nodes),
            (line.score, line.pv.first().cloned(), line.depth, line.nodes),
            "{fen}"
        );
    }
    EnginePool::close(&pool).await;

    let mut bad = req.clone();
    bad.syzygy = true;
    assert!(
        evaluate(&bad, &opts, algorithm_id()).await.is_err(),
        "syzygy needs SYZYGY_PATH"
    );
}
