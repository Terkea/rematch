//! Deterministic in-process `Analyser` for tests (the Rust twin of the TS `fakePool`).

use std::sync::Mutex;
use std::time::Duration;

use chess_core::EngineScore;
use engine::{AnalyseOptions, Analyser, AnalysisLine, AnalysisResult, EngineError};
use futures::future::BoxFuture;

/// Wall time reported for every fake analysis.
pub const FAKE_WALL: Duration = Duration::from_millis(5);
/// CPU time reported for every fake analysis.
pub const FAKE_CPU: Duration = Duration::from_millis(3);

pub struct FakeAnalyser {
    /// Side-to-move score for a FEN.
    pub score: Box<dyn Fn(&str) -> EngineScore + Send + Sync>,
    /// `(fen, nodes, multipv)` per call, in call order.
    pub calls: Mutex<Vec<(String, u64, u32)>>,
    /// The first `n` calls succeed and every later call fails with `EngineError::Crashed`.
    pub fail_after: Option<usize>,
    /// Never resolves (until the caller drops the future).
    pub hang: bool,
    /// Sleeps this long per call and reports it as the wall time (zero: reports `FAKE_WALL`).
    pub delay: Duration,
}

impl FakeAnalyser {
    /// Scores every position `cp 30`, with pv `e2e4 e7e5`.
    pub fn new() -> Self {
        Self::with_score(|_| EngineScore::Cp(30))
    }

    pub fn with_score(score: impl Fn(&str) -> EngineScore + Send + Sync + 'static) -> Self {
        Self {
            score: Box::new(score),
            calls: Mutex::new(Vec::new()),
            fail_after: None,
            hang: false,
            delay: Duration::ZERO,
        }
    }

    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn called_fens(&self) -> Vec<String> {
        let calls = self.calls.lock().unwrap_or_else(|e| e.into_inner());
        calls.iter().map(|c| c.0.clone()).collect()
    }
}

impl Default for FakeAnalyser {
    fn default() -> Self {
        Self::new()
    }
}

impl Analyser for FakeAnalyser {
    fn analyse<'a>(
        &'a self,
        fen: &'a str,
        opts: AnalyseOptions,
    ) -> BoxFuture<'a, Result<AnalysisResult, EngineError>> {
        Box::pin(async move {
            let n = {
                let mut calls = self.calls.lock().unwrap_or_else(|e| e.into_inner());
                calls.push((fen.to_string(), opts.nodes, opts.multipv));
                calls.len()
            };
            if self.hang {
                futures::future::pending::<()>().await;
            }
            if !self.delay.is_zero() {
                tokio::time::sleep(self.delay).await;
            }
            if self.fail_after.is_some_and(|limit| n > limit) {
                return Err(EngineError::Crashed);
            }
            Ok(AnalysisResult {
                lines: vec![AnalysisLine {
                    multipv: 1,
                    score: (self.score)(fen),
                    pv: vec!["e2e4".to_string(), "e7e5".to_string()],
                    depth: 10,
                    nodes: 1,
                }],
                wall: if self.delay.is_zero() {
                    FAKE_WALL
                } else {
                    self.delay
                },
                cpu: Some(FAKE_CPU),
            })
        })
    }
}
