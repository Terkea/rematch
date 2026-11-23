//! The determinism audit of the engine settings (ADR 0104, issue #52): the UCI lines a pooled
//! engine receives, and proof that sending `Threads 1` and `Hash 16` explicitly gives the same
//! evaluations as Stockfish's own defaults.

mod common;

use std::path::PathBuf;
use std::time::Duration;

use engine::pool::PoolOptions;
use engine::uci::{StartOptions, UciEngine};
use engine::{AnalyseOptions, ENGINE_HASH_MB, ENGINE_THREADS};

/// A fake UCI engine: logs every line it reads to `$1`, answers the handshake, the barrier and
/// every `go` with one fixed line.
const FAKE: &str = r#"#!/bin/sh
log="$0.log"
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$log"
  case "$line" in
    uci) printf 'id name FakeFish\nuciok\n' ;;
    isready) printf 'readyok\n' ;;
    go*) printf 'info depth 1 multipv 1 score cp 12 nodes 10 pv e2e4\nbestmove e2e4\n' ;;
    quit) exit 0 ;;
  esac
done
"#;

#[cfg(unix)]
fn fake_engine() -> (tempdir::Dir, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempdir::Dir::new();
    let path = dir.0.join("fakefish.sh");
    std::fs::write(&path, FAKE).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, path)
}

mod tempdir {
    pub struct Dir(pub std::path::PathBuf);
    impl Dir {
        pub fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "uci-audit-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            std::fs::create_dir_all(&p).unwrap();
            Dir(p)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_pooled_engine_gets_threads_hash_ucinewgame_and_a_node_limit() {
    let (_dir, path) = fake_engine();
    let pool = common::create_pool(PoolOptions {
        size: 1,
        path: path.clone(),
        hash_mb: None,
    })
    .await
    .expect("fake pool");
    let fens = [
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
    ];
    for fen in fens {
        pool.analyse(
            fen,
            AnalyseOptions {
                nodes: 1234,
                multipv: 1,
                timeout: Duration::from_secs(10),
            },
        )
        .await
        .expect("fake search");
    }
    pool.close().await;

    let log = std::fs::read_to_string(path.with_file_name("fakefish.sh.log")).unwrap();
    let lines: Vec<&str> = log.lines().collect();
    assert!(
        lines.contains(&format!("setoption name Threads value {ENGINE_THREADS}").as_str()),
        "{lines:?}"
    );
    assert!(
        lines.contains(&format!("setoption name Hash value {ENGINE_HASH_MB}").as_str()),
        "{lines:?}"
    );
    assert_eq!(ENGINE_THREADS, 1);
    assert_eq!(ENGINE_HASH_MB, 16);
    let positions: Vec<usize> = (0..lines.len())
        .filter(|&i| lines[i].starts_with("position "))
        .collect();
    assert_eq!(positions.len(), fens.len(), "{lines:?}");
    for &i in &positions {
        assert!(
            lines[..i]
                .iter()
                .rev()
                .find(|l| l.starts_with("position ") || *l == &"ucinewgame")
                == Some(&"ucinewgame"),
            "no ucinewgame right before line {i}: {lines:?}"
        );
    }
    let gos: Vec<&&str> = lines.iter().filter(|l| l.starts_with("go")).collect();
    assert_eq!(gos.len(), fens.len());
    assert!(gos.iter().all(|g| **g == "go nodes 1234"), "{gos:?}");
    assert!(!lines.iter().any(|l| l.contains("movetime")));
}

/// The defaults the explicit settings replace are Stockfish's own: its `uci` answer lists them.
#[test]
fn stockfish_defaults_are_the_values_we_send() {
    let Some(path) = testkit::stockfish() else {
        return;
    };
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("printf 'uci\\nquit\\n' | {}", path.display()))
        .output()
        .expect("stockfish runs");
    let text = String::from_utf8_lossy(&out.stdout);
    let default_of = |name: &str| -> Option<u32> {
        text.lines()
            .find(|l| l.starts_with(&format!("option name {name} type spin")))
            .and_then(|l| l.split(" default ").nth(1))
            .and_then(|r| r.split_whitespace().next())
            .and_then(|v| v.parse().ok())
    };
    assert_eq!(default_of("Threads"), Some(ENGINE_THREADS), "{text}");
    assert_eq!(default_of("Hash"), Some(ENGINE_HASH_MB), "{text}");
}

/// Fixture positions: the opening, a middlegame, a tactic and an endgame.
const POSITIONS: [&str; 4] = [
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    "r1bqkb1r/pppp1ppp/2n2n2/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR w KQkq - 4 4",
    "r2q1rk1/pp2bppp/2n1pn2/3p4/2PP4/2N1PN2/PP3PPP/R2QKB1R w KQ - 0 9",
    "8/5k2/8/3K4/8/4P3/8/8 w - - 0 1",
];

async fn evals(threads: Option<u32>, hash_mb: Option<u32>, path: PathBuf) -> Vec<String> {
    let mut e: UciEngine = common::start_engine(StartOptions {
        path,
        threads,
        hash_mb,
        syzygy_path: None,
        handshake_timeout: None,
    })
    .await
    .expect("stockfish starts");
    let mut out = Vec::new();
    for fen in POSITIONS {
        let r = e
            .analyse(
                fen,
                AnalyseOptions {
                    nodes: 50_000,
                    multipv: 2,
                    timeout: Duration::from_secs(30),
                },
            )
            .await
            .expect("search");
        for l in r.lines {
            out.push(format!(
                "{fen} | {} | {:?} | {}",
                l.multipv,
                l.score,
                l.pv.join(" ")
            ));
        }
    }
    e.quit().await;
    out
}

/// Sending the values explicitly changes no evaluation: the protocol stays `sf19-n300k-v1`.
#[tokio::test]
async fn explicit_threads_and_hash_give_the_same_evals_as_the_defaults() {
    let Some(path) = testkit::stockfish() else {
        return;
    };
    let before = evals(None, None, path.clone()).await;
    let after = evals(Some(ENGINE_THREADS), Some(ENGINE_HASH_MB), path).await;
    assert!(!before.is_empty());
    assert_eq!(before, after);
}
