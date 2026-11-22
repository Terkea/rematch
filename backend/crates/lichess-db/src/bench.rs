//! Throughput benchmark (R2 Task 20). Never part of the gate: it runs only from
//! `lichess-db bench`.
//!
//! Each row streams a compressed dump through the real chain (SHA-256, zstd, splitter) and
//! [`run_pipeline`] into an in-memory [`ExplorerSink`] (replay of 30 plies, counts, flushes that
//! drop the rows), so it measures read, parse and aggregate but not Postgres.
//!
//! `Synthetic` needs no network: it repeats the five fixture games with distinct `Site` values
//! and compresses them in memory, in frames of [`SYNTHETIC_FRAME_GAMES`] games. Repeated games
//! compress far better than a real month and add few explorer keys, so read and aggregate are
//! cheaper than on real data; treat it as a parser ceiling. `Http` streams the first
//! `max_games` games of a real month and stops (no drain, no verification).

use std::collections::BTreeMap;
use std::sync::Arc;

use async_compression::Level;
use async_compression::tokio::bufread::ZstdEncoder;
use bytes::Bytes;
use tokio::io::AsyncReadExt;

use crate::config::{DumpSource, http_client};
use crate::download::{DumpError, DumpStream, RetryPolicy, open_dump_with};
use crate::explorer_build::{EXPLORER_TIME_CLASSES, ExplorerSink};
use crate::pipeline::{PipelineConfig, PipelineStats, run_pipeline};
use crate::sample::SampleFilter;
use crate::split::split_games;

/// The five fixture games of R2 Task 19.
pub const FIXTURE_PGN: &str = include_str!("../../../fixtures/lichess-db/sample.pgn");

/// Games per zstd frame of the synthetic dump.
pub const SYNTHETIC_FRAME_GAMES: u64 = 20_000;

/// What to benchmark.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BenchSource {
    /// `games` synthetic games, compressed in memory once and reused for every row.
    Synthetic { games: u64 },
    /// The first `max_games` games of a real month.
    Http { src: DumpSource, max_games: u64 },
}

/// One benchmark row.
#[derive(Clone, Debug, PartialEq)]
pub struct BenchRow {
    pub threads: usize,
    pub games: u64,
    pub games_per_sec: f64,
    /// Compressed megabytes (10^6 bytes) per second.
    pub mb_per_sec: f64,
    /// PGN megabytes per second, after decompression.
    pub pgn_mb_per_sec: f64,
    pub elapsed_s: f64,
    pub stage_ms: BTreeMap<String, f64>,
    /// The stage with the most busy time.
    pub bottleneck: String,
}

/// The synthetic games: the fixture games in turn, the `n`-th with `Site` `.../s<n>`.
pub fn synthetic_games(games: u64) -> impl Iterator<Item = String> {
    let base: Vec<(String, String)> = split_games(FIXTURE_PGN.lines())
        .into_iter()
        .map(|g| {
            let s = g.find("[Site \"").expect("fixture Site") + 7;
            let e = s + g[s..].find('"').expect("fixture Site end");
            let (head, tail) = (&g[..s], &g[e..]);
            (format!("{head}https://lichess.org/s"), tail.to_owned())
        })
        .collect();
    (0..games).map(move |i| {
        let (head, tail) = &base[(i % base.len() as u64) as usize];
        format!("{head}{i:010}{tail}\n")
    })
}

/// The synthetic dump as compressed frames.
pub async fn synthetic_dump(games: u64) -> Result<Vec<Bytes>, DumpError> {
    let mut frames = Vec::new();
    let mut it = synthetic_games(games).peekable();
    while it.peek().is_some() {
        let text: String = it.by_ref().take(SYNTHETIC_FRAME_GAMES as usize).collect();
        let mut enc = ZstdEncoder::with_quality(text.as_bytes(), Level::Default);
        let mut out = Vec::new();
        enc.read_to_end(&mut out)
            .await
            .map_err(|e| DumpError::Decode(format!("zstd encode: {e}")))?;
        frames.push(Bytes::from(out));
    }
    Ok(frames)
}

fn bench_filter(max_games: Option<u64>) -> SampleFilter {
    SampleFilter {
        max_games,
        time_classes: Some(EXPLORER_TIME_CLASSES.to_vec()),
        exclude_bots: true,
        ..SampleFilter::default()
    }
}

