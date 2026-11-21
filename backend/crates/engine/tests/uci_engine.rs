//! Ported from apps/worker/test/engine/uci-engine.test.ts.

mod common;

use std::time::Duration;

use chess_core::EngineScore;
use engine::uci::{StartOptions, sent_fen, validate_fen};
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
        handshake_timeout: None,
    }
}

#[tokio::test]
async fn analyses_the_start_position() {
    let Some(sf) = testkit::stockfish() else {
        return;
    };
    let mut e = common::start_engine(start_opts(sf)).await.unwrap();
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
    let mut e = common::start_engine(start_opts(sf)).await.unwrap();
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
    let mut e = common::start_engine(start_opts(sf)).await.unwrap();
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
    let mut e = common::start_engine(start_opts(sf)).await.unwrap();
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
    let mut e = common::start_engine(start_opts(sf)).await.unwrap();
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
    let mut e = common::start_engine(start_opts(sf)).await.unwrap();
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
    let mut e = common::start_engine(start_opts(sf)).await.unwrap();
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
    // The killed child was reaped, not left as a zombie.
    let pid = e.pid().unwrap();
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "engine process {pid} still present"
    );
    // A dead engine reports a crash rather than hanging.
    let again = e.analyse(START, opts(1000, 1)).await.unwrap_err();
    assert!(matches!(again, EngineError::Crashed));
}

#[tokio::test]
async fn cpu_time_is_reported_on_linux() {
    let Some(sf) = testkit::stockfish() else {
        return;
    };
    let mut e = common::start_engine(start_opts(sf)).await.unwrap();
    let r = e.analyse(START, opts(1_000_000, 1)).await.unwrap();
    assert!(r.cpu.is_some_and(|c| c > Duration::ZERO), "cpu {:?}", r.cpu);
    assert!(r.wall > Duration::ZERO);
    e.quit().await;
}

#[tokio::test]
async fn killed_engine_is_detected_as_crashed() {
    let Some(sf) = testkit::stockfish() else {
        return;
    };
    let mut e = common::start_engine(start_opts(sf)).await.unwrap();
    assert!(e.pid().is_some());
    e.kill().await;
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

const ITALIAN: &str = "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3";

#[tokio::test]
async fn a_dropped_analysis_does_not_corrupt_the_next_one() {
    let Some(sf) = testkit::stockfish() else {
        return;
    };
    let mut e = common::start_engine(start_opts(sf.clone())).await.unwrap();
    let mut fresh = common::start_engine(start_opts(sf)).await.unwrap();
    let long = AnalyseOptions {
        nodes: 5_000_000_000,
        multipv: 1,
        timeout: Duration::from_secs(60),
    };
    // Drop the future only once the search is provably running: Stockfish
    // idles at ~0 CPU, so 150 ms of extra CPU time means `go` is executing.
    let pid = e.pid().unwrap();
    let base = cpu_time(pid).unwrap();
    tokio::select! {
        _ = e.analyse(START, long) => panic!("the long search finished"),
        _ = testkit::wait_for("the search to burn CPU", Duration::from_secs(20), Duration::from_millis(10), || async {
            cpu_time(pid).filter(|c| c.saturating_sub(base) >= Duration::from_millis(150))
        }) => {}
    }
    assert!(e.is_searching(), "the future was dropped outside a search");
    let o = AnalyseOptions {
        nodes: 200_000,
        multipv: 1,
        timeout: Duration::from_secs(10),
    };
    let r = e.analyse(ITALIAN, o).await.unwrap();
    let f = fresh.analyse(ITALIAN, o).await.unwrap();
    // The PV must be legal in the NEW position.
    let mut pos = chess_core::board::Position::from_fen(ITALIAN).unwrap();
    assert!(!r.lines[0].pv.is_empty());
    for mv in &r.lines[0].pv {
        pos = pos
            .play_uci(mv)
            .unwrap_or_else(|_| panic!("{mv} illegal in {ITALIAN}: pv {:?}", r.lines[0].pv));
    }
    assert_eq!(r.lines[0].score, f.lines[0].score);
    assert_eq!(r.lines[0].pv[0], f.lines[0].pv[0]);
    e.quit().await;
    fresh.quit().await;
}

#[tokio::test]
async fn an_externally_killed_engine_is_detected_by_eof() {
    let Some(sf) = testkit::stockfish() else {
        return;
    };
    let mut e = common::start_engine(start_opts(sf)).await.unwrap();
    let pid = e.pid().unwrap();
    // `sh`'s builtin kill: the image has no standalone kill binary.
    let st = std::process::Command::new("sh")
        .args(["-c", &format!("kill -9 {pid}")])
        .status()
        .unwrap();
    assert!(st.success());
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while e.is_alive() && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!e.is_alive(), "EOF did not mark the engine dead");
    let err = e.analyse(START, opts(1000, 1)).await.unwrap_err();
    assert!(matches!(err, EngineError::Crashed), "{err:?}");
    assert!(!e.is_alive());
    // The dead child was reaped, not left as a zombie.
    assert!(!proc_exists(pid), "zombie {pid} left behind");
}

