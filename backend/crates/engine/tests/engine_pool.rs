//! Ported from apps/worker/test/engine/engine-pool.test.ts, plus Rust-only
//! supervision cases (timeouts, cancelled callers, Hash option).

mod common;

use std::sync::Arc;
use std::time::Duration;

use engine::pool::{EnginePool, PoolOptions};
use engine::{AnalyseOptions, Analyser, EngineError};

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

fn nodes(n: u64) -> AnalyseOptions {
    AnalyseOptions {
        nodes: n,
        multipv: 1,
        timeout: Duration::from_secs(30),
    }
}

async fn pool(size: usize) -> Option<Arc<EnginePool>> {
    let path = testkit::stockfish()?;
    Some(
        common::create_pool(PoolOptions {
            size,
            path,
            hash_mb: None,
        })
        .await
        .unwrap(),
    )
}

/// `sh`'s builtin kill: the image has no standalone kill binary.
fn kill9(pid: u32) {
    let _ = std::process::Command::new("sh")
        .args(["-c", &format!("kill -9 {pid}")])
        .status();
}

/// A reaped process has no `/proc` entry (a zombie still has one).
fn is_dead(pid: u32) -> bool {
    !std::path::Path::new(&format!("/proc/{pid}")).exists()
}

#[tokio::test]
async fn resolves_two_parallel_analyses() {
    let Some(pool) = pool(2).await else { return };
    let (a, b) = tokio::join!(
        pool.analyse(START, nodes(20_000)),
        pool.analyse(START, nodes(20_000))
    );
    assert_eq!(a.unwrap().lines.len(), 1);
    assert_eq!(b.unwrap().lines.len(), 1);
    pool.close().await;
}

#[tokio::test]
async fn pool_recovers_after_engine_crash() {
    let Some(pool) = pool(2).await else { return };
    let pids = pool.pids();
    let pid = *pids.first().expect("a live engine pid");
    kill9(pid);
    let results =
        futures::future::join_all((0..4).map(|_| pool.analyse(START, nodes(20_000)))).await;
    assert!(
        results
            .iter()
            .all(|r| r.as_ref().is_ok_and(|r| r.lines.len() == 1)),
        "{results:?}"
    );
    // The killed engine was replaced transparently: the pool is back to full size.
    let now = pool.pids();
    assert_eq!(now.len(), 2, "{now:?}");
    assert!(!now.contains(&pid), "dead pid {pid} still listed: {now:?}");
    pool.close().await;
}

#[tokio::test]
async fn retries_an_in_flight_job_once_when_the_engine_dies_mid_search() {
    let Some(pool) = pool(2).await else { return };
    let p = pool.analyse(START, nodes(3_000_000));
    let killer = async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        for pid in pool.pids() {
            kill9(pid);
        }
    };
    let (r, ()) = tokio::join!(p, killer);
    assert_eq!(r.unwrap().lines.len(), 1);
    pool.close().await;
}

#[tokio::test]
async fn rejects_invalid_fen_without_waiting_for_a_free_engine() {
    let Some(pool) = pool(1).await else { return };
    let busy = pool.analyse(START, nodes(3_000_000));
    let invalid = async {
        let r = tokio::time::timeout(
            Duration::from_secs(2),
            pool.analyse("not a fen", nodes(1000)),
        )
        .await
        .expect("an invalid FEN waited for the busy engine");
        assert!(matches!(r, Err(EngineError::InvalidFen { .. })), "{r:?}");
    };
    let (busy, ()) = tokio::join!(busy, invalid);
    busy.unwrap();
    pool.close().await;
}

#[tokio::test]
async fn rejects_queued_and_in_flight_jobs_on_close_leaves_no_processes_rejects_later_calls() {
    let Some(pool) = pool(1).await else { return };
    let pids = pool.pids();
    assert_eq!(pids.len(), 1);
    let in_flight = pool.analyse(START, nodes(50_000_000));
    let queued = pool.analyse(START, nodes(1000));
    let closer = async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        pool.close().await;
    };
    let (in_flight, queued, ()) = tokio::join!(in_flight, queued, closer);
    assert!(matches!(queued, Err(EngineError::PoolClosed)), "{queued:?}");
    assert!(
        matches!(in_flight, Err(EngineError::PoolClosed)),
        "{in_flight:?}"
    );
    for pid in pids {
        assert!(is_dead(pid), "engine {pid} still running after close");
    }
    let later = pool.analyse(START, nodes(1000)).await;
    assert!(matches!(later, Err(EngineError::PoolClosed)), "{later:?}");
    assert!(pool.pids().is_empty());
}

#[tokio::test]
async fn a_timed_out_engine_is_replaced_before_release() {
    let Some(pool) = pool(1).await else { return };
    let before = pool.pids();
    let r = pool
        .analyse(
            START,
            AnalyseOptions {
                nodes: 5_000_000_000,
                multipv: 1,
                timeout: Duration::from_millis(200),
            },
        )
        .await;
    // Timeouts are surfaced as they are, not retried.
    assert!(matches!(r, Err(EngineError::Timeout(_))), "{r:?}");
    let after = pool.pids();
    assert_eq!(after.len(), 1);
    assert_ne!(after, before, "the killed engine was not replaced");
    assert!(is_dead(before[0]));
    assert_eq!(
        pool.analyse(START, nodes(10_000))
            .await
            .unwrap()
            .lines
            .len(),
        1
    );
    pool.close().await;
}

