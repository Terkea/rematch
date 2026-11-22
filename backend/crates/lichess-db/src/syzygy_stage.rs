//! Staging the Syzygy tables on a local disk (R2 Task 35, ADR-R2-25).
//!
//! Each probe sends `ucinewgame`, and Stockfish then re-opens the whole table set (ADR-R2-12).
//! On a slow mount, such as Docker Desktop's bind mount of a Windows folder, a probe cost about
//! 0.5 s idle and 9.5 s under load. A verified copy on the container's own disk answers in
//! milliseconds with the same bytes. A wrong table would give wrong answers that `position_tb`
//! keeps for ever, so a file is only exposed under a table's name once it is proven whole.
//!
//! One pass ([`stage_tables_with`]):
//! 1. **Lock.** An exclusive lock on `<stage>/.lock` is held for the whole pass. A second
//!    worker or CLI process on the same volume waits, then finds the files current.
//! 2. Leftover temporary copies (`.*.partial`, from a crash or a full disk) are removed.
//! 3. **Fast check.** A table is current when the stage manifest (`.staged.json`) has an entry
//!    matching the staged file's size and mtime and the source's size and mtime.
//! 4. **Verification.** Otherwise the staged file, if any, is hashed and kept only when its
//!    SHA-256 equals the expected one.
//! 5. **Copy.** Otherwise the source is copied to a unique `create_new` temporary name, fsynced,
//!    re-read and hashed, then renamed into place, and the directory is fsynced.
//!
//! The expected hash is the pinned manifest's when it names the file (ADR-R2-22). A source that
//! fails it is refused with `InvalidData`. Otherwise it is the source's own hash.
//!
//! 6. Tables no longer in the source are removed. The stage manifest is written last (temporary
//!    name, fsync, rename, directory fsync), so a crash before it only costs a re-hash.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::syzygy::Manifest;

/// The stage manifest's file name.
pub const STAGE_MANIFEST: &str = ".staged.json";
/// The lock file's name.
pub const STAGE_LOCK: &str = ".lock";

/// What a pass did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StageReport {
    /// Tables copied (missing, changed, or failing verification).
    pub copied: usize,
    /// Tables already current (fast check or a verified hash).
    pub current: usize,
    /// Tables removed because the source no longer has them.
    pub removed: usize,
    /// Bytes copied.
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct StagedFile {
    size: u64,
    mtime_ns: u128,
    src_size: u64,
    src_mtime_ns: u128,
    sha256: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct StageManifest {
    files: BTreeMap<String, StagedFile>,
}

fn is_table(name: &str) -> bool {
    name.ends_with(".rtbw") || name.ends_with(".rtbz")
}

fn mtime_ns(m: &fs::Metadata) -> io::Result<u128> {
    Ok(m.modified()?
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos()))
}

fn sha_of(path: &Path) -> io::Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

fn sync_dir(dir: &Path) -> io::Result<()> {
    // Directories can be opened read-only and fsynced on Unix; elsewhere this is best effort.
    match File::open(dir) {
        Ok(d) => match d.sync_all() {
            Ok(()) => Ok(()),
            Err(e) if cfg!(unix) => Err(e),
            Err(_) => Ok(()),
        },
        Err(e) if cfg!(unix) => Err(e),
        Err(_) => Ok(()),
    }
}

fn unique_tmp(dst: &Path, name: &str) -> std::path::PathBuf {
    dst.join(format!(
        ".{name}.{}.{}.partial",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ))
}

