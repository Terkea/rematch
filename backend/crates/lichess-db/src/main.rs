//! `lichess-db`: operator CLI for the Lichess database pipeline (R-11, ADR-R2-1, ADR-R2-9).
//!
//! Subcommands from R2 Tasks 19-22: `scan`, `build`, `bench`, `syzygy`; Task 35: `resume`;
//! EXP-0009 (ADR 0114): `cohort`.

use clap::{Args, Parser, Subcommand};
use futures::StreamExt;
use lichess_db::bench::{BenchSource, bench, markdown_table};
use lichess_db::config::{DEFAULT_BASE_URL, DumpSource, data_dir, http_client, is_valid_month};
use lichess_db::download::{drain_to_end, fetch_expected_sha, open_dump};
use lichess_db::explorer_build::{BuildOptions, Verify, build};
use lichess_db::pipeline::PipelineConfig;
use lichess_db::sample::{SampleFilter, Sampler};
use lichess_db::syzygy::{DEFAULT_SYZYGY_BASE_URL, Manifest, fetch_syzygy};
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
    /// Build the opening explorer from one monthly dump (DATABASE_URL): sample across the whole
    /// month, verify the SHA-256, prune, then activate.
    Build(BuildArgs),
    /// Measure pipeline throughput (no database). Prints a Markdown table.
    Bench(BenchArgs),
    /// Fetch the Syzygy 3-4-5 tablebases (about 1 GB), verified against the pinned manifest.
    Syzygy(SyzygyArgs),
    /// Re-enqueue the reference-game jobs of verified `building` band-reference builds after a
    /// crash between the row insert and the last job batch (idempotent; DATABASE_URL).
    Resume(ResumeArgs),
    /// Follow the same anonymised players across monthly dumps (EXP-0009, ADR 0114): stream one
    /// month, verify its SHA-256, and write the followed players' rapid summary (and, with
    /// --games, their rapid games with Lichess's evaluations) to --out. No database.
    Cohort(CohortArgs),
}

#[derive(Debug, Args)]
struct CohortArgs {
    /// Dump month, YYYY-MM.
    #[arg(long, value_parser = parse_month)]
    month: String,
    /// Mirror root.
    #[arg(long, default_value = DEFAULT_BASE_URL)]
    base_url: String,
    /// 64 hex digits; the same salt every month follows the same players.
    #[arg(long)]
    salt: String,
    /// Follow a side when the fraction of its hash is below this, in [0, 1].
    #[arg(long, default_value_t = 1.0)]
    player_rate: f64,
    /// Also write games.jsonl: the followed players' rapid games with [%eval].
    #[arg(long)]
    games: bool,
    /// Output directory (created); the files appear only after the SHA-256 is verified.
    #[arg(long)]
    out: std::path::PathBuf,
}

