//! Port of apps/worker/test/recovery/shutdown.test.ts (signals come through an mpsc channel),
//! plus the final-attempt case: a shutdown on the job's last attempt uses no attempt.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::{RUY, add_game, analyse_job, deps, env_for, game, wait_until};
use db::games::AnalysisStatus;
use engine::{AnalyseOptions, Analyser, AnalysisResult, EngineError};
use futures::FutureExt;
use futures::future::BoxFuture;
use queue::queue::Queue;
use queue::runner::RunnerConfig;
use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use testkit::fake_engine::FakeAnalyser;
use tokio::sync::{Notify, mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use worker::shutdown::{
    Closable, Drainable, ShutdownHandles, ShutdownOptions, install_with_signals, shutdown,
};
use worker::start::{StartOptions, spawn_analyse_runner, start_workers};

type Log = Arc<Mutex<Vec<String>>>;

fn push(log: &Log, s: &str) {
    log.lock().unwrap().push(s.to_owned());
}

fn entries(log: &Log) -> Vec<String> {
    log.lock().unwrap().clone()
}

#[derive(Default)]
struct FakeRunner {
    log: Log,
    /// `wait_idle` waits for this before resolving.
    gate: Option<Arc<Notify>>,
    /// `wait_idle` never resolves.
    stuck: bool,
    stops: AtomicUsize,
}

impl Drainable for FakeRunner {
    fn stop(&self) {
        self.stops.fetch_add(1, Ordering::SeqCst);
        push(&self.log, "stop");
    }

    fn wait_idle(&self) -> BoxFuture<'_, ()> {
        push(&self.log, "wait_idle");
        async move {
            if self.stuck {
                futures::future::pending::<()>().await;
            }
            if let Some(gate) = &self.gate {
                gate.notified().await;
            }
        }
        .boxed()
    }

    fn force(&self) -> BoxFuture<'_, ()> {
        push(&self.log, "force");
        async {}.boxed()
    }
}

struct FakeClose {
    name: &'static str,
    log: Log,
    fail: bool,
    calls: Arc<AtomicUsize>,
}

impl FakeClose {
    fn new(name: &'static str, log: &Log) -> FakeClose {
        FakeClose {
            name,
            log: log.clone(),
            fail: false,
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl Closable for FakeClose {
    fn close(&self) -> BoxFuture<'_, anyhow::Result<()>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        push(&self.log, self.name);
        let fail = self.fail;
        async move {
            if fail {
                anyhow::bail!("boom");
            }
            Ok(())
        }
        .boxed()
    }
}

fn opts(grace: Duration) -> ShutdownOptions {
    ShutdownOptions { grace }
}

#[tokio::test]
async fn stops_and_drains_the_runners_then_closes_the_engines_and_the_db_and_exits_0() {
    let log = Log::default();
    let handles = ShutdownHandles {
        runners: vec![Box::new(FakeRunner {
            log: log.clone(),
            ..FakeRunner::default()
        })],
        engines: Box::new(FakeClose::new("engines", &log)),
        db: Box::new(FakeClose::new("db", &log)),
    };
    assert_eq!(shutdown(handles, opts(Duration::from_secs(25))).await, 0);
    assert_eq!(entries(&log), ["stop", "wait_idle", "engines", "db"]);
}

#[tokio::test]
async fn a_second_signal_does_not_close_anything_twice() {
    let log = Log::default();
    let gate = Arc::new(Notify::new());
    let runner = Arc::new(FakeRunner {
        log: log.clone(),
        gate: Some(gate.clone()),
        ..FakeRunner::default()
    });
    struct Shared(Arc<FakeRunner>);
    impl Drainable for Shared {
        fn stop(&self) {
            self.0.stop()
        }
        fn wait_idle(&self) -> BoxFuture<'_, ()> {
            self.0.wait_idle()
        }
        fn force(&self) -> BoxFuture<'_, ()> {
            self.0.force()
        }
    }
    let engines = FakeClose::new("engines", &log);
    let engine_calls = engines.calls.clone();
    let handles = ShutdownHandles {
        runners: vec![Box::new(Shared(runner.clone()))],
        engines: Box::new(engines),
        db: Box::new(FakeClose::new("db", &log)),
    };
    let (signals, rx) = mpsc::unbounded_channel();
    let (exit_tx, exit_rx) = oneshot::channel();
    let _trigger = install_with_signals(
        handles,
        opts(Duration::from_secs(25)),
        rx,
        Box::new(move |code| {
            let _ = exit_tx.send(code);
        }),
    );
    for s in ["SIGTERM", "SIGTERM", "SIGINT"] {
        signals.send(s).unwrap();
    }
    // Let every signal reach the handler before the drain can finish.
    tokio::time::sleep(Duration::from_millis(100)).await;
    gate.notify_one();
    let code = tokio::time::timeout(Duration::from_secs(10), exit_rx)
        .await
        .expect("exit within the grace")
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(runner.stops.load(Ordering::SeqCst), 1);
    assert_eq!(engine_calls.load(Ordering::SeqCst), 1);
    assert_eq!(entries(&log), ["stop", "wait_idle", "engines", "db"]);
}

