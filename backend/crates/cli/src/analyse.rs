//! `cli analyse`: import a player's games, wait for the workers to analyse them, print a summary.
//! A port of `apps/worker/src/cli.ts`.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use clap::{Arg, Command};
use db::import_runs::{ImportRunRow, ImportRunStatus, list_runs};
use futures::FutureExt;
use futures::future::BoxFuture;
use queue::kinds::{KIND_ANALYSE, KIND_IMPORT, KIND_SCHEDULE};
use queue::queue::Queue;
use sqlx::PgPool;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::format::to_fixed;

pub const USAGE: &str = "usage: cli analyse --platform <lichess|chesscom> --username <name> [--max N] [--timeout MINUTES]";

const DEFAULT_TIMEOUT_MIN: f64 = 30.0;
const PLATFORMS: [&str; 2] = ["lichess", "chesscom"];
/// Statuses this command does not wait for. `imported` games were not selected by the scheduler
/// (ADR-R2-16): they wait for a later schedule run, not for this one.
const TERMINAL: [&str; 4] = ["done", "failed", "skipped", "imported"];

pub type Printer = Box<dyn Fn(&str) + Send + Sync>;
pub type EnqueueImport =
    Box<dyn Fn(Uuid, Option<u32>) -> BoxFuture<'static, anyhow::Result<()>> + Send + Sync>;
pub type QueuesIdle = Box<dyn Fn(Uuid) -> BoxFuture<'static, anyhow::Result<bool>> + Send + Sync>;
/// The job kinds the command waits for. The schedule-analysis job sits between the import and
/// the analyses (ADR-R2-16): the import enqueues it before its own job ends, and it enqueues the
/// analyses before its own job ends, so one of these is always live until the analyses finish.
pub const WAIT_KINDS: [&str; 3] = [KIND_IMPORT, KIND_SCHEDULE, KIND_ANALYSE];

/// [`AnalyseDeps::queues_idle`] on a real queue: no job of [`WAIT_KINDS`] of the account is
/// queued, running or paused.
pub fn queues_idle_on(queue: Queue) -> QueuesIdle {
    Box::new(move |account| {
        let queue = queue.clone();
        async move { Ok(queue.live_count(&WAIT_KINDS, Some(account)).await? == 0) }.boxed()
    })
}

pub type ListRuns =
    Box<dyn Fn(Uuid) -> BoxFuture<'static, anyhow::Result<Vec<ImportRunRow>>> + Send + Sync>;

pub struct AnalyseDeps {
    pub pool: PgPool,
    pub enqueue_import: EnqueueImport,
    /// True when no import, schedule-analysis or analysis job of the account is queued, running
    /// or paused ([`queues_idle_on`]).
    pub queues_idle: QueuesIdle,
    pub print: Printer,
    pub print_error: Printer,
    /// DB polling interval (1 s in production).
    pub poll: Duration,
    /// Idle-queue grace before giving up on unfinished games (60 s in production).
    pub grace: Duration,
    /// Cancelled by `run_cli` on SIGINT or SIGTERM.
    pub cancel: CancellationToken,
    /// Test seam: the account's import runs (default: `db::import_runs::list_runs`).
    pub list_runs: Option<ListRuns>,
}

#[derive(Debug, PartialEq)]
struct Parsed {
    platform: String,
    username: String,
    max: Option<u32>,
    timeout: Duration,
}

fn command() -> Command {
    Command::new("cli")
        .no_binary_name(true)
        .disable_help_flag(true)
        .disable_version_flag(true)
        .disable_help_subcommand(true)
        .arg(Arg::new("pos").num_args(0..))
        .arg(
            Arg::new("platform")
                .long("platform")
                .allow_hyphen_values(true),
        )
        .arg(
            Arg::new("username")
                .long("username")
                .allow_hyphen_values(true),
        )
        .arg(Arg::new("max").long("max").allow_hyphen_values(true))
        .arg(
            Arg::new("timeout")
                .long("timeout")
                .allow_hyphen_values(true),
        )
}

