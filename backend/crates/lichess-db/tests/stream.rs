//! Streamed dump download: SHA-256 pass-through, zstd, splitting, whole-file verification (R-9)
//! and bounded memory. No live network: wiremock or a raw local HTTP server serves a body that the
//! test compresses itself.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_compression::Level;
use async_compression::tokio::bufread::ZstdEncoder;
use bytes::Bytes;
use futures::StreamExt;
use lichess_db::config::{DEFAULT_BASE_URL, DumpSource, MAX_GAME_BYTES, http_client};
use lichess_db::download::{DumpError, DumpStream, drain_to_end, fetch_expected_sha, open_dump};
use lichess_db::sample::{SampleFilter, filter_sample};
use lichess_db::split::split_games;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use wiremock::matchers::{header_regex, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const MONTH: &str = "2026-10";
const DUMP_PATH: &str = "/standard/lichess_db_standard_rated_2026-10.pgn.zst";
const SUMS_PATH: &str = "/standard/sha256sums.txt";

fn sample_pgn() -> String {
    testkit::fixture("lichess-db/sample.pgn")
}

async fn zstd(data: &[u8], level: i32) -> Vec<u8> {
    let mut enc = ZstdEncoder::with_quality(data, Level::Precise(level));
    let mut out = Vec::new();
    enc.read_to_end(&mut out).await.unwrap();
    out
}

fn sha_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

fn sums_line(hex: &str) -> String {
    format!(
        "{}  lichess_db_standard_rated_2026-09.pgn.zst\n{hex}  lichess_db_standard_rated_2026-10.pgn.zst\n",
        sha_hex(b"another month")
    )
}

/// A wiremock server serving `body` as the 2026-10 dump and `sums` as `sha256sums.txt`.
async fn dump_server(body: Vec<u8>, sums: String) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(SUMS_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string(sums))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(DUMP_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(&server)
        .await;
    server
}

fn source(base: &str) -> DumpSource {
    DumpSource::new(base, MONTH)
}

async fn collect_games(games: &mut DumpStream) -> Vec<String> {
    let mut out = Vec::new();
    while let Some(g) = games.next().await {
        out.push(g.expect("game"));
    }
    out
}

fn chunked(data: &[u8], size: usize) -> Vec<Result<Bytes, std::io::Error>> {
    data.chunks(size)
        .map(|c| Ok(Bytes::copy_from_slice(c)))
        .collect()
}

/// Raw HTTP/1.1 server that sends `body` with chunked transfer encoding, one `chunk` bytes per
/// chunk, flushing after each. With `close_early` it closes the socket after half the chunks,
/// without the terminating chunk.
async fn raw_chunked_server(body: Vec<u8>, chunk: usize, close_early: bool) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let body = Arc::new(body);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let body = body.clone();
            tokio::spawn(async move {
                let _ = sock.set_nodelay(true);
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await;
                let _ = sock
                    .write_all(b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n")
                    .await;
                let chunks: Vec<&[u8]> = body.chunks(chunk).collect();
                let limit = if close_early {
                    chunks.len() / 2
                } else {
                    chunks.len()
                };
                for c in &chunks[..limit] {
                    let mut frame = format!("{:x}\r\n", c.len()).into_bytes();
                    frame.extend_from_slice(c);
                    frame.extend_from_slice(b"\r\n");
                    let _ = sock.write_all(&frame).await;
                    let _ = sock.flush().await;
                }
                if !close_early {
                    let _ = sock.write_all(b"0\r\n\r\n").await;
                    let _ = sock.flush().await;
                }
                let _ = sock.shutdown().await;
            });
        }
    });
    format!("http://{addr}")
}

#[test]
fn dump_source_names_the_file_and_the_sums_url() {
    let src = DumpSource::new(DEFAULT_BASE_URL, MONTH);
    assert_eq!(DEFAULT_BASE_URL, "https://database.lichess.org");
    assert_eq!(src.file_name(), "lichess_db_standard_rated_2026-10.pgn.zst");
    assert_eq!(
        src.url(),
        "https://database.lichess.org/standard/lichess_db_standard_rated_2026-10.pgn.zst"
    );
    assert_eq!(
        src.sums_url(),
        "https://database.lichess.org/standard/sha256sums.txt"
    );
    let slash = DumpSource::new("http://127.0.0.1:9/", MONTH);
    assert_eq!(
        slash.sums_url(),
        "http://127.0.0.1:9/standard/sha256sums.txt"
    );
}

