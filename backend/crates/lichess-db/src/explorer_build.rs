//! Opening explorer build (R2 Task 20, ADR-R2-10; TS M2 Task 11 and ADR 0035).
//!
//! One month, one pass: the dump streams through [`run_pipeline`] into an [`ExplorerSink`],
//! which replays only the first `max_ply` SANs of each kept game and counts
//! `fen_key|band|tc|uci` → (white, draws, black). Counts are flushed to the `building` build
//! every `flush_every_games` games. After the stream, the rest of the body is drained through
//! the hasher (R-9), the digest is verified, rows below `min_row_count` are pruned, and only
//! then is the build activated. Any failure marks the build `failed`, which deletes its rows
//! and leaves the previous active build in place.
//!
//! Flushes are never retried: `flush_explorer` adds counts, so a retried chunk could count
//! twice. A failed flush fails the whole build instead (the stream cannot be replayed anyway),
//! so a double-counted chunk can never be activated.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use chess_core::TimeClass;
use chess_core::board::Position;
use chess_core::fen::fen_key;
use db::stage_metrics::{StageMetrics, record_stage_metrics};
use futures::future::BoxFuture;
use profile_core::bands::band_of;
use profile_core::types::Band;
use refdata::builds::{
    BuildKind, NewBuild, activate_build, create_build, fail_build, mark_unverified, set_source_sha,
};
use refdata::explorer::{ExplorerCounts, ExplorerKey, flush_explorer, prune_explorer};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::{DumpSource, is_valid_month};
use crate::download::{
    DumpError, RetryPolicy, ShaHandle, drain_to_end, fetch_expected_sha_with, open_dump_with,
};
use crate::headers::DumpHeader;
use crate::movetext::movetext_sans;
use crate::pipeline::{GameSink, PipelineConfig, PipelineStats, run_pipeline};
use crate::sample::{SampleFilter, proportional_rate};

/// `job_metrics.kind` of a build.
pub const BUILD_METRICS_KIND: &str = "lichess-db:build";

/// Games in a month when no earlier full build says otherwise. A rough figure for 2026
/// (2026-10 is 31 GB compressed); the operator overrides it with `--expected-games`, and every
/// full build records the real count for the next one.
pub const DEFAULT_EXPECTED_GAMES: u64 = 100_000_000;

/// The explorer's time classes (ADR 0035): correspondence is excluded.
pub const EXPLORER_TIME_CLASSES: [TimeClass; 4] = [
    TimeClass::Bullet,
    TimeClass::Blitz,
    TimeClass::Rapid,
    TimeClass::Classical,
];

/// The kept-games cap for a target of `max_games`: 25 % headroom. A sample at the proportional
/// rate lands within a few standard deviations of the target (about ±0.1 % at 2M), so the cap
/// only bites when the month is much larger than expected; then the build is marked `capped`
/// and its count is not used as the next month's estimate.
pub fn safety_cap(max_games: u64) -> u64 {
    max_games.saturating_add(max_games / 4)
}

/// The band of the mean of both ratings (TS `explorerBand`).
pub fn explorer_band(white_elo: i32, black_elo: i32) -> Band {
    band_of((f64::from(white_elo) + f64::from(black_elo)) / 2.0)
}

fn store(e: impl std::fmt::Display) -> DumpError {
    DumpError::Store(e.to_string())
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One kept game as the explorer sees it: its cell, its result and its first moves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExplorerGame {
    pub band: Band,
    pub time_class: TimeClass,
    /// `(white, draws, black)` increments: exactly one is 1.
    pub result: (i64, i64, i64),
    /// `(fen_key, uci)` per replayed ply.
    pub moves: Vec<(String, String)>,
    /// A SAN token did not replay; the moves before it are kept.
    pub illegal: bool,
}

/// Replays the first `max_ply` SANs of `pgn`. `None` for an unfinished game (`*`) or a missing
/// rating.
pub fn explorer_game(
    pgn: &str,
    h: &DumpHeader,
    tc: TimeClass,
    max_ply: usize,
) -> Option<ExplorerGame> {
    let result = match h.result.as_str() {
        "1-0" => (1, 0, 0),
        "1/2-1/2" => (0, 1, 0),
        "0-1" => (0, 0, 1),
        _ => return None,
    };
    let band = explorer_band(h.white_elo?, h.black_elo?);
    let mut pos = Position::start();
    let mut moves = Vec::new();
    let mut illegal = false;
    for san in movetext_sans(pgn, max_ply) {
        let key = fen_key(&pos.fen());
        match pos.play_san(san) {
            Ok(played) => moves.push((key, played.uci)),
            Err(_) => {
                illegal = true;
                break;
            }
        }
    }
    Some(ExplorerGame {
        band,
        time_class: tc,
        result,
        moves,
        illegal,
    })
}

/// Counters of an [`ExplorerSink`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ExplorerSinkStats {
    /// Games counted.
    pub games: u64,
    /// Games whose replay stopped at an illegal SAN.
    pub illegal: u64,
    pub flushes: u64,
    /// Rows written (after summing repeated keys within one flush).
    pub rows_flushed: u64,
    pub flush_ms: f64,
}