/// A positive finite number, like the TS `Number(v)` check (`Number("")` is 0, so it fails).
fn positive(v: Option<&String>, name: &str) -> Result<Option<f64>, String> {
    let Some(v) = v else { return Ok(None) };
    match v.trim().parse::<f64>() {
        Ok(n) if n.is_finite() && n > 0.0 => Ok(Some(n)),
        _ => Err(format!("--{name} must be a positive number")),
    }
}

/// `Err` carries the full message to print (with the usage line where the TS prints it).
fn parse(argv: &[String]) -> Result<Parsed, String> {
    let m = command()
        .try_get_matches_from(argv)
        .map_err(|e| format!("{}\n{USAGE}", e.render().to_string().trim_end()))?;
    let first = m
        .get_many::<String>("pos")
        .and_then(|mut p| p.next())
        .map(String::as_str);
    if first != Some("analyse") {
        return Err(USAGE.to_owned());
    }
    let platform = match m.get_one::<String>("platform").map(String::as_str) {
        Some(p) if PLATFORMS.contains(&p) => p.to_owned(),
        _ => return Err(format!("--platform must be lichess or chesscom\n{USAGE}")),
    };
    let username = m
        .get_one::<String>("username")
        .map(|u| u.trim().to_owned())
        .unwrap_or_default();
    if username.is_empty() {
        return Err(format!("--username is required\n{USAGE}"));
    }
    let max = positive(m.get_one::<String>("max"), "max")?;
    let timeout = positive(m.get_one::<String>("timeout"), "timeout")?;
    Ok(Parsed {
        platform,
        username,
        // `as` saturates; the value is positive and finite.
        max: max.map(|n| n.floor() as u32),
        timeout: Duration::try_from_secs_f64(timeout.unwrap_or(DEFAULT_TIMEOUT_MIN) * 60.0)
            .unwrap_or(Duration::from_secs(u64::from(u32::MAX))),
    })
}

#[derive(Debug, Default, Clone, Copy)]
struct Counts {
    pending: i64,
    analysing: i64,
    done: i64,
    failed: i64,
    skipped: i64,
}

impl Counts {
    fn total(&self) -> i64 {
        self.pending + self.analysing + self.done + self.failed + self.skipped
    }
    fn settled(&self) -> i64 {
        self.done + self.failed + self.skipped
    }
}

async fn status_counts(pool: &PgPool, account_id: Uuid) -> anyhow::Result<Counts> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT analysis_status::text, count(*) FROM games WHERE account_id = $1 GROUP BY 1",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;
    let mut c = Counts::default();
    for (status, n) in rows {
        match status.as_str() {
            "pending" => c.pending = n,
            "analysing" => c.analysing = n,
            "done" => c.done = n,
            "failed" => c.failed = n,
            "skipped" => c.skipped = n,
            _ => {}
        }
    }
    Ok(c)
}

struct Summary {
    counts: Counts,
    line: String,
}

async fn summarise(pool: &PgPool, account_id: Uuid) -> anyhow::Result<Summary> {
    let counts = status_counts(pool, account_id).await?;
    let avg: Option<f64> = sqlx::query_scalar(
        "SELECT avg(CASE WHEN player_color = 'white' THEN white_accuracy ELSE black_accuracy END)\
         ::float8 FROM games WHERE account_id = $1 AND analysis_status = 'done'",
    )
    .bind(account_id)
    .fetch_one(pool)
    .await?;
    let blunders: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM moves m JOIN games g ON m.game_id = g.id \
         WHERE g.account_id = $1 AND g.analysis_status = 'done' \
           AND m.judgement = 'blunder' AND m.color = g.player_color",
    )
    .bind(account_id)
    .fetch_one(pool)
    .await?;
    let per_game = if counts.done > 0 {
        blunders as f64 / counts.done as f64
    } else {
        0.0
    };
    let line = format!(
        "games: {}  analysed: {}  skipped: {}  failed: {}  avg accuracy: {}  blunders/game: {}",
        counts.total(),
        counts.done,
        counts.skipped,
        counts.failed,
        to_fixed(avg.unwrap_or(0.0), 1),
        to_fixed(per_game, 2),
    );
    Ok(Summary { counts, line })
}

