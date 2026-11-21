//! Graceful shutdown (port of `apps/worker/src/lifecycle/shutdown.ts`).
//!
//! Within `grace` (25 s; compose gives the container 30 s before SIGKILL):
//! 1. every runner stops claiming, and in-flight jobs drain for up to 60 % of `grace`;
//! 2. if they have not, the engine pool is closed: in-flight analyses fail with `PoolClosed`,
//!    put their game back to `pending` and release their job without using an attempt; the
//!    runners drain again until 90 %;
//! 3. runners still busy are forced: their handlers are cancelled and their jobs released
//!    (`release_owned`), again without using an attempt;
//! 4. the engines (if not closed yet) and the database pool are closed.
//!
//! Every step is bounded by the remaining budget (at least [`MIN_STEP`]); failures and timeouts
//! are logged and make the exit code 1. A runner that had to be forced is not a failure: it is
//! the designed path for a job longer than the grace.

use std::sync::Arc;
use std::time::Duration;

use engine::pool::EnginePool;
use futures::FutureExt;
use futures::future::{BoxFuture, join_all};
use queue::runner::RunnerHandle;
use sqlx::PgPool;
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

/// The default total budget from the signal to the exit.
pub const DEFAULT_GRACE: Duration = Duration::from_secs(25);
/// The smallest budget any one step gets, even past the grace.
pub const MIN_STEP: Duration = Duration::from_millis(100);

/// A job runner that can be drained.
pub trait Drainable: Send + Sync {
    /// Stop claiming new jobs (idempotent).
    fn stop(&self);
    /// Resolves once every in-flight job has finished.
    fn wait_idle(&self) -> BoxFuture<'_, ()>;
    /// Cancel in-flight jobs and hand them back to the queue.
    fn force(&self) -> BoxFuture<'_, ()>;
}

impl Drainable for RunnerHandle {
    fn stop(&self) {
        RunnerHandle::stop(self);
    }

    fn wait_idle(&self) -> BoxFuture<'_, ()> {
        RunnerHandle::wait_idle(self).boxed()
    }

    fn force(&self) -> BoxFuture<'_, ()> {
        RunnerHandle::force(self).boxed()
    }
}

/// A resource closed once at shutdown.
pub trait Closable: Send + Sync {
    fn close(&self) -> BoxFuture<'_, anyhow::Result<()>>;
}

impl Closable for Arc<EnginePool> {
    fn close(&self) -> BoxFuture<'_, anyhow::Result<()>> {
        async move {
            EnginePool::close(self).await;
            Ok(())
        }
        .boxed()
    }
}

impl Closable for PgPool {
    fn close(&self) -> BoxFuture<'_, anyhow::Result<()>> {
        async move {
            PgPool::close(self).await;
            Ok(())
        }
        .boxed()
    }
}

pub struct ShutdownHandles {
    pub runners: Vec<Box<dyn Drainable>>,
    pub engines: Box<dyn Closable>,
    pub db: Box<dyn Closable>,
}

#[derive(Clone, Copy, Debug)]
pub struct ShutdownOptions {
    /// Total budget from the signal to the exit (default 25 s).
    pub grace: Duration,
}

impl Default for ShutdownOptions {
    fn default() -> Self {
        ShutdownOptions {
            grace: DEFAULT_GRACE,
        }
    }
}

/// Runs the shutdown (see the module docs) and returns the exit code: 0, or 1 if any close
/// failed or timed out.
pub async fn shutdown(handles: ShutdownHandles, opts: ShutdownOptions) -> i32 {
    let started = Instant::now();
    let grace = opts.grace;
    let budget = |fraction: f64| {
        (started + grace.mul_f64(fraction))
            .saturating_duration_since(Instant::now())
            .max(MIN_STEP)
    };
    let mut failed = false;

    for runner in &handles.runners {
        runner.stop();
    }
    // One drain future for both waits, so each runner's `wait_idle` starts once.
    let mut draining = join_all(handles.runners.iter().map(|r| r.wait_idle())).boxed();
    let drained = tokio::time::timeout(budget(0.6), &mut draining)
        .await
        .is_ok();
    let mut engines_closed = false;
    if !drained {
        tracing::warn!(
            grace_ms = grace.as_millis() as u64,
            "runners did not drain; closing the engines to abort in-flight analyses"
        );
        engines_closed = true;
        failed |= !close("engines", handles.engines.as_ref(), budget(0.9)).await;
        let settled = tokio::time::timeout(budget(0.9), &mut draining)
            .await
            .is_ok();
        if !settled {
            tracing::warn!("forcing the runners; their jobs are released without an attempt");
            let forced = join_all(handles.runners.iter().map(|r| r.force()));
            if tokio::time::timeout(budget(1.0), forced).await.is_err() {
                failed = true;
                tracing::error!(resource = "runners", "force timed out");
            }
        }
    }
    drop(draining);
    if !engines_closed {
        failed |= !close("engines", handles.engines.as_ref(), budget(1.0)).await;
    }
    failed |= !close("db", handles.db.as_ref(), budget(1.0)).await;
    tracing::info!(
        ms = started.elapsed().as_millis() as u64,
        ok = !failed,
        "shutdown complete"
    );
    i32::from(failed)
}

