//! Parallel parse pipeline (R2 Task 20, ADR-R2-10).
//!
//! One pass over the dump. The async reader pulls games from the stream (network, SHA-256, zstd
//! and the splitter all run there) and fills `Vec<String>` batches into a bounded channel. Each
//! batch goes to a rayon pool, which parses it with `par_iter`: headers, then
//! [`keep`](crate::sample::keep), then the sink's [`GameSink::observe`] for every kept game. The
//! results come back **in batch order** (at most [`IN_FLIGHT_BATCHES`] at a time), and the
//! sink's [`GameSink::merge`] runs on the async side, one batch after the other. So the merged
//! games, the `max_games` cut and every sum are the same at any thread count.
//!
//! Memory is bounded by `channel_batches + IN_FLIGHT_BATCHES` batches of `batch_games` games.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chess_core::TimeClass;
use futures::channel::{mpsc, oneshot};
use futures::future::BoxFuture;
use futures::{SinkExt, Stream, StreamExt};

use crate::download::DumpError;
use crate::headers::{DumpHeader, parse_dump_headers};
use crate::sample::{SampleFilter, keep};

/// Batches parsed at the same time. Two keep every rayon thread busy across a batch's tail
/// while staying in order.
pub const IN_FLIGHT_BATCHES: usize = 2;

/// Pipeline sizing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipelineConfig {
    /// Rayon threads (default: `available_parallelism`).
    pub threads: usize,
    /// Games per batch (default 2 000).
    pub batch_games: usize,
    /// Batches buffered between the reader and the parser (default `2 × threads`).
    pub channel_batches: usize,
}

fn all_cores() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

impl PipelineConfig {
    /// The defaults with `threads` threads; `0` means every core.
    pub fn with_threads(threads: usize) -> Self {
        let threads = if threads == 0 { all_cores() } else { threads };
        Self {
            threads,
            batch_games: 2_000,
            channel_batches: 2 * threads,
        }
    }
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self::with_threads(0)
    }
}

/// Counters and timings of one run.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PipelineStats {
    /// Games parsed, up to and including the last kept game when the cap was reached.
    pub games_seen: u64,
    pub games_kept: u64,
    /// Games whose headers did not parse (skipped; counted in `games_seen`).
    pub games_malformed: u64,
    /// Compressed bytes read. [`run_pipeline`] cannot see them and leaves 0; the caller sets it
    /// from [`crate::download::ShaHandle::bytes_read`].
    pub bytes_compressed: u64,
    /// PGN bytes of the games the reader passed on.
    pub bytes_pgn: u64,
    /// True when `max_games` stopped the run before the source ended.
    pub capped: bool,
    pub elapsed: Duration,
    /// Busy milliseconds per stage: `read` (waiting on the stream: network, hash, zstd, split),
    /// `parse` (batch wall time on the rayon pool), `aggregate` (merge minus flush) and `flush`
    /// (the sink's own writes). The stages overlap, so they do not add up to `elapsed`; the
    /// largest one is the bottleneck.
    pub stage_ms: BTreeMap<String, f64>,
}

impl PipelineStats {
    /// The stage with the most busy time, or `None` with no stages.
    pub fn bottleneck(&self) -> Option<&str> {
        self.stage_ms
            .iter()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(k, _)| k.as_str())
    }
}

/// Where kept games go. `observe` runs on the rayon pool, once per kept game; `merge` runs on
/// the async side with one batch's results, in source order.
pub trait GameSink: Sync {
    type Partial: Send + 'static;

    fn observe(&self, pgn: &str, h: &DumpHeader, tc: TimeClass) -> Self::Partial;

    fn merge(&self, parts: Vec<Self::Partial>) -> BoxFuture<'_, Result<(), DumpError>>;

    /// Extra stage timings of the sink (e.g. `flush`), in milliseconds, so far.
    fn stage_ms(&self) -> BTreeMap<String, f64> {
        BTreeMap::new()
    }
}

enum Outcome<P> {
    Malformed,
    Skipped,
    Kept(P),
}

struct Parsed<P> {
    outcomes: Vec<Outcome<P>>,
    parse_ms: f64,
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1_000.0
}

fn parse_batch<S: GameSink>(
    batch: &[String],
    filter: &SampleFilter,
    sink: &S,
) -> Vec<Outcome<S::Partial>> {
    use rayon::prelude::*;
    batch
        .par_iter()
        .map(|pgn| match parse_dump_headers(pgn) {
            Err(_) => Outcome::Malformed,
            Ok(h) => match keep(&h, filter) {
                None => Outcome::Skipped,
                Some(tc) => Outcome::Kept(sink.observe(pgn, &h, tc)),
            },
        })
        .collect()
}

/// Runs the pipeline over `games` until the source ends or `filter.max_games` games are kept.
///
/// Takes the stream by value; pass `&mut stream` to keep it (a dump must then be drained to the
/// end before its SHA-256 is verified, R-9). The sink is shared with the rayon jobs, hence the
/// `Arc`. A source error, a merge error or a panic in `observe` ends the run with that error.
pub async fn run_pipeline<G, S>(
    games: G,
    filter: &SampleFilter,
    sink: &Arc<S>,
    cfg: &PipelineConfig,
) -> Result<PipelineStats, DumpError>
where
    G: Stream<Item = Result<String, DumpError>> + Unpin,
    S: GameSink + Send + 'static,
{
    let started = Instant::now();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(cfg.threads.max(1))
        .thread_name(|i| format!("lichess-db-parse-{i}"))
        .build()
        .map_err(|e| DumpError::Config(format!("rayon pool: {e}")))?;
    let pool = Arc::new(pool);
    let filter = Arc::new(filter.clone());
    let batch_games = cfg.batch_games.max(1);
    let (tx, rx) = mpsc::channel::<Vec<String>>(cfg.channel_batches.max(1));

    let reader = read_batches(games, tx, batch_games);
    let processor = process_batches(rx, &pool, &filter, sink);
    let (read, processed) = futures::join!(reader, processor);
    let (read_ms, bytes_pgn) = read?;
    let mut stats = processed?;
    stats.bytes_pgn = bytes_pgn;
    stats.elapsed = started.elapsed();
    stats.stage_ms.insert("read".into(), read_ms);
    let extra = sink.stage_ms();
    let flush = extra.get("flush").copied().unwrap_or(0.0);
    if let Some(agg) = stats.stage_ms.get_mut("aggregate") {
        *agg = (*agg - flush).max(0.0);
    }
    stats.stage_ms.extend(extra);
    stats.stage_ms.entry("flush".into()).or_insert(0.0);
    Ok(stats)
}