#[tokio::test]
async fn a_cancelled_caller_returns_its_engine_to_the_pool() {
    let Some(pool) = pool(1).await else { return };
    tokio::select! {
        _ = pool.analyse(START, nodes(5_000_000_000)) => panic!("the long search finished"),
        _ = tokio::time::sleep(Duration::from_millis(300)) => {}
    }
    // The single engine is free again (the next call stops the abandoned search).
    let r = tokio::time::timeout(Duration::from_secs(20), pool.analyse(START, nodes(10_000)))
        .await
        .expect("the engine of a dropped call was never released");
    assert_eq!(r.unwrap().lines.len(), 1);
    pool.close().await;
}

#[tokio::test]
async fn a_cancelled_waiter_does_not_lose_the_engine() {
    let Some(pool) = pool(1).await else { return };
    let busy = pool.analyse(START, nodes(2_000_000));
    let waiter = async {
        // Queued behind `busy`, then abandoned.
        let _ = tokio::time::timeout(Duration::from_millis(100), pool.analyse(START, nodes(1000)))
            .await;
    };
    let (busy, ()) = tokio::join!(busy, waiter);
    busy.unwrap();
    let r = tokio::time::timeout(Duration::from_secs(20), pool.analyse(START, nodes(10_000)))
        .await
        .expect("the engine was lost to an abandoned waiter");
    assert_eq!(r.unwrap().lines.len(), 1);
    pool.close().await;
}

#[tokio::test]
async fn works_through_the_analyser_trait_with_a_hash_size() {
    let Some(path) = testkit::stockfish() else {
        return;
    };
    let pool = common::create_pool(PoolOptions {
        size: 1,
        path,
        hash_mb: Some(16),
    })
    .await
    .unwrap();
    let a: &dyn Analyser = pool.as_ref();
    assert_eq!(
        a.analyse(START, nodes(10_000)).await.unwrap().lines.len(),
        1
    );
    pool.close().await;
    // close is idempotent.
    pool.close().await;
}

#[tokio::test]
async fn create_fails_with_spawn_for_a_missing_binary() {
    let r = common::create_pool(PoolOptions {
        size: 2,
        path: "/nonexistent/stockfish".into(),
        hash_mb: None,
    })
    .await;
    assert!(matches!(r, Err(EngineError::Spawn(_))), "{:?}", r.err());
}

#[tokio::test]
async fn a_caller_cancelled_during_the_post_call_restart_keeps_the_slot() {
    let Some(pool) = pool(1).await else { return };
    // The engine times out at 200 ms and is killed; the caller is cancelled
    // ~60 ms later, while the pool is restarting the engine.
    let r = tokio::time::timeout(
        Duration::from_millis(260),
        pool.analyse(
            START,
            AnalyseOptions {
                nodes: 5_000_000_000,
                multipv: 1,
                timeout: Duration::from_millis(200),
            },
        ),
    )
    .await;
    assert!(
        r.is_err(),
        "the outer timeout did not cancel the call: {r:?}"
    );
    let r = tokio::time::timeout(Duration::from_secs(20), pool.analyse(START, nodes(10_000)))
        .await
        .expect("the slot was lost: the pool deadlocked");
    assert_eq!(r.unwrap().lines.len(), 1);
    pool.close().await;
}

#[tokio::test]
async fn an_engine_handed_to_a_dropped_waiter_is_recovered() {
    let Some(pool) = pool(1).await else { return };
    // `a` takes the only engine; `b` queues behind it.
    let mut a = Box::pin(pool.analyse(START, nodes(5_000_000_000)));
    assert!(futures::poll!(a.as_mut()).is_pending());
    let mut b = Box::pin(pool.analyse(START, nodes(1000)));
    assert!(futures::poll!(b.as_mut()).is_pending());
    // Dropping `a` hands the engine to `b`'s queue entry; dropping `b`
    // before it is polled again must put the engine back.
    drop(a);
    drop(b);
    let r = tokio::time::timeout(Duration::from_secs(20), pool.analyse(START, nodes(10_000)))
        .await
        .expect("the engine was lost to a dropped waiter");
    assert_eq!(r.unwrap().lines.len(), 1);
    pool.close().await;
}

#[tokio::test]
async fn an_abandoned_search_is_stopped_on_release() {
    let Some(pool) = pool(1).await else { return };
    let pid = pool.pids()[0];
    tokio::select! {
        _ = pool.analyse(START, nodes(5_000_000_000)) => panic!("the long search finished"),
        _ = tokio::time::sleep(Duration::from_millis(300)) => {}
    }
    // Once the stop lands, the idle engine burns no CPU.
    testkit::wait_for(
        "the abandoned search to stop",
        Duration::from_secs(10),
        Duration::from_millis(100),
        || async move {
            let a = cpu_time(pid)?;
            tokio::time::sleep(Duration::from_millis(300)).await;
            let b = cpu_time(pid)?;
            (b.saturating_sub(a) < Duration::from_millis(100)).then_some(())
        },
    )
    .await;
    pool.close().await;
}

#[tokio::test]
async fn pids_omits_an_engine_killed_while_idle() {
    let Some(pool) = pool(2).await else { return };
    let pids = pool.pids();
    assert_eq!(pids.len(), 2);
    let victim = pids[0];
    kill9(victim);
    testkit::wait_for(
        "the killed engine to leave pids()",
        Duration::from_secs(5),
        Duration::from_millis(20),
        || {
            let now = pool.pids();
            async move { (!now.contains(&victim)).then_some(()) }
        },
    )
    .await;
    assert_eq!(pool.pids().len(), 1);
    pool.close().await;
}

/// utime + stime from `/proc/<pid>/stat` (100 ticks/s).
fn cpu_time(pid: u32) -> Option<Duration> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = stat.get(stat.rfind(')')? + 2..)?;
    let f: Vec<&str> = rest.split_whitespace().collect();
    let ticks: u64 = f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?;
    Some(Duration::from_millis(ticks * 10))
}