fn row(threads: usize, stats: &PipelineStats) -> BenchRow {
    let secs = stats.elapsed.as_secs_f64().max(1e-9);
    BenchRow {
        threads,
        games: stats.games_seen,
        games_per_sec: stats.games_seen as f64 / secs,
        mb_per_sec: stats.bytes_compressed as f64 / 1e6 / secs,
        pgn_mb_per_sec: stats.bytes_pgn as f64 / 1e6 / secs,
        elapsed_s: secs,
        stage_ms: stats.stage_ms.clone(),
        bottleneck: stats.bottleneck().unwrap_or("-").to_owned(),
    }
}

/// Runs `source` once per entry of `threads` (`0` = every core).
pub async fn bench(source: &BenchSource, threads: &[usize]) -> Result<Vec<BenchRow>, DumpError> {
    let synthetic = match source {
        BenchSource::Synthetic { games } => Some(synthetic_dump(*games).await?),
        BenchSource::Http { .. } => None,
    };
    let client = match source {
        BenchSource::Http { .. } => {
            Some(http_client().map_err(|e| DumpError::Transport(e.to_string()))?)
        }
        BenchSource::Synthetic { .. } => None,
    };
    let mut rows = Vec::new();
    for &t in threads {
        let cfg = PipelineConfig::with_threads(t);
        let sink = Arc::new(ExplorerSink::in_memory(30, 50_000));
        let (mut games, sha, max) = match (source, &synthetic, &client) {
            (BenchSource::Synthetic { .. }, Some(frames), _) => {
                let chunks = frames.clone().into_iter().map(Ok::<_, std::io::Error>);
                let (g, s) = DumpStream::from_bytes(futures::stream::iter(chunks));
                (g, s, None)
            }
            (BenchSource::Http { src, max_games }, _, Some(client)) => {
                let (g, s) = open_dump_with(client, src, &RetryPolicy::default()).await?;
                (g, s, Some(*max_games))
            }
            _ => unreachable!("prepared above"),
        };
        let mut stats = run_pipeline(&mut games, &bench_filter(max), &sink, &cfg).await?;
        sink.flush_rest().await?;
        stats.bytes_compressed = sha.bytes_read();
        rows.push(row(cfg.threads, &stats));
    }
    Ok(rows)
}

fn stage(r: &BenchRow, k: &str) -> String {
    r.stage_ms
        .get(k)
        .map_or_else(|| "-".to_owned(), |v| format!("{v:.0}"))
}

/// The rows as a Markdown table.
pub fn markdown_table(rows: &[BenchRow]) -> String {
    let mut out = String::from(
        "| threads | games | s | games/s | compressed MB/s | PGN MB/s | read ms | parse ms | aggregate ms | flush ms | max stage |\n\
         |---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|\n",
    );
    for r in rows {
        out.push_str(&format!(
            "| {} | {} | {:.2} | {:.0} | {:.1} | {:.1} | {} | {} | {} | {} | {} |\n",
            r.threads,
            r.games,
            r.elapsed_s,
            r.games_per_sec,
            r.mb_per_sec,
            r.pgn_mb_per_sec,
            stage(r, "read"),
            stage(r, "parse"),
            stage(r, "aggregate"),
            stage(r, "flush"),
            r.bottleneck
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_games_have_distinct_sites() {
        let games: Vec<String> = synthetic_games(12).collect();
        let ids: std::collections::BTreeSet<String> = games
            .iter()
            .map(|g| {
                crate::headers::parse_dump_headers(g)
                    .unwrap()
                    .lichess_game_id
            })
            .collect();
        assert_eq!(ids.len(), 12);
        assert_eq!(split_games(games.concat().lines()).len(), 12);
    }

    #[tokio::test]
    async fn synthetic_bench_counts_every_game() {
        let rows = bench(&BenchSource::Synthetic { games: 500 }, &[1, 2])
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.games == 500 && r.games_per_sec > 0.0));
        assert_eq!(rows[1].threads, 2);
        let table = markdown_table(&rows);
        assert_eq!(table.lines().count(), 4);
        assert!(["read", "parse", "aggregate", "flush"].contains(&rows[0].bottleneck.as_str()));
    }
}