#[test]
fn dump_stream_and_sha_handle_can_move_across_tasks() {
    fn assert_send<T: Send + 'static>() {}
    assert_send::<DumpStream>();
    assert_send::<lichess_db::download::ShaHandle>();
    assert_send::<DumpError>();
}

#[test]
fn split_rule_needs_a_blank_line_before_event() {
    let lines = [
        "",
        "[Event \"A\"]",
        "[Site \"https://lichess.org/aaaaaaaa\"]",
        "",
        "1. e4 e5 *",
        "[Event \"not a new game: no blank line before it\"]",
        "",
        "",
        "[Event \"B\"]\r",
        "",
        "1. d4 *",
    ];
    let games = split_games(lines);
    assert_eq!(games.len(), 2, "{games:?}");
    assert_eq!(
        games[0],
        "[Event \"A\"]\n[Site \"https://lichess.org/aaaaaaaa\"]\n\n1. e4 e5 *\n\
         [Event \"not a new game: no blank line before it\"]\n"
    );
    assert_eq!(games[1], "[Event \"B\"]\n\n1. d4 *\n");
    assert!(split_games(["", "  ", ""]).is_empty());
}

#[tokio::test]
async fn streams_five_games_and_verifies_sha() {
    let body = zstd(sample_pgn().as_bytes(), 3).await;
    let digest = sha_hex(&body);
    let server = dump_server(body.clone(), sums_line(&digest)).await;
    let client = http_client().unwrap();
    let src = source(&server.uri());

    let expected = fetch_expected_sha(&client, &src).await.unwrap();
    assert_eq!(expected, digest);

    let (mut games, sha) = open_dump(&client, &src).await.unwrap();
    let out = collect_games(&mut games).await;
    assert_eq!(out.len(), 5);
    for g in &out {
        assert!(g.starts_with("[Event "), "{g}");
    }
    assert_eq!(out, split_games(sample_pgn().lines()));
    assert_eq!(drain_to_end(games).await.unwrap(), 0);
    assert!(sha.is_complete());
    assert_eq!(sha.bytes_read(), body.len() as u64);
    assert_eq!(sha.clone().finish(), expected);
    assert_eq!(sha.verify(&expected.to_uppercase()).unwrap(), expected);
}

#[tokio::test]
async fn sha_mismatch_is_reported_after_the_stream() {
    let body = zstd(sample_pgn().as_bytes(), 3).await;
    let actual = sha_hex(&body);
    let wrong = sha_hex(b"not the dump");
    let server = dump_server(body, sums_line(&wrong)).await;
    let client = http_client().unwrap();
    let src = source(&server.uri());

    let expected = fetch_expected_sha(&client, &src).await.unwrap();
    let (mut games, sha) = open_dump(&client, &src).await.unwrap();
    // The mismatch is not visible while streaming: every game still arrives.
    assert_eq!(collect_games(&mut games).await.len(), 5);
    drain_to_end(games).await.unwrap();
    match sha.verify(&expected) {
        Err(DumpError::ChecksumMismatch {
            expected: e,
            actual: a,
        }) => {
            assert_eq!(e, wrong);
            assert_eq!(a, actual);
        }
        other => panic!("expected ChecksumMismatch, got {other:?}"),
    }
}

#[tokio::test]
async fn chunk_boundaries_do_not_split_games() {
    let body = zstd(sample_pgn().as_bytes(), 3).await;
    let whole = {
        let (mut g, _) = DumpStream::from_bytes(futures::stream::iter(chunked(&body, body.len())));
        collect_games(&mut g).await
    };
    assert_eq!(whole.len(), 5);

    // Exactly 7-byte chunks straight into the decoder chain.
    let (mut g, sha) = DumpStream::from_bytes(futures::stream::iter(chunked(&body, 7)));
    assert_eq!(collect_games(&mut g).await, whole);
    drain_to_end(g).await.unwrap();
    assert_eq!(sha.finish(), sha_hex(&body));

    // And over HTTP: wiremock cannot stream a body in pieces, so a raw HTTP/1.1 server sends it
    // as 7-byte transfer-encoding chunks.
    let base = raw_chunked_server(body.clone(), 7, false).await;
    let client = http_client().unwrap();
    let (mut g, sha) = open_dump(&client, &source(&base)).await.unwrap();
    assert_eq!(collect_games(&mut g).await, whole);
    drain_to_end(g).await.unwrap();
    assert_eq!(sha.finish(), sha_hex(&body));
}

