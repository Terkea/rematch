//! Shared engine types (final in Task 1).

use std::time::Duration;

use chess_core::EngineScore;
use futures::future::BoxFuture;

/// Nodes per position for the pinned analysis protocol: 1 million, the count EXP-0008 chose
/// against deeper searches (ADR 0113, `lab/experiments/EXP-0008/decision.md`).
pub const ENGINE_NODES: u64 = 1_000_000;
/// Analysis protocol id stored with every eval and analysed game.
pub const ANALYSIS_PROTOCOL: &str = "sf19-n1m-v1";
/// The protocols before [`ANALYSIS_PROTOCOL`], newest first. Their stored evals stay readable
/// (the chat's cached eval falls back to them), but are never served as the current protocol's.
pub const EARLIER_ANALYSIS_PROTOCOLS: &[&str] = &["sf19-n300k-v1"];
/// `setoption name Threads` sent to every pooled engine. One thread makes a node-limited search
/// reproducible (ADR 0104). It equals Stockfish 19's default, so sending it changed no eval.
pub const ENGINE_THREADS: u32 = 1;
/// `setoption name Hash` (MB) sent to every pooled engine unless `PoolOptions::hash_mb` says
/// otherwise. It equals Stockfish 19's default (`tests/uci_audit.rs` checks both).
pub const ENGINE_HASH_MB: u32 = 16;
/// Per-position analysis timeout.
pub const DEFAULT_ANALYSIS_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq)]
pub struct AnalysisLine {
    pub multipv: u32,
    /// From the side to move's point of view.
    pub score: EngineScore,
    pub pv: Vec<String>,
    pub depth: u32,
    pub nodes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnalyseOptions {
    pub nodes: u64,
    pub multipv: u32,
    pub timeout: Duration,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnalysisResult {
    pub lines: Vec<AnalysisLine>,
    pub wall: Duration,
    pub cpu: Option<Duration>,
}

#[derive(thiserror::Error, Debug)]
pub enum EngineError {
    #[error("Invalid FEN: {fen:?} ({reason})")]
    InvalidFen { fen: String, reason: String },
    #[error("Engine process exited unexpectedly")]
    Crashed,
    #[error("Engine analysis timed out after {0:?}")]
    Timeout(Duration),
    #[error("Engine pool is closed")]
    PoolClosed,
    #[error("engine spawn failed: {0}")]
    Spawn(String),
}

pub trait Analyser: Send + Sync {
    fn analyse<'a>(
        &'a self,
        fen: &'a str,
        opts: AnalyseOptions,
    ) -> BoxFuture<'a, Result<AnalysisResult, EngineError>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-0008 chose 1 million nodes (ADR 0113); the protocol id names the count, and a stored
    /// eval of an earlier protocol is never read as one of this protocol.
    #[test]
    fn the_protocol_is_the_one_exp_0008_chose() {
        assert_eq!(ENGINE_NODES, 1_000_000);
        assert_eq!(ANALYSIS_PROTOCOL, "sf19-n1m-v1");
        assert_eq!(ENGINE_THREADS, 1);
        assert_eq!(ENGINE_HASH_MB, 16);
        assert_eq!(EARLIER_ANALYSIS_PROTOCOLS, &["sf19-n300k-v1"]);
        assert!(!EARLIER_ANALYSIS_PROTOCOLS.contains(&ANALYSIS_PROTOCOL));
    }
}
