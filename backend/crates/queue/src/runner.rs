//! The queue runner (ADR 0032): one tokio task per runner claims jobs of one kind, up to
//! `concurrency` at a time, runs each under a renewed lease, and reaps expired leases.
//!
//! - **Wake-ups:** a listener task `LISTEN`s on [`JOBS_CHANNEL`] and wakes the loop for its kind;
//!   the loop also polls every `idle_poll`, so a lost notification only costs latency.
//! - **Claiming:** `dequeue` is never raced against anything in `select!` (it is not
//!   cancel-safe: dropping it after COMMIT would orphan the claimed jobs until their lease
//!   expires). The stop flag is checked between calls instead.
//! - **Leases:** jobs are leased under the runner's own owner, `<worker_id>/<kind>`, so two
//!   runners of one process never release each other's jobs. Every job renews its lease every
//!   `renew_every`; the renew runs beside the handler (which keeps being polled) and is bounded
//!   by the local lease deadline. The local deadline starts before the claim and runs on the
//!   monotonic clock, so it is never later than the database's `lease_until` unless the
//!   database clock steps forward (which shortens the database lease by the step; the
//!   two-pass reaper below keeps that from reaping a live lease). When a renew says the lease
//!   is lost (the reaper
//!   re-queued the job and someone else may hold it), or when no renew has succeeded by
//!   `lease_until - CANCEL_GRACE`, the job's [`JobContext::cancel`] fires, the handler gets
//!   [`CANCEL_GRACE`], and its result is dropped: a cancelled handler never outlives its lease
//!   by more than that.
//! - **Hard deadline:** after `hard_timeout` the token is cancelled, the handler gets 1 s to
//!   wind down, and the attempt fails as `Retryable { code: "timeout" }`.
//! - **Stalls:** every `reap_every` the loop lists its kind's leases that look expired by the
//!   database clock ([`Queue::expired_leases`]) and recovers only those that have looked expired,
//!   with the same lease token and heartbeat, for at least `stall_grace` on the monotonic clock
//!   ([`Queue::recover_marked`]). A forward step of the database clock therefore cannot reap a
//!   live owner, which renews (and so changes its heartbeat) every `renew_every` <
//!   `stall_grace`. A job that stalled more than `max_stalls` times is dead and
//!   [`Handler::on_dead`] is called.
//! - **Shutdown:** [`RunnerHandle::stop`] stops claiming and lets in-flight jobs finish
//!   ([`RunnerHandle::wait_idle`]); [`RunnerHandle::force`] cancels every handler, waits up to
//!   1 s, then hands the remaining jobs back with [`Queue::release_owned`] without using an
//!   attempt. A retryable failure after `force` (handler error, hard timeout or panic) is
//!   recorded as a release too; an `Unrecoverable` one stays failed, provided its write lands
//!   within [`FORCE_GRACE`] (otherwise `release_owned` re-queues it). Both are re-entrant;
//!   [`RunnerHandle::shutdown`] combines them with a grace period.

use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures::FutureExt;
use futures::future::BoxFuture;
use serde_json::Value;
use sqlx::PgPool;
use sqlx::postgres::PgListener;
use tokio::sync::{Notify, watch};
use tokio::task::{AbortHandle, JoinError, JoinSet};
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use std::collections::HashMap;

use crate::queue::{
    Backoff, FailOutcome, JOBS_CHANNEL, MAX_ERROR_CHARS, Queue, QueueError, truncate_chars,
};
use crate::reaper::{LeaseMark, STALLED_REASON};
use crate::types::{Job, JobError};

/// How long a cancelled handler (hard timeout, lost lease) gets before it is dropped. A job's
/// token is cancelled this long before its local lease deadline (at most half the lease).
pub const CANCEL_GRACE: Duration = Duration::from_secs(1);
/// How long `force` waits for cancelled handlers before releasing their jobs.
pub const FORCE_GRACE: Duration = Duration::from_secs(1);
/// Delay between attempts to (re)connect the NOTIFY listener.
const LISTEN_RETRY: Duration = Duration::from_secs(1);

