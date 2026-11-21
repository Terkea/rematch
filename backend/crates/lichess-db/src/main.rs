//! `lichess-db`: operator CLI for the Lichess database pipeline (R-11, ADR-R2-1).
//!
//! Subcommands are added by R2 Tasks 19-22 (`scan`, `build`, `bench`, `syzygy`).

use clap::{Parser, Subcommand};

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
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Version => println!("lichess-db {}", env!("CARGO_PKG_VERSION")),
    }
    Ok(())
}
