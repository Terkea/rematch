//! Streamed download with SHA-256 (R2 Task 19, ADR-R2-9).
//!
//! The dump is never stored. Bytes flow
//! `reqwest bytes_stream` → [`HashingStream`] (updates a SHA-256 over the compressed bytes) →
//! `tokio_util::io::StreamReader` → `async_compression` `ZstdDecoder` (multi-frame, so pzstd
//! output with skippable frames decodes) → a line splitter → one `String` per game.
//!
//! Memory is bounded: the chain is pull-based (nothing is read before the consumer asks for the
//! next game), each stage holds at most one network chunk or one [`DECODE_BUFFER_BYTES`] buffer,
//! the zstd window is capped by the decoder's default `window_log_max` (Lichess uses 8 MiB), and
//! a game longer than [`MAX_GAME_BYTES`] is an error instead of a growing buffer.
//!
//! Verification is whole-file (R-9): after the consumer stops parsing, [`drain_to_end`] pulls the
//! rest of the body through the hasher without decompressing it, and only then does
//! [`ShaHandle::verify`] compare the digest with `sha256sums.txt`.

use std::error::Error as StdError;
use std::fmt;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, ready};

use async_compression::tokio::bufread::ZstdDecoder;
use bytes::Bytes;
use futures::stream::{BoxStream, Stream, StreamExt};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufRead, BufReader};
use tokio_util::io::StreamReader;

use crate::config::{DECODE_BUFFER_BYTES, DumpSource, MAX_GAME_BYTES, MAX_SUMS_BYTES};
use crate::split::GameSplitter;

/// Errors of the dump pipeline.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DumpError {
    /// The mirror answered with a non-success status.
    #[error("HTTP status {0}")]
    Http(u16),
    /// Connection, TLS, timeout or body-read failure.
    #[error("transport error: {0}")]
    Transport(String),
    /// Invalid zstd, an oversized game, or a malformed PGN header.
    #[error("decode error: {0}")]
    Decode(String),
    /// The streamed SHA-256 differs from `sha256sums.txt`.
    #[error("checksum mismatch: expected {expected}, actual {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    /// `sha256sums.txt` has no (valid) line for this file.
    #[error("no SHA-256 for {0} in sha256sums.txt")]
    SumsMissing(String),
}

/// `err` and its sources, joined with `": "` (reqwest's own message is terse).
fn error_chain(err: &dyn StdError) -> String {
    let mut out = err.to_string();
    let mut source = err.source();
    while let Some(s) = source {
        out.push_str(": ");
        out.push_str(&s.to_string());
        source = s.source();
    }
    out
}

fn transport(err: reqwest::Error) -> DumpError {
    DumpError::Transport(error_chain(&err))
}

/// The expected digest of `src` from `<base>/standard/sha256sums.txt` (lowercase hex).
pub async fn fetch_expected_sha(
    client: &reqwest::Client,
    src: &DumpSource,
) -> Result<String, DumpError> {
    let res = client.get(src.sums_url()).send().await.map_err(transport)?;
    let status = res.status();
    if !status.is_success() {
        return Err(DumpError::Http(status.as_u16()));
    }
    let mut body = Vec::new();
    let mut chunks = res.bytes_stream();
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(transport)?;
        if body.len() + chunk.len() > MAX_SUMS_BYTES {
            return Err(DumpError::Decode(format!(
                "sha256sums.txt exceeds {MAX_SUMS_BYTES} bytes"
            )));
        }
        body.extend_from_slice(&chunk);
    }
    parse_sums(&String::from_utf8_lossy(&body), &src.file_name())
}

/// Finds `file_name` in `sha256sum` output (`<hex>  <file>` or `<hex> *<file>` lines).
pub fn parse_sums(text: &str, file_name: &str) -> Result<String, DumpError> {
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(digest), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        if name.strip_prefix('*').unwrap_or(name) != file_name {
            continue;
        }
        if digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Ok(digest.to_ascii_lowercase());
        }
        return Err(DumpError::SumsMissing(format!(
            "{file_name} (malformed digest {digest:?})"
        )));
    }
    Err(DumpError::SumsMissing(file_name.to_owned()))
}