#[derive(Default)]
struct SinkState {
    acc: HashMap<ExplorerKey, ExplorerCounts>,
    since_flush: u64,
    stats: ExplorerSinkStats,
}

/// Aggregates explorer counts in memory and flushes them to a build (or, for the benchmark,
/// discards them at each flush).
pub struct ExplorerSink {
    target: Option<(PgPool, Uuid)>,
    max_ply: usize,
    flush_every_games: u64,
    state: Mutex<SinkState>,
}

impl ExplorerSink {
    /// Flushes into `build_id`.
    pub fn new(pool: PgPool, build_id: Uuid, max_ply: usize, flush_every_games: u64) -> Self {
        Self::with_target(Some((pool, build_id)), max_ply, flush_every_games)
    }

    /// Counts without a database: a flush only counts and drops the rows (benchmark).
    pub fn in_memory(max_ply: usize, flush_every_games: u64) -> Self {
        Self::with_target(None, max_ply, flush_every_games)
    }

    fn with_target(target: Option<(PgPool, Uuid)>, max_ply: usize, flush_every_games: u64) -> Self {
        Self {
            target,
            max_ply,
            flush_every_games: flush_every_games.max(1),
            state: Mutex::default(),
        }
    }

    pub fn stats(&self) -> ExplorerSinkStats {
        lock(&self.state).stats
    }

    /// Flushes whatever is still in memory.
    pub async fn flush_rest(&self) -> Result<(), DumpError> {
        let acc = {
            let mut st = lock(&self.state);
            st.since_flush = 0;
            std::mem::take(&mut st.acc)
        };
        self.flush(acc).await
    }

    async fn flush(&self, acc: HashMap<ExplorerKey, ExplorerCounts>) -> Result<(), DumpError> {
        if acc.is_empty() {
            return Ok(());
        }
        let t = Instant::now();
        let n = acc.len() as u64;
        if let Some((pool, id)) = &self.target {
            let rows: Vec<(ExplorerKey, ExplorerCounts)> = acc.into_iter().collect();
            flush_explorer(pool, *id, &rows).await.map_err(store)?;
        }
        let mut st = lock(&self.state);
        st.stats.flushes += 1;
        st.stats.rows_flushed += n;
        st.stats.flush_ms += t.elapsed().as_secs_f64() * 1_000.0;
        Ok(())
    }
}

impl GameSink for ExplorerSink {
    type Partial = Option<ExplorerGame>;

    fn observe(&self, pgn: &str, h: &DumpHeader, tc: TimeClass) -> Self::Partial {
        explorer_game(pgn, h, tc, self.max_ply)
    }

    fn merge(&self, parts: Vec<Self::Partial>) -> BoxFuture<'_, Result<(), DumpError>> {
        Box::pin(async move {
            let full = {
                let mut st = lock(&self.state);
                for game in parts.into_iter().flatten() {
                    st.stats.games += 1;
                    st.since_flush += 1;
                    if game.illegal {
                        st.stats.illegal += 1;
                    }
                    let (w, d, b) = game.result;
                    for (fen_key, uci) in game.moves {
                        let c = st
                            .acc
                            .entry(ExplorerKey {
                                fen_key,
                                band: game.band,
                                time_class: game.time_class,
                                uci,
                            })
                            .or_default();
                        c.white += w;
                        c.draws += d;
                        c.black += b;
                    }
                }
                if st.since_flush >= self.flush_every_games {
                    st.since_flush = 0;
                    Some(std::mem::take(&mut st.acc))
                } else {
                    None
                }
            };
            match full {
                Some(acc) => self.flush(acc).await,
                None => Ok(()),
            }
        })
    }

    fn stage_ms(&self) -> BTreeMap<String, f64> {
        BTreeMap::from([("flush".to_owned(), lock(&self.state).stats.flush_ms)])
    }
}

/// Whether the dump's SHA-256 is checked before activation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verify {
    /// Drain to the end, compare with `sha256sums.txt`, activate on a match (R-9).
    Full,
    /// Development: no drain, no check; the build ends `unverified` and is never served.
    None,
}