#[tokio::test]
async fn drain_after_max_games_still_hashes_whole_file() {
    let body = zstd(sample_pgn().as_bytes(), 3).await;
    let digest = sha_hex(&body);
    let server = dump_server(body.clone(), sums_line(&digest)).await;
    let client = http_client().unwrap();
    let src = source(&server.uri());

    let expected = fetch_expected_sha(&client, &src).await.unwrap();
    let (mut games, sha) = open_dump(&client, &src).await.unwrap();
    let filter = SampleFilter {
        max_games: Some(2),
        ..SampleFilter::default()
    };
    let kept: Vec<_> = filter_sample(&mut games, filter).collect().await;
    assert_eq!(kept.len(), 2);
    assert!(kept.iter().all(|k| k.is_ok()));
    // Parsing stopped after two games; the rest is hashed without being parsed.
    drain_to_end(games).await.unwrap();
    assert!(sha.is_complete());
    assert_eq!(sha.bytes_read(), body.len() as u64);
    assert_eq!(sha.verify(&expected).unwrap(), digest);
}

fn assert_dir_empty(dir: &Path) {
    let entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert!(entries.is_empty(), "files written: {entries:?}");
}

#[tokio::test]
async fn nothing_is_written_to_disk() {
    let body = zstd(sample_pgn().as_bytes(), 3).await;
    let digest = sha_hex(&body);
    let server = dump_server(body, sums_line(&digest)).await;
    let tmp = std::env::temp_dir().join(format!("lichess-db-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp).unwrap();

    // The CLI runs with DATA_DIR, the working directory and TMPDIR all pointing at the empty
    // temp directory, so any file the pipeline wrote would show up there.
    let out = tokio::process::Command::new(env!("CARGO_BIN_EXE_lichess-db"))
        .args(["scan", "--month", MONTH, "--base-url", &server.uri()])
        .env("DATA_DIR", &tmp)
        .env("TMPDIR", &tmp)
        .current_dir(&tmp)
        .output()
        .await
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("games seen: 5"), "{stdout}");
    assert!(stdout.contains("games kept: 5"), "{stdout}");
    assert!(
        stdout.contains(&format!("sha256: {digest} (verified)")),
        "{stdout}"
    );
    assert_dir_empty(&tmp);
    std::fs::remove_dir(&tmp).unwrap();
}

#[tokio::test]
async fn scan_exits_non_zero_on_checksum_mismatch() {
    let body = zstd(sample_pgn().as_bytes(), 3).await;
    let server = dump_server(body, sums_line(&sha_hex(b"wrong"))).await;
    let out = tokio::process::Command::new(env!("CARGO_BIN_EXE_lichess-db"))
        .args(["scan", "--month", MONTH, "--base-url", &server.uri()])
        .output()
        .await
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("checksum mismatch"), "{stderr}");
}

/// Lichess dumps are pzstd output: each zstd frame is preceded by a skippable frame (magic
/// 0x184D2A50) holding the next frame's compressed size.
#[tokio::test]
async fn pzstd_layout_with_skippable_frames_is_decoded() {
    let pgn = sample_pgn();
    let games = split_games(pgn.lines());
    let halves = [games[..2].join("\n"), games[2..].join("\n")];
    let mut body = Vec::new();
    for half in &halves {
        let frame = zstd(format!("{half}\n").as_bytes(), 3).await;
        body.extend_from_slice(&0x184D_2A50u32.to_le_bytes());
        body.extend_from_slice(&4u32.to_le_bytes());
        body.extend_from_slice(&(frame.len() as u32).to_le_bytes());
        body.extend_from_slice(&frame);
    }
    let (mut g, sha) = DumpStream::from_bytes(futures::stream::iter(chunked(&body, 4096)));
    let out = collect_games(&mut g).await;
    assert_eq!(out, games);
    drain_to_end(g).await.unwrap();
    assert_eq!(sha.finish(), sha_hex(&body));
}

#[tokio::test]
async fn http_errors_are_reported() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(SUMS_PATH))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let client = http_client().unwrap();
    let src = source(&server.uri());
    assert!(matches!(
        fetch_expected_sha(&client, &src).await,
        Err(DumpError::Http(500))
    ));
    assert!(matches!(
        open_dump(&client, &src).await.map(|_| ()),
        Err(DumpError::Http(404))
    ));
}