async fn non_terminal_ids(pool: &PgPool, account_id: Uuid) -> anyhow::Result<Vec<Uuid>> {
    let rows: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, analysis_status::text FROM games WHERE account_id = $1")
            .bind(account_id)
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .filter(|(_, s)| !TERMINAL.contains(&s.as_str()))
        .map(|(id, _)| id)
        .collect())
}

/// The newest run this invocation owns: a run that did not exist before it, or one whose status
/// changed since (the import job reuses a paused run, T21).
fn newest_followed<'a>(
    runs: &'a [ImportRunRow],
    known: &HashMap<Uuid, ImportRunStatus>,
) -> Option<&'a ImportRunRow> {
    runs.iter()
        .filter(|r| known.get(&r.id) != Some(&r.status))
        .max_by_key(|r| (r.started_at, r.id))
}

/// Runs one `analyse` command against already-running workers; returns the exit code.
pub async fn run_analyse(argv: &[String], deps: &AnalyseDeps) -> i32 {
    let args = match parse(argv) {
        Ok(a) => a,
        Err(message) => {
            (deps.print_error)(&message);
            return 1;
        }
    };
    match analyse(&args, deps).await {
        Ok(code) => code,
        Err(e) => {
            (deps.print_error)(&format!("{e:#}"));
            1
        }
    }
}

