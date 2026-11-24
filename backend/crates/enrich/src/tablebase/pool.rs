//! Tablebase engine pool (R2 Task 23, ADR-R2-12).
//!
//! Its own `UciEngine`s, started with `SyzygyPath`; never the main `EnginePool`, so the main
//! analysis protocol (`ANALYSIS_PROTOCOL`) is unchanged.

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use engine::types::{AnalyseOptions, EngineError};
use engine::uci::{StartOptions, UciEngine};
use futures::future::BoxFuture;
use profile_core::types::Wdl;
use tokio::sync::{Mutex, Semaphore};

use super::wdl::wdl_from_lines;
use super::{MAX_PIECES, covered_fen, material_sides};
use crate::types::TablebaseProbe;

/// Per-probe deadline. The search is 1000 nodes, but `ucinewgame` makes Stockfish re-read the
/// table directory, which is slow on a busy host or a bind mount: same budget as main analysis.
pub const PROBE_TIMEOUT: Duration = engine::types::DEFAULT_ANALYSIS_TIMEOUT;

/// A pool of Stockfish processes with `SyzygyPath` set, answering [`TablebaseProbe`].
pub struct SyzygyPool {
    /// `None` when the directory holds no `.rtbw` files: every probe is `None`.
    start: Option<StartOptions>,
    /// Stems of the `.rtbw` files found (`KQvK`).
    tables: HashSet<String>,
    idle: Mutex<Vec<UciEngine>>,
    permits: Semaphore,
    nodes: u64,
    calls: AtomicUsize,
    closed: AtomicBool,
}

fn wdl_tables(dir: &Path) -> HashSet<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return HashSet::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            name.strip_suffix(".rtbw").map(str::to_owned)
        })
        .collect()
}

impl SyzygyPool {
    /// Start `size` (normally 1) engines searching `nodes` (normally 1000) per probe. When
    /// `syzygy_path` holds no `.rtbw` file (or does not exist), no engine is started, one warning
    /// is logged, and every probe answers `None`.
    pub async fn start(
        stockfish: &Path,
        syzygy_path: &Path,
        size: usize,
        nodes: u64,
    ) -> Result<SyzygyPool, EngineError> {
        let tables = wdl_tables(syzygy_path);
        let size = size.max(1);
        let mut pool = SyzygyPool {
            start: None,
            tables,
            idle: Mutex::new(Vec::new()),
            permits: Semaphore::new(size),
            nodes: nodes.max(1),
            calls: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
        };
        if pool.tables.is_empty() {
            tracing::warn!(
                path = %syzygy_path.display(),
                "no Syzygy .rtbw files found; tablebase probes disabled"
            );
            return Ok(pool);
        }
        let opts = StartOptions {
            path: stockfish.to_path_buf(),
            threads: None,
            hash_mb: None,
            syzygy_path: Some(syzygy_path.display().to_string()),
            handshake_timeout: None,
        };
        let mut engines = Vec::with_capacity(size);
        for _ in 0..size {
            match UciEngine::start(opts.clone()).await {
                Ok(e) => engines.push(e),
                Err(err) => {
                    for e in engines {
                        e.quit().await;
                    }
                    return Err(err);
                }
            }
        }
        pool.idle = Mutex::new(engines);
        pool.start = Some(opts);
        tracing::info!(
            tables = pool.tables.len(),
            size,
            "syzygy probe pool started"
        );
        Ok(pool)
    }

    /// Number of `.rtbw` tables found at start.
    pub fn tables_found(&self) -> usize {
        self.tables.len()
    }

