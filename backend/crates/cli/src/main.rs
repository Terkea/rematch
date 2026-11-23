//! The `cli` entry point: `cli analyse ...` starts the workers in-process (no reconciler);
//! `cli db migrate` only migrates; `cli profile`, `cli eta` and `cli queue-lag` only read;
//! `cli recompute-metrics` rewrites stale metrics from the stored evals (no engine, no worker);
//! `cli report-rebuild` queues a report run for a running worker (ADR 0086); `cli bench run`
//! runs the pipeline benchmark (ADR 0100); `cli algo` shows and compares algorithm manifests
//! (ADR 0105).

use std::time::Duration;

use cli::{
    AnalyseDeps, CliDeps, Signal, Started, db_cmd, profile_cmd, queue_cmd, recompute_cmd,
    report_cmd, run_cli,
};
use futures::FutureExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use worker::config::WorkerEnv;
use worker::shutdown::{ShutdownOptions, shutdown, signals};
use worker::start::{StartOptions, start_workers};

#[tokio::main]
async fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().map(String::as_str) == Some("db") {
        let code = db_cmd::run_db(&argv, &|l| println!("{l}"), &|l| eprintln!("{l}")).await;
        std::process::exit(code);
    }
    // The pipeline benchmark (ADR 0100): its own throwaway database and engines, no workers.
    if argv.first().map(String::as_str) == Some("bench") {
        let code = cli::bench_cmd::run_bench(
            &argv,
            &|k| std::env::var(k).ok(),
            &|l| println!("{l}"),
            &|l| eprintln!("{l}"),
        )
        .await;
        std::process::exit(code);
    }
    // The algorithm manifest (ADR 0105): the database only for an id argument.
    if argv.first().map(String::as_str) == Some("algo") {
        let pool = if cli::algo_cmd::needs_db(&argv) {
            match std::env::var("DATABASE_URL") {
                Ok(url) => db::pool::connect(&url, db::pool::DbOptions::default())
                    .await
                    .ok(),
                Err(_) => None,
            }
        } else {
            None
        };
        let code = cli::algo_cmd::run_algo(&argv, pool.as_ref(), &|l| println!("{l}"), &|l| {
            eprintln!("{l}")
        })
        .await;
        std::process::exit(code);
    }
    // Read-only views (R2 Task 35) and the metrics recompute: the database only, no workers.
    if let Some(cmd @ ("profile" | "eta" | "queue-lag" | "recompute-metrics" | "report-rebuild")) =
        argv.first().map(String::as_str)
    {
        let code = read_only(cmd, &argv).await;
        std::process::exit(code);
    }
    let mut raw = signals(); // before the slow startup
    let (tx, rx) = mpsc::channel(8);
    tokio::spawn(async move {
        while let Some(name) = raw.recv().await {
            let sig = if name == "SIGTERM" {
                Signal::Term
            } else {
                Signal::Int
            };
            if tx.send(sig).await.is_err() {
                return;
            }
        }
    });
    let start = async {
        let env = WorkerEnv::from_env()?;
        let running = start_workers(
            &env,
            StartOptions {
                with_reconciler: false,
                ai: worker::ai_jobs::AiSetup::None,
                // Benchmark knob: R1's analysis without the profile hook (ADR-R2-25).
                profile_hook: std::env::var("CLI_ANALYSIS_ONLY").as_deref() != Ok("1"),
                ..StartOptions::default()
            },
        )
        .await?;
        let (pool, queue) = (running.pool.clone(), running.queue.clone());
        let q_enqueue = queue.clone();
        let analyse = AnalyseDeps {
            pool,
            enqueue_import: Box::new(move |account, max| {
                let queue = q_enqueue.clone();
                async move {
                    // false = an import of this account is already live; wait for it.
                    analysis::jobs::import::enqueue_import(&queue, account, max).await?;
                    Ok(())
                }
                .boxed()
            }),
            queues_idle: cli::analyse::queues_idle_on(queue),
            print: Box::new(|l| println!("{l}")),
            print_error: Box::new(|l| eprintln!("{l}")),
            poll: Duration::from_secs(1),
            grace: Duration::from_secs(60),
            cancel: CancellationToken::new(),
            list_runs: None,
        };
        let close = async move {
            shutdown(
                running.into_shutdown_handles(),
                ShutdownOptions::with_grace(Duration::from_secs(25)),
            )
            .await
        }
        .boxed();
        anyhow::Ok(Started { analyse, close })
    }
    .boxed();
    let code = run_cli(
        &argv,
        CliDeps {
            start,
            signals: rx,
            exit: Box::new(|c| std::process::exit(c)),
            print_error: Box::new(|l| eprintln!("{l}")),
        },
    )
    .await;
    std::process::exit(code);
}

async fn read_only(cmd: &str, argv: &[String]) -> i32 {
    let print = |l: &str| println!("{l}");
    let print_error = |l: &str| eprintln!("{l}");
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("DATABASE_URL is not set");
        return 1;
    };
    let pool = match db::pool::connect(&url, db::pool::DbOptions::default()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("connecting to DATABASE_URL: {e:#}");
            return 1;
        }
    };
    let code = match cmd {
        "profile" => {
            profile_cmd::run_profile(
                argv,
                &pool,
                time::OffsetDateTime::now_utc(),
                &print,
                &print_error,
            )
            .await
        }
        "eta" => profile_cmd::run_eta(argv, &pool, &print, &print_error).await,
        "recompute-metrics" => {
            recompute_cmd::run_recompute_metrics(argv, &pool, &print, &print_error).await
        }
        // The deployment's AI plan (AI_DEFAULT_PLAN) decides which report gate applies.
        "report-rebuild" => match WorkerEnv::from_env() {
            Ok(env) => {
                report_cmd::run_report_rebuild(argv, &pool, env.ai_plan, &print, &print_error).await
            }
            Err(e) => {
                eprintln!("{e:#}");
                1
            }
        },
        _ => queue_cmd::run_queue_lag(argv, &pool, &print, &print_error).await,
    };
    pool.close().await;
    code
}
