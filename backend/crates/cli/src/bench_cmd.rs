//! `cli bench run` (ADR 0100): the Tier 1 pipeline benchmark. It needs `DATABASE_URL` (a
//! throwaway database is created next to it and dropped afterwards) and `STOCKFISH_PATH`.
//! Writes `bench-results.json` and `bench-scorecard.md` into `--out` (default `.`). With
//! `--baseline`, exits 2 when a score regressed beyond the baseline's tolerance. `--suite-file`
//! runs a suite read from JSON (the `bench::suite::Suite` shape) instead of a named one, for the
//! lab's experiments (ADR 0106). `--record FILE` appends the run's KPIs (ADR 0119) as a
//! `run-record-v1` to the `bench-history-v1` JSON at FILE, creating it when missing.
//!
//! `cli bench compare A.json B.json [--json]` differences two `bench-results.json` files metric
//! by metric (B minus A), with paired bootstrap intervals when both ran the same players on the
//! same seeds, and a verdict read from each interval.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bench::kpi::{self, Kpi};
use bench::results::{Baseline, regressions};
use bench::run::{RunOptions, RunOutput, run};
use bench::suite::Suite;
use clap::{Arg, ArgAction, Command};
use serde::{Deserialize, Serialize};

pub const BENCH_USAGE: &str = "usage: cli bench run [--suite smoke|full | --suite-file FILE] [--json] [--out DIR] [--baseline FILE] [--record FILE] [--engines N] [--nodes N] [--keep-db]\n       cli bench compare A.json B.json [--json]";

/// The bench history's schemas (`site/scripts/kpi-charts.mjs` reads them).
pub const HISTORY_SCHEMA: &str = "bench-history-v1";
pub const RUN_SCHEMA: &str = "run-record-v1";

/// One run of the bench history: the leaderboard's `{value, interval, level}` per metric.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    pub schema: String,
    pub suite: String,
    /// `YYYY-MM-DD`, UTC.
    pub date: String,
    pub algorithm_id: String,
    /// The git commit of the build, when known (`GITHUB_SHA`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// The suite's players and seeds, digested: two runs with the same value ran the same players.
    pub seeds: String,
    /// `synthetic`: Tier 1 players.
    pub tier: String,
    pub engines: usize,
    pub nodes: u64,
    pub metrics: BTreeMap<String, Kpi>,
}

/// FNV-1a 64 of the suite's `name=seed;` list, with its name: `smoke:0123456789abcdef`.
pub fn seeds_digest(s: &Suite) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in &s.players {
        for b in format!("{}={};", p.name, p.seed).bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{}:{h:016x}", s.name)
}

/// The record of a finished run.
pub fn record(
    out: &RunOutput,
    algorithm_id: &str,
    date: time::Date,
    commit: Option<String>,
    engines: usize,
) -> RunRecord {
    RunRecord {
        schema: RUN_SCHEMA.to_owned(),
        suite: out.suite.name.clone(),
        date: format!(
            "{:04}-{:02}-{:02}",
            date.year(),
            u8::from(date.month()),
            date.day()
        ),
        algorithm_id: algorithm_id.to_owned(),
        commit: commit.filter(|c| !c.trim().is_empty()),
        seeds: seeds_digest(&out.suite),
        tier: "synthetic".to_owned(),
        engines,
        nodes: out.suite.nodes,
        metrics: out.scorecard.kpis.clone(),
    }
}