#[tokio::test]
async fn missing_sums_line_is_reported() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(SUMS_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            "{}  lichess_db_standard_rated_2026-09.pgn.zst\n",
            sha_hex(b"x")
        )))
        .mount(&server)
        .await;
    let client = http_client().unwrap();
    match fetch_expected_sha(&client, &source(&server.uri())).await {
        Err(DumpError::SumsMissing(name)) => {
            assert!(
                name.contains("lichess_db_standard_rated_2026-10.pgn.zst"),
                "{name}"
            );
        }
        other => panic!("expected SumsMissing, got {other:?}"),
    }
}

#[tokio::test]
async fn requests_identify_the_client() {
    let body = zstd(sample_pgn().as_bytes(), 3).await;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(DUMP_PATH))
        .and(header_regex("user-agent", "^rematch/"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(&server)
        .await;
    let client = http_client().unwrap();
    let (mut g, _) = open_dump(&client, &source(&server.uri())).await.unwrap();
    assert_eq!(collect_games(&mut g).await.len(), 5);
}

#[tokio::test]
async fn invalid_and_truncated_zstd_are_decode_errors() {
    let garbage = b"this is not a zstd stream at all".to_vec();
    let (mut g, _) = DumpStream::from_bytes(futures::stream::iter(chunked(&garbage, 8)));
    assert!(
        matches!(g.next().await, Some(Err(DumpError::Decode(_)))),
        "garbage"
    );
    assert!(g.next().await.is_none(), "the stream ends after an error");

    let body = zstd(sample_pgn().as_bytes(), 3).await;
    let half = body[..body.len() / 2].to_vec();
    let (mut g, sha) = DumpStream::from_bytes(futures::stream::iter(chunked(&half, 64)));
    let mut saw_error = false;
    while let Some(item) = g.next().await {
        if let Err(e) = item {
            assert!(matches!(e, DumpError::Decode(_)), "{e:?}");
            saw_error = true;
        }
    }
    assert!(saw_error, "a truncated frame must not end silently");
    drain_to_end(g).await.unwrap();
    // The whole (truncated) body was hashed, so verification against the real digest fails.
    assert!(matches!(
        sha.verify(&sha_hex(&body)),
        Err(DumpError::ChecksumMismatch { .. })
    ));
}

#[tokio::test]
async fn connection_closed_mid_body_is_a_transport_error() {
    let body = zstd(sample_pgn().as_bytes(), 3).await;
    let base = raw_chunked_server(body, 64, true).await;
    let client = http_client().unwrap();
    let (mut g, sha) = open_dump(&client, &source(&base)).await.unwrap();
    let mut err = None;
    while let Some(item) = g.next().await {
        if let Err(e) = item {
            err = Some(e);
        }
    }
    assert!(matches!(err, Some(DumpError::Transport(_))), "{err:?}");
    assert!(!sha.is_complete());
    assert!(sha.verify(&"0".repeat(64)).is_err());
}

#[tokio::test]
async fn oversized_game_is_rejected_instead_of_buffered() {
    let mut text = b"[Event \"huge\"]\n\n1. e4 {".to_vec();
    text.resize(text.len() + MAX_GAME_BYTES + 1024, b'a');
    let body = zstd(&text, 1).await;
    let (mut g, _) = DumpStream::from_bytes(futures::stream::iter(chunked(&body, 65536)));
    match g.next().await {
        Some(Err(DumpError::Decode(msg))) => assert!(msg.contains("exceeds"), "{msg}"),
        other => panic!("expected Decode error, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// Bounded memory on a large synthetic stream.

const FRAME_GAMES: u64 = 800;
/// 200 frames: about 85 MB compressed and 200 MiB of PGN.
const FRAMES: u64 = 200;
const NET_CHUNK: usize = 64 * 1024;
const READ_AHEAD_SLACK: u64 = 1024 * 1024;
/// The measured peak is about 9 MiB (debug build, whole test binary); more than 5x headroom,
/// yet buffering either the compressed body or the PGN would exceed it.
const PEAK_RSS_BOUND_MIB: u64 = 48;

/// ~1 MiB of distinct games whose comments are pseudo-random hex, so a frame compresses only
/// about 2.5:1 and the whole stream is large both compressed and decompressed.
fn synthetic_frame_text() -> String {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut text = String::new();
    for i in 0..FRAME_GAMES {
        text.push_str(&format!(
            "[Event \"Rated Blitz game\"]\n[Site \"https://lichess.org/s{i:07}\"]\n\
             [White \"w{i}\"]\n[Black \"b{i}\"]\n[Result \"1-0\"]\n[UTCDate \"2026.10.01\"]\n\
             [UTCTime \"00:00:00\"]\n[WhiteElo \"1500\"]\n[BlackElo \"1500\"]\n\
             [TimeControl \"180+0\"]\n[Termination \"Normal\"]\n\n1. e4 {{ "
        ));
        for _ in 0..64 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            text.push_str(&format!("{x:016x}"));
        }
        text.push_str(" } 1... e5 1-0\n\n");
    }
    text
}

/// Peak resident set size of this process in KiB (Linux only).
fn peak_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

#[tokio::test]
async fn bounded_memory_on_a_large_synthetic_stream() {
    let frame = Bytes::from(zstd(synthetic_frame_text().as_bytes(), 1).await);
    let frame_len = frame.len() as u64;
    let total = frame_len * FRAMES;
    assert!(
        total > 1024 * 1024 * PEAK_RSS_BOUND_MIB * 13 / 10,
        "the stream ({total} B) must be well above the memory bound to prove anything"
    );

    // The source hands out zero-copy slices of one shared frame, so it costs no memory itself.
    let pulled = Arc::new(AtomicU64::new(0));
    let counter = pulled.clone();
    let source_frame = frame.clone();
    let source = futures::stream::iter((0..FRAMES).flat_map(move |_| {
        let f = source_frame.clone();
        (0..f.len().div_ceil(NET_CHUNK))
            .map(move |i| f.slice(i * NET_CHUNK..((i + 1) * NET_CHUNK).min(f.len())))
    }))
    .map(move |chunk| {
        counter.fetch_add(chunk.len() as u64, Ordering::Relaxed);
        Ok::<Bytes, std::io::Error>(chunk)
    });

    let (mut games, sha) = DumpStream::from_bytes(source);
    let mut seen = 0u64;
    while let Some(g) = games.next().await {
        let g = g.expect("game");
        assert!(g.starts_with("[Event "));
        seen += 1;
        // Backpressure: the reader never runs more than READ_AHEAD_SLACK past the frame that
        // holds the game just returned.
        let frames_done = seen / FRAME_GAMES;
        let bound = (frames_done + 1) * frame_len + READ_AHEAD_SLACK;
        let now = pulled.load(Ordering::Relaxed);
        assert!(
            now <= bound,
            "game {seen}: pulled {now} B > bound {bound} B"
        );
    }
    assert_eq!(seen, FRAME_GAMES * FRAMES);
    assert_eq!(drain_to_end(games).await.unwrap(), 0);
    assert_eq!(sha.bytes_read(), total);

    let mut expected = Sha256::new();
    for _ in 0..FRAMES {
        expected.update(&frame);
    }
    assert_eq!(sha.finish(), hex::encode(expected.finalize()));

    match peak_rss_kib() {
        Some(kib) => {
            eprintln!(
                "streamed {} MiB compressed / {} games; peak RSS {} MiB",
                total >> 20,
                seen,
                kib >> 10
            );
            assert!(
                kib < PEAK_RSS_BOUND_MIB * 1024,
                "peak RSS {kib} KiB exceeds {PEAK_RSS_BOUND_MIB} MiB"
            );
        }
        None if testkit::in_docker() => panic!("/proc/self/status must be readable in Docker"),
        None => eprintln!("peak RSS not measurable on this OS; backpressure checked only"),
    }
}

#[tokio::test]
async fn transport_errors_from_the_source_are_reported() {
    let body = zstd(sample_pgn().as_bytes(), 3).await;
    let mut items = chunked(&body[..body.len() / 2], 64);
    items.push(Err(std::io::Error::other("connection reset")));
    let (mut g, sha) = DumpStream::from_bytes(futures::stream::iter(items));
    let mut err = None;
    while let Some(item) = g.next().await {
        if let Err(e) = item {
            err = Some(e);
        }
    }
    match err {
        Some(DumpError::Transport(msg)) => assert!(msg.contains("connection reset"), "{msg}"),
        other => panic!("expected Transport, got {other:?}"),
    }
    assert!(!sha.is_complete());
    // A short timeout guard: draining after a transport error returns promptly.
    let drained = tokio::time::timeout(Duration::from_secs(10), drain_to_end(g)).await;
    assert!(drained.is_ok());
}
