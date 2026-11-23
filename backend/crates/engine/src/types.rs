//! Shared engine types (final in Task 1).

use std::time::Duration;

use chess_core::EngineScore;
use futures::future::BoxFuture;

/// Nodes per position for the pinned analysis protocol.
pub const ENGINE_NODES: u64 = 300_000;
/// Analysis protocol id stored with every eval and analysed game.
pub const ANALYSIS_PROTOCOL: &str = "sf19-n300k-v1";
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
