//! Ported from apps/worker/test/smoke.test.ts.

use std::path::Path;
use std::time::Duration;

use engine::EngineError;
use engine::version::stockfish_version;

#[tokio::test]
async fn is_stockfish_19() {
    let Some(sf) = testkit::stockfish() else {
        return;
    };
    let v = stockfish_version(&sf, Duration::from_secs(5))
        .await
        .unwrap();
    // `/^Stockfish 19\b/`
    let rest = v
        .strip_prefix("Stockfish 19")
        .unwrap_or_else(|| panic!("{v:?}"));
    assert!(
        rest.chars().next().is_none_or(|c| !c.is_alphanumeric()),
        "{v:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn times_out_when_no_banner_arrives() {
    // `cat` echoes `uci` back and never prints a Stockfish banner.
    let r = stockfish_version(Path::new("/bin/cat"), Duration::from_millis(300)).await;
    assert!(matches!(r, Err(EngineError::Timeout(_))), "{r:?}");
}

#[cfg(unix)]
#[tokio::test]
async fn fails_when_the_process_exits_before_the_banner() {
    let r = stockfish_version(Path::new("/bin/true"), Duration::from_secs(5)).await;
    assert!(matches!(r, Err(EngineError::Crashed)), "{r:?}");
}

#[tokio::test]
async fn fails_to_spawn_a_missing_binary() {
    let r = stockfish_version(Path::new("/nonexistent/stockfish"), Duration::from_secs(1)).await;
    assert!(matches!(r, Err(EngineError::Spawn(_))), "{r:?}");
}

/// A throwaway fake engine in its own temp directory, removed on drop.
///
/// The script is written by a child `sh`, never by this process. Exec'ing a
/// file that is open for writing fails with `ETXTBSY`, and this test binary
/// forks from many threads at once (every `Command::spawn`): a forked child
/// inherits our open fds until it execs, so a script written with
/// `std::fs::write` can still count as "open for writing" in another test's
/// child when we exec it. A writer that is a separate, already-reaped process
/// leaves no such fd behind.
#[cfg(unix)]
struct FakeEngine {
    dir: std::path::PathBuf,
    path: std::path::PathBuf,
}

#[cfg(unix)]
impl FakeEngine {
    fn new(name: &str, body: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("sfv-{}-{name}", std::process::id()));
        // A leftover from a killed run that had the same (recycled) pid.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("engine");
        let status = std::process::Command::new("sh")
            .args([
                "-c",
                "printf '%s\n' \"$1\" > \"$2\" && chmod 755 \"$2\"",
                "sh",
            ])
            .arg(format!("#!/bin/sh\n{body}"))
            .arg(&path)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "could not write the fake engine: {status}"
        );
        Self { dir, path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(unix)]
impl Drop for FakeEngine {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

// The trailing `exec sleep 30` replaces the shell, so the probe's kill ends
// the sleep itself: no orphan outlives the test.
#[cfg(unix)]
#[tokio::test]
async fn falls_back_to_the_id_name_line() {
    let p = FakeEngine::new(
        "idname",
        "read l; echo 'id name Stockfish 19'; echo 'id author x'; echo uciok; exec sleep 30",
    );
    let started = std::time::Instant::now();
    let v = stockfish_version(p.path(), Duration::from_secs(10))
        .await
        .unwrap();
    assert_eq!(v, "Stockfish 19");
    // Returned at `uciok`, not at the timeout.
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[cfg(unix)]
#[tokio::test]
async fn returns_early_at_uciok_without_a_stockfish_name() {
    let p = FakeEngine::new(
        "other",
        "read l; echo 'id name Komodo'; echo uciok; exec sleep 30",
    );
    let started = std::time::Instant::now();
    let r = stockfish_version(p.path(), Duration::from_secs(10)).await;
    assert!(matches!(r, Err(EngineError::Spawn(_))), "{r:?}");
    assert!(started.elapsed() < Duration::from_secs(5));
}
