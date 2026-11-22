//! Staging the Syzygy tables on a local disk (R2 Task 35, ADR-R2-25).
//!
//! Each probe sends `ucinewgame`, and Stockfish then re-opens the whole table set (ADR-R2-12).
//! On a slow mount, such as Docker Desktop's bind mount of a Windows folder, that made each probe
//! cost about 0.5 s on an idle host and 9.5 s under load. That was three quarters of a game's
//! enrichment. A copy on the container's own disk (a named volume) answers in milliseconds. The
//! files are the same bytes, so the answers are the same.
//!
//! The copy is incremental: a file whose size already matches is kept. Each copy is written to a
//! temporary name and renamed, so a crash never leaves a truncated table under a table's name.

use std::io;
use std::path::Path;

/// What [`stage_tables`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StageReport {
    /// Files copied (missing, or a different size).
    pub copied: usize,
    /// Files already current.
    pub current: usize,
    /// Bytes copied.
    pub bytes: u64,
}

fn is_table(name: &str) -> bool {
    name.ends_with(".rtbw") || name.ends_with(".rtbz")
}

/// Copies every `.rtbw` and `.rtbz` file of `src` into `dst` (created if missing). Blocking: call
/// it from `spawn_blocking`.
pub fn stage_tables(src: &Path, dst: &Path) -> io::Result<StageReport> {
    std::fs::create_dir_all(dst)?;
    let mut report = StageReport::default();
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if !is_table(name_str) {
            continue;
        }
        let size = entry.metadata()?.len();
        let target = dst.join(&name);
        if std::fs::metadata(&target).is_ok_and(|m| m.is_file() && m.len() == size) {
            report.current += 1;
            continue;
        }
        let tmp = dst.join(format!(".{name_str}.partial"));
        std::fs::copy(entry.path(), &tmp)?;
        std::fs::rename(&tmp, &target)?;
        report.copied += 1;
        report.bytes += size;
    }
    Ok(report)
}
