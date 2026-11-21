//! Ported from apps/worker/test/engine/uci-engine.test.ts.

use std::time::Duration;

use chess_core::EngineScore;
use engine::uci::{StartOptions, UciEngine, validate_fen};
use engine::{AnalyseOptions, AnalysisLine, EngineError};

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

fn opts(nodes: u64, multipv: u32) -> AnalyseOptions {
    AnalyseOptions {
        nodes,
        multipv,
        timeout: Duration::from_secs(30),
    }
}

fn start_opts(path: std::path::PathBuf) -> StartOptions {
    StartOptions {
        path,
        threads: None,
        hash_mb: None,
        syzygy_path: None,
    }
}

#[tokio::test]
async fn analyses_the_start_position() {
    let Some(sf) = testkit::stockfish() else {
        return;
    };
    let mut e = UciEngine::start(start_opts(sf)).await.unwrap();
    let r = e.analyse(START, opts(50_000, 1)).await.unwrap();
    let first = &r.lines[0];
    match first.score {
        EngineScore::Cp(cp) => assert!(cp.abs() <= 100),
        EngineScore::Mate(_) => panic!("unexpected mate"),
    }
    let mv = &first.pv[0];
    assert!(mv.len() >= 4 && mv.len() <= 5);
    e.quit().await;
}

#[tokio::test]
async fn finds_mate_in_one() {
    let Some(sf) = testkit::stockfish() else {
        return;
    };
    let mut e = UciEngine::start(start_opts(sf)).await.unwrap();
    let r = e
        .analyse("6k1/5ppp/8/8/8/8/5PPP/3R2K1 w - - 0 1", opts(50_000, 1))
        .await
        .unwrap();
    assert_eq!(r.lines[0].score, EngineScore::Mate(1));
    assert_eq!(r.lines[0].pv[0], "d1d8");
    e.quit().await;
}

#[tokio::test]
async fn returns_multipv_lines() {
    let Some(sf) = testkit::stockfish() else {
        return;
    };
    let mut e = UciEngine::start(start_opts(sf)).await.unwrap();
    let r = e.analyse(START, opts(50_000, 3)).await.unwrap();
    let ids: Vec<u32> = r.lines.iter().map(|l| l.multipv).collect();
    assert_eq!(ids, vec![1, 2, 3]);
    e.quit().await;
}

#[tokio::test]
async fn rejects_invalid_fen_without_killing_engine() {
    let Some(sf) = testkit::stockfish() else {
        return;
    };
    let mut e = UciEngine::start(start_opts(sf)).await.unwrap();
    let err = e.analyse("not a fen", opts(1000, 1)).await.unwrap_err();
    assert!(matches!(err, EngineError::InvalidFen { .. }));
    assert!(e.is_alive());
    let r = e.analyse(START, opts(10_000, 1)).await.unwrap();
    assert_eq!(r.lines.len(), 1);
    e.quit().await;
}

#[tokio::test]
async fn returns_a_synthetic_mate_0_line_for_checkmate() {
    let Some(sf) = testkit::stockfish() else {
        return;
    };
    let mut e = UciEngine::start(start_opts(sf)).await.unwrap();
    let fen = "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3";
    let r = e.analyse(fen, opts(1000, 1)).await.unwrap();
    assert_eq!(
        r.lines,
        vec![AnalysisLine {
            multipv: 1,
            score: EngineScore::Mate(0),
            pv: vec![],
            depth: 0,
            nodes: 0
        }]
    );
    e.quit().await;
}

#[tokio::test]
async fn returns_a_synthetic_cp_0_line_for_stalemate() {
    let Some(sf) = testkit::stockfish() else {
        return;
    };
    let mut e = UciEngine::start(start_opts(sf)).await.unwrap();
    let r = e
        .analyse("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1", opts(1000, 1))
        .await
        .unwrap();
    assert_eq!(
        r.lines,
        vec![AnalysisLine {
            multipv: 1,
            score: EngineScore::Cp(0),
            pv: vec![],
            depth: 0,
            nodes: 0
        }]
    );
    e.quit().await;
}

#[tokio::test]
async fn times_out_kills_the_process_and_rejects() {
    let Some(sf) = testkit::stockfish() else {
        return;
    };
    let mut e = UciEngine::start(start_opts(sf)).await.unwrap();
    let err = e
        .analyse(
            START,
            AnalyseOptions {
                nodes: 5_000_000_000,
                multipv: 1,
                timeout: Duration::from_millis(200),
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, EngineError::Timeout(_)));
    assert!(err.to_string().contains("timed out"));
    assert!(!e.is_alive());
    // A dead engine reports a crash rather than hanging.
    let again = e.analyse(START, opts(1000, 1)).await.unwrap_err();
    assert!(matches!(again, EngineError::Crashed));
}

#[tokio::test]
async fn cpu_time_is_reported_on_linux() {
    let Some(sf) = testkit::stockfish() else {
        return;
    };
    let mut e = UciEngine::start(start_opts(sf)).await.unwrap();
    let r = e.analyse(START, opts(100_000, 1)).await.unwrap();
    assert!(r.cpu.is_some());
    assert!(r.wall > Duration::ZERO);
    e.quit().await;
}

#[tokio::test]
async fn killed_engine_is_detected_as_crashed() {
    let Some(sf) = testkit::stockfish() else {
        return;
    };
    let mut e = UciEngine::start(start_opts(sf)).await.unwrap();
    assert!(e.pid().is_some());
    e.kill();
    assert!(!e.is_alive());
    let err = e.analyse(START, opts(1000, 1)).await.unwrap_err();
    assert!(matches!(err, EngineError::Crashed));
}

#[test]
fn validate_fen_accepts_start_and_rejects_garbage() {
    assert!(validate_fen(START).is_ok());
    assert!(validate_fen("not a fen").is_err());
    assert!(validate_fen("").is_err());
}
