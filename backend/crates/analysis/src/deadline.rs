//! Job deadlines: a cancellation token that fires after a duration, optionally re-armed.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

/// Hard cap on one analysis attempt.
pub const ANALYSE_DEADLINE: Duration = Duration::from_secs(10 * 60);
/// An import that makes no progress (no game streamed) for this long is abandoned.
pub const IMPORT_IDLE_DEADLINE: Duration = Duration::from_secs(30 * 60);

/// A job overran its deadline.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
#[error("{kind} exceeded its {ms} ms deadline")]
pub struct JobDeadlineError {
    /// `"analyse-game"` or `"import-account"`.
    pub kind: &'static str,
    pub ms: u64,
}

pub struct Deadline {
    kind: &'static str,
    ms: u64,
    token: CancellationToken,
    expired: Arc<AtomicBool>,
    rearm: Arc<Notify>,
    task: tokio::task::JoinHandle<()>,
}

impl Deadline {
    /// Starts the timer. Must be called inside a tokio runtime.
    pub fn new(kind: &'static str, d: Duration) -> Deadline {
        let token = CancellationToken::new();
        let expired = Arc::new(AtomicBool::new(false));
        let rearm = Arc::new(Notify::new());
        let task = {
            let (token, expired, rearm) = (token.clone(), expired.clone(), rearm.clone());
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep(d) => {
                            expired.store(true, Ordering::SeqCst);
                            token.cancel();
                            return;
                        }
                        _ = rearm.notified() => {}
                        _ = token.cancelled() => return,
                    }
                }
            })
        };
        Deadline {
            kind,
            ms: u64::try_from(d.as_millis()).unwrap_or(u64::MAX),
            token,
            expired,
            rearm,
            task,
        }
    }

    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    /// Re-arms the timer (for idle deadlines). No effect once expired.
    pub fn reset(&self) {
        self.rearm.notify_one();
    }

    /// `Some` once the timer has fired.
    pub fn expired(&self) -> Option<JobDeadlineError> {
        self.expired
            .load(Ordering::SeqCst)
            .then_some(JobDeadlineError {
                kind: self.kind,
                ms: self.ms,
            })
    }
}

impl Drop for Deadline {
    fn drop(&mut self) {
        self.task.abort();
    }
}