/// Options of [`build`]. [`BuildOptions::new`] gives the ADR 0035 defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildOptions {
    /// `YYYY-MM`; must match the source.
    pub month: String,
    /// Target sample size (2 000 000). The sample rate aims at it over the whole month; the
    /// pipeline stops only at the safety cap [`safety_cap`]`(max_games)`, which a correct
    /// estimate of the month's size never reaches.
    pub max_games: u64,
    /// SANs replayed per game (30).
    pub max_ply: usize,
    /// Rows below this total are pruned (3).
    pub min_row_count: i64,
    /// Games between two flushes (50 000).
    pub flush_every_games: u64,
    pub verify: Verify,
    /// Build the opening explorer (default `true`).
    pub explorer: bool,
    /// Build the band reference (Task 21; refused until then).
    pub band_reference: bool,
    /// Sample rate; `None` derives `max_games / expected games` (T19 review ruling).
    pub sample_rate: Option<f64>,
    /// Games expected in the month; `None` takes the last full build's count, else
    /// [`DEFAULT_EXPECTED_GAMES`].
    pub expected_games: Option<u64>,
    pub seed: u32,
    pub pipeline: PipelineConfig,
    pub retry: RetryPolicy,
}

impl BuildOptions {
    pub fn new(month: impl Into<String>) -> Self {
        Self {
            month: month.into(),
            max_games: 2_000_000,
            max_ply: 30,
            min_row_count: 3,
            flush_every_games: 50_000,
            verify: Verify::Full,
            explorer: true,
            band_reference: false,
            sample_rate: None,
            expected_games: None,
            seed: 0,
            pipeline: PipelineConfig::default(),
            retry: RetryPolicy::default(),
        }
    }
}

/// What [`build`] produced.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildOutcome {
    pub explorer_build: Option<Uuid>,
    pub reference_build: Option<Uuid>,
    pub stats: PipelineStats,
    /// The sample rate used.
    pub sample_rate: f64,
    pub explorer: ExplorerSinkStats,
}