/// Opens the dump of `src` as a stream of games plus the handle of its running SHA-256.
pub async fn open_dump(
    client: &reqwest::Client,
    src: &DumpSource,
) -> Result<(DumpStream, ShaHandle), DumpError> {
    let url = src.url();
    let res = client.get(&url).send().await.map_err(transport)?;
    let status = res.status();
    if !status.is_success() {
        return Err(DumpError::Http(status.as_u16()));
    }
    tracing::info!(%url, content_length = res.content_length(), "streaming dump");
    let body = res
        .bytes_stream()
        .map(|chunk| chunk.map_err(|e| error_chain(&e)));
    Ok(DumpStream::from_bytes(body))
}

/// Reads the rest of the compressed body through the hasher without decompressing or parsing
/// it (R-9). Returns the number of compressed bytes drained.
pub async fn drain_to_end(stream: DumpStream) -> Result<u64, DumpError> {
    stream.drain_to_end().await
}

// ---------------------------------------------------------------------------------------------
// SHA-256 pass-through

#[derive(Default)]
struct ShaState {
    hasher: Sha256,
    bytes: u64,
    complete: bool,
    failed: bool,
}

type Shared = Arc<Mutex<ShaState>>;

fn lock(state: &Shared) -> MutexGuard<'_, ShaState> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Marks an I/O error as coming from the transport, so it is not reported as a decode error
/// after passing through the decoder.
#[derive(Debug)]
struct TransportFailure(String);

impl fmt::Display for TransportFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl StdError for TransportFailure {}

fn from_io(err: io::Error) -> DumpError {
    match err
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<TransportFailure>())
    {
        Some(t) => DumpError::Transport(t.0.clone()),
        None => DumpError::Decode(err.to_string()),
    }
}

/// Passes chunks through unchanged while hashing them. Ends for good after the first error.
struct HashingStream {
    inner: BoxStream<'static, Result<Bytes, String>>,
    state: Shared,
    ended: bool,
}

impl Stream for HashingStream {
    type Item = io::Result<Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.ended {
            return Poll::Ready(None);
        }
        let item = ready!(self.inner.poll_next_unpin(cx));
        match item {
            Some(Ok(chunk)) => {
                let mut s = lock(&self.state);
                s.hasher.update(&chunk);
                s.bytes += chunk.len() as u64;
                drop(s);
                Poll::Ready(Some(Ok(chunk)))
            }
            Some(Err(msg)) => {
                lock(&self.state).failed = true;
                self.ended = true;
                Poll::Ready(Some(Err(io::Error::other(TransportFailure(msg)))))
            }
            None => {
                let mut s = lock(&self.state);
                s.complete = !s.failed;
                drop(s);
                self.ended = true;
                Poll::Ready(None)
            }
        }
    }
}

/// Handle on the SHA-256 of the compressed body, shared with the [`DumpStream`].
#[derive(Clone)]
pub struct ShaHandle {
    state: Shared,
}

impl fmt::Debug for ShaHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = lock(&self.state);
        f.debug_struct("ShaHandle")
            .field("bytes", &s.bytes)
            .field("complete", &s.complete)
            .field("failed", &s.failed)
            .finish()
    }
}

impl ShaHandle {
    /// Lowercase hex SHA-256 of every compressed byte read so far. It is the digest of the
    /// whole file once the body is fully read ([`ShaHandle::is_complete`], after
    /// [`drain_to_end`]).
    pub fn finish(self) -> String {
        hex::encode(lock(&self.state).hasher.clone().finalize())
    }

    /// Compressed bytes hashed so far.
    pub fn bytes_read(&self) -> u64 {
        lock(&self.state).bytes
    }

    /// True once the body ended cleanly (no transport error) and every byte was hashed.
    pub fn is_complete(&self) -> bool {
        lock(&self.state).complete
    }

