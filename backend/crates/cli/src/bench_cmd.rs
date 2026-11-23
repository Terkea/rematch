//! `cli bench run` (ADR 0098): the Tier 1 pipeline benchmark. It needs `DATABASE_URL` (a
//! throwaway database is created next to it and dropped afterwards) and `STOCKFISH_PATH`.
//! Writes `bench-results.json` and `bench-scorecard.md` into `--out` (default `.`). With
//! `--baseline`, exits 2 when a score regressed beyond the baseline's tolerance.

use std::path::PathBuf;

use bench::results::{Baseline, regressions};
use bench::run::{RunOptions, run};
use clap::{Arg, ArgAction, Command};

pub const BENCH_USAGE: &str = "usage: cli bench run [--suite smoke|full] [--json] [--out DIR] [--baseline FILE] [--engines N] [--nodes N] [--keep-db]";

fn command() -> Command {
    Command::new("cli")
        .no_binary_name(true)
        .disable_help_flag(true)
        .disable_version_flag(true)
        .disable_help_subcommand(true)
        .arg(Arg::new("pos").num_args(0..))
        .arg(Arg::new("suite").long("suite"))
        .arg(Arg::new("json").long("json").action(ArgAction::SetTrue))
        .arg(Arg::new("out").long("out"))
        .arg(Arg::new("baseline").long("baseline"))
        .arg(Arg::new("engines").long("engines"))
        .arg(Arg::new("nodes").long("nodes"))
        .arg(Arg::new("keep-db").long("keep-db").action(ArgAction::SetTrue))
}

#[derive(Debug, PartialEq)]
pub struct BenchArgs {
    pub suite: String,
    pub json: bool,
    pub out: PathBuf,
    pub baseline: Option<PathBuf>,
    pub engines: usize,
    pub nodes: Option<u64>,
    pub keep_db: bool,
}

fn positive<T: std::str::FromStr + PartialOrd + Default>(
    v: Option<&String>,
    name: &str,
) -> Result<Option<T>, String> {
    match v {
        None => Ok(None),
        Some(s) => match s.trim().parse::<T>() {
            Ok(n) if n > T::default() => Ok(Some(n)),
            _ => Err(format!("--{name} must be a positive integer\n{BENCH_USAGE}")),
        },
    }
}

pub fn parse(argv: &[String]) -> Result<BenchArgs, String> {
    let m = command()
        .try_get_matches_from(argv)
        .map_err(|e| format!("{}\n{BENCH_USAGE}", e.render().to_string().trim_end()))?;
    let pos: Vec<&str> = m
        .get_many::<String>("pos")
        .map(|p| p.map(String::as_str).collect())
        .unwrap_or_default();
    if pos != ["bench", "run"] {
        return Err(BENCH_USAGE.to_owned());
    }
    let suite = m
        .get_one::<String>("suite")
        .map_or("smoke", String::as_str)
        .to_owned();
    if bench::suite::by_name(&suite).is_none() {
        return Err(format!("--suite must be smoke or full\n{BENCH_USAGE}"));
    }
    Ok(BenchArgs {
        suite,
        json: m.get_flag("json"),
        out: PathBuf::from(m.get_one::<String>("out").map_or(".", String::as_str)),
        baseline: m.get_one::<String>("baseline").map(PathBuf::from),
        engines: positive(m.get_one::<String>("engines"), "engines")?.unwrap_or(4),
        nodes: positive(m.get_one::<String>("nodes"), "nodes")?,
        keep_db: m.get_flag("keep-db"),
    })
}

/// Runs the benchmark. Exit codes: 0 done (thresholds are reported, not enforced), 1 usage or
/// run error, 2 a regression against `--baseline`.
pub async fn run_bench(
    argv: &[String],
    env: &dyn Fn(&str) -> Option<String>,
    print: &dyn Fn(&str),
    print_error: &dyn Fn(&str),
) -> i32 {
    let args = match parse(argv) {
        Ok(a) => a,
        Err(m) => {
            print_error(&m);
            return 1;
        }
    };
    let (Some(database_url), Some(stockfish)) = (env("DATABASE_URL"), env("STOCKFISH_PATH")) else {
        print_error("DATABASE_URL and STOCKFISH_PATH must be set");
        return 1;
    };
    let baseline: Option<Baseline> = match &args.baseline {
        None => None,
        Some(p) => match std::fs::read_to_string(p)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
        {
            Ok(b) => Some(b),
            Err(e) => {
                print_error(&format!("reading {}: {e}", p.display()));
                return 1;
            }
        },
    };
    let Some(mut suite) = bench::suite::by_name(&args.suite) else {
        return 1;
    };
    if let Some(n) = args.nodes {
        suite.nodes = n;
    }
    let opts = RunOptions {
        database_url,
        stockfish: PathBuf::from(stockfish),
        engines: args.engines,
        out_dir: args.out.clone(),
        keep_db: args.keep_db,
    };
    let out = match run(&suite, &opts, Box::new(|l| eprintln!("bench: {l}"))).await {
        Ok(o) => o,
        Err(e) => {
            print_error(&format!("bench: {e:#}"));
            return 1;
        }
    };
    if args.json {
        match serde_json::to_string_pretty(&out) {
            Ok(j) => print(&j),
            Err(e) => {
                print_error(&e.to_string());
                return 1;
            }
        }
    } else {
        print(&bench::results::markdown(&out.scorecard));
    }
    eprintln!(
        "bench: {:.0}s, {} engine searches, {} served from memory; wrote {}",
        out.seconds,
        out.engine_searches,
        out.memo_hits,
        args.out.display()
    );
    if let Some(b) = baseline {
        let r = regressions(&b, &out.scorecard.metrics);
        if !r.is_empty() {
            for line in &r {
                print_error(&format!("regression: {line}"));
            }
            return 2;
        }
        eprintln!("bench: no regression against the baseline");
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn parses_defaults_and_flags() {
        let a = parse(&argv("bench run")).unwrap();
        assert_eq!(a.suite, "smoke");
        assert_eq!(a.engines, 4);
        assert!(!a.json);
        let a = parse(&argv(
            "bench run --suite full --json --out /tmp/x --baseline b.json --engines 2 --nodes 5000",
        ))
        .unwrap();
        assert_eq!(a.suite, "full");
        assert!(a.json);
        assert_eq!(a.out, PathBuf::from("/tmp/x"));
        assert_eq!(a.baseline, Some(PathBuf::from("b.json")));
        assert_eq!((a.engines, a.nodes), (2, Some(5000)));
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse(&argv("bench")).is_err());
        assert!(parse(&argv("bench run --suite huge")).unwrap_err().contains("smoke or full"));
        assert!(parse(&argv("bench run --engines 0")).is_err());
    }
}