#[tokio::test]
async fn exits_non_zero_when_a_close_fails_and_still_closes_the_rest() {
    let log = Log::default();
    let mut engines = FakeClose::new("engines", &log);
    engines.fail = true;
    let handles = ShutdownHandles {
        runners: vec![Box::new(FakeRunner {
            log: log.clone(),
            ..FakeRunner::default()
        })],
        engines: Box::new(engines),
        db: Box::new(FakeClose::new("db", &log)),
    };
    assert_eq!(shutdown(handles, opts(Duration::from_secs(25))).await, 1);
    assert_eq!(entries(&log), ["stop", "wait_idle", "engines", "db"]);
}

#[tokio::test]
async fn does_not_wait_past_the_grace_for_a_stuck_runner() {
    let log = Log::default();
    let handles = ShutdownHandles {
        runners: vec![Box::new(FakeRunner {
            log: log.clone(),
            stuck: true,
            ..FakeRunner::default()
        })],
        engines: Box::new(FakeClose::new("engines", &log)),
        db: Box::new(FakeClose::new("db", &log)),
    };
    let started = Instant::now();
    let code = shutdown(handles, opts(Duration::from_millis(300))).await;
    assert!(
        started.elapsed() < Duration::from_millis(300 + 700),
        "{:?}",
        started.elapsed()
    );
    // A runner that never drained is not a failed close: it was forced, as designed.
    assert_eq!(code, 0);
    // The engines close early to abort in-flight work, then the runner is forced.
    assert_eq!(
        entries(&log),
        ["stop", "wait_idle", "engines", "force", "db"]
    );
}

// ---------------------------------------------------------------------------------------------
// Real stack

/// A long game: 300k nodes per position keeps the engine busy well past the grace period.
const LONG: &str = "1. e4 c5 2. Nf3 d6 3. d4 cxd4 4. Nxd4 Nf6 5. Nc3 a6 6. Be3 e5 7. Nb3 Be6 \
    8. f3 Be7 9. Qd2 O-O 10. O-O-O Nbd7 11. g4 b5 12. g5 b4 13. Ne2 Ne8 14. f4 a5 15. f5 a4 *";

fn pid_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn sigterm_loses_nothing(pool_opts: PgPoolOptions, opts_: PgConnectOptions) {
    let Some(env) = env_for(&opts_) else {
        return;
    };
    let pool: PgPool = pool_opts.connect_with(opts_).await.unwrap();
    let (acct, id) = add_game(&pool, LONG, "sigterm").await;
    let running = start_workers(
        &env,
        StartOptions {
            pool_size: Some(1),
            runner: Some(RunnerConfig::fast_for_tests()),
            with_reconciler: false,
        },
    )
    .await
    .expect("start_workers");
    let pids = running.engines.pids();
    assert_eq!(pids.len(), 1);
    assert!(
        analysis::jobs::analyse::enqueue_analyse(&running.queue, id, acct)
            .await
            .unwrap()
    );
    wait_until("analysis to start", Duration::from_secs(20), || {
        let pool = pool.clone();
        async move { (game(&pool, id).await.analysis_status == AnalysisStatus::Analysing).then_some(()) }
    })
    .await;

    let grace = Duration::from_secs(2);
    let (signals, rx) = mpsc::unbounded_channel();
    let (exit_tx, exit_rx) = oneshot::channel();
    let started = Instant::now();
    let _trigger = install_with_signals(
        running.into_shutdown_handles(),
        ShutdownOptions { grace },
        rx,
        Box::new(move |code| {
            let _ = exit_tx.send(code);
        }),
    );
    signals.send("SIGTERM").unwrap();
    let code = tokio::time::timeout(Duration::from_secs(20), exit_rx)
        .await
        .expect("the shutdown finishes")
        .unwrap();
    assert!(
        started.elapsed() < grace + Duration::from_millis(500),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(code, 0);

    // 30 plies at 300k nodes cannot finish within the drain window: the analysis is aborted,
    // the game is pending again and its job is queued without using an attempt.
    assert_eq!(
        game(&pool, id).await.analysis_status,
        AnalysisStatus::Pending
    );
    assert_eq!(analyse_job(&pool, id).await, ("queued".to_owned(), 0));
    for pid in pids {
        assert!(!pid_alive(pid), "engine {pid} still running");
    }

    // Nothing is lost: the next worker (here with a fake engine) finishes the game.
    let next = spawn_analyse_runner(
        Queue::new(pool.clone()),
        deps(&pool, Arc::new(FakeAnalyser::new())),
        &RunnerConfig::fast_for_tests(),
        1,
    );
    wait_until("the next worker to finish the game", Duration::from_secs(20), || {
        let pool = pool.clone();
        async move { (game(&pool, id).await.analysis_status == AnalysisStatus::Done).then_some(()) }
    })
    .await;
    next.shutdown(Duration::from_secs(5)).await;
}

/// Answers each call after `delay`, or with `PoolClosed` once closed (like the engine pool).
struct SlowAnalyser {
    delay: Duration,
    closed: CancellationToken,
    inner: FakeAnalyser,
}

impl Analyser for SlowAnalyser {
    fn analyse<'a>(
        &'a self,
        fen: &'a str,
        opts: AnalyseOptions,
    ) -> BoxFuture<'a, Result<AnalysisResult, EngineError>> {
        async move {
            tokio::select! {
                () = self.closed.cancelled() => Err(EngineError::PoolClosed),
                () = tokio::time::sleep(self.delay) => self.inner.analyse(fen, opts).await,
            }
        }
        .boxed()
    }
}

