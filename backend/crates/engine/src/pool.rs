//! Supervised Stockfish pool (port of `apps/worker/src/engine/engine-pool.ts`).
//!
//! Idle engines sit on a stack; callers that find none wait in a FIFO queue.
//! A dead engine (crashed, killed externally, or killed after a timeout) is
//! replaced transparently, and a job whose engine crashes mid-search is
//! retried once on a fresh engine. Engines are started with Stockfish's
//! default `Threads` and `Hash` (only `hash_mb` overrides Hash), so searches
//! stay deterministic and comparable with the TS worker.
//!
//! Every process is tracked by a [`Ticket`]; `close()` resolves only after
//! every ticket is gone, that is after every process was quit or killed and
//! reaped. Tickets hold a `Weak` reference, so dropping the pool without
//! `close()` still drops (and, through `kill_on_drop`, kills) idle engines.

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

use futures::future::BoxFuture;
use tokio::sync::{oneshot, watch};
use tokio_util::sync::CancellationToken;

use crate::types::{AnalyseOptions, Analyser, AnalysisResult, EngineError};
use crate::uci::{StartOptions, UciEngine, validate_fen};

#[derive(Clone, Debug)]
pub struct PoolOptions {
    pub size: usize,
    pub path: PathBuf,
    /// `setoption name Hash`; `None` keeps the Stockfish default.
    pub hash_mb: Option<u32>,
}

/// Accounts for one engine process (or one being started).
struct Ticket {
    id: u64,
    inner: Weak<Inner>,
}

impl Drop for Ticket {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            inner.state().pids.remove(&self.id);
            inner.live.send_modify(|n| *n = n.saturating_sub(1));
        }
    }
}

struct Pooled {
    engine: UciEngine,
    ticket: Ticket,
}

#[derive(Default)]
struct State {
    idle: Vec<Pooled>,
    waiters: VecDeque<oneshot::Sender<Pooled>>,
    closed: bool,
    /// Engine id -> pid of every started engine not yet retired or replaced.
    pids: BTreeMap<u64, u32>,
    next_id: u64,
}

struct Inner {
    opts: PoolOptions,
    state: Mutex<State>,
    /// Number of tickets alive (processes started or starting, not yet reaped).
    live: watch::Sender<usize>,
    /// Interrupts in-flight searches on `close()`.
    cancel: CancellationToken,
}

impl Inner {
    /// The state lock. Never held across an await, and never while a
    /// `Pooled` is dropped (its ticket takes the lock).
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn is_closed(&self) -> bool {
        self.state().closed
    }
}

pub struct EnginePool {
    inner: Arc<Inner>,
}

/// An engine checked out of the pool. Dropping it (for example when the
/// caller's future is cancelled) returns the engine; `UciEngine::analyse` is
/// cancel-safe, so the next user stops and drains any abandoned search.
struct Lease {
    pooled: Option<Pooled>,
    inner: Arc<Inner>,
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(p) = self.pooled.take() {
            release(&self.inner, p);
        }
    }
}

/// A queued caller. If it is dropped after an engine was handed to it but
/// before it took it, the engine is released again instead of being lost.
struct Waiting {
    rx: Option<oneshot::Receiver<Pooled>>,
    inner: Arc<Inner>,
}

impl Drop for Waiting {
    fn drop(&mut self) {
        if let Some(mut rx) = self.rx.take() {
            rx.close();
            if let Ok(p) = rx.try_recv() {
                release(&self.inner, p);
            }
        }
    }
}

impl EnginePool {
    /// Start `size` engines. If any fails to start, the ones already started
    /// are shut down and the error is returned.
    pub async fn create(opts: PoolOptions) -> Result<Arc<EnginePool>, EngineError> {
        let size = opts.size;
        let (live, _) = watch::channel(0);
        let pool = Arc::new(EnginePool {
            inner: Arc::new(Inner {
                opts,
                state: Mutex::new(State::default()),
                live,
                cancel: CancellationToken::new(),
            }),
        });
        for _ in 0..size {
            match spawn(&pool.inner).await {
                Ok(p) => pool.inner.state().idle.push(p),
                Err(e) => {
                    pool.close().await;
                    return Err(e);
                }
            }
        }
        Ok(pool)
    }

    /// Pids of the pool's engines (idle and in use). An engine that died
    /// while idle stays listed until it is next used or the pool closes.
    pub fn pids(&self) -> Vec<u32> {
        self.inner.state().pids.values().copied().collect()
    }

    /// Analyse one position on a pooled engine.
    pub async fn analyse(
        &self,
        fen: &str,
        opts: AnalyseOptions,
    ) -> Result<AnalysisResult, EngineError> {
        let inner = &self.inner;
        if inner.is_closed() {
            return Err(EngineError::PoolClosed);
        }
        // An invalid FEN fails at once, without waiting for a free engine.
        validate_fen(fen)?;
        // The engine stays in the lease while in use, so a cancelled caller
        // still returns it.
        let mut lease = acquire(inner).await?;
        let result = run(inner, &mut lease, fen, opts).await;
        let Some(mut p) = lease.pooled.take() else {
            return result;
        };
        if inner.is_closed() {
            retire(p).await;
            return Err(EngineError::PoolClosed);
        }
        if !p.engine.is_alive() {
            // Restart so the slot is not lost (for example after a timeout kill).
            // On failure the dead engine goes back, and its next user retries.
            let _ = replace(inner, &mut p).await;
        }
        release(inner, p);
        result
    }

