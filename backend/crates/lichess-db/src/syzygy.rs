//! Syzygy 3-4-5 fetch (R2 Task 22): every file is streamed to `<name>.part` while it is hashed,
//! renamed only when the SHA-256 matches the pinned manifest, and deleted otherwise. Files already
//! present with a matching hash are skipped, so a re-run makes no body requests.

use std::path::{Path, PathBuf};

use futures::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::download::DumpError;

/// The Lichess mirror root. The mirror has no single `3-4-5` directory: the manifest names carry
/// the `3-4-5-wdl/` or `3-4-5-dtz/` directory, and files land flat in the destination.
pub const DEFAULT_SYZYGY_BASE_URL: &str = "https://tablebase.lichess.ovh/tables/standard";

/// The pinned manifest of all 3-4-5 tables (145 WDL + 145 DTZ), taken from the mirror's own
/// `sha256` listing.
pub const MANIFEST_TEXT: &str = include_str!("../data/syzygy-345.sha256");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestEntry {
    pub sha256: String,
    /// Path relative to the base URL, e.g. `3-4-5-wdl/KPvK.rtbw`.
    pub file: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Manifest {
    entries: Vec<ManifestEntry>,
}

impl Manifest {
    /// Parses `<sha256>  <file>` lines (also `<sha256> *<file>`). Blank lines, `#` comments and
    /// malformed lines are ignored.
    pub fn parse(text: &str) -> Manifest {
        let entries = text
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.starts_with('#') {
                    return None;
                }
                let mut parts = line.split_whitespace();
                let (digest, name) = (parts.next()?, parts.next()?);
                if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return None;
                }
                Some(ManifestEntry {
                    sha256: digest.to_ascii_lowercase(),
                    file: name.strip_prefix('*').unwrap_or(name).to_owned(),
                })
            })
            .collect();
        Manifest { entries }
    }

    /// The pinned 3-4-5 manifest compiled into the binary.
    pub fn pinned() -> Manifest {
        Manifest::parse(MANIFEST_TEXT)
    }

    pub fn entries(&self) -> &[ManifestEntry] {
        &self.entries
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FetchSummary {
    /// Files downloaded and verified in this run.
    pub files: u64,
    /// Bytes of those files.
    pub bytes: u64,
    /// Files already present with a matching hash.
    pub skipped: u64,
}

fn io_err(what: &str, err: std::io::Error) -> DumpError {
    DumpError::Transport(format!("{what}: {err}"))
}

fn net_err(err: reqwest::Error) -> DumpError {
    let mut out = err.to_string();
    let mut source = std::error::Error::source(&err);
    while let Some(s) = source {
        out.push_str(": ");
        out.push_str(&s.to_string());
        source = s.source();
    }
    DumpError::Transport(out)
}

/// The destination name of a manifest file: its last path segment. Rejects anything that could
/// escape `dest`.
fn dest_name(file: &str) -> Result<&str, DumpError> {
    let bad = file.is_empty()
        || file.starts_with('/')
        || file.contains('\\')
        || file.contains(':')
        || file
            .split('/')
            .any(|seg| seg.is_empty() || seg == "." || seg == "..");
    if bad {
        return Err(DumpError::Decode(format!("unsafe manifest name {file:?}")));
    }
    Ok(file.rsplit('/').next().unwrap_or(file))
}

async fn sha_of_file(path: &Path) -> std::io::Result<Option<String>> {
    let mut f = match tokio::fs::File::open(path).await {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(Some(hex::encode(hasher.finalize())))
}

/// Streams one file to `part`, returning its digest and size.
async fn download_to(
    client: &reqwest::Client,
    url: &str,
    part: &Path,
) -> Result<(String, u64), DumpError> {
    let res = client.get(url).send().await.map_err(net_err)?;
    let status = res.status();
    if !status.is_success() {
        return Err(DumpError::Http(status.as_u16()));
    }
    let mut out = tokio::fs::File::create(part)
        .await
        .map_err(|e| io_err("create .part", e))?;
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut chunks = res.bytes_stream();
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(net_err)?;
        hasher.update(&chunk);
        total += chunk.len() as u64;
        out.write_all(&chunk)
            .await
            .map_err(|e| io_err("write .part", e))?;
    }
    out.flush().await.map_err(|e| io_err("flush .part", e))?;
    out.sync_all().await.map_err(|e| io_err("sync .part", e))?;
    Ok((hex::encode(hasher.finalize()), total))
}

/// Downloads every manifest file under `base_url` into `dest`, verifying each SHA-256.
///
/// Stops at the first error; completed files stay, and no `.part` file is left behind.
pub async fn fetch_syzygy(
    client: &reqwest::Client,
    base_url: &str,
    manifest: &Manifest,
    dest: &Path,
) -> Result<FetchSummary, DumpError> {
    // Validate every name first, so a bad manifest makes no request and writes nothing.
    for e in manifest.entries() {
        dest_name(&e.file)?;
    }
    tokio::fs::create_dir_all(dest)
        .await
        .map_err(|e| io_err("create dest", e))?;
    let base = base_url.trim_end_matches('/');
    let mut summary = FetchSummary::default();
    for e in manifest.entries() {
        let name = dest_name(&e.file)?;
        let target: PathBuf = dest.join(name);
        let existing = sha_of_file(&target)
            .await
            .map_err(|err| io_err("hash existing file", err))?;
        if existing.as_deref() == Some(e.sha256.as_str()) {
            summary.skipped += 1;
            continue;
        }
        let part = dest.join(format!("{name}.part"));
        let url = format!("{base}/{}", e.file);
        let (actual, bytes) = match download_to(client, &url, &part).await {
            Ok(v) => v,
            Err(err) => {
                let _ = tokio::fs::remove_file(&part).await;
                return Err(err);
            }
        };
        if actual != e.sha256 {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(DumpError::ChecksumMismatch {
                expected: e.sha256.clone(),
                actual,
            });
        }
        tokio::fs::rename(&part, &target)
            .await
            .map_err(|err| io_err("rename .part", err))?;
        tracing::info!(file = %e.file, bytes, "syzygy file verified");
        summary.files += 1;
        summary.bytes += bytes;
    }
    Ok(summary)
}

/// The HTTP client for the tablebase mirror. Always the anonymous project User-Agent, never one
/// carrying `CONTACT_EMAIL` (data-fetch rule).
pub fn http_client() -> Result<reqwest::Client, reqwest::Error> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .user_agent(crate::config::ANONYMOUS_USER_AGENT)
        .connect_timeout(crate::config::CONNECT_TIMEOUT)
        .read_timeout(crate::config::READ_TIMEOUT)
        .build()
}
