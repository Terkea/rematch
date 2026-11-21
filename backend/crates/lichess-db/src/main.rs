//! `lichess-db`: operator CLI for the Lichess database pipeline (R-11, ADR-R2-1, ADR-R2-9).
//!
//! Subcommands are added by R2 Tasks 19-22 (`scan`, `build`, `bench`, `syzygy`).

use clap::{Args, Parser, Subcommand};
use futures::StreamExt;
use lichess_db::config::{DEFAULT_BASE_URL, DumpSource, data_dir, http_client, is_valid_month};
use lichess_db::download::{drain_to_end, fetch_expected_sha, open_dump};
use lichess_db::sample::{SampleFilter, Sampler};
use lichess_db::syzygy::{self, DEFAULT_SYZYGY_BASE_URL, Manifest, fetch_syzygy};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(
    name = "lichess-db",
    version,
    about = "Lichess database pipeline (operator CLI)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the crate version.
    Version,
    /// Stream one monthly dump without storing it: count and sample games, then verify the
    /// whole-file SHA-256 against sha256sums.txt (the body is always read to the end).
    Scan(ScanArgs),
    /// Fetch the Syzygy 3-4-5 tablebases (about 1 GB), verified against the pinned manifest.
    Syzygy(SyzygyArgs),
}

#[derive(Debug, Args)]
struct SyzygyArgs {
    /// Destination directory (default: $DATA_DIR/syzygy, DATA_DIR defaulting to ./data).
    #[arg(long)]
    dest: Option<std::path::PathBuf>,
    /// Mirror root (manifest names carry the 3-4-5-wdl/ and 3-4-5-dtz/ directories).
    #[arg(long, default_value = DEFAULT_SYZYGY_BASE_URL)]
    base_url: String,
}

async fn syzygy(args: SyzygyArgs) -> anyhow::Result<()> {
    let client = http_client()?;
    let dest = args.dest.unwrap_or_else(|| data_dir().join("syzygy"));
    let s = fetch_syzygy(&client, &args.base_url, &Manifest::pinned(), &dest).await?;
    println!(
        "syzygy: {} files downloaded ({} bytes), {} already present, in {}",
        s.files,
        s.bytes,
        s.skipped,
        dest.display()
    );
    Ok(())
}

#[derive(Debug, Args)]
struct ScanArgs {
    /// Dump month, YYYY-MM.
    #[arg(long, value_parser = parse_month)]
    month: String,
    /// Mirror root.
    #[arg(long, default_value = DEFAULT_BASE_URL)]
    base_url: String,
    /// Stop parsing after this many kept games (the rest is still hashed).
    #[arg(long)]
    max_games: Option<u64>,
}

fn parse_month(s: &str) -> Result<String, String> {
    if is_valid_month(s) {
        Ok(s.to_owned())
    } else {
        Err(format!("expected YYYY-MM, got {s:?}"))
    }
}

const PROGRESS_EVERY: u64 = 1_000_000;

async fn scan(args: ScanArgs) -> anyhow::Result<()> {
    let client = syzygy::http_client()?;
    let src = DumpSource::new(args.base_url, args.month);
    let expected = fetch_expected_sha(&client, &src).await?;
    let (mut games, sha) = open_dump(&client, &src).await?;
    let mut sampler = Sampler::new(SampleFilter {
        max_games: args.max_games,
        ..SampleFilter::default()
    });
    while !sampler.is_full() {
        let Some(game) = games.next().await else {
            break;
        };
        sampler.offer(game?);
        let st = sampler.stats();
        if st.seen.is_multiple_of(PROGRESS_EVERY) {
            tracing::info!(
                seen = st.seen,
                kept = st.kept,
                compressed_bytes = sha.bytes_read(),
                "scanning"
            );
        }
    }
    let drained = drain_to_end(games).await?;
    let st = sampler.stats();
    println!("month: {}", src.month);
    println!("games seen: {}", st.seen);
    println!("games kept: {}", st.kept);
    println!("malformed: {}", st.malformed);
    println!(
        "compressed bytes: {} ({drained} drained unparsed)",
        sha.bytes_read()
    );
    let digest = sha.verify(&expected)?;
    println!("sha256: {digest} (verified)");
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    match Cli::parse().command {
        Command::Version => println!("lichess-db {}", env!("CARGO_PKG_VERSION")),
        Command::Scan(args) => scan(args).await?,
        Command::Syzygy(args) => syzygy(args).await?,
    }
    Ok(())
}