struct CloseSlow(Arc<SlowAnalyser>);

impl Closable for CloseSlow {
    fn close(&self) -> BoxFuture<'_, anyhow::Result<()>> {
        self.0.closed.cancel();
        async { Ok(()) }.boxed()
    }
}

struct Noop;

impl Closable for Noop {
    fn close(&self) -> BoxFuture<'_, anyhow::Result<()>> {
        async { Ok(()) }.boxed()
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn sigterm_on_final_attempt_then_fresh_worker_completes(
    pool_opts: PgPoolOptions,
    opts_: PgConnectOptions,
) {
    let env = env_for(&opts_);
    let pool: PgPool = pool_opts.connect_with(opts_).await.unwrap();
    let (acct, id) = add_game(&pool, RUY, "final-sigterm").await;
    let queue = Queue::new(pool.clone());
    assert!(
        analysis::jobs::analyse::enqueue_analyse(&queue, id, acct)
            .await
            .unwrap()
    );
    // Two attempts used already: the next claim is the last of 3.
    sqlx::query("UPDATE jobs SET attempts = 2 WHERE dedupe_key = $1")
        .bind(queue::analyse_key(id))
        .execute(&pool)
        .await
        .unwrap();
    let slow = Arc::new(SlowAnalyser {
        delay: Duration::from_millis(300),
        closed: CancellationToken::new(),
        inner: FakeAnalyser::new(),
    });
    let runner = spawn_analyse_runner(
        queue.clone(),
        deps(&pool, slow.clone()),
        &RunnerConfig::fast_for_tests(),
        1,
    );
    wait_until("analysis to start", Duration::from_secs(20), || {
        let pool = pool.clone();
        async move { (game(&pool, id).await.analysis_status == AnalysisStatus::Analysing).then_some(()) }
    })
    .await;
    assert_eq!(analyse_job(&pool, id).await, ("running".to_owned(), 3));

    let handles = ShutdownHandles {
        runners: vec![Box::new(runner)],
        engines: Box::new(CloseSlow(slow)),
        db: Box::new(Noop),
    };
    assert_eq!(
        shutdown(
            handles,
            ShutdownOptions {
                grace: Duration::from_secs(2)
            }
        )
        .await,
        0
    );
    assert_eq!(analyse_job(&pool, id).await, ("queued".to_owned(), 2));
    assert_eq!(
        game(&pool, id).await.analysis_status,
        AnalysisStatus::Pending
    );

    // A fresh worker completes it on its final attempt.
    let Some(env) = env else {
        return;
    };
    let running = start_workers(
        &env,
        StartOptions {
            pool_size: Some(1),
            runner: Some(RunnerConfig::fast_for_tests()),
            with_reconciler: false,
        },
    )
    .await
    .expect("start_workers");
    let done = wait_until(
        "the fresh worker to finish",
        Duration::from_secs(60),
        || {
            let pool = pool.clone();
            async move {
                let g = game(&pool, id).await;
                (g.analysis_status == AnalysisStatus::Done).then_some(g)
            }
        },
    )
    .await;
    assert_eq!(done.analysis_attempts, 2);
    // The job completes right after the game's done commit.
    let job = wait_until("the job to complete", Duration::from_secs(10), || {
        let pool = pool.clone();
        async move {
            let job = analyse_job(&pool, id).await;
            (job.0 == "done").then_some(job)
        }
    })
    .await;
    assert_eq!(job, ("done".to_owned(), 3));
    assert_eq!(
        shutdown(
            running.into_shutdown_handles(),
            ShutdownOptions {
                grace: Duration::from_secs(5)
            }
        )
        .await,
        0
    );
}
