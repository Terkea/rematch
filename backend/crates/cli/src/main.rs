//! The `cli` entry point: `cli analyse ...` starts the workers in-process (no reconciler);

//! `cli db migrate` only migrates.

use std::time::Duration;

use cli::{AnalyseDeps, CliDeps, Signal, Started, db_cmd, run_cli};
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