async fn run_cohort(args: CohortArgs) -> anyhow::Result<()> {
    use lichess_db::cohort::{CohortSink, write_line};
    use std::io::{BufWriter, Write};
    let salt = lichess_db::reference_select::parse_salt(&args.salt).map_err(anyhow::Error::msg)?;
    anyhow::ensure!(
        (0.0..=1.0).contains(&args.player_rate),
        "--player-rate is in [0, 1]"
    );
    std::fs::create_dir_all(&args.out)?;
    let client = http_client()?;
    let src = DumpSource::new(args.base_url, args.month);
    let expected = fetch_expected_sha(&client, &src).await?;
    let (mut games, sha) = open_dump(&client, &src).await?;
    let mut sink = CohortSink::new(salt, args.player_rate);
    let part = args.out.join("games.jsonl.part");
    let mut out = if args.games {
        Some(BufWriter::new(std::fs::File::create(&part)?))
    } else {
        None
    };
    let mut written = 0u64;
    while let Some(game) = games.next().await {
        if let Some(line) = sink.offer(game?)
            && let Some(w) = out.as_mut()
        {
            write_line(w, &line)?;
            written += 1;
        }
        let st = sink.stats();
        if st.seen.is_multiple_of(PROGRESS_EVERY) {
            tracing::info!(
                seen = st.seen,
                kept = st.kept,
                eval_games = written,
                compressed_bytes = sha.bytes_read(),
                "cohort"
            );
        }
    }
    let drained = drain_to_end(games).await?;
    let compressed = sha.bytes_read();
    let digest = sha.verify(&expected)?;
    if let Some(mut w) = out {
        w.flush()?;
        std::fs::rename(&part, args.out.join("games.jsonl"))?;
    }
    let mut w = BufWriter::new(std::fs::File::create(args.out.join("players.jsonl"))?);
    let players = sink.players();
    for p in &players {
        write_line(&mut w, p)?;
    }
    w.flush()?;
    let st = sink.stats();
    let meta = serde_json::json!({
        "month": src.month,
        "url": src.url(),
        "sha256": digest,
        "compressed_bytes": compressed,
        "drained_unparsed_bytes": drained,
        "games_seen": st.seen,
        "games_kept": st.kept,
        "malformed": st.malformed,
        "player_rate": args.player_rate,
        "salt": args.salt.trim(),
        "players": players.len(),
        "eval_games": written,
    });
    std::fs::write(
        args.out.join("meta.json"),
        format!(
            "{}
",
            serde_json::to_string_pretty(&meta)?
        ),
    )?;
    println!("{meta}");
    Ok(())
}

#[derive(Debug, Args)]
struct ResumeArgs {
    /// One build (default: every verified building band-reference build).
    #[arg(long)]
    build: Option<uuid::Uuid>,
}

