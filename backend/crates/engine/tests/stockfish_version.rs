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

/// A throwaway executable script in its own temp directory.
#[cfg(unix)]
fn script(name: &str, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("sfv-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("engine");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
#[tokio::test]
async fn falls_back_to_the_id_name_line() {
    let p = script(
        "idname",
        "read l; echo 'id name Stockfish 19'; echo 'id author x'; echo uciok; sleep 30",
    );
    let started = std::time::Instant::now();
    let v = stockfish_version(&p, Duration::from_secs(10))
        .await
        .unwrap();
    assert_eq!(v, "Stockfish 19");
    // Returned at `uciok`, not at the timeout.
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[cfg(unix)]
#[tokio::test]
async fn returns_early_at_uciok_without_a_stockfish_name() {
    let p = script(
        "other",
        "read l; echo 'id name Komodo'; echo uciok; sleep 30",
    );
    let started = std::time::Instant::now();
    let r = stockfish_version(&p, Duration::from_secs(10)).await;
    assert!(matches!(r, Err(EngineError::Spawn(_))), "{r:?}");
    assert!(started.elapsed() < Duration::from_secs(5));
}