/// The code of one job kind.
pub trait Handler: Send + Sync + 'static {
    /// The job kind this handler runs (one runner per kind).
    fn kind(&self) -> &'static str;

    /// Runs one attempt. `Ok` completes the job; see [`JobError`] for the failure modes.
    /// `ctx.cancel` fires on a hard timeout, a lost lease or a forced shutdown.
    fn handle(&self, job: Job, ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>>;

    /// Called once when the job is dead-lettered: its final retryable attempt failed (`reason`
    /// is that error), or it stalled more than `max_stalls` times (`reason` is
    /// [`STALLED_REASON`]). The default does nothing.
    fn on_dead<'a>(&'a self, job: &'a Job, reason: &'a str) -> BoxFuture<'a, ()> {
        let _ = (job, reason);
        Box::pin(async {})
    }
}

/// What a running handler gets besides its job.
#[derive(Clone, Debug)]
pub struct JobContext {
    /// Fires when the handler should stop: hard timeout, lost lease or forced shutdown.
    pub cancel: CancellationToken,
}

#[derive(Clone, Debug)]
pub struct RunnerConfig {
    /// The prefix of this runner's `lease_owner`, which is `<worker_id>/<kind>`. It must be
    /// unique per *process*: runners of one process (one per kind) may share it, but two
    /// replicas with the same `worker_id` would release each other's jobs on `force`. The
    /// default is `<host>-<pid>-<random>`.
    pub worker_id: String,
    /// Jobs of this kind run at once.
    pub concurrency: usize,
    /// Lease length; a job not renewed within it is recovered by the reaper (60 s).
    pub lease: Duration,
    /// Renewal period, well below `lease` (20 s).
    pub renew_every: Duration,
    /// Stall recovery period (30 s).
    pub reap_every: Duration,
    /// How long a lease must keep looking expired, unchanged, on the monotonic clock before the
    /// reaper recovers it (25 s; must exceed `renew_every`, so a live owner always renews in
    /// between). Guards against forward steps of the database clock.
    pub stall_grace: Duration,
    /// Fallback poll when no notification arrives (500 ms).
    pub idle_poll: Duration,
    pub backoff: Backoff,
    /// Per-handler hard deadline; `None` for none.
    pub hard_timeout: Option<Duration>,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            worker_id: default_worker_id(),
            concurrency: 1,
            lease: Duration::from_secs(60),
            renew_every: Duration::from_secs(20),
            reap_every: Duration::from_secs(30),
            stall_grace: Duration::from_secs(25),
            idle_poll: Duration::from_millis(500),
            backoff: Backoff::default(),
            hard_timeout: None,
        }
    }
}

impl RunnerConfig {
    /// Short timings for tests: lease 4 s, renew 500 ms, reap 250 ms, stall grace 1 s, poll
    /// 50 ms, backoff 50 ms. The lease leaves a renew 2.5 s before the local deadline.
    pub fn fast_for_tests() -> Self {
        Self {
            lease: Duration::from_secs(4),
            renew_every: Duration::from_millis(500),
            reap_every: Duration::from_millis(250),
            stall_grace: Duration::from_secs(1),
            idle_poll: Duration::from_millis(50),
            backoff: Backoff {
                base: Duration::from_millis(50),
            },
            ..Self::default()
        }
    }
}

/// `<host>-<pid>-<random>`, so two runners never share a lease owner.
fn default_worker_id() -> String {
    let host = std::env::var("HOSTNAME").unwrap_or_else(|_| "worker".to_owned());
    let random = Uuid::new_v4().simple().to_string();
    format!("{host}-{}-{}", std::process::id(), &random[..8])
}