async fn run_resume(args: ResumeArgs) -> anyhow::Result<()> {
    let url =
        std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL is required"))?;
    let pool = db::pool::connect(&url, db::pool::DbOptions::default()).await?;
    let queue = queue::queue::Queue::new(pool.clone());
    let resumed = match args.build {
        Some(id) => vec![(
            id,
            lichess_db::reference_select::resume_reference_enqueue(&pool, &queue, id).await?,
        )],
        None => lichess_db::reference_select::resume_all_reference_enqueues(&pool, &queue).await?,
    };
    if resumed.is_empty() {
        println!("no verified building band-reference build");
    }
    for (id, added) in resumed {
        println!("build {id}: {added} reference-game jobs added");
    }
    pool.close().await;
    Ok(())
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
        "syzygy: {} files downloaded ({} bytes), {} already present (rehash {:.1?}), in {}",
        s.files,
        s.bytes,
        s.skipped,
        s.rehash,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum VerifyArg {
    Full,
    None,
}

#[derive(Debug, Args)]
struct BuildArgs {
    /// Dump month, YYYY-MM.
    #[arg(long, value_parser = parse_month)]
    month: String,
    /// Mirror root.
    #[arg(long, default_value = DEFAULT_BASE_URL)]
    base_url: String,
    /// Target sample size and safety cap (ADR 0035: 2 000 000).
    #[arg(long, default_value_t = 2_000_000)]
    max_games: u64,
    /// Games expected in the month (default: the last full build's count, else 100M).
    #[arg(long)]
    expected_games: Option<u64>,
    /// Fixed sample rate in (0, 1] instead of max_games / expected games.
    #[arg(long)]
    sample_rate: Option<f64>,
    /// Build the opening explorer.
    #[arg(long)]
    explorer: bool,
    /// Build the band reference (R2 Task 21).
    #[arg(long)]
    band_reference: bool,
    /// Reference selection salt, 64 hex digits (default: a fresh random salt, recorded on the
    /// build). To reproduce a stored build, pass its `params.salt` together with
    /// `--expected-games <params.expected_games>`: the player rate depends on both.
    #[arg(long, value_parser = lichess_db::reference_select::parse_salt)]
    salt: Option<[u8; 32]>,
    #[arg(long, value_enum, default_value_t = VerifyArg::Full)]
    verify: VerifyArg,
    /// Parse threads (0 = all cores).
    #[arg(long, default_value_t = 0)]
    threads: usize,
}

async fn run_build(args: BuildArgs) -> anyhow::Result<()> {
    let url =
        std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL is required"))?;
    // Prune and activation touch millions of rows: no 30 s statement timeout here.
    let pool = db::pool::connect(
        &url,
        db::pool::DbOptions {
            max_connections: 4,
            statement_timeout: std::time::Duration::from_secs(6 * 3600),
            ..db::pool::DbOptions::default()
        },
    )
    .await?;
    let client = http_client()?;
    let src = DumpSource::new(args.base_url, args.month.clone());
    let opts = BuildOptions {
        max_games: args.max_games,
        expected_games: args.expected_games,
        sample_rate: args.sample_rate,
        explorer: args.explorer || !args.band_reference,
        band_reference: args.band_reference,
        reference_salt: args.salt,
        verify: match args.verify {
            VerifyArg::Full => Verify::Full,
            VerifyArg::None => Verify::None,
        },
        pipeline: PipelineConfig::with_threads(args.threads),
        ..BuildOptions::new(args.month)
    };
    let out = build(&pool, &client, &src, &opts).await?;
    let s = &out.stats;
    println!("explorer build: {:?}", out.explorer_build);
    println!(
        "reference build: {:?} ({} games enqueued)",
        out.reference_build, out.reference_games
    );
    println!("sample rate: {}", out.sample_rate);
    println!(
        "games seen: {}, kept: {}, malformed: {}, capped: {}",
        s.games_seen, s.games_kept, s.games_malformed, s.capped
    );
    println!(
        "explorer: {} games, {} illegal, {} flushes, {} rows flushed",
        out.explorer.games, out.explorer.illegal, out.explorer.flushes, out.explorer.rows_flushed
    );
    println!(
        "compressed bytes: {}, elapsed: {:.1?}",
        s.bytes_compressed, s.elapsed
    );
    for (k, v) in &s.stage_ms {
        println!("  {k}: {v:.0} ms");
    }
    Ok(())
}

#[derive(Debug, Args)]
struct BenchArgs {
    /// Synthetic games (no network).
    #[arg(long, conflicts_with = "month")]
    synthetic: Option<u64>,
    /// Real dump month, YYYY-MM (streams the first --max-games games).
    #[arg(long, value_parser = parse_month)]
    month: Option<String>,
    #[arg(long, default_value_t = 1_000_000)]
    max_games: u64,
    #[arg(long, default_value = DEFAULT_BASE_URL)]
    base_url: String,
    /// Thread counts, comma-separated (0 = all cores).
    #[arg(long, value_delimiter = ',', default_value = "1,4,0")]
    threads: Vec<usize>,
}

/// The host CPU model (Linux `/proc/cpuinfo`), if readable.
fn cpu_model() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|t| {
            t.lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split_once(':'))
                .map(|(_, v)| v.trim().to_owned())
        })
        .unwrap_or_else(|| "unknown".to_owned())
}

async fn run_bench(args: BenchArgs) -> anyhow::Result<()> {
    let source = match (args.synthetic, args.month) {
        (Some(games), None) => BenchSource::Synthetic { games },
        (None, Some(month)) => BenchSource::Http {
            src: DumpSource::new(args.base_url, month),
            max_games: args.max_games,
        },
        _ => anyhow::bail!("pass --synthetic N or --month M"),
    };
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    println!("host: {} ({cores} logical cores visible)", cpu_model());
    println!("source: {source:?}");
    println!(
        "build: {}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    println!();
    let rows = bench(&source, &args.threads).await?;
    print!("{}", markdown_table(&rows));
    Ok(())
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
    let client = http_client()?;
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
        Command::Build(args) => run_build(args).await?,
        Command::Bench(args) => run_bench(args).await?,
        Command::Resume(args) => run_resume(args).await?,
        Command::Cohort(args) => run_cohort(args).await?,
    }
    Ok(())
}