    /// The verify step: the whole-file digest must equal `expected` (case-insensitive).
    /// Returns the digest, or `ChecksumMismatch` with both digests. A body that was not fully
    /// read is a `Transport` error, never a match.
    pub fn verify(self, expected: &str) -> Result<String, DumpError> {
        let (complete, bytes) = {
            let s = lock(&self.state);
            (s.complete, s.bytes)
        };
        if !complete {
            return Err(DumpError::Transport(format!(
                "body not fully read after {bytes} bytes; nothing to verify"
            )));
        }
        let actual = self.finish();
        let expected = expected.trim().to_ascii_lowercase();
        if actual == expected {
            Ok(actual)
        } else {
            Err(DumpError::ChecksumMismatch { expected, actual })
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Game stream

type Decoder = BufReader<ZstdDecoder<StreamReader<HashingStream, Bytes>>>;

/// The games of one dump, in order, each as its PGN text. Pull-based: bytes are read from the
/// source only while the consumer polls. Ends after the last game or after the first error.
pub struct DumpStream {
    reader: Box<Decoder>,
    line: Vec<u8>,
    splitter: GameSplitter,
    ready: Option<String>,
    eof: bool,
    done: bool,
}

impl fmt::Debug for DumpStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DumpStream")
            .field(
                "buffered",
                &(self.line.len() + self.splitter.buffered_len()),
            )
            .field("eof", &self.eof)
            .field("done", &self.done)
            .finish()
    }
}

impl DumpStream {
    /// Builds the hash → zstd → split chain over any stream of compressed chunks (`open_dump`
    /// passes the HTTP body; tests and benchmarks pass in-memory streams).
    pub fn from_bytes<S, E>(bytes: S) -> (DumpStream, ShaHandle)
    where
        S: Stream<Item = Result<Bytes, E>> + Send + 'static,
        E: fmt::Display,
    {
        let state: Shared = Arc::default();
        let hashing = HashingStream {
            inner: bytes.map(|r| r.map_err(|e| e.to_string())).boxed(),
            state: state.clone(),
            ended: false,
        };
        let mut decoder = ZstdDecoder::new(StreamReader::new(hashing));
        // Lichess dumps are pzstd output: many frames, each after a skippable frame.
        decoder.multiple_members(true);
        let stream = DumpStream {
            reader: Box::new(BufReader::with_capacity(DECODE_BUFFER_BYTES, decoder)),
            line: Vec::new(),
            splitter: GameSplitter::new(),
            ready: None,
            eof: false,
            done: false,
        };
        (stream, ShaHandle { state })
    }

    /// See [`drain_to_end`]. The decoder and its buffers are dropped; their bytes were already
    /// hashed when they passed the hashing stage.
    pub async fn drain_to_end(self) -> Result<u64, DumpError> {
        let decoder: ZstdDecoder<_> = self.reader.into_inner();
        let mut raw = decoder.into_inner().into_inner();
        let mut drained = 0u64;
        while let Some(chunk) = raw.next().await {
            drained += chunk.map_err(from_io)?.len() as u64;
        }
        Ok(drained)
    }
}

/// Feeds the completed `line` to the splitter and clears it.
fn feed(line: &mut Vec<u8>, splitter: &mut GameSplitter) -> Option<String> {
    let game = match std::str::from_utf8(line) {
        Ok(text) => splitter.push_line(text),
        Err(_) => splitter.push_line(&String::from_utf8_lossy(line)),
    };
    line.clear();
    game
}

fn oversized() -> DumpError {
    DumpError::Decode(format!(
        "a game exceeds {MAX_GAME_BYTES} bytes; the stream is corrupt or not a PGN dump"
    ))
}

impl Stream for DumpStream {
    type Item = Result<String, DumpError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            if let Some(game) = this.ready.take() {
                return Poll::Ready(Some(Ok(game)));
            }
            if this.done {
                return Poll::Ready(None);
            }
            if this.eof {
                this.done = true;
                return Poll::Ready(this.splitter.finish().map(Ok));
            }
            let buf = match ready!(Pin::new(&mut *this.reader).poll_fill_buf(cx)) {
                Ok(buf) => buf,
                Err(err) => {
                    this.done = true;
                    return Poll::Ready(Some(Err(from_io(err))));
                }
            };
            if buf.is_empty() {
                this.eof = true;
                if !this.line.is_empty() {
                    this.ready = feed(&mut this.line, &mut this.splitter);
                }
                continue;
            }
            let mut used = 0;
            while used < buf.len() && this.ready.is_none() {
                let rest = &buf[used..];
                match rest.iter().position(|&b| b == b'\n') {
                    Some(i) => {
                        this.line.extend_from_slice(&rest[..i]);
                        used += i + 1;
                        this.ready = feed(&mut this.line, &mut this.splitter);
                    }
                    None => {
                        this.line.extend_from_slice(rest);
                        used = buf.len();
                    }
                }
                if this.line.len() + this.splitter.buffered_len() > MAX_GAME_BYTES {
                    this.done = true;
                    return Poll::Ready(Some(Err(oversized())));
                }
            }
            Pin::new(&mut *this.reader).consume(used);
        }
    }
}