    /// Number of engine searches run so far (for tests and metrics).
    pub fn engine_calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    /// Quit every engine. Later probes answer `None`.
    pub async fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        let engines = std::mem::take(&mut *self.idle.lock().await);
        for e in engines {
            e.quit().await;
        }
    }

    fn has_table(&self, placement: &str) -> bool {
        let (w, b) = material_sides(placement);
        self.tables.contains(&format!("{w}v{b}")) || self.tables.contains(&format!("{b}v{w}"))
    }

    /// `available` with the number of `.rtbw` tables found, or `unavailable` when there are none
    /// (then every probe answers `None`).
    pub fn status(&self) -> TablebaseStatus {
        match self.start {
            Some(_) => TablebaseStatus::Available {
                tables: self.tables.len(),
            },
            None => TablebaseStatus::Unavailable,
        }
    }

    /// The position as sent (without clocks), `Ok(None)` for bare kings (a draw without an
    /// engine call), or `Err(())` when the tables cannot answer it.
    fn covered_head(&self, fen: &str) -> Result<Option<String>, ()> {
        if self.start.is_none() {
            return Err(());
        }
        let canon = covered_fen(fen, MAX_PIECES).ok_or(())?;
        let mut it = canon.rsplitn(3, ' ');
        let (_full, _half, head) = (it.next(), it.next(), it.next().ok_or(())?);
        let placement = head.split(' ').next().ok_or(())?;
        if placement.chars().filter(char::is_ascii_alphabetic).count() == 2 {
            return Ok(None); // bare kings: no table file, always a draw
        }
        if !self.has_table(placement) {
            return Err(()); // Stockfish would search, not probe: never a guess
        }
        Ok(Some(head.to_owned()))
    }

    async fn probe_fen(&self, fen: &str) -> Option<Wdl> {
        match self.covered_head(fen).ok()? {
            None => Some(0),
            // Clocks reset: the answer depends only on the fen_key (cacheable), with the 50-move
            // rule counted from this position (ADR-R2-12).
            Some(head) => self.search(&head, 0).await,
        }
    }

    /// Distance to zeroing (plies to a capture, pawn move or mate with best play, Syzygy's sign
    /// convention: positive when the side to move wins, negative when it loses, 0 for a draw),
    /// counted from this position with a fresh 50-move clock. `None` when the tables cannot
    /// answer, as [`TablebaseProbe::probe`]. A win needing more than 99 plies (cursed) is 0, as
    /// its WDL.
    ///
    /// Stockfish does not print DTZ, but its root rule uses it: with halfmove clock `h` the
    /// result stays a win (or a loss) only while `|dtz| + h <= 100`, and turns into a cursed win
    /// (blessed loss), read as 0, past that. The largest `h` in `0..=99` that keeps the result is
    /// found by bisection (7 searches) and gives `|dtz| = 100 - h`. The rule was measured on
    /// Stockfish 19 against the Lichess API values (ADR-R2-12). Meant for drills, not for the
    /// per-move pipeline.
    pub async fn probe_dtz(&self, fen: &str) -> Option<i32> {
        let Some(head) = self.covered_head(fen).ok()? else {
            return Some(0);
        };
        let wdl = self.search(&head, 0).await?;
        if wdl == 0 {
            return Some(0);
        }
        // Largest clock in 0..=99 that still gives `wdl` (the result is monotone in the clock).
        let (mut lo, mut hi) = (0_i32, 100_i32);
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if self.search(&head, mid).await? == wdl {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Some(i32::from(wdl) * (100 - lo))
    }

    /// One search of `head` (a FEN without clocks) with halfmove clock `halfmove`.
    async fn search(&self, head: &str, halfmove: i32) -> Option<Wdl> {
        let start = self.start.as_ref()?;
        let sent = format!("{head} {halfmove} 1");
        let opts = AnalyseOptions {
            nodes: self.nodes,
            multipv: 1,
            timeout: PROBE_TIMEOUT,
        };
        let _permit = self.permits.acquire().await.ok()?;
        for attempt in 0..2 {
            if self.closed.load(Ordering::SeqCst) {
                return None;
            }
            let popped = self.idle.lock().await.pop();
            let mut engine = match popped {
                Some(e) if e.is_alive() => e,
                _ => match UciEngine::start(start.clone()).await {
                    Ok(e) => e,
                    Err(err) => {
                        tracing::warn!(%err, "syzygy engine failed to start");
                        return None;
                    }
                },
            };
            self.calls.fetch_add(1, Ordering::SeqCst);
            match engine.analyse(&sent, opts).await {
                Ok(res) => {
                    self.idle.lock().await.push(engine);
                    return wdl_from_lines(&res.lines);
                }
                Err(err @ (EngineError::Crashed | EngineError::Timeout(_))) => {
                    engine.kill().await;
                    tracing::warn!(%err, attempt, fen = %sent, "syzygy probe failed");
                }
                Err(err) => {
                    self.idle.lock().await.push(engine);
                    tracing::warn!(%err, fen = %sent, "syzygy probe rejected");
                    return None;
                }
            }
        }
        None
    }
}

/// Whether the local tables can answer at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TablebaseStatus {
    /// `tables` `.rtbw` files were found and the engines started.
    Available { tables: usize },
    /// No `.rtbw` file (or no directory): every probe answers `None`, analysis goes on.
    Unavailable,
}

impl TablebaseStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TablebaseStatus::Available { .. } => "available",
            TablebaseStatus::Unavailable => "unavailable",
        }
    }
}

impl TablebaseProbe for SyzygyPool {
    fn max_pieces(&self) -> u32 {
        MAX_PIECES
    }

    fn probe<'a>(&'a self, fen: &'a str) -> BoxFuture<'a, Option<Wdl>> {
        Box::pin(self.probe_fen(fen))
    }
}