/// Pulls games into batches and sends them. Stops when the source ends (after sending the last
/// partial batch), on a source error, or when the processor hangs up. Returns the milliseconds
/// spent waiting on the source and the PGN bytes sent.
async fn read_batches<G>(
    mut games: G,
    mut tx: mpsc::Sender<Vec<String>>,
    batch_games: usize,
) -> Result<(f64, u64), DumpError>
where
    G: Stream<Item = Result<String, DumpError>> + Unpin,
{
    let mut read = Duration::ZERO;
    let mut bytes = 0u64;
    let mut batch = Vec::with_capacity(batch_games);
    loop {
        let t = Instant::now();
        let next = games.next().await;
        read += t.elapsed();
        match next {
            Some(Ok(pgn)) => {
                bytes += pgn.len() as u64;
                batch.push(pgn);
                if batch.len() >= batch_games {
                    let full = std::mem::replace(&mut batch, Vec::with_capacity(batch_games));
                    if tx.send(full).await.is_err() {
                        break;
                    }
                }
            }
            Some(Err(err)) => return Err(err),
            None => {
                if !batch.is_empty() {
                    let _ = tx.send(batch).await;
                }
                break;
            }
        }
    }
    Ok((ms(read), bytes))
}

/// Parses batches on the pool (in order, [`IN_FLIGHT_BATCHES`] at a time), applies the cap and
/// merges. Dropping `rx` on return tells the reader to stop.
async fn process_batches<S>(
    rx: mpsc::Receiver<Vec<String>>,
    pool: &Arc<rayon::ThreadPool>,
    filter: &Arc<SampleFilter>,
    sink: &Arc<S>,
) -> Result<PipelineStats, DumpError>
where
    S: GameSink + Send + 'static,
{
    // Every pool job holds a clone of `alive` until its last statement, after it has dropped its
    // `sink` and `filter` clones. The stream below owns the original, so once it is dropped the
    // channel closes only when every job spawned so far has finished. So when `run_pipeline`
    // returns, no worker still holds the sink, even after a cap, an error or a panic stopped the
    // loop early (a caller may then `Arc::try_unwrap` it).
    let (alive, mut all_done) = mpsc::channel::<()>(0);
    let parsed = rx
        .map(move |batch| {
            let (done, wait) = oneshot::channel();
            let (filter, sink, alive) = (filter.clone(), sink.clone(), alive.clone());
            pool.spawn(move || {
                let t = Instant::now();
                let result =
                    catch_unwind(AssertUnwindSafe(|| parse_batch(&batch, &filter, &*sink)));
                drop(sink);
                drop(filter);
                let _ = done.send(result.map(|outcomes| Parsed {
                    outcomes,
                    parse_ms: ms(t.elapsed()),
                }));
                drop(alive);
            });
            wait
        })
        .buffered(IN_FLIGHT_BATCHES);

    // merge_parsed owns the stream and drops it (and with it `rx`, which stops the reader, and
    // `alive`) when it returns, on every path. Then wait for the jobs still running.
    let stats = merge_parsed(parsed, filter.max_games, sink).await;
    while all_done.next().await.is_some() {}
    stats
}

/// Merges parsed batches in order until the source ends or the cap is reached.
async fn merge_parsed<P, S>(
    mut parsed: P,
    max: Option<u64>,
    sink: &Arc<S>,
) -> Result<PipelineStats, DumpError>
where
    P: Stream<Item = Result<std::thread::Result<Parsed<S::Partial>>, oneshot::Canceled>> + Unpin,
    S: GameSink + Send + 'static,
{
    let mut stats = PipelineStats::default();
    let (mut parse_ms, mut merge) = (0.0, Duration::ZERO);
    while let Some(result) = parsed.next().await {
        let batch = match result {
            Ok(Ok(batch)) => batch,
            Ok(Err(_)) | Err(_) => {
                return Err(DumpError::Decode("a parse worker panicked".into()));
            }
        };
        parse_ms += batch.parse_ms;
        let mut parts = Vec::new();
        for outcome in batch.outcomes {
            if max.is_some_and(|m| stats.games_kept >= m) {
                stats.capped = true;
                break;
            }
            stats.games_seen += 1;
            match outcome {
                Outcome::Malformed => stats.games_malformed += 1,
                Outcome::Skipped => {}
                Outcome::Kept(p) => {
                    stats.games_kept += 1;
                    parts.push(p);
                }
            }
        }
        if !parts.is_empty() {
            let t = Instant::now();
            sink.merge(parts).await?;
            merge += t.elapsed();
        }
        if stats.capped || max.is_some_and(|m| stats.games_kept >= m) {
            stats.capped = true;
            break;
        }
    }
    stats.stage_ms.insert("parse".into(), parse_ms);
    stats.stage_ms.insert("aggregate".into(), ms(merge));
    Ok(stats)
}