/// Closes one resource within `budget`; `false` (logged) if it failed or timed out.
async fn close(name: &'static str, resource: &dyn Closable, budget: Duration) -> bool {
    match tokio::time::timeout(budget, resource.close()).await {
        Ok(Ok(())) => true,
        Ok(Err(e)) => {
            tracing::error!(resource = name, error = %format!("{e:#}"), "close failed");
            false
        }
        Err(_) => {
            tracing::error!(
                resource = name,
                budget_ms = budget.as_millis() as u64,
                "close timed out"
            );
            false
        }
    }
}

/// Called once with the exit code when the shutdown is over.
pub type Exit = Box<dyn FnOnce(i32) + Send>;

struct Request {
    reason: String,
    exit_code: i32,
}

/// Starts the shutdown from code, for example when something fails after startup.
#[derive(Clone)]
pub struct ShutdownTrigger {
    tx: mpsc::UnboundedSender<Request>,
    code: watch::Receiver<Option<i32>>,
}

impl ShutdownTrigger {
    /// Runs the shutdown once (later calls and signals are logged and ignored); the process
    /// exits with `max(shutdown code, exit_code)`.
    pub fn trigger(&self, reason: &str, exit_code: i32) {
        let _ = self.tx.send(Request {
            reason: reason.to_owned(),
            exit_code,
        });
    }

    /// Resolves with the exit code once the shutdown has finished (just before `exit` runs).
    pub async fn wait(&self) -> i32 {
        let mut rx = self.code.clone();
        match rx.wait_for(Option::is_some).await {
            Ok(code) => code.unwrap_or(1),
            Err(_) => 1,
        }
    }
}

/// On SIGTERM or SIGINT, runs [`shutdown`] once and exits the process with its code. Later
/// signals are logged with warn and ignored. `main` should call [`signals`] before startup and
/// pass it to [`install_with_signals`] instead, so a signal during startup is not lost.
pub fn install_graceful_shutdown(
    handles: ShutdownHandles,
    opts: ShutdownOptions,
) -> ShutdownTrigger {
    install_with_signals(handles, opts, signals(), process_exit())
}

/// Exits the process.
pub fn process_exit() -> Exit {
    Box::new(|code| std::process::exit(code))
}

/// Registers SIGTERM and SIGINT handlers now (must run inside the tokio runtime) and forwards
/// each signal by name. Signals that arrive before anyone reads the channel are kept, so this can
/// be called before a slow startup: as PID 1 in a container, a signal without a handler would be
/// ignored and the container killed at the end of its stop grace.
#[cfg(unix)]
pub fn signals() -> mpsc::UnboundedReceiver<&'static str> {
    use tokio::signal::unix::{SignalKind, signal};
    let (tx, rx) = mpsc::unbounded_channel();
    let (mut term, mut int) = match (
        signal(SignalKind::terminate()),
        signal(SignalKind::interrupt()),
    ) {
        (Ok(t), Ok(i)) => (t, i),
        (Err(e), _) | (_, Err(e)) => {
            tracing::error!(error = %e, "cannot install signal handlers");
            return rx;
        }
    };
    tokio::spawn(async move {
        loop {
            let name = tokio::select! {
                Some(()) = term.recv() => "SIGTERM",
                Some(()) = int.recv() => "SIGINT",
                else => return,
            };
            if tx.send(name).is_err() {
                return;
            }
        }
    });
    rx
}

#[cfg(not(unix))]
pub fn signals() -> mpsc::UnboundedReceiver<&'static str> {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while tokio::signal::ctrl_c().await.is_ok() {
            if tx.send("SIGINT").is_err() {
                return;
            }
        }
    });
    rx
}

/// [`install_graceful_shutdown`] with injected signals (by name) and exit, for tests.
pub fn install_with_signals(
    handles: ShutdownHandles,
    opts: ShutdownOptions,
    mut signals: mpsc::UnboundedReceiver<&'static str>,
    exit: Exit,
) -> ShutdownTrigger {
    let (tx, mut requests) = mpsc::unbounded_channel::<Request>();
    let (code_tx, code_rx) = watch::channel(None);
    tokio::spawn(async move {
        let mut handles = Some((handles, exit));
        loop {
            let (reason, exit_code) = tokio::select! {
                Some(name) = signals.recv() => (name.to_owned(), 0),
                Some(req) = requests.recv() => (req.reason, req.exit_code),
                else => return,
            };
            let Some((h, exit)) = handles.take() else {
                tracing::warn!(reason = %reason, "shutdown already in progress");
                continue;
            };
            tracing::info!(reason = %reason, "shutting down");
            let code_tx = code_tx.clone();
            tokio::spawn(async move {
                let code = shutdown(h, opts).await.max(exit_code);
                let _ = code_tx.send(Some(code));
                exit(code);
            });
        }
    });
    ShutdownTrigger { tx, code: code_rx }
}
