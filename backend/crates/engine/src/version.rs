//! Stockfish version probe (port of `apps/worker/src/stockfish-version.ts`).

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::types::EngineError;

/// Start the binary, send `uci`, and return the first stdout line that starts
/// with `Stockfish` (for example `Stockfish 19 by the Stockfish developers`),
/// trimmed. The process is killed and reaped afterwards.
///
/// Errors: `Spawn` if it cannot start, `Crashed` if its output ends before
/// the banner, `Timeout(timeout)` if no banner arrives within `timeout`.
pub async fn stockfish_version(path: &Path, timeout: Duration) -> Result<String, EngineError> {
    let mut child = Command::new(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| EngineError::Spawn(e.to_string()))?;
    let mut stdin = child.stdin.take().ok_or(EngineError::Crashed)?;
    let stdout = child.stdout.take().ok_or(EngineError::Crashed)?;
    let read = async {
        // A process that has already exited closes stdin; its output decides.
        let _ = stdin.write_all(b"uci\n").await;
        let _ = stdin.flush().await;
        let mut reader = BufReader::new(stdout);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf).await {
                Ok(0) | Err(_) => return Err(EngineError::Crashed),
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buf);
                    let line = line.trim();
                    if line.starts_with("Stockfish") {
                        return Ok(line.to_string());
                    }
                }
            }
        }
    };
    let result = match tokio::time::timeout(timeout, read).await {
        Ok(r) => r,
        Err(_) => Err(EngineError::Timeout(timeout)),
    };
    let _ = child.start_kill();
    let _ = child.wait().await;
    result
}