/// Copies `src` to a fresh temporary file in `dst`, fsyncs it, re-reads it and checks it hashes
/// to `want` (or, without `want`, to the source bytes it copied). Returns the temporary path and
/// the hash. The temporary file is removed on any failure.
fn copy_verified(
    src: &Path,
    dst: &Path,
    name: &str,
    want: Option<&str>,
) -> io::Result<(std::path::PathBuf, String)> {
    let tmp = unique_tmp(dst, name);
    let result = (|| {
        let mut out = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        let mut input = File::open(src)?;
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = input.read(&mut buf)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
            out.write_all(&buf[..n])?;
        }
        out.sync_all()?;
        drop(out);
        let copied = hex::encode(h.finalize());
        if let Some(want) = want
            && copied != want
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{name}: the source does not match the pinned SHA-256"),
            ));
        }
        if sha_of(&tmp)? != copied {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{name}: the staged copy does not read back as written"),
            ));
        }
        Ok(copied)
    })();
    match result {
        Ok(sha) => Ok((tmp, sha)),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// [`stage_tables_with`] against the pinned 3-4-5 manifest. Blocking: call it from
/// `spawn_blocking`.
pub fn stage_tables(src: &Path, dst: &Path) -> io::Result<StageReport> {
    stage_tables_with(src, dst, &Manifest::pinned())
}

/// One staging pass of `src` into `dst` (see the module docs). Blocking.
pub fn stage_tables_with(src: &Path, dst: &Path, pinned: &Manifest) -> io::Result<StageReport> {
    let pinned: HashMap<&str, &str> = pinned
        .entries()
        .iter()
        .map(|e| {
            (
                e.file.rsplit('/').next().unwrap_or(&e.file),
                e.sha256.as_str(),
            )
        })
        .collect();
    // Read the source before touching the stage: a missing source changes nothing.
    let mut sources: Vec<(String, fs::Metadata)> = Vec::new();
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if is_table(&name) {
            sources.push((name, entry.metadata()?));
        }
    }
    sources.sort_by(|a, b| a.0.cmp(&b.0));

    fs::create_dir_all(dst)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dst.join(STAGE_LOCK))?;
    lock.lock()?;

    for entry in fs::read_dir(dst)? {
        let name = entry?.file_name();
        if let Some(n) = name.to_str()
            && n.starts_with('.')
            && n.ends_with(".partial")
        {
            fs::remove_file(dst.join(n))?;
        }
    }
    let old: StageManifest = fs::read(dst.join(STAGE_MANIFEST))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let mut manifest = StageManifest::default();
    let mut report = StageReport::default();
    for (name, src_meta) in &sources {
        let src_path = src.join(name);
        let target = dst.join(name);
        let (src_size, src_mtime) = (src_meta.len(), mtime_ns(src_meta)?);
        let pinned_sha = pinned.get(name.as_str()).copied();
        let staged_meta = fs::metadata(&target).ok().filter(|m| m.is_file());
        if let (Some(e), Some(m)) = (old.files.get(name), &staged_meta)
            && e.size == m.len()
            && e.mtime_ns == mtime_ns(m)?
            && e.src_size == src_size
            && e.src_mtime_ns == src_mtime
            && pinned_sha.is_none_or(|p| p == e.sha256)
        {
            manifest.files.insert(name.clone(), e.clone());
            report.current += 1;
            continue;
        }
        // Slow path: the expected hash, then the staged file only if it hashes to it.
        let expected = match pinned_sha {
            Some(p) => p.to_owned(),
            None => sha_of(&src_path)?,
        };
        if let Some(m) = &staged_meta
            && m.len() == src_size
            && sha_of(&target)? == expected
        {
            if let Some(p) = pinned_sha
                && sha_of(&src_path)? != p
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{name}: the source does not match the pinned SHA-256"),
                ));
            }
            manifest.files.insert(
                name.clone(),
                StagedFile {
                    size: m.len(),
                    mtime_ns: mtime_ns(m)?,
                    src_size,
                    src_mtime_ns: src_mtime,
                    sha256: expected,
                },
            );
            report.current += 1;
            continue;
        }
        let (tmp, sha) = copy_verified(&src_path, dst, name, Some(&expected))?;
        fs::rename(&tmp, &target)?;
        sync_dir(dst)?;
        let m = fs::metadata(&target)?;
        manifest.files.insert(
            name.clone(),
            StagedFile {
                size: m.len(),
                mtime_ns: mtime_ns(&m)?,
                src_size,
                src_mtime_ns: src_mtime,
                sha256: sha,
            },
        );
        report.copied += 1;
        report.bytes += src_size;
    }
    // Tables the source no longer has.
    for entry in fs::read_dir(dst)? {
        let Ok(name) = entry?.file_name().into_string() else {
            continue;
        };
        if is_table(&name) && !manifest.files.contains_key(&name) {
            fs::remove_file(dst.join(&name))?;
            report.removed += 1;
        }
    }
    // The manifest last, atomically.
    let tmp = unique_tmp(dst, STAGE_MANIFEST.trim_start_matches('.'));
    let body = serde_json::to_vec(&manifest).map_err(io::Error::other)?;
    let written = (|| {
        let mut f = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(&body)?;
        f.sync_all()?;
        fs::rename(&tmp, dst.join(STAGE_MANIFEST))?;
        sync_dir(dst)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written?;
    lock.unlock()?;
    Ok(report)
}
