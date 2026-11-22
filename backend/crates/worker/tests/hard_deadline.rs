//! The shutdown's hard deadline (`grace + hard_after`), on tokio's paused clock: no real sleeps,
//! every timer fires at an exact virtual instant.

use std::time::Duration;

use futures::FutureExt;
use futures::future::BoxFuture;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;
use worker::shutdown::{
    Closable, DEFAULT_HARD_AFTER, Drainable, ShutdownHandles, ShutdownOptions,
    install_with_signals, shutdown,
};

/// Never drains, and its `force` ignores the budget it is given.
struct Wedged;

impl Drainable for Wedged {
    fn stop(&self) {}

    fn wait_idle(&self) -> BoxFuture<'_, ()> {
        futures::future::pending().boxed()
    }

    fn force(&self, _budget: Duration) -> BoxFuture<'_, ()> {
        futures::future::pending().boxed()
    }
}

/// A close that never returns.
struct Hangs;

impl Closable for Hangs {
    fn close(&self) -> BoxFuture<'_, anyhow::Result<()>> {
        futures::future::pending().boxed()
    }
}

fn wedged() -> ShutdownHandles {
    ShutdownHandles {
        runners: vec![Box::new(Wedged), Box::new(Wedged)],
        engines: Box::new(Hangs),
        db: Box::new(Hangs),
    }
}

#[test]
fn the_default_deadline_ends_before_compose_kills_the_container() {
    let opts = ShutdownOptions::default();
    assert_eq!(opts.grace, Duration::from_secs(25));
    assert_eq!(opts.hard_after, DEFAULT_HARD_AFTER);
    // compose: `stop_grace_period: 30s`.
    assert!(opts.grace + opts.hard_after < Duration::from_secs(30));
    assert_eq!(
        ShutdownOptions::with_grace(Duration::from_secs(2)).hard_after,
        DEFAULT_HARD_AFTER
    );
}

/// Each step is bounded on its own but takes at least `MIN_STEP` (100 ms), so with a wedged
/// runner and hanging closes the steps would run to about grace + 300 ms. The hard deadline cuts
/// them off at exactly grace + hard_after and reports a failure.
#[tokio::test(start_paused = true)]
async fn the_hard_deadline_ends_a_shutdown_whose_steps_overrun_the_grace() {
    let opts = ShutdownOptions {
        grace: Duration::from_secs(1),
        hard_after: Duration::from_millis(50),
    };
    let started = Instant::now();
    let code = shutdown(wedged(), opts).await;
    assert_eq!(started.elapsed(), Duration::from_millis(1050));
    assert_eq!(code, 1);
}

/// With the production timings, even a fully wedged worker is done by grace + hard_after.
#[tokio::test(start_paused = true)]
async fn a_wedged_worker_is_done_by_the_default_deadline() {
    let opts = ShutdownOptions::default();
    let started = Instant::now();
    let code = shutdown(wedged(), opts).await;
    assert!(started.elapsed() >= opts.grace, "{:?}", started.elapsed());
    assert!(
        started.elapsed() <= opts.grace + opts.hard_after,
        "{:?}",
        started.elapsed()
    );
    assert_eq!(code, 1);
}

/// The signal path calls `exit(1)` at the hard deadline.
#[tokio::test(start_paused = true)]
async fn the_signal_path_exits_at_the_hard_deadline() {
    let opts = ShutdownOptions {
        grace: Duration::from_secs(4),
        hard_after: Duration::from_millis(10),
    };
    let (signals, rx) = mpsc::unbounded_channel();
    let (exit_tx, exit_rx) = oneshot::channel();
    let trigger = install_with_signals(
        wedged(),
        opts,
        rx,
        Box::new(move |code| {
            let _ = exit_tx.send((code, Instant::now()));
        }),
    );
    let started = Instant::now();
    signals.send("SIGTERM").unwrap();
    let (code, at) = exit_rx.await.unwrap();
    assert_eq!(code, 1);
    assert_eq!(at - started, Duration::from_millis(4010));
    assert_eq!(trigger.wait().await, 1);
}