    /// Fail queued callers and in-flight analyses with `PoolClosed`, quit
    /// every engine, and resolve once every engine process has exited.
    /// Later calls fail with `PoolClosed`. Idempotent.
    pub async fn close(&self) {
        let inner = &self.inner;
        let (idle, waiters) = {
            let mut st = inner.state();
            st.closed = true;
            (
                std::mem::take(&mut st.idle),
                std::mem::take(&mut st.waiters),
            )
        };
        // Dropped senders fail their waiters with `PoolClosed`.
        drop(waiters);
        inner.cancel.cancel();
        futures::future::join_all(idle.into_iter().map(retire)).await;
        // In-flight analyses and engines still starting retire their own.
        let mut rx = inner.live.subscribe();
        let _ = rx.wait_for(|n| *n == 0).await;
    }
}

impl Analyser for EnginePool {
    fn analyse<'a>(
        &'a self,
        fen: &'a str,
        opts: AnalyseOptions,
    ) -> BoxFuture<'a, Result<AnalysisResult, EngineError>> {
        Box::pin(EnginePool::analyse(self, fen, opts))
    }
}

/// The analysis with its one crash retry. The engine stays in `lease`.
async fn run(
    inner: &Arc<Inner>,
    lease: &mut Lease,
    fen: &str,
    opts: AnalyseOptions,
) -> Result<AnalysisResult, EngineError> {
    let Some(p) = lease.pooled.as_mut() else {
        return Err(EngineError::PoolClosed);
    };
    let mut attempt = 0;
    loop {
        if !p.engine.is_alive() {
            replace(inner, p).await.map_err(|e| closed_or(inner, e))?;
        }
        let r = tokio::select! {
            biased;
            () = inner.cancel.cancelled() => Err(EngineError::PoolClosed),
            r = p.engine.analyse(fen, opts) => r,
        };
        // close() interrupts the search; a result produced after close()
        // began (possibly truncated) is discarded.
        if inner.is_closed() {
            return Err(EngineError::PoolClosed);
        }
        match r {
            // A dead process is restarted and the job retried once.
            // Timeouts and invalid FENs are returned as they are.
            Err(EngineError::Crashed) if attempt == 0 => {
                attempt += 1;
                replace(inner, p).await.map_err(|e| closed_or(inner, e))?;
            }
            other => return other,
        }
    }
}

fn closed_or(inner: &Inner, e: EngineError) -> EngineError {
    if inner.is_closed() {
        EngineError::PoolClosed
    } else {
        e
    }
}

async fn acquire(inner: &Arc<Inner>) -> Result<Lease, EngineError> {
    let rx = {
        let mut st = inner.state();
        if st.closed {
            return Err(EngineError::PoolClosed);
        }
        if let Some(p) = st.idle.pop() {
            return Ok(Lease {
                pooled: Some(p),
                inner: inner.clone(),
            });
        }
        let (tx, rx) = oneshot::channel();
        st.waiters.push_back(tx);
        rx
    };
    let mut waiting = Waiting {
        rx: Some(rx),
        inner: inner.clone(),
    };
    let received = match waiting.rx.as_mut() {
        Some(rx) => rx.await,
        None => return Err(EngineError::PoolClosed),
    };
    waiting.rx = None;
    match received {
        Ok(p) => Ok(Lease {
            pooled: Some(p),
            inner: inner.clone(),
        }),
        // The sender was dropped by close().
        Err(_) => Err(EngineError::PoolClosed),
    }
}

/// Hand an engine to the first live waiter, or push it on the idle stack.
/// After close() it is retired instead.
fn release(inner: &Arc<Inner>, mut p: Pooled) {
    {
        let mut st = inner.state();
        if !st.closed {
            while let Some(w) = st.waiters.pop_front() {
                match w.send(p) {
                    Ok(()) => return,
                    // That waiter is gone; try the next one.
                    Err(back) => p = back,
                }
            }
            st.idle.push(p);
            return;
        }
    }
    match tokio::runtime::Handle::try_current() {
        Ok(h) => {
            h.spawn(retire(p));
        }
        // No runtime: dropping kills the process (`kill_on_drop`).
        Err(_) => drop(p),
    }
}

/// Start one engine and register it.
async fn spawn(inner: &Arc<Inner>) -> Result<Pooled, EngineError> {
    let id = {
        let mut st = inner.state();
        if st.closed {
            return Err(EngineError::PoolClosed);
        }
        st.next_id += 1;
        // Counted under the lock, so a concurrent close() waits for it.
        inner.live.send_modify(|n| *n += 1);
        st.next_id
    };
    let ticket = Ticket {
        id,
        inner: Arc::downgrade(inner),
    };
    let engine = UciEngine::start(StartOptions {
        path: inner.opts.path.clone(),
        threads: None,
        hash_mb: inner.opts.hash_mb,
        syzygy_path: None,
    })
    .await?;
    let p = Pooled { engine, ticket };
    let closed = {
        let mut st = inner.state();
        if !st.closed
            && let Some(pid) = p.engine.pid()
        {
            st.pids.insert(id, pid);
        }
        st.closed
    };
    if closed {
        retire(p).await;
        return Err(EngineError::PoolClosed);
    }
    Ok(p)
}

/// Swap `slot` for a fresh engine and retire the old one. On failure the old
/// (dead) engine stays in `slot`.
async fn replace(inner: &Arc<Inner>, slot: &mut Pooled) -> Result<(), EngineError> {
    inner.state().pids.remove(&slot.ticket.id);
    let fresh = spawn(inner).await?;
    let old = std::mem::replace(slot, fresh);
    retire(old).await;
    Ok(())
}

/// Quit (or kill) and reap the process, then drop its ticket.
async fn retire(p: Pooled) {
    let Pooled { engine, ticket } = p;
    engine.quit().await;
    drop(ticket);
}