/// `games_scanned` of the newest build that read its whole month (not capped).
async fn last_full_month_games(pool: &PgPool) -> Result<Option<u64>, DumpError> {
    let n: Option<i64> = sqlx::query_scalar(
        "SELECT games_scanned FROM reference_builds \
         WHERE status IN ('active', 'superseded') AND games_scanned > 0 \
           AND params->>'capped' = 'false' \
         ORDER BY source_month DESC, started_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(store)?;
    Ok(n.and_then(|n| u64::try_from(n).ok()))
}

async fn resolve_rate(pool: &PgPool, opts: &BuildOptions) -> Result<f64, DumpError> {
    if let Some(rate) = opts.sample_rate {
        if !(rate > 0.0 && rate <= 1.0) {
            return Err(DumpError::Config(format!(
                "sample rate {rate} is not in (0, 1]"
            )));
        }
        return Ok(rate);
    }
    let expected = match opts.expected_games {
        Some(n) => n,
        None => last_full_month_games(pool)
            .await?
            .unwrap_or(DEFAULT_EXPECTED_GAMES),
    };
    Ok(proportional_rate(opts.max_games, expected))
}

fn ms_since(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1_000.0
}

/// Builds the opening explorer from one monthly dump; see the module doc for the order of steps.
/// Records one `job_metrics` row (`kind = "lichess-db:build"`, stages `<stage>_ms`) once the
/// pipeline has run, whether or not the build was activated.
pub async fn build(
    pool: &PgPool,
    client: &reqwest::Client,
    src: &DumpSource,
    opts: &BuildOptions,
) -> Result<BuildOutcome, DumpError> {
    let started = Instant::now();
    if opts.band_reference {
        return Err(DumpError::Config(
            "the band reference build is not wired yet (R2 Task 21)".into(),
        ));
    }
    if !opts.explorer {
        return Err(DumpError::Config(
            "nothing to build: pass --explorer".into(),
        ));
    }
    if !is_valid_month(&opts.month) || opts.month != src.month {
        return Err(DumpError::Config(format!(
            "month {:?} must be YYYY-MM and match the source month {:?}",
            opts.month, src.month
        )));
    }
    let rate = resolve_rate(pool, opts).await?;
    let expected_sha = match opts.verify {
        Verify::Full => Some(fetch_expected_sha_with(client, src, &opts.retry).await?),
        Verify::None => None,
    };
    let filter = SampleFilter {
        max_games: Some(safety_cap(opts.max_games)),
        sample_rate: rate,
        seed: opts.seed,
        require_clock: false,
        time_classes: Some(EXPLORER_TIME_CLASSES.to_vec()),
        exclude_bots: true,
        exclude_provisional: true,
    };
    let params = json!({
        "max_games": opts.max_games,
        "safety_cap": safety_cap(opts.max_games),
        "max_ply": opts.max_ply,
        "min_row_count": opts.min_row_count,
        "flush_every_games": opts.flush_every_games,
        "sample_rate": rate,
        "seed": opts.seed,
        "time_classes": EXPLORER_TIME_CLASSES.iter().map(|t| t.as_str()).collect::<Vec<_>>(),
        "exclude_bots": true,
        "exclude_provisional": true,
        "verify": format!("{:?}", opts.verify).to_lowercase(),
    });
    let id = create_build(
        pool,
        &NewBuild {
            kind: BuildKind::OpeningExplorer,
            source_month: opts.month.clone(),
            source_url: Some(src.url()),
            params,
            protocol: None,
            enrichment_protocol: None,
            difficulty_model: None,
        },
    )
    .await
    .map_err(store)?;

    let sink = Arc::new(ExplorerSink::new(
        pool.clone(),
        id,
        opts.max_ply,
        opts.flush_every_games,
    ));
    let mut stats = None;
    let result = run_build(
        pool,
        client,
        src,
        opts,
        id,
        &filter,
        &sink,
        expected_sha.as_deref(),
        &mut stats,
    )
    .await;
    if let Some(stats) = &stats {
        let m = StageMetrics {
            kind: BUILD_METRICS_KIND.to_owned(),
            total_ms: ms_since(started),
            stages: stats
                .stage_ms
                .iter()
                .map(|(k, v)| (format!("{k}_ms"), *v))
                .collect(),
            ..StageMetrics::default()
        };
        if let Err(e) = record_stage_metrics(pool, &m).await {
            tracing::warn!(error = %e, "could not record build metrics");
        }
    }
    match result {
        Ok(()) => Ok(BuildOutcome {
            explorer_build: Some(id),
            reference_build: None,
            stats: stats.unwrap_or_default(),
            sample_rate: rate,
            explorer: sink.stats(),
        }),
        Err(err) => {
            tracing::error!(build = %id, error = %err, "explorer build failed");
            if let Err(e) = fail_build(pool, id, &err.to_string()).await {
                tracing::error!(build = %id, error = %e, "could not mark the build failed");
            }
            Err(err)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_build(
    pool: &PgPool,
    client: &reqwest::Client,
    src: &DumpSource,
    opts: &BuildOptions,
    id: Uuid,
    filter: &SampleFilter,
    sink: &Arc<ExplorerSink>,
    expected_sha: Option<&str>,
    out: &mut Option<PipelineStats>,
) -> Result<(), DumpError> {
    let (mut games, sha): (_, ShaHandle) = open_dump_with(client, src, &opts.retry).await?;
    let mut stats = run_pipeline(&mut games, filter, sink, &opts.pipeline).await?;
    let t = Instant::now();
    sink.flush_rest().await?;
    let tail_flush = ms_since(t);
    *stats.stage_ms.entry("flush".into()).or_insert(0.0) += tail_flush;
    stats.bytes_compressed = sha.bytes_read();
    tracing::info!(
        build = %id,
        seen = stats.games_seen,
        kept = stats.games_kept,
        capped = stats.capped,
        "explorer pipeline done"
    );
    *out = Some(stats);
    let stats = out.as_mut().expect("just set");

    let Some(expected) = expected_sha else {
        let t = Instant::now();
        prune_explorer(pool, id, opts.min_row_count)
            .await
            .map_err(store)?;
        stats.stage_ms.insert("prune".into(), ms_since(t));
        mark_unverified(pool, id).await.map_err(store)?;
        return Ok(());
    };
    let t = Instant::now();
    drain_to_end(games).await?;
    stats.stage_ms.insert("drain".into(), ms_since(t));
    stats.bytes_compressed = sha.bytes_read();
    let digest = sha.verify(expected)?;
    sqlx::query(
        "UPDATE reference_builds SET params = params || jsonb_build_object('capped', $2::bool) \
         WHERE id = $1",
    )
    .bind(id)
    .bind(stats.capped)
    .execute(pool)
    .await
    .map_err(store)?;
    let scanned = i64::try_from(stats.games_seen).unwrap_or(i64::MAX);
    set_source_sha(pool, id, &digest, scanned)
        .await
        .map_err(store)?;
    let t = Instant::now();
    prune_explorer(pool, id, opts.min_row_count)
        .await
        .map_err(store)?;
    stats.stage_ms.insert("prune".into(), ms_since(t));
    let t = Instant::now();
    activate_build(pool, id).await.map_err(store)?;
    stats.stage_ms.insert("activate".into(), ms_since(t));
    Ok(())
}