/// Appends `rec` to the history at `path` (a new history when the file is missing). The runs
/// already there are kept as they are.
pub fn append_record(path: &Path, rec: &RunRecord) -> Result<(), String> {
    let mut h: serde_json::Value = match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| format!("{}: {e}", path.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            serde_json::json!({ "schema": HISTORY_SCHEMA, "runs": [] })
        }
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    if h.get("schema").and_then(|v| v.as_str()) != Some(HISTORY_SCHEMA) {
        return Err(format!("{}: not a {HISTORY_SCHEMA}", path.display()));
    }
    let runs = h
        .get_mut("runs")
        .and_then(|r| r.as_array_mut())
        .ok_or_else(|| format!("{}: runs is not a list", path.display()))?;
    runs.push(serde_json::to_value(rec).map_err(|e| e.to_string())?);
    let mut text = serde_json::to_string_pretty(&h).map_err(|e| e.to_string())?;
    text.push('\n');
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Whether two runs drew the same players on the same seeds (so a comparison can pair them).
pub fn same_players(a: &Suite, b: &Suite) -> bool {
    fn key(s: &Suite) -> Vec<&bench::suite::PlayerDef> {
        let mut v: Vec<&bench::suite::PlayerDef> = s.players.iter().collect();
        v.sort_by(|x, y| x.name.cmp(&y.name));
        v
    }
    key(a) == key(b) && a.checkpoints == b.checkpoints
}

/// The comparison of two `bench-results.json` runs, B minus A.
pub fn compare_runs(a: &RunOutput, b: &RunOutput) -> kpi::Comparison {
    let paired = same_players(&a.suite, &b.suite);
    kpi::compare(
        &bench::results::units(&a.players),
        &bench::results::units(&b.players),
        paired,
        kpi::BOOTSTRAP,
        kpi::SEED,
    )
}

fn read_run(path: &str) -> Result<RunOutput, String> {
    std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
        .map_err(|e| format!("reading {path}: {e}"))
}

/// `cli bench compare A.json B.json [--json]`. Exit codes: 0 done, 1 usage or input error.
pub fn run_compare(argv: &[String], print: &dyn Fn(&str), print_error: &dyn Fn(&str)) -> i32 {
    let json = argv.iter().any(|a| a == "--json");
    let pos: Vec<&String> = argv.iter().filter(|a| *a != "--json").collect();
    let [_, _, a, b] = pos.as_slice() else {
        print_error(BENCH_USAGE);
        return 1;
    };
    if a.starts_with("--") || b.starts_with("--") {
        print_error(BENCH_USAGE);
        return 1;
    }
    let (ra, rb) = match (read_run(a), read_run(b)) {
        (Ok(x), Ok(y)) => (x, y),
        (Err(e), _) | (_, Err(e)) => {
            print_error(&e);
            return 1;
        }
    };
    let c = compare_runs(&ra, &rb);
    if json {
        match serde_json::to_string_pretty(&c) {
            Ok(j) => print(&j),
            Err(e) => {
                print_error(&e.to_string());
                return 1;
            }
        }
    } else {
        print(&kpi::compare_markdown(&c, a, b));
    }
    0
}

fn command() -> Command {
    Command::new("cli")
        .no_binary_name(true)
        .disable_help_flag(true)
        .disable_version_flag(true)
        .disable_help_subcommand(true)
        .arg(Arg::new("pos").num_args(0..))
        .arg(Arg::new("suite").long("suite"))
        .arg(Arg::new("suite-file").long("suite-file"))
        .arg(Arg::new("json").long("json").action(ArgAction::SetTrue))
        .arg(Arg::new("out").long("out"))
        .arg(Arg::new("baseline").long("baseline"))
        .arg(Arg::new("record").long("record"))
        .arg(Arg::new("engines").long("engines"))
        .arg(Arg::new("nodes").long("nodes"))
        .arg(
            Arg::new("keep-db")
                .long("keep-db")
                .action(ArgAction::SetTrue),
        )
}

#[derive(Debug, PartialEq)]
pub struct BenchArgs {
    pub suite: String,
    /// A suite read from this JSON file instead of the named `suite`.
    pub suite_file: Option<PathBuf>,
    pub json: bool,
    pub out: PathBuf,
    pub baseline: Option<PathBuf>,
    /// Append the run's KPIs to this bench history (`bench-history-v1`).
    pub record: Option<PathBuf>,
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
            _ => Err(format!(
                "--{name} must be a positive integer\n{BENCH_USAGE}"
            )),
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
    let suite_file = m.get_one::<String>("suite-file").map(PathBuf::from);
    if suite_file.is_some() && m.get_one::<String>("suite").is_some() {
        return Err(format!(
            "--suite and --suite-file cannot be used together\n{BENCH_USAGE}"
        ));
    }
    Ok(BenchArgs {
        suite,
        suite_file,
        json: m.get_flag("json"),
        out: PathBuf::from(m.get_one::<String>("out").map_or(".", String::as_str)),
        baseline: m.get_one::<String>("baseline").map(PathBuf::from),
        record: m.get_one::<String>("record").map(PathBuf::from),
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
    if argv.get(1).map(String::as_str) == Some("compare") {
        return run_compare(argv, print, print_error);
    }
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
    let suite = match &args.suite_file {
        None => bench::suite::by_name(&args.suite),
        Some(p) => match std::fs::read_to_string(p)
            .map_err(anyhow::Error::from)
            .and_then(|s| bench::suite::from_json(&s))
        {
            Ok(s) => Some(s),
            Err(e) => {
                print_error(&format!("reading {}: {e:#}", p.display()));
                return 1;
            }
        },
    };
    let Some(mut suite) = suite else {
        return 1;
    };
    if let Some(n) = args.nodes {
        suite.nodes = n;
    }
    // `ALGO_VARIANT=<name>` runs that variant side by side, on the same games (EXP-0006).
    let variant = match bench::variant_from_env(env("ALGO_VARIANT")) {
        Ok(v) => v,
        Err(e) => {
            print_error(&format!("ALGO_VARIANT: {e}"));
            return 1;
        }
    };
    if let Some((name, _)) = &variant {
        eprintln!("bench: algorithm variant {name} side by side");
    }
    let opts = RunOptions {
        database_url,
        stockfish: PathBuf::from(stockfish),
        engines: args.engines,
        out_dir: args.out.clone(),
        keep_db: args.keep_db,
        variant,
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
    if let Some(path) = &args.record {
        let rec = record(
            &out,
            common::algorithm::id(),
            time::OffsetDateTime::now_utc().date(),
            env("GITHUB_SHA"),
            args.engines,
        );
        if let Err(e) = append_record(path, &rec) {
            print_error(&format!("bench: recording the run: {e}"));
            return 1;
        }
        eprintln!("bench: recorded the run in {}", path.display());
    }
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
        let r = parse(&argv("bench run --suite full --record /tmp/h.json")).unwrap();
        assert_eq!(r.record, Some(PathBuf::from("/tmp/h.json")));
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse(&argv("bench")).is_err());
        assert!(
            parse(&argv("bench run --suite huge"))
                .unwrap_err()
                .contains("smoke or full")
        );
        assert!(parse(&argv("bench run --engines 0")).is_err());
        assert!(
            parse(&argv("bench run --suite smoke --suite-file s.json"))
                .unwrap_err()
                .contains("together")
        );
    }

    #[test]
    fn parses_a_suite_file() {
        let a = parse(&argv("bench run --suite-file /tmp/s.json --json")).unwrap();
        assert_eq!(a.suite_file, Some(PathBuf::from("/tmp/s.json")));
        assert_eq!(a.suite, "smoke");
    }

    use bench::flaws::{Tally, Unit as FlawUnit};
    use bench::results::{Checkpoint, FlawInfo, GenStats, PlayerResult, RankedItem, score};

    fn player(name: &str, flaw: Option<&str>, rank: Option<u32>) -> PlayerResult {
        let cp = |games| Checkpoint {
            games,
            ranked: rank
                .map(|r| {
                    vec![RankedItem {
                        key: "W1".to_owned(),
                        rank: r,
                        priority: 1.0,
                        confidence: 0.9,
                        posterior_rate: 0.1,
                        interval_low: 0.05,
                        interval_high: 0.15,
                        band_median: 0.01,
                        events: 5,
                        opportunities: 50,
                        drills: vec!["safe_unsafe".to_owned()],
                    }]
                })
                .unwrap_or_default(),
            estimates: vec![],
            benchmark_players: Some(12),
        };
        PlayerResult {
            name: name.to_owned(),
            flaw: flaw.map(|id| FlawInfo {
                id: id.to_owned(),
                rate: 0.05,
                target_key: "W1".to_owned(),
                unit: FlawUnit::Move,
                expected_drills: vec!["safe_unsafe".to_owned()],
            }),
            tally: Tally::default(),
            generation: GenStats::default(),
            checkpoints: vec![cp(30), cp(80)],
            perturbed: Some(cp(80)),
            deterministic: true,
            variant: None,
        }
    }

    fn output(ranks: [Option<u32>; 4]) -> RunOutput {
        let mut suite = bench::suite::smoke();
        suite.players.truncate(4);
        let players: Vec<PlayerResult> = suite
            .players
            .iter()
            .zip(ranks)
            .map(|(p, r)| player(&p.name, p.flaw.as_ref().map(|f| f.0.as_str()), r))
            .collect();
        RunOutput {
            scorecard: score(&suite.name, suite.nodes, &players),
            suite,
            engine_searches: 0,
            memo_hits: 0,
            seconds: 1.0,
            players,
            variant: None,
        }
    }

    #[test]
    fn a_record_appends_to_a_bench_history_the_site_reads() {
        let dir = std::env::temp_dir().join(format!("bench-history-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.json");
        let _ = std::fs::remove_file(&path);
        let out = output([Some(1), Some(1), None, Some(2)]);
        let date = time::Date::from_calendar_date(2026, time::Month::November, 28).unwrap();
        let rec = record(&out, "algo-6.0.0+f40a2b27677a", date, Some("abc".into()), 4);
        assert_eq!(rec.date, "2026-11-28");
        assert_eq!(rec.tier, "synthetic");
        assert!(rec.seeds.starts_with("smoke:"));
        append_record(&path, &rec).unwrap();
        append_record(&path, &rec).unwrap();
        let h: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(h["schema"], HISTORY_SCHEMA);
        let runs = h["runs"].as_array().unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0]["schema"], RUN_SCHEMA);
        let m = &runs[0]["metrics"];
        for name in [
            "recall_at_2",
            "recall_at_2_macro",
            "km_median_games",
            "top2_jaccard",
            "clean_false_weakness",
            "flaw_types_detectable",
            "deterministic",
        ] {
            assert!(m.get(name).is_some(), "{name} missing from {m}");
            assert!(m[name].get("level").is_some(), "{name} has no level");
            assert!(m[name].get("n").is_some(), "{name} has no n");
        }
        assert_eq!(m["deterministic"]["level"], "exact");
        // The record round-trips.
        let back: RunRecord = serde_json::from_value(runs[1].clone()).unwrap();
        assert_eq!(back, rec);
        // A file that is not a history is refused, not overwritten.
        std::fs::write(&path, "{\"schema\": \"other\"}").unwrap();
        assert!(append_record(&path, &rec).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn compare_pairs_runs_on_the_same_seeds_and_reads_the_interval() {
        let a = output([Some(1), Some(1), None, None]);
        let b = output([Some(1), Some(1), Some(1), None]);
        // Through JSON, as `cli bench compare` reads bench-results.json.
        let a: RunOutput = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        let c = compare_runs(&a, &b);
        assert!(c.paired);
        let same = compare_runs(&a, &a);
        let r = same
            .metrics
            .iter()
            .find(|d| d.name == "recall_at_2")
            .unwrap();
        assert_eq!(r.verdict, "no clear difference");
        let r = c.metrics.iter().find(|d| d.name == "recall_at_2").unwrap();
        assert!(r.difference.unwrap() > 0.0);
        let mut other = b.clone();
        other.suite.players[0].seed += 1;
        assert!(!compare_runs(&a, &other).paired);
        let printed = std::cell::RefCell::new(String::new());
        let dir = std::env::temp_dir().join(format!("bench-compare-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (pa, pb) = (dir.join("a.json"), dir.join("b.json"));
        std::fs::write(&pa, serde_json::to_string(&a).unwrap()).unwrap();
        std::fs::write(&pb, serde_json::to_string(&b).unwrap()).unwrap();
        let args = vec![
            "bench".to_owned(),
            "compare".to_owned(),
            pa.display().to_string(),
            pb.display().to_string(),
            "--json".to_owned(),
        ];
        let code = run_compare(&args, &|l| printed.borrow_mut().push_str(l), &|_| {});
        assert_eq!(code, 0);
        let j: serde_json::Value = serde_json::from_str(&printed.borrow()).unwrap();
        assert_eq!(j["schema"], "bench-compare-v1");
        assert_eq!(j["paired"], true);
        assert_eq!(
            run_compare(&argv("bench compare only-one.json"), &|_| {}, &|_| {}),
            1
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
