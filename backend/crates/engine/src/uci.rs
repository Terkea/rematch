//! Stockfish UCI engine over a child process (port of `apps/worker/src/engine/uci-engine.ts`).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use chess_core::EngineScore;
use chess_core::board::Position;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::mpsc;

use crate::types::{AnalyseOptions, AnalysisLine, AnalysisResult, EngineError};

/// Deadline for each handshake step (`uciok`, `readyok`).
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Deadline for the engine to answer `stop` with `bestmove` after a cancelled search.
pub const STOP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug)]
pub struct StartOptions {
    pub path: PathBuf,
    pub threads: Option<u32>,
    pub hash_mb: Option<u32>,
    pub syzygy_path: Option<String>,
}

/// Parse one `info` line; `None` for lines that must be ignored (bounds, no pv, no score).
pub fn parse_info_line(line: &str) -> Option<AnalysisLine> {
    if !line.starts_with("info ") {
        return None;
    }
    let t: Vec<&str> = line.split_whitespace().collect();
    if t.contains(&"lowerbound") || t.contains(&"upperbound") {
        return None;
    }
    let pv_idx = t.iter().position(|&s| s == "pv")?;
    let score_idx = t.iter().position(|&s| s == "score")?;
    if pv_idx + 1 >= t.len() {
        return None;
    }
    let kind = *t.get(score_idx + 1)?;
    let value: i32 = t.get(score_idx + 2)?.parse().ok()?;
    let score = match kind {
        "cp" => EngineScore::Cp(value),
        "mate" => EngineScore::Mate(value),
        _ => return None,
    };
    let num = |key: &str| -> u64 {
        t.iter()
            .position(|&s| s == key)
            .and_then(|i| t.get(i + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };
    Some(AnalysisLine {
        multipv: if t.contains(&"multipv") {
            num("multipv") as u32
        } else {
            1
        },
        score,
        pv: t[pv_idx + 1..].iter().map(|s| (*s).to_string()).collect(),
        depth: num("depth") as u32,
        nodes: num("nodes"),
    })
}

fn invalid(fen: &str, reason: impl Into<String>) -> EngineError {
    EngineError::InvalidFen {
        fen: fen.to_string(),
        reason: reason.into(),
    }
}

/// Strictly validate a FEN and return it re-serialised from the parsed board.
/// Only this canonical form is ever sent to the process: Stockfish 19 exits on
/// bad input, and a raw string could smuggle extra commands.
pub fn canonical_fen(fen: &str) -> Result<String, EngineError> {
    if !fen.bytes().all(|b| b == b' ' || b.is_ascii_graphic()) {
        return Err(invalid(fen, "non-ASCII or control character"));
    }
    let fields: Vec<&str> = fen.split(' ').collect();
    if fields.len() != 6 || fields.iter().any(|f| f.is_empty()) {
        return Err(invalid(fen, "expected 6 single-space-separated fields"));
    }
    let ranks: Vec<&str> = fields[0].split('/').collect();
    if ranks.len() != 8 {
        return Err(invalid(fen, "expected 8 ranks"));
    }
    if ranks.iter().any(|r| {
        r.as_bytes()
            .windows(2)
            .any(|w| w[0].is_ascii_digit() && w[1].is_ascii_digit())
    }) {
        return Err(invalid(fen, "adjacent digits in a rank"));
    }
    // chess-core parses the clocks itself (a halfmove clock over 100 is
    // accepted, as in chess.js and Stockfish) and serialises their real values.
    let pos = Position::from_fen(fen).map_err(|e| invalid(fen, e.to_string()))?;
    Ok(pos.fen())
}

/// Validate a FEN before anything reaches the process.
pub fn validate_fen(fen: &str) -> Result<(), EngineError> {
    canonical_fen(fen).map(|_| ())
}

/// Stockfish answers `bestmove (none)` with no pv for terminal positions.
fn terminal_lines(fen: &str) -> Vec<AnalysisLine> {
    let Ok(pos) = Position::from_fen(fen) else {
        return vec![];
    };
    let score = if pos.is_checkmate() {
        EngineScore::Mate(0)
    } else if pos.is_stalemate() {
        EngineScore::Cp(0)
    } else {
        return vec![];
    };
    vec![AnalysisLine {
        multipv: 1,
        score,
        pv: vec![],
        depth: 0,
        nodes: 0,
    }]
}

fn matches_prefix(line: &str, prefix: &str) -> bool {
    line == prefix
        || line
            .strip_prefix(prefix)
            .is_some_and(|r| r.starts_with(' '))
}

/// utime + stime of a process from `/proc/<pid>/stat` (100 ticks/s). Blocking.
fn read_cpu_time(pid: u32) -> Option<Duration> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The comm field may contain spaces; fields resume after the last ')'.
    let rest = stat.get(stat.rfind(')')? + 2..)?;
    let f: Vec<&str> = rest.split_whitespace().collect();
    // Field 3 (state) is f[0], so utime (14) is f[11] and stime (15) is f[12].
    let ticks: u64 = f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?;
    Some(Duration::from_millis(ticks * 10))
}

/// One Stockfish process.
///
/// `analyse` is cancel-safe. A future dropped mid-search leaves `searching`
/// set, and the next call sends `stop` and drains the old `bestmove` first. A
/// write dropped mid-line leaves `write_pending` set, and the engine is then
/// killed, because the stream state is unknown.
pub struct UciEngine {
    child: Child,
    stdin: ChildStdin,
    lines: mpsc::UnboundedReceiver<String>,
    alive: Arc<AtomicBool>,
    pid: Option<u32>,
    searching: bool,
    write_pending: bool,
}

impl UciEngine {
    pub async fn start(opts: StartOptions) -> Result<UciEngine, EngineError> {
        let mut child = Command::new(&opts.path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| EngineError::Spawn(e.to_string()))?;
        let stdin = child.stdin.take().ok_or(EngineError::Crashed)?;
        let stdout = child.stdout.take().ok_or(EngineError::Crashed)?;
        let pid = child.id();
        let alive = Arc::new(AtomicBool::new(true));
        let (tx, rx) = mpsc::unbounded_channel();
        let alive_r = alive.clone();
        tokio::spawn(async move {
            // Lossy: invalid UTF-8 must not end the reader (that would look like a crash).
            let mut reader = BufReader::new(stdout);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match reader.read_until(b'\n', &mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let line = String::from_utf8_lossy(&buf).trim().to_string();
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                }
            }
            alive_r.store(false, Ordering::SeqCst);
        });
        let mut engine = UciEngine {
            child,
            stdin,
            lines: rx,
            alive,
            pid,
            searching: false,
            write_pending: false,
        };
        match engine.handshake(&opts).await {
            Ok(()) => Ok(engine),
            Err(e) => {
                engine.kill().await;
                Err(e)
            }
        }
    }

    async fn handshake(&mut self, opts: &StartOptions) -> Result<(), EngineError> {
        self.send("uci").await?;
        self.wait_for("uciok", HANDSHAKE_TIMEOUT, |_| {}).await?;
        if let Some(t) = opts.threads {
            self.send(&format!("setoption name Threads value {t}"))
                .await?;
        }
        if let Some(h) = opts.hash_mb {
            self.send(&format!("setoption name Hash value {h}")).await?;
        }
        if let Some(s) = opts.syzygy_path.as_deref().filter(|s| !s.is_empty()) {
            self.send(&format!("setoption name SyzygyPath value {s}"))
                .await?;
        }
        self.send("isready").await?;
        self.wait_for("readyok", HANDSHAKE_TIMEOUT, |_| {}).await
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub async fn analyse(
        &mut self,
        fen: &str,
        opts: AnalyseOptions,
    ) -> Result<AnalysisResult, EngineError> {
        // Invalid input must never reach the engine; only the canonical form does.
        let fen = canonical_fen(fen)?;
        if !self.is_alive() {
            return Err(EngineError::Crashed);
        }
        if self.searching {
            // A previous analyse was dropped mid-search: end that search first.
            // On timeout `wait_for` kills the engine.
            self.send("stop").await?;
            self.wait_for("bestmove", STOP_TIMEOUT, |_| {}).await?;
            self.searching = false;
        }
        self.drain_stale();
        let nodes = opts.nodes.max(1);
        let multipv = opts.multipv.max(1);
        self.send(&format!("setoption name MultiPV value {multipv}"))
            .await?;
        self.send("ucinewgame").await?;
        self.send(&format!("position fen {fen}")).await?;
        // Sync barrier: every line before `readyok` is discarded.
        self.send("isready").await?;
        self.wait_for("readyok", opts.timeout, |_| {}).await?;
        let mut best: BTreeMap<u32, AnalysisLine> = BTreeMap::new();
        let cpu_before = self.cpu_time().await;
        let started = Instant::now();
        self.searching = true;
        self.send(&format!("go nodes {nodes}")).await?;
        self.wait_for("bestmove", opts.timeout, |line| {
            if let Some(info) = parse_info_line(line) {
                best.insert(info.multipv, info);
            }
        })
        .await?;
        self.searching = false;
        let wall = started.elapsed();
        let cpu = match (cpu_before, self.cpu_time().await) {
            (Some(a), Some(b)) => Some(b.saturating_sub(a)),
            _ => None,
        };
        let lines = if best.is_empty() {
            terminal_lines(&fen)
        } else {
            best.into_values().collect()
        };
        Ok(AnalysisResult { lines, wall, cpu })
    }

    /// Ask the engine to quit. Kill (and reap) it if it is not usable or has
    /// not exited after 1 s.
    pub async fn quit(mut self) {
        if self.is_alive()
            && !self.write_pending
            && self.send("quit").await.is_ok()
            && matches!(
                tokio::time::timeout(Duration::from_secs(1), self.child.wait()).await,
                Ok(Ok(_))
            )
        {
            self.alive.store(false, Ordering::SeqCst);
            return;
        }
        self.kill().await;
    }

    /// Kill the process and wait for it, so no zombie is left behind.
    pub async fn kill(&mut self) {
        self.alive.store(false, Ordering::SeqCst);
        let _ = self.child.kill().await;
    }

    /// Discard lines buffered from earlier commands.
    fn drain_stale(&mut self) {
        while self.lines.try_recv().is_ok() {}
    }

    /// Write one complete command line with a single `write_all`.
    async fn send(&mut self, cmd: &str) -> Result<(), EngineError> {
        if self.write_pending {
            // An earlier write was cancelled mid-line: the stream state is unknown.
            self.kill().await;
            return Err(EngineError::Crashed);
        }
        if !self.is_alive() {
            return Err(EngineError::Crashed);
        }
        let mut line = String::with_capacity(cmd.len() + 1);
        line.push_str(cmd);
        line.push('\n');
        self.write_pending = true;
        let res = async {
            self.stdin.write_all(line.as_bytes()).await?;
            self.stdin.flush().await
        }
        .await;
        self.write_pending = false;
        if res.is_err() {
            self.kill().await;
            return Err(EngineError::Crashed);
        }
        Ok(())
    }

    /// Wait for a line matching `prefix`; kills the process on timeout, `Crashed` on EOF.
    async fn wait_for(
        &mut self,
        prefix: &str,
        timeout: Duration,
        mut on_line: impl FnMut(&str),
    ) -> Result<(), EngineError> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            match tokio::time::timeout_at(deadline, self.lines.recv()).await {
                Err(_) => {
                    self.kill().await;
                    return Err(EngineError::Timeout(timeout));
                }
                Ok(None) => {
                    self.alive.store(false, Ordering::SeqCst);
                    return Err(EngineError::Crashed);
                }
                Ok(Some(line)) => {
                    if matches_prefix(&line, prefix) {
                        return Ok(());
                    }
                    on_line(&line);
                }
            }
        }
    }

    /// CPU time of the engine process, with procfs read off the async runtime.
    async fn cpu_time(&self) -> Option<Duration> {
        let pid = self.pid?;
        tokio::task::spawn_blocking(move || read_cpu_time(pid))
            .await
            .ok()
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_full_info_line() {
        let l = parse_info_line(
            "info depth 10 seldepth 12 multipv 2 score cp -35 nodes 12000 nps 1 pv e7e5 g1f3",
        )
        .unwrap();
        assert_eq!(
            l,
            AnalysisLine {
                multipv: 2,
                score: EngineScore::Cp(-35),
                pv: vec!["e7e5".into(), "g1f3".into()],
                depth: 10,
                nodes: 12000
            }
        );
    }

    #[test]
    fn parses_mate_scores() {
        let l = parse_info_line("info depth 3 score mate -2 nodes 5 pv a1a2").unwrap();
        assert_eq!(l.score, EngineScore::Mate(-2));
    }

    #[test]
    fn ignores_bound_lines() {
        assert!(parse_info_line("info depth 5 score cp 10 lowerbound nodes 1 pv e2e4").is_none());
        assert!(parse_info_line("info depth 5 score cp 10 upperbound nodes 1 pv e2e4").is_none());
    }

    #[test]
    fn ignores_lines_without_pv_or_ending_in_pv() {
        assert!(parse_info_line("info depth 5 score cp 10 nodes 1").is_none());
        assert!(parse_info_line("info depth 5 score cp 10 nodes 1 pv").is_none());
        assert!(parse_info_line("bestmove e2e4").is_none());
    }

    #[test]
    fn missing_multipv_defaults_to_one() {
        let l = parse_info_line("info depth 5 score cp 10 nodes 1 pv e2e4").unwrap();
        assert_eq!(l.multipv, 1);
    }

    #[test]
    fn canonical_fen_reserialises_and_keeps_clocks() {
        // The ep square is dropped when no capture is possible; clocks are kept.
        assert_eq!(
            canonical_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1").unwrap(),
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1"
        );
        assert_eq!(
            canonical_fen("4k3/8/8/8/8/8/4P3/4K3 w - - 150 80").unwrap(),
            "4k3/8/8/8/8/8/4P3/4K3 w - - 150 80"
        );
    }

    #[test]
    fn prefix_match_requires_word_boundary() {
        assert!(matches_prefix("bestmove e2e4", "bestmove"));
        assert!(matches_prefix("readyok", "readyok"));
        assert!(!matches_prefix("readyokay", "readyok"));
    }
}