async fn analyse(args: &Parsed, deps: &AnalyseDeps) -> anyhow::Result<i32> {
    let pool = &deps.pool;
    let account =
        db::accounts::find_or_create_account(pool, &args.platform, &args.username).await?;
    let id = account.id;
    let runs_now = || async {
        match &deps.list_runs {
            Some(f) => f(id).await,
            None => Ok(list_runs(pool, id).await?),
        }
    };
    let known: HashMap<Uuid, ImportRunStatus> = runs_now()
        .await?
        .into_iter()
        .map(|r| (r.id, r.status))
        .collect();
    // Games left pending/analysing by an earlier killed run. Only the reconciler re-enqueues
    // those, and it does not run in this process, so they must not stall this invocation.
    let stale: HashSet<Uuid> = non_terminal_ids(pool, id).await?.into_iter().collect();
    (deps.enqueue_import)(id, args.max).await?;

    let deadline = Instant::now() + args.timeout;
    let mut last_progress = status_counts(pool, id).await?.done / 10 * 10;
    let mut idle_since: Option<Instant> = None;
    loop {
        if deps.cancel.is_cancelled() {
            (deps.print_error)("interrupted");
            return Ok(1);
        }
        let runs = runs_now().await?;
        let run = newest_followed(&runs, &known);
        if let Some(r) = run
            && r.status == ImportRunStatus::Failed
            && r.error_code.as_deref() == Some("not_found")
        {
            (deps.print_error)(&format!(
                "User \"{}\" not found on {}",
                args.username, args.platform
            ));
            return Ok(1);
        }
        let counts = status_counts(pool, id).await?;
        let (total, settled) = (counts.total(), counts.settled());
        while counts.done >= last_progress + 10 {
            last_progress += 10;
            (deps.print)(&format!("analysed {last_progress}/{total} games"));
        }
        if (deps.queues_idle)(id).await? {
            match run.map(|r| r.status) {
                Some(ImportRunStatus::Done) => {
                    let open = non_terminal_ids(pool, id).await?;
                    let in_scope = open.iter().filter(|g| !stale.contains(g)).count();
                    let since = *idle_since.get_or_insert_with(Instant::now);
                    if in_scope == 0 {
                        if !open.is_empty() {
                            (deps.print_error)(&format!(
                                "warning: {} game(s) left over from an earlier run are still pending/analysing and were ignored",
                                open.len()
                            ));
                        }
                        break;
                    }
                    if since.elapsed() >= deps.grace {
                        (deps.print_error)(&format!(
                            "warning: queues are idle but {in_scope} game(s) are not finished; giving up ({settled}/{total} settled)"
                        ));
                        break;
                    }
                }
                Some(ImportRunStatus::Failed) | None => {
                    let reason = run
                        .and_then(|r| r.last_error.as_deref())
                        .unwrap_or("no import run completed");
                    (deps.print_error)(&format!("import failed: {reason}"));
                    return Ok(1);
                }
                // A paused or running import with no live job: keep waiting (the timeout ends it).
                Some(ImportRunStatus::Paused | ImportRunStatus::Running) => idle_since = None,
            }
        } else {
            idle_since = None;
        }
        if Instant::now() >= deadline {
            (deps.print_error)(&format!(
                "timed out after {} min ({settled}/{total} games settled)",
                (args.timeout.as_secs_f64() / 60.0).round()
            ));
            return Ok(1);
        }
        tokio::select! {
            () = deps.cancel.cancelled() => {}
            () = tokio::time::sleep(deps.poll) => {}
        }
    }
    let s = summarise(pool, id).await?;
    if s.counts.total() == 0 {
        (deps.print)(&format!(
            "no standard games found for {} on {}",
            args.username, args.platform
        ));
        return Ok(0);
    }
    (deps.print)(&s.line);
    Ok(i32::from(s.counts.failed == s.counts.total()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Int,
    Term,
}

impl Signal {
    fn name(self) -> &'static str {
        match self {
            Signal::Int => "SIGINT",
            Signal::Term => "SIGTERM",
        }
    }
    fn code(self) -> i32 {
        match self {
            Signal::Int => 130,
            Signal::Term => 143,
        }
    }
}

/// What `start` produces: the analyse dependencies (its `cancel` is replaced by `run_cli`'s) and
/// the shutdown path, which returns its exit code (0 or 1).
pub struct Started {
    pub analyse: AnalyseDeps,
    pub close: BoxFuture<'static, i32>,
}

pub struct CliDeps {
    pub start: BoxFuture<'static, anyhow::Result<Started>>,
    pub signals: mpsc::Receiver<Signal>,
    /// Used for a second signal: exits immediately.
    pub exit: Box<dyn Fn(i32) + Send + Sync>,
    pub print_error: Printer,
}

/// Runs the CLI: SIGINT or SIGTERM cancel the wait, the workers are shut down, and 128+n is
/// returned. A second signal exits immediately.
pub async fn run_cli(argv: &[String], deps: CliDeps) -> i32 {
    let CliDeps {
        start,
        mut signals,
        exit,
        print_error,
    } = deps;
    let cancel = CancellationToken::new();
    let mut signal_code: Option<i32> = None;
    let mut on_signal = |sig: Signal| {
        if signal_code.is_some() {
            print_error("second signal received; exiting immediately");
            exit(sig.code());
            return;
        }
        signal_code = Some(sig.code());
        print_error(&format!("{} received; shutting down", sig.name()));
        cancel.cancel();
    };
    let work = async {
        let Started { mut analyse, close } = match start.await {
            Ok(s) => s,
            Err(e) => {
                print_error(&format!("{e:#}"));
                return 1;
            }
        };
        analyse.cancel = cancel.clone();
        let code = run_analyse(argv, &analyse).await;
        let close_code = close.await;
        if code == 0 && close_code == 1 {
            1
        } else {
            code
        }
    };
    tokio::pin!(work);
    let code = loop {
        tokio::select! {
            biased;
            Some(sig) = signals.recv() => on_signal(sig),
            code = &mut work => break code,
        }
    };
    // Signals that arrived while the last step ran (a second signal during shutdown).
    while let Ok(sig) = signals.try_recv() {
        on_signal(sig);
    }
    signal_code.unwrap_or(code)
}