/// Counters since the runner started (the "meter" for releases, stalls and lost leases).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunnerStats {
    pub completed: u64,
    /// Failed attempts re-queued with backoff (or a recurring job's next run).
    pub retried: u64,
    /// Re-queued without using an attempt (`JobError::Release`, for example an open breaker).
    /// Unbounded by design, so it is counted and logged.
    pub released: u64,
    /// Unrecoverable failures.
    pub failed: u64,
    /// Dead-lettered: final attempt failed, or stalled more than `max_stalls` times.
    pub dead: u64,
    /// Expired leases this runner's reaper recovered (including those that went dead).
    pub stalled: u64,
    /// Results dropped because the lease was lost.
    pub lease_lost: u64,
    /// Attempts that hit `hard_timeout`.
    pub timed_out: u64,
}

#[derive(Default)]
struct Counters {
    completed: AtomicU64,
    retried: AtomicU64,
    released: AtomicU64,
    failed: AtomicU64,
    dead: AtomicU64,
    stalled: AtomicU64,
    lease_lost: AtomicU64,
    timed_out: AtomicU64,
}

fn bump(c: &AtomicU64) {
    c.fetch_add(1, Ordering::Relaxed);
}

struct Shared {
    queue: Queue,
    handler: Arc<dyn Handler>,
    cfg: RunnerConfig,
    /// The `lease_owner` of this runner's jobs: `<worker_id>/<kind>`.
    owner: String,
    /// Stop claiming.
    stop: CancellationToken,
    /// Cancel every handler (parent of each job's token).
    force: CancellationToken,
    /// Cancelled when the main loop exits or is dropped.
    finished: CancellationToken,
    in_flight: watch::Sender<usize>,
    wake: Arc<Notify>,
    counters: Counters,
}

/// Decrements `in_flight` when a job task ends, however it ends (including abort).
struct InFlight(Arc<Shared>);

impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.in_flight.send_modify(|n| *n = n.saturating_sub(1));
    }
}

