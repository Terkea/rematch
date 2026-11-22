//! The analysis worker: `from_env`, `start_workers`, then wait for SIGTERM or SIGINT.

use worker::config::WorkerEnv;
use worker::shutdown::{ShutdownOptions, install_with_signals, process_exit, signals};
use worker::start::{StartOptions, start_workers};

#[tokio::main]
async fn main() {
    worker::logging::init();
    // Before the (possibly slow) startup: a SIGTERM meanwhile is handled once the worker runs.
    let signals = signals();
    let env = match WorkerEnv::from_env() {
        Ok(env) => env,
        Err(e) => {
            tracing::error!(error = %format!("{e:#}"), "invalid environment");
            eprintln!("worker: {e:#}");
            std::process::exit(1);
        }
    };
    // `start_workers` closes whatever it created if it fails partway.
    let running = match start_workers(&env, StartOptions::default()).await {
        Ok(running) => running,
        Err(e) => {
            tracing::error!(error = %format!("{e:#}"), "worker failed to start");
            eprintln!("worker: {e:#}");
            std::process::exit(1);
        }
    };
    let trigger = install_with_signals(
        running.into_shutdown_handles(),
        ShutdownOptions::default(),
        signals,
        process_exit(),
    );
    // The shutdown task exits the process; this only returns if it could not.
    let code = trigger.wait().await;
    std::process::exit(code);
}