#[tokio::test]
async fn a_crash_mid_search_reports_crashed_and_reaps_the_process() {
    let Some(sf) = testkit::stockfish() else {
        return;
    };
    let mut e = common::start_engine(start_opts(sf)).await.unwrap();
    let pid = e.pid().unwrap();
    let long = AnalyseOptions {
        nodes: 5_000_000_000,
        multipv: 1,
        timeout: Duration::from_secs(60),
    };
    let killer = async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        kill9(pid);
    };
    let (r, ()) = tokio::join!(e.analyse(START, long), killer);
    assert!(matches!(r, Err(EngineError::Crashed)), "{r:?}");
    assert!(!proc_exists(pid), "zombie {pid} left behind");
}

#[tokio::test]
async fn analyses_a_position_with_huge_clocks() {
    let Some(sf) = testkit::stockfish() else {
        return;
    };
    let mut e = common::start_engine(start_opts(sf)).await.unwrap();
    // Sent as `... 150 10000`: Stockfish reads the clocks as `int`.
    let fen = "4k3/8/8/8/8/8/4P3/4K3 w - - 4000000000 4000000000";
    let r = e.analyse(fen, opts(10_000, 1)).await.unwrap();
    assert_eq!(r.lines.len(), 1);
    assert!(e.is_alive());
    e.quit().await;
}

#[test]
fn sent_fen_clamps_the_clocks() {
    assert_eq!(
        sent_fen("4k3/8/8/8/8/8/4P3/4K3 w - - 151 10001").unwrap(),
        "4k3/8/8/8/8/8/4P3/4K3 w - - 150 10000"
    );
    assert_eq!(
        sent_fen("4k3/8/8/8/8/8/4P3/4K3 w - - 4000000000 4000000000").unwrap(),
        "4k3/8/8/8/8/8/4P3/4K3 w - - 150 10000"
    );
    // At or under the caps the clocks pass through.
    assert_eq!(
        sent_fen("4k3/8/8/8/8/8/4P3/4K3 w - - 150 10000").unwrap(),
        "4k3/8/8/8/8/8/4P3/4K3 w - - 150 10000"
    );
    assert_eq!(sent_fen(START).unwrap(), START);
    assert!(sent_fen("not a fen").is_err());
}

fn kill9(pid: u32) {
    let _ = std::process::Command::new("sh")
        .args(["-c", &format!("kill -9 {pid}")])
        .status();
}

/// A zombie still has a `/proc` entry; a reaped process does not.
fn proc_exists(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// utime + stime from `/proc/<pid>/stat` (100 ticks/s).
fn cpu_time(pid: u32) -> Option<Duration> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = stat.get(stat.rfind(')')? + 2..)?;
    let f: Vec<&str> = rest.split_whitespace().collect();
    let ticks: u64 = f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?;
    Some(Duration::from_millis(ticks * 10))
}

#[tokio::test]
async fn analyses_a_position_with_halfmove_clock_over_100() {
    let Some(sf) = testkit::stockfish() else {
        return;
    };
    let mut e = common::start_engine(start_opts(sf)).await.unwrap();
    let fen = "4k3/8/8/8/8/8/4P3/4K3 w - - 150 80";
    let r = e.analyse(fen, opts(10_000, 1)).await.unwrap();
    assert_eq!(r.lines.len(), 1);
    assert!(e.is_alive());
    e.quit().await;
}

#[test]
fn fen_gate_table() {
    let bad = [
        (
            "7 ranks",
            "rnbqkbnr/pppppppp/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        ),
        (
            "9 ranks",
            "rnbqkbnr/pppppppp/8/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        ),
        (
            "44",
            "rnbqkbnr/pppppppp/44/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        ),
        (
            "17",
            "rnbqkbnr/pppppppp/17/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        ),
        (
            "80",
            "rnbqkbnr/pppppppp/80/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        ),
        (
            "double space",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR  w KQkq - 0 1",
        ),
        (
            "leading space",
            " rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        ),
        (
            "trailing space",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 ",
        ),
        (
            "tab",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR\tw KQkq - 0 1",
        ),
        (
            "newline",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1\nquit",
        ),
        (
            "non-ascii",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1\u{e9}",
        ),
        (
            "non-ascii piece",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQ\u{2654}BNR w KQkq - 0 1",
        ),
        (
            "5 fields",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0",
        ),
        (
            "7 fields",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1 x",
        ),
        (
            "no white king",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQ1BNR w kq - 0 1",
        ),
        (
            "two white kings",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBKKBNR w kq - 0 1",
        ),
        (
            "pawn on rank 8",
            "Pnbqkbnr/pppppppp/8/8/8/8/1PPPPPPP/RNBQKBNR w KQk - 0 1",
        ),
        (
            "pawn on rank 1",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPP1/RNBQKBNp w Qkq - 0 1",
        ),
        (
            "ep on wrong rank",
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e4 0 1",
        ),
        (
            "ep with no pawn",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR b KQkq e3 0 1",
        ),
        (
            "ep garbage",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq z9 0 1",
        ),
        (
            "side x",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR x KQkq - 0 1",
        ),
        (
            "negative halfmove",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - -1 1",
        ),
        (
            "move 0",
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 0",
        ),
    ];
    for (name, fen) in bad {
        assert!(validate_fen(fen).is_err(), "{name} accepted: {fen:?}");
    }
    let good = [
        ("start", START),
        ("halfmove 150", "4k3/8/8/8/8/8/4P3/4K3 w - - 150 80"),
        (
            "ep",
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
        ),
    ];
    for (name, fen) in good {
        assert!(validate_fen(fen).is_ok(), "{name} rejected: {fen:?}");
    }
}