/// Aborts a task when dropped.
struct AbortOnDrop(AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub struct Runner;

impl Runner {
    /// Starts a runner for `handler.kind()` on the current tokio runtime.
    ///
    /// # Panics
    /// When `concurrency` is 0, when `renew_every` does not fit before the local lease
    /// deadline (`lease - min(CANCEL_GRACE, lease / 2)`), or when `stall_grace` does not exceed
    /// `renew_every`.
    pub fn spawn(queue: Queue, handler: Arc<dyn Handler>, cfg: RunnerConfig) -> RunnerHandle {
        assert!(cfg.concurrency > 0, "runner concurrency must be at least 1");
        assert!(
            cfg.renew_every + CANCEL_GRACE.min(cfg.lease / 2) < cfg.lease,
            "renew_every ({:?}) must be shorter than the lease ({:?}) minus the cancel grace",
            cfg.renew_every,
            cfg.lease
        );
        assert!(
            cfg.stall_grace > cfg.renew_every,
            "stall_grace ({:?}) must exceed renew_every ({:?})",
            cfg.stall_grace,
            cfg.renew_every
        );
        let owner = format!("{}/{}", cfg.worker_id, handler.kind());
        let shared = Arc::new(Shared {
            queue,
            handler,
            cfg,
            owner,
            stop: CancellationToken::new(),
            force: CancellationToken::new(),
            finished: CancellationToken::new(),
            in_flight: watch::Sender::new(0),
            wake: Arc::new(Notify::new()),
            counters: Counters::default(),
        });
        let main = tokio::spawn(run(shared.clone())).abort_handle();
        RunnerHandle { shared, main }
    }
}

/// Controls a running [`Runner`]. Dropping the handle does not stop the runner.
pub struct RunnerHandle {
    shared: Arc<Shared>,
    main: AbortHandle,
}

impl RunnerHandle {
    /// Stops claiming new jobs; in-flight jobs run to completion. Idempotent.
    pub fn stop(&self) {
        let s = &self.shared;
        if !s.stop.is_cancelled() {
            tracing::info!(lease_owner = %s.owner, kind = s.handler.kind(), "runner stopping");
        }
        s.stop.cancel();
    }

    /// After [`stop`](Self::stop): resolves once every in-flight job has finished and the runner
    /// has exited. Before it: resolves when no job is in flight at that moment.
    pub async fn wait_idle(&self) {
        let s = &self.shared;
        if s.stop.is_cancelled() {
            s.finished.cancelled().await;
        }
        let mut rx = s.in_flight.subscribe();
        let _ = rx.wait_for(|n| *n == 0).await;
    }

    /// Stops claiming, cancels every handler, waits up to [`FORCE_GRACE`] for them, then aborts
    /// the rest and hands this runner's running jobs back to the queue without using an attempt
    /// ([`Queue::release_owned`] for its own `<worker_id>/<kind>` owner only, so other runners
    /// sharing the `worker_id` are untouched). Re-entrant.
    pub async fn force(&self) {
        let s = &self.shared;
        self.stop();
        s.force.cancel();
        let mut rx = s.in_flight.subscribe();
        let _ = tokio::time::timeout(FORCE_GRACE, rx.wait_for(|n| *n == 0)).await;
        self.main.abort();
        let _ = tokio::time::timeout(FORCE_GRACE, s.finished.cancelled()).await;
        let _ = tokio::time::timeout(FORCE_GRACE, rx.wait_for(|n| *n == 0)).await;
        match s.queue.release_owned(&s.owner).await {
            Ok(0) => {}
            Ok(n) => tracing::warn!(
                lease_owner = %s.owner,
                kind = s.handler.kind(),
                released = n,
                "force-released in-flight jobs without using an attempt"
            ),
            Err(e) => tracing::error!(
                lease_owner = %s.owner,
                kind = s.handler.kind(),
                error = %e,
                "releasing in-flight jobs failed; their leases will expire"
            ),
        }
    }

    /// Graceful shutdown: [`stop`](Self::stop), wait up to `grace` for in-flight jobs, then
    /// [`force`](Self::force). `true` when everything finished within `grace`. Re-entrant.
    /// After `force`, an `Unrecoverable` verdict stays failed only if its write lands within
    /// [`FORCE_GRACE`]; otherwise the job is released like the rest.
    pub async fn shutdown(&self, grace: Duration) -> bool {
        self.stop();
        if tokio::time::timeout(grace, self.wait_idle()).await.is_ok() {
            return true;
        }
        tracing::warn!(
            lease_owner = %self.shared.owner,
            kind = self.shared.handler.kind(),
            in_flight = self.in_flight(),
            "jobs did not finish within the grace period; forcing"
        );
        self.force().await;
        false
    }

    /// Kills the runner and its jobs without touching the database (a simulated crash): their
    /// leases expire and the reaper recovers them.
    pub fn abort(&self) {
        self.shared.stop.cancel();
        self.main.abort();
    }

    /// Jobs currently running.
    pub fn in_flight(&self) -> usize {
        *self.shared.in_flight.borrow()
    }

    pub fn worker_id(&self) -> &str {
        &self.shared.cfg.worker_id
    }

    /// The `lease_owner` this runner claims under: `<worker_id>/<kind>`.
    pub fn lease_owner(&self) -> &str {
        &self.shared.owner
    }

    pub fn stats(&self) -> RunnerStats {
        let c = &self.shared.counters;
        let get = |a: &AtomicU64| a.load(Ordering::Relaxed);
        RunnerStats {
            completed: get(&c.completed),
            retried: get(&c.retried),
            released: get(&c.released),
            failed: get(&c.failed),
            dead: get(&c.dead),
            stalled: get(&c.stalled),
            lease_lost: get(&c.lease_lost),
            timed_out: get(&c.timed_out),
        }
    }
}

/// The main loop: claim, wait (notification, poll, a finished job, reap tick or stop), repeat.
async fn run(s: Arc<Shared>) {
    let _finished = s.finished.clone().drop_guard();
    let kind = s.handler.kind();
    let _listener = AbortOnDrop(
        tokio::spawn(listen(s.queue.pool().clone(), kind, s.wake.clone())).abort_handle(),
    );
    // Dropping the set (when the loop ends or is aborted) aborts every job task.
    let mut tasks: JoinSet<()> = JoinSet::new();
    let mut reap = tokio::time::interval(s.cfg.reap_every);
    reap.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut stalls = StallTracker::default();
    loop {
        if s.stop.is_cancelled() {
            break;
        }
        let free = s.cfg.concurrency.saturating_sub(*s.in_flight.borrow());
        if free > 0 {
            // Measured before the claim, so the local lease deadline is never later than the
            // database's `lease_until = now() + lease`.
            let claimed_at = Instant::now();
            // Not raced against anything: see the module docs.
            match s
                .queue
                .dequeue(&[kind], &s.owner, free as i64, s.cfg.lease)
                .await
            {
                Ok(jobs) => {
                    for job in jobs {
                        s.in_flight.send_modify(|n| *n += 1);
                        let guard = InFlight(s.clone());
                        tasks.spawn(run_job(s.clone(), job, claimed_at, guard));
                    }
                }
                Err(e) => tracing::error!(kind, error = %e, "claiming jobs failed"),
            }
        }
        tokio::select! {
            biased;
            _ = s.stop.cancelled() => {}
            Some(res) = tasks.join_next(), if !tasks.is_empty() => log_join(kind, res),
            _ = reap.tick() => reap_expired(&s, &mut stalls, &mut tasks).await,
            _ = s.wake.notified() => {}
            _ = tokio::time::sleep(s.cfg.idle_poll) => {}
        }
    }
    while let Some(res) = tasks.join_next().await {
        log_join(kind, res);
    }
    tracing::info!(lease_owner = %s.owner, kind, "runner stopped");
}

fn log_join(kind: &str, res: Result<(), JoinError>) {
    if let Err(e) = res
        && !e.is_cancelled()
    {
        tracing::error!(kind, error = %e, "job task ended abnormally");
    }
}

/// Forwards `NOTIFY jobs` payloads for `kind` to `wake`, reconnecting forever.
async fn listen(pool: PgPool, kind: &'static str, wake: Arc<Notify>) {
    loop {
        let connected = async {
            let mut listener = PgListener::connect_with(&pool).await?;
            listener.listen(JOBS_CHANNEL).await?;
            Ok::<_, sqlx::Error>(listener)
        }
        .await;
        match connected {
            Ok(mut listener) => {
                // Anything enqueued while we were not listening.
                wake.notify_one();
                loop {
                    match listener.recv().await {
                        Ok(n) if n.payload() == kind => wake.notify_one(),
                        Ok(_) => {}
                        Err(e) => {
                            tracing::warn!(kind, error = %e, "job listener failed; polling until it reconnects");
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!(kind, error = %e, "job listener could not connect; polling meanwhile");
            }
        }
        tokio::time::sleep(LISTEN_RETRY).await;
    }
}

/// Two-pass stall confirmation: a lease is recovered only once it has looked expired, with the
/// same token and heartbeat, for at least the grace period on the monotonic clock.
#[derive(Default)]
struct StallTracker {
    /// Leases that looked expired at the last pass, with when each was first seen unchanged.
    seen: HashMap<i64, (LeaseMark, Instant)>,
}

impl StallTracker {
    /// Records this pass's expired-looking leases and returns those confirmed: seen unchanged
    /// for at least `grace`. A lease that changed (renewed: new heartbeat; re-claimed: new
    /// token) starts over; one no longer listed is forgotten.
    fn observe(&mut self, marks: Vec<LeaseMark>, now: Instant, grace: Duration) -> Vec<LeaseMark> {
        let mut next = HashMap::with_capacity(marks.len());
        let mut confirmed = Vec::new();
        for mark in marks {
            let first = match self.seen.get(&mark.id) {
                Some((old, at)) if *old == mark => *at,
                _ => now,
            };
            if now.saturating_duration_since(first) >= grace {
                confirmed.push(mark.clone());
            }
            next.insert(mark.id, (mark, first));
        }
        self.seen = next;
        confirmed
    }
}

/// Recovers this kind's confirmed expired leases; dead ones get `on_dead` (tracked in `tasks`,
/// so a drain waits for it).
async fn reap_expired(s: &Arc<Shared>, stalls: &mut StallTracker, tasks: &mut JoinSet<()>) {
    let kind = s.handler.kind();
    let marks = match s.queue.expired_leases(&[kind]).await {
        Ok(m) => m,
        Err(e) => {
            tracing::error!(kind, error = %e, "listing expired leases failed");
            return;
        }
    };
    let confirmed = stalls.observe(marks, Instant::now(), s.cfg.stall_grace);
    if confirmed.is_empty() {
        return;
    }
    let recovered = match s.queue.recover_marked(&confirmed).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(kind, error = %e, "stall recovery failed");
            return;
        }
    };
    for r in recovered {
        bump(&s.counters.stalled);
        if r.dead {
            bump(&s.counters.dead);
            tracing::error!(
                job_id = r.job.id,
                kind,
                attempt = r.job.attempts,
                max_attempts = r.job.max_attempts,
                error = STALLED_REASON,
                error_code = "stalled",
                outcome = "dead",
                "job failed"
            );
            let s = s.clone();
            tasks.spawn(async move { s.handler.on_dead(&r.job, STALLED_REASON).await });
        } else {
            tracing::warn!(job_id = r.job.id, kind, "job stalled; re-queued");
        }
    }
}

enum Ended {
    Finished(std::thread::Result<Result<Value, JobError>>),
    /// The lease is gone (`why`): the result must be dropped.
    LeaseLost(&'static str),
    TimedOut,
}

/// One renew: when it started, and its outcome unless it ran into the local lease deadline.
type RenewOut = (
    Instant,
    Result<Result<bool, QueueError>, tokio::time::error::Elapsed>,
);

/// Runs one claimed job: renews its lease beside the handler, enforces the hard deadline and the
/// local lease deadline, then records the result.
async fn run_job(s: Arc<Shared>, job: Job, claimed_at: Instant, _guard: InFlight) {
    let token = s.force.child_token();
    let handler = s.handler.clone();
    let started = Instant::now();
    // Boxed, so a cancelled handler can be dropped (freeing what it holds) before a slow write.
    let mut fut = Box::pin(
        AssertUnwindSafe(handler.handle(
            job.clone(),
            JobContext {
                cancel: token.clone(),
            },
        ))
        .catch_unwind(),
    );

    let lease = s.cfg.lease;
    let grace = CANCEL_GRACE.min(lease / 2);
    let mut renew = tokio::time::interval_at(claimed_at + s.cfg.renew_every, s.cfg.renew_every);
    renew.set_missed_tick_behavior(MissedTickBehavior::Delay);
    // The local view of the lease; never later than the database's.
    let mut lease_until = claimed_at + lease;
    let deadline = s.cfg.hard_timeout.map(|d| started + d);
    // At most one renew in flight, polled beside the handler and the deadlines.
    let mut renewing: Option<BoxFuture<'_, RenewOut>> = None;
    let (queue, job_ref) = (&s.queue, &job);
    let ended = loop {
        tokio::select! {
            biased;
            res = &mut fut => break Ended::Finished(res),
            _ = sleep_until_opt(deadline), if deadline.is_some() => break Ended::TimedOut,
            _ = tokio::time::sleep_until(lease_until - grace) => {
                break Ended::LeaseLost("not renewed before the lease deadline");
            }
            (at, out) = async { renewing.as_mut().expect("guarded").await },
                if renewing.is_some() =>
            {
                renewing = None;
                match out {
                    Ok(Ok(true)) => lease_until = at + lease,
                    Ok(Ok(false)) => break Ended::LeaseLost("another owner holds the lease"),
                    Ok(Err(e)) => tracing::warn!(
                        job_id = job.id,
                        kind = %job.kind,
                        error = %e,
                        "lease renewal failed"
                    ),
                    Err(_) => {
                        break Ended::LeaseLost("renewal did not finish before the lease deadline");
                    }
                }
            }
            _ = renew.tick(), if renewing.is_none() => {
                let until = lease_until;
                renewing = Some(Box::pin(async move {
                    let at = Instant::now();
                    (at, tokio::time::timeout_at(until, queue.renew(job_ref, lease)).await)
                }));
            }
        }
    };
    // An unfinished renew is dropped (its transaction rolls back); nothing more is renewed.
    drop(renewing);

    match ended {
        Ended::LeaseLost(why) => {
            token.cancel();
            bump(&s.counters.lease_lost);
            tracing::warn!(
                job_id = job.id,
                kind = %job.kind,
                attempt = job.attempts,
                reason = why,
                "lease lost; handler cancelled and its result dropped"
            );
            let _ = tokio::time::timeout(grace, &mut fut).await;
            drop(fut);
        }
        Ended::TimedOut => {
            token.cancel();
            let _ = tokio::time::timeout(grace, &mut fut).await;
            drop(fut);
            bump(&s.counters.timed_out);
            let limit = s.cfg.hard_timeout.unwrap_or_default();
            let err = JobError::Retryable {
                message: format!("job exceeded its hard timeout of {limit:?}"),
                code: Some("timeout".to_owned()),
            };
            record_failure(&s, &job, err).await;
        }
        Ended::Finished(Ok(Ok(_))) => match s.queue.complete(&job).await {
            Ok(true) => {
                bump(&s.counters.completed);
                tracing::debug!(
                    job_id = job.id,
                    kind = %job.kind,
                    attempt = job.attempts,
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "job done"
                );
            }
            Ok(false) => {
                bump(&s.counters.lease_lost);
                tracing::warn!(
                    job_id = job.id,
                    kind = %job.kind,
                    attempt = job.attempts,
                    "lease lost before completion; result dropped"
                );
            }
            Err(e) => tracing::error!(
                job_id = job.id,
                kind = %job.kind,
                attempt = job.attempts,
                error = %e,
                "completing the job failed; if it did not commit, the reaper recovers it"
            ),
        },
        Ended::Finished(Ok(Err(err))) => record_failure(&s, &job, err).await,
        Ended::Finished(Err(panic)) => {
            let err = JobError::Retryable {
                message: format!("handler panicked: {}", panic_message(panic.as_ref())),
                code: Some("panic".to_owned()),
            };
            record_failure(&s, &job, err).await;
        }
    }
}

async fn sleep_until_opt(deadline: Option<Instant>) {
    match deadline {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

fn panic_message(p: &(dyn Any + Send)) -> String {
    if let Some(s) = p.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = p.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_owned()
    }
}

/// `Queue::fail`, then logs (job id, kind and attempt), meters and calls `on_dead` when dead.
/// After `force`, a retryable failure (handler error, hard timeout or panic) is most likely the
/// cancellation itself, so it is recorded as a release that uses no attempt; an `Unrecoverable`
/// one is the handler's own verdict and stays failed.
async fn record_failure(s: &Arc<Shared>, job: &Job, err: JobError) {
    let err = match err {
        JobError::Retryable { .. } if s.force.is_cancelled() => JobError::Release {
            delay: Duration::ZERO,
            reason: "shutdown".to_owned(),
        },
        other => other,
    };
    let code = match &err {
        JobError::Retryable { code, .. } | JobError::Unrecoverable { code, .. } => {
            code.clone().unwrap_or_default()
        }
        JobError::Release { reason, .. } => reason.clone(),
    };
    let outcome = match s.queue.fail(job, &err, s.cfg.backoff).await {
        Ok(o) => o,
        Err(e) => {
            tracing::error!(
                job_id = job.id,
                kind = %job.kind,
                attempt = job.attempts,
                error = %err,
                queue_error = %e,
                "recording the failure failed; if it did not commit, the reaper recovers the job"
            );
            return;
        }
    };
    let name = match &outcome {
        FailOutcome::Released { run_at } => {
            bump(&s.counters.released);
            if let JobError::Release { delay, reason } = &err {
                tracing::warn!(
                    job_id = job.id,
                    kind = %job.kind,
                    attempt = job.attempts,
                    reason = %reason,
                    delay_ms = delay.as_millis() as u64,
                    run_at = %run_at,
                    "job released without using an attempt"
                );
            }
            return;
        }
        FailOutcome::LeaseLost => {
            bump(&s.counters.lease_lost);
            tracing::warn!(
                job_id = job.id,
                kind = %job.kind,
                attempt = job.attempts,
                error = %err,
                "lease lost before the failure was recorded; result dropped"
            );
            return;
        }
        FailOutcome::Retried { .. } => {
            bump(&s.counters.retried);
            "retried"
        }
        FailOutcome::Failed => {
            bump(&s.counters.failed);
            "failed"
        }
        FailOutcome::Dead => {
            bump(&s.counters.dead);
            "dead"
        }
    };
    tracing::error!(
        job_id = job.id,
        kind = %job.kind,
        attempt = job.attempts,
        max_attempts = job.max_attempts,
        error = %err,
        error_code = %code,
        outcome = name,
        "job failed"
    );
    if outcome == FailOutcome::Dead {
        // The same bound as the stored `last_error` (TS parity).
        let reason = truncate_chars(&err.to_string(), MAX_ERROR_CHARS);
        s.handler.on_dead(job, &reason).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    fn mark(id: i64, beat: i64) -> LeaseMark {
        LeaseMark {
            id,
            lease_token: Uuid::from_u128(id as u128),
            heartbeat_at: OffsetDateTime::from_unix_timestamp(beat).unwrap(),
        }
    }

    const GRACE: Duration = Duration::from_secs(5);

    #[test]
    fn a_lease_is_confirmed_only_after_looking_expired_unchanged_for_the_grace() {
        let mut t = StallTracker::default();
        let t0 = Instant::now();
        assert!(t.observe(vec![mark(1, 100)], t0, GRACE).is_empty());
        let later = t0 + Duration::from_secs(4);
        assert!(t.observe(vec![mark(1, 100)], later, GRACE).is_empty());
        let after = t0 + GRACE;
        assert_eq!(
            t.observe(vec![mark(1, 100)], after, GRACE),
            vec![mark(1, 100)]
        );
    }

    #[test]
    fn a_new_heartbeat_or_token_starts_over() {
        let mut t = StallTracker::default();
        let t0 = Instant::now();
        t.observe(vec![mark(1, 100)], t0, GRACE);
        // Renewed in between (as after a forward clock step on a live lease).
        let t1 = t0 + Duration::from_secs(6);
        assert!(t.observe(vec![mark(1, 101)], t1, GRACE).is_empty());
        let mut reclaimed = mark(1, 101);
        reclaimed.lease_token = Uuid::from_u128(99);
        let t2 = t1 + Duration::from_secs(6);
        assert!(t.observe(vec![reclaimed.clone()], t2, GRACE).is_empty());
        let t3 = t2 + GRACE;
        assert_eq!(
            t.observe(vec![reclaimed.clone()], t3, GRACE),
            vec![reclaimed]
        );
    }

    #[test]
    fn a_lease_that_stops_looking_expired_is_forgotten() {
        let mut t = StallTracker::default();
        let t0 = Instant::now();
        t.observe(vec![mark(1, 100), mark(2, 100)], t0, GRACE);
        t.observe(vec![mark(2, 100)], t0 + Duration::from_secs(1), GRACE);
        // Job 1 looks expired again later: its clock starts over.
        let t2 = t0 + Duration::from_secs(6);
        assert_eq!(
            t.observe(vec![mark(1, 100), mark(2, 100)], t2, GRACE),
            vec![mark(2, 100)]
        );
    }

    #[test]
    fn a_zero_grace_confirms_at_once() {
        let mut t = StallTracker::default();
        assert_eq!(
            t.observe(vec![mark(7, 1)], Instant::now(), Duration::ZERO),
            vec![mark(7, 1)]
        );
    }
}
