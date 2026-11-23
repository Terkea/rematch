//! `cli lab analyse|profile|rank` (ADR 0106): the lab bridge. Each reads one JSON request on stdin
//! and writes one JSON response on stdout (`lab/schemas/`); logs go to stderr. `profile` and `rank`
//! are pure functions of their input. `analyse` needs `DATABASE_URL` (a throwaway database is
//! created next to it and dropped) and `STOCKFISH_PATH`. Exit codes: 0 done, 1 usage, input or
//! run error.

use std::io::Read;
use std::path::PathBuf;

use bench::lab::{AnalyseOptions, AnalyseRequest, ProfileRequest, RankRequest};

pub const LAB_USAGE: &str = "usage: cli lab analyse|profile|rank < request.json > response.json";

/// The id of the algorithm that answers. Until the manifest lands (ADR 0104, lane
/// `feat/algorithm-manifest`), the bridge reports `unversioned`; after it, this returns
/// `common::algorithm::id()`.
pub fn algorithm_id() -> &'static str {
    "unversioned"
}

/// Pretty JSON with a final newline: the same response is the same bytes.
pub fn to_output<T: serde::Serialize>(v: &T) -> anyhow::Result<String> {
    let mut s = serde_json::to_string_pretty(v)?;
    s.push('\n');
    Ok(s)
}

/// The request; serde_json's message names the line and column of a bad field.
fn parse<T: serde::de::DeserializeOwned>(input: &str) -> anyhow::Result<T> {
    Ok(serde_json::from_str(input)?)
}

/// `profile` or `rank` on a request string (no I/O): what the tests and the golden file use.
pub fn run_pure(cmd: &str, input: &str) -> anyhow::Result<String> {
    match cmd {
        "profile" => to_output(&bench::lab::profile(
            &parse::<ProfileRequest>(input)?,
            algorithm_id(),
        )?),
        "rank" => to_output(&bench::lab::rank(
            &parse::<RankRequest>(input)?,
            algorithm_id(),
        )?),
        _ => anyhow::bail!("{LAB_USAGE}"),
    }
}

pub async fn run_lab(
    argv: &[String],
    env: &dyn Fn(&str) -> Option<String>,
    print: &dyn Fn(&str),
    print_error: &dyn Fn(&str),
) -> i32 {
    let cmd = match argv {
        [lab, cmd] if lab == "lab" => cmd.as_str(),
        _ => {
            print_error(LAB_USAGE);
            return 1;
        }
    };
    if !matches!(cmd, "analyse" | "profile" | "rank") {
        print_error(LAB_USAGE);
        return 1;
    }
    let mut input = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut input) {
        print_error(&format!("lab: reading stdin: {e}"));
        return 1;
    }
    let out = if cmd == "analyse" {
        let (Some(database_url), Some(stockfish)) = (env("DATABASE_URL"), env("STOCKFISH_PATH"))
        else {
            print_error("DATABASE_URL and STOCKFISH_PATH must be set");
            return 1;
        };
        let opts = AnalyseOptions {
            database_url,
            stockfish: PathBuf::from(stockfish),
        };
        match parse::<AnalyseRequest>(&input) {
            Ok(req) => bench::lab::analyse(
                &req,
                &opts,
                algorithm_id(),
                Box::new(|l| eprintln!("lab: {l}")),
            )
            .await
            .and_then(|r| to_output(&r)),
            Err(e) => Err(e),
        }
    } else {
        run_pure(cmd, &input)
    };
    match out {
        Ok(s) => {
            print(s.trim_end_matches('\n'));
            0
        }
        Err(e) => {
            print_error(&format!("lab {cmd}: {e:#}"));
            1
        }
    }
}
