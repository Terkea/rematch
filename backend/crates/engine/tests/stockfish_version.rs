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
