//! Parity harness: compares TS and Rust worker output on the same games (Task 24).
//!
//! - `parity seed --database-url URL [--enqueue] [--phase K] [--fixtures DIR]`
//! - `parity phases [--fixtures DIR]`: the number of seed phases (see `seed::seed_phase`)
//! - `parity enqueue-pending --database-url URL`
//! - `parity compare --a URL --b URL`: exit 0 on parity, otherwise every mismatch and exit 1.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use db::pool::{DbOptions, connect};
use parity::compare::{diff, load};
use parity::seed::{enqueue_pending, phase_count, seed, seed_phase};

#[derive(Parser)]
#[command(name = "parity")]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Insert the fixture set, every game pending since an hour ago.
    Seed {
        #[arg(long)]
        database_url: String,
        /// Also enqueue a Rust analyse job per game.
        #[arg(long)]
        enqueue: bool,
        /// Only this phase (0-based) of the fixture set.
        #[arg(long)]
        phase: Option<usize>,
        #[arg(long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures"))]
        fixtures: PathBuf,
    },
    /// Print the number of seed phases.
    Phases {
        #[arg(long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures"))]
        fixtures: PathBuf,
    },
    /// Enqueue a Rust analyse job for every pending game.
    EnqueuePending {
        #[arg(long)]
        database_url: String,
    },
    /// Compare two analysed databases.
    Compare {
        #[arg(long)]
        a: String,
        #[arg(long)]
        b: String,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Args::parse()).await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("parity: {e:#}");
            ExitCode::from(2)
        }
    }
}

async fn run(args: Args) -> anyhow::Result<ExitCode> {
    match args.cmd {
        Cmd::Seed {
            database_url,
            enqueue,
            phase,
            fixtures,
        } => {
            let pool = connect(&database_url, DbOptions::default()).await?;
            let n = match phase {
                Some(k) => seed_phase(&pool, &fixtures, enqueue, k).await?,
                None => seed(&pool, &fixtures, enqueue).await?,
            };
            println!(
                "parity: seeded {n} games{}",
                if enqueue { ", enqueued" } else { "" }
            );
        }
        Cmd::Phases { fixtures } => println!("{}", phase_count(&fixtures)?),
        Cmd::EnqueuePending { database_url } => {
            let pool = connect(&database_url, DbOptions::default()).await?;
            let n = enqueue_pending(&pool).await?;
            println!("parity: enqueued {n} analyse jobs");
        }
        Cmd::Compare { a, b } => {
            let a = load(&connect(&a, DbOptions::default()).await?).await?;
            let b = load(&connect(&b, DbOptions::default()).await?).await?;
            let report = diff(&a, &b);
            for m in &report.mismatches {
                println!("{m}");
            }
            println!("{}", report.summary());
            if !report.mismatches.is_empty() {
                return Ok(ExitCode::from(1));
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}
