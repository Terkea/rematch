//! Supervised Stockfish pool (port of `apps/worker/src/engine/engine-pool.ts`).
//!
//! Idle engines sit on a stack; callers that find none wait in a FIFO queue.
//! A dead engine (crashed, killed externally, or killed after a timeout) is
//! replaced transparently, and a job whose engine crashes mid-search is
//! retried once on a fresh engine. Every engine is sent `Threads`
//! ([`ENGINE_THREADS`]) and `Hash` ([`ENGINE_HASH_MB`] unless `hash_mb` says
//! otherwise) explicitly, so searches stay deterministic whatever a build's
//! defaults are (ADR 0104).
//!
//! Every process is tracked by a [`Ticket`]; `close()` resolves only after
//! every ticket is gone, that is after every process was quit or killed and
//! reaped. Tickets hold a `Weak` reference, so dropping the pool without
//! `close()` still drops (and, through `kill_on_drop`, kills) idle engines.

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

use futures::future::BoxFuture;
use tokio::sync::{Semaphore, oneshot, watch};
use tokio_util::sync::CancellationToken;

use crate::types::{
    AnalyseOptions, Analyser, AnalysisResult, ENGINE_HASH_MB, ENGINE_THREADS, EngineError,
};
use crate::uci::{AliveHandle, StartOptions, UciEngine, validate_fen};

/// Engines started at once per pool. A cold start loads the NNUE net from
/// disk, and a burst of starts can exceed the handshake deadline.
pub const SPAWN_CONCURRENCY: usize = 2;

#[derive(Clone, Debug)]
pub struct PoolOptions {
    pub size: usize,
    pub path: PathBuf,
    /// `setoption name Hash`; `None` sends [`ENGINE_HASH_MB`].
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
    /// Engine id -> pid and liveness of every started engine not yet
    /// retired or replaced.
    pids: BTreeMap<u64, (u32, AliveHandle)>,
    next_id: u64,
}

struct Inner {
    opts: PoolOptions,
    state: Mutex<State>,
    /// Number of tickets alive (processes started or starting, not yet reaped).
    live: watch::Sender<usize>,
    /// Interrupts in-flight searches on `close()`.
    cancel: CancellationToken,
    /// Bounds concurrent engine starts ([`SPAWN_CONCURRENCY`]).
    spawn_permits: Semaphore,
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

/// An engine checked out of the pool. The engine stays inside the lease for
/// the whole call, including the post-call restart and the closed-pool kill,
/// so dropping it at any await point (a cancelled caller) returns the engine
/// and the pool never loses a slot. An abandoned search is stopped on
/// release (`UciEngine::analyse` is cancel-safe either way).
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
                spawn_permits: Semaphore::new(SPAWN_CONCURRENCY),
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

    /// Pids of the pool's live engines (idle and in use).
    pub fn pids(&self) -> Vec<u32> {
        self.inner
            .state()
            .pids
            .values()
            .filter(|(_, alive)| alive.is_alive())
            .map(|(pid, _)| *pid)
            .collect()
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
        // The engine never leaves the lease until it is released, so a
        // caller cancelled at any await below still returns it (no lost slot).
        let mut lease = acquire(inner).await?;
        let result = run(inner, &mut lease, fen, opts).await;
        if inner.is_closed() {
            // Kill and reap in place; dropping the lease then retires it.
            if let Some(p) = lease.pooled.as_mut() {
                p.engine.kill().await;
            }
            drop(lease);
            return Err(EngineError::PoolClosed);
        }
        if let Some(p) = lease.pooled.as_mut()
            && !p.engine.is_alive()
        {
            // Restart so the slot is not lost (for example after a timeout kill).
            // On failure the dead engine goes back, and its next user retries.
            let _ = replace(inner, p).await;
        }
        drop(lease);
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
/// An engine still running an abandoned search is stopped first, so it burns
/// no CPU while idle. After close() it is retired instead.
fn release(inner: &Arc<Inner>, mut p: Pooled) {
    if p.engine.is_searching()
        && p.engine.is_alive()
        && !inner.is_closed()
        && let Ok(h) = tokio::runtime::Handle::try_current()
    {
        let inner = inner.clone();
        h.spawn(async move {
            // On failure the engine is dead and is replaced by its next user.
            let _ = p.engine.stop_search().await;
            release(&inner, p);
        });
        return;
    }
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
///
/// At most [`SPAWN_CONCURRENCY`] starts run at once, and a start whose
/// handshake times out (a cold net load) is retried once.
async fn spawn(inner: &Arc<Inner>) -> Result<Pooled, EngineError> {
    let _permit = inner
        .spawn_permits
        .acquire()
        .await
        .map_err(|_| EngineError::PoolClosed)?;
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
    let start = || {
        UciEngine::start(StartOptions {
            path: inner.opts.path.clone(),
            threads: Some(ENGINE_THREADS),
            hash_mb: Some(inner.opts.hash_mb.unwrap_or(ENGINE_HASH_MB)),
            syzygy_path: None,
            handshake_timeout: None,
        })
    };
    let engine = match start().await {
        Err(EngineError::Timeout(t)) if !inner.is_closed() => {
            tracing::warn!(?t, "engine handshake timed out; retrying once");
            start().await?
        }
        r => r?,
    };
    let p = Pooled { engine, ticket };
    let closed = {
        let mut st = inner.state();
        if !st.closed
            && let Some(pid) = p.engine.pid()
        {
            st.pids.insert(id, (pid, p.engine.alive_handle()));
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
