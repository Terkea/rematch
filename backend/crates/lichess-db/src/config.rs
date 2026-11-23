//! Pipeline configuration (R2 Task 19): the dump source, the HTTP client and the memory limits.

use std::path::PathBuf;
use std::time::Duration;

/// The CC0 Lichess database mirror.
pub const DEFAULT_BASE_URL: &str = "https://database.lichess.org";

/// Largest single game (headers + movetext) the splitter buffers. Lichess games are a few KB
/// even with `[%eval]` and `[%clk]` on every ply; a longer "game" means a corrupt stream, and
/// failing beats buffering without a bound.
pub const MAX_GAME_BYTES: usize = 1024 * 1024;

/// Read buffer between the zstd decoder and the line splitter.
pub const DECODE_BUFFER_BYTES: usize = 64 * 1024;

/// Largest zstd window the decoder accepts, as a power of two (16 MiB; Lichess uses 2^23). A
/// frame asking for more is a decode error instead of a large allocation.
pub const MAX_ZSTD_WINDOW_LOG: u32 = 24;

/// Upper bound for `sha256sums.txt` (about 18 KB in 2026, one line per month).
pub const MAX_SUMS_BYTES: usize = 1024 * 1024;

/// TCP + TLS connect timeout.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Longest silence between two body reads. There is no total timeout: a month is ~30 GB.
pub const READ_TIMEOUT: Duration = Duration::from_secs(120);

/// The fixed User-Agent of every outbound request, the same string as `platforms` sends
/// (ADR 0043: no request ever carries an email address, so this crate has no `CONTACT_EMAIL`
/// plumbing).
pub const ANONYMOUS_USER_AGENT: &str = "rematch/0.1 (+https://github.com/Terkea/rematch)";

/// One monthly standard-rated dump, e.g. `lichess_db_standard_rated_2026-10.pgn.zst`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DumpSource {
    /// Mirror root, `"https://database.lichess.org"` by default (a trailing `/` is ignored).
    pub base_url: String,
    /// `YYYY-MM` (see [`is_valid_month`]).
    pub month: String,
}

impl DumpSource {
    pub fn new(base_url: impl Into<String>, month: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            month: month.into(),
        }
    }

    /// The month on the default Lichess mirror.
    pub fn lichess(month: impl Into<String>) -> Self {
        Self::new(DEFAULT_BASE_URL, month)
    }

    pub fn file_name(&self) -> String {
        format!("lichess_db_standard_rated_{}.pgn.zst", self.month)
    }

    pub fn url(&self) -> String {
        format!("{}/standard/{}", self.base(), self.file_name())
    }

    pub fn sums_url(&self) -> String {
        format!("{}/standard/sha256sums.txt", self.base())
    }

    fn base(&self) -> &str {
        self.base_url.trim_end_matches('/')
    }
}

/// `YYYY-MM` with a month in 01..=12.
pub fn is_valid_month(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 7
        && b[4] == b'-'
        && b[..4].iter().chain(&b[5..]).all(u8::is_ascii_digit)
        && matches!(s[5..].parse::<u8>(), Ok(1..=12))
}

/// The fixed project User-Agent ([`ANONYMOUS_USER_AGENT`]); never derived from the environment.
pub fn user_agent() -> String {
    ANONYMOUS_USER_AGENT.to_owned()
}

/// The HTTP client for the dump and `sha256sums.txt`: identifying User-Agent, connect and
/// per-read timeouts, no total timeout. Installs the ring rustls provider first, because
/// reqwest is built with `rustls-no-provider`.
pub fn http_client() -> Result<reqwest::Client, reqwest::Error> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .user_agent(user_agent())
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .build()
}

/// `DATA_DIR`, or `data` under the current directory. The dump itself is never written there
/// (ADR-R2-9); later tasks keep Syzygy tables and operator scratch in it.
pub fn data_dir() -> PathBuf {
    std::env::var_os("DATA_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn months_are_validated() {
        assert!(is_valid_month("2026-10"));
        assert!(is_valid_month("2013-01"));
        for bad in [
            "2026-13", "2026-00", "2026-1", "26-10", "2026/10", "../x/y", "2026-1a",
        ] {
            assert!(!is_valid_month(bad), "{bad}");
        }
    }
}
