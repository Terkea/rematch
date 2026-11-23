//! The benchmark's engine: real Stockfish at a small, fixed node budget, with every result kept
//! in memory by FEN.
//!
//! The generator searches every position it plays a move in. The analysis and the enrichment
//! later ask for the same positions, so they are served the same search instead of a second one.
//! The generator and the pipeline therefore see exactly the same evaluations, and an injected
//! flaw is judged on the numbers the injector saw. A request for more lines than the stored
//! search has runs a new search and replaces it.
//!
//! Every search uses [`MemoAnalyser::nodes`], whatever the caller asked for: the production
//! protocol (300k nodes, `engine::ENGINE_NODES`) would make one smoke run take hours. The
//! engine sends `ucinewgame` before each search (`engine::uci`), so a node-limited search on one
//! thread gives the same result every time.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use engine::{AnalyseOptions, AnalysisResult, Analyser, EngineError};
use futures::FutureExt;
use futures::future::BoxFuture;

/// Lines every search asks for (at least the enrichment's `MULTIPV` of 3).
pub const GEN_MULTIPV: u32 = 4;

struct Stored {
    result: AnalysisResult,
    /// Lines the search asked for. Fewer came back only when the position has fewer legal moves.
    asked: u32,
}

pub struct MemoAnalyser {
    inner: Arc<dyn Analyser>,
    nodes: u64,
    cache: Mutex<HashMap<String, Arc<Stored>>>,
    searches: AtomicU64,
    hits: AtomicU64,
}

impl MemoAnalyser {
    pub fn new(inner: Arc<dyn Analyser>, nodes: u64) -> Self {
        MemoAnalyser {
            inner,
            nodes: nodes.max(1),
            cache: Mutex::new(HashMap::new()),
            searches: AtomicU64::new(0),
            hits: AtomicU64::new(0),
        }
    }

    pub fn nodes(&self) -> u64 {
        self.nodes
    }

    /// `(searches run, requests served from memory)`.
    pub fn counts(&self) -> (u64, u64) {
        (
            self.searches.load(Ordering::Relaxed),
            self.hits.load(Ordering::Relaxed),
        )
    }

    fn cached(&self, fen: &str, multipv: u32) -> Option<AnalysisResult> {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let hit = cache.get(fen)?;
        (hit.asked >= multipv).then(|| truncate(&hit.result, multipv))
    }

    /// The search of `fen` with up to `multipv` lines (memoised).
    pub async fn search(&self, fen: &str, multipv: u32) -> Result<AnalysisResult, EngineError> {
        let multipv = multipv.max(1);
        if let Some(r) = self.cached(fen, multipv) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(r);
        }
        let asked = multipv.max(GEN_MULTIPV);
        let opts = AnalyseOptions {
            nodes: self.nodes,
            multipv: asked,
            timeout: engine::DEFAULT_ANALYSIS_TIMEOUT,
        };
        let mut result = self.inner.analyse(fen, opts).await?;
        self.searches.fetch_add(1, Ordering::Relaxed);
        result.lines.sort_by_key(|l| l.multipv);
        let out = truncate(&result, multipv);
        self.cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(fen.to_owned(), Arc::new(Stored { result, asked }));
        Ok(out)
    }
}

fn truncate(r: &AnalysisResult, multipv: u32) -> AnalysisResult {
    AnalysisResult {
        lines: r
            .lines
            .iter()
            .filter(|l| l.multipv <= multipv)
            .cloned()
            .collect(),
        wall: r.wall,
        cpu: r.cpu,
    }
}

impl Analyser for MemoAnalyser {
    fn analyse<'a>(
        &'a self,
        fen: &'a str,
        opts: AnalyseOptions,
    ) -> BoxFuture<'a, Result<AnalysisResult, EngineError>> {
        async move { self.search(fen, opts.multipv).await }.boxed()
    }
}
