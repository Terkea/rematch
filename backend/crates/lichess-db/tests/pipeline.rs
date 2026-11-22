//! Parallel parse pipeline (R2 Task 20): batching, ordering and the `max_games` cap, sampling
//! across the whole month, BOT and provisional filters, pzstd frame boundaries, the zstd window
//! cap, and the in-process HTTP resume. No live network: in-memory streams, wiremock, or a raw
//! local HTTP server.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_compression::Level;
use async_compression::tokio::bufread::ZstdEncoder;
use async_compression::zstd::CParameter;
use bytes::Bytes;
use chess_core::TimeClass;
use futures::StreamExt;
use futures::future::BoxFuture;
use lichess_db::config::DumpSource;
use lichess_db::download::{DumpError, DumpStream, RetryPolicy, drain_to_end, open_dump_with};
use lichess_db::headers::{DumpHeader, looks_provisional, parse_dump_headers};
use lichess_db::pipeline::{GameSink, PipelineConfig, run_pipeline};
use lichess_db::sample::{SampleFilter, keep, proportional_rate};
use lichess_db::split::split_games;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const MONTH: &str = "2026-10";
const DUMP_PATH: &str = "/standard/lichess_db_standard_rated_2026-10.pgn.zst";

/// One synthetic game. `day` is the October 2026 day of `UTCDate`.
fn game(id: &str, day: u32, tags: &str, moves: &str) -> String {
    format!(
        "[Event \"Rated Blitz game\"]\n[Site \"https://lichess.org/{id}\"]\n\
         [White \"FixtureW{id}\"]\n[Black \"FixtureB{id}\"]\n[Result \"1-0\"]\n\
         [UTCDate \"2026.10.{day:02}\"]\n[UTCTime \"12:00:00\"]\n[WhiteElo \"1550\"]\n\
         [BlackElo \"1450\"]\n[TimeControl \"180+0\"]\n[Termination \"Normal\"]\n{tags}\n\
         {moves} 1-0\n\n"
    )
}

fn plain(id: &str, day: u32) -> String {
    game(id, day, "", "1. e4 e5 2. Nf3 Nc6")
}

fn ids_of(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("g{i:06}")).collect()
}

/// Records the id and UTC day of every observed game, in merge order.
#[derive(Default)]
struct Recorder {
    seen: Mutex<Vec<(String, u8)>>,
    merges: AtomicUsize,
}

impl GameSink for Recorder {
    type Partial = (String, u8);

    fn observe(&self, _pgn: &str, h: &DumpHeader, _tc: TimeClass) -> Self::Partial {
        (h.lichess_game_id.clone(), h.utc.day())
    }

    fn merge(&self, parts: Vec<Self::Partial>) -> BoxFuture<'_, Result<(), DumpError>> {
        self.merges.fetch_add(1, Ordering::SeqCst);
        self.seen.lock().unwrap().extend(parts);
        Box::pin(async { Ok(()) })
    }
}

fn cfg(threads: usize, batch_games: usize) -> PipelineConfig {
    PipelineConfig {
        threads,
        batch_games,
        channel_batches: 2 * threads,
    }
}

fn source(games: Vec<String>) -> impl futures::Stream<Item = Result<String, DumpError>> + Unpin {
    futures::stream::iter(games.into_iter().map(Ok))
}

#[test]
fn default_config_uses_every_core_and_2000_game_batches() {
    let c = PipelineConfig::default();
    let cores = std::thread::available_parallelism().unwrap().get();
    assert_eq!(c.threads, cores);
    assert_eq!(c.batch_games, 2_000);
    assert_eq!(c.channel_batches, 2 * cores);
    assert_eq!(PipelineConfig::with_threads(0).threads, cores);
    assert_eq!(PipelineConfig::with_threads(3).channel_batches, 6);
}

#[tokio::test]
async fn pipeline_observes_every_kept_game_in_order_with_stage_timings() {
    let games: Vec<String> = ids_of(25).iter().map(|id| plain(id, 1)).collect();
    let sink = Arc::new(Recorder::default());
    let stats = run_pipeline(source(games), &SampleFilter::default(), &sink, &cfg(4, 3))
        .await
        .unwrap();
    assert_eq!(stats.games_seen, 25);
    assert_eq!(stats.games_kept, 25);
    assert_eq!(stats.games_malformed, 0);
    assert!(!stats.capped);
    let seen: Vec<String> = sink
        .seen
        .lock()
        .unwrap()
        .iter()
        .map(|s| s.0.clone())
        .collect();
    assert_eq!(seen, ids_of(25), "merged in source order");
    assert_eq!(
        sink.merges.load(Ordering::SeqCst),
        9,
        "ceil(25 / 3) batches"
    );
    for stage in ["read", "parse", "aggregate", "flush"] {
        assert!(stats.stage_ms.contains_key(stage), "{stage} missing");
        assert!(stats.stage_ms[stage] >= 0.0);
    }
    assert!(stats.bytes_pgn > 0);
}

#[tokio::test]
async fn malformed_and_filtered_games_are_counted_not_observed() {
    let mut games = vec![plain("aaa1", 1)];
    games.push("[Event \"broken\"]\n[Site \"x\"\n\n1. e4 *\n".to_owned());
    games.push(game("bbb2", 1, "", "1. d4 d5").replace("180+0", "-"));
    games.push(plain("ccc3", 2));
    let sink = Arc::new(Recorder::default());
    let stats = run_pipeline(source(games), &SampleFilter::default(), &sink, &cfg(2, 2))
        .await
        .unwrap();
    assert_eq!(
        (stats.games_seen, stats.games_kept, stats.games_malformed),
        (4, 2, 1)
    );
    let seen: Vec<String> = sink
        .seen
        .lock()
        .unwrap()
        .iter()
        .map(|s| s.0.clone())
        .collect();
    assert_eq!(seen, ["aaa1", "ccc3"]);
}

#[tokio::test]
async fn max_games_cap_is_exact_and_stops_reading() {
    let pulled = Arc::new(AtomicUsize::new(0));
    let counter = pulled.clone();
    let games = futures::stream::iter(ids_of(5_000)).map(move |id| {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(plain(&id, 1))
    });
    let sink = Arc::new(Recorder::default());
    let filter = SampleFilter {
        max_games: Some(10),
        ..SampleFilter::default()
    };
    let stats = run_pipeline(games, &filter, &sink, &cfg(4, 3))
        .await
        .unwrap();
    assert_eq!(stats.games_kept, 10);
    assert_eq!(stats.games_seen, 10, "seen stops at the 10th kept game");
    assert!(stats.capped);
    let seen: Vec<String> = sink
        .seen
        .lock()
        .unwrap()
        .iter()
        .map(|s| s.0.clone())
        .collect();
    assert_eq!(seen, ids_of(10));
    // Bounded read-ahead: at most the in-flight batches and the channel, never the whole source.
    let n = pulled.load(Ordering::SeqCst);
    assert!(n < 200, "pulled {n} games for a cap of 10");
}

#[tokio::test]
async fn the_cap_keeps_the_same_games_at_every_thread_count() {
    let games: Vec<String> = ids_of(400)
        .iter()
        .enumerate()
        .map(|(i, id)| plain(id, 1 + (i % 28) as u32))
        .collect();
    let filter = SampleFilter {
        max_games: Some(77),
        sample_rate: 0.5,
        seed: 11,
        ..SampleFilter::default()
    };
    let mut runs = Vec::new();
    for threads in [1, 4] {
        let sink = Arc::new(Recorder::default());
        let stats = run_pipeline(source(games.clone()), &filter, &sink, &cfg(threads, 7))
            .await
            .unwrap();
        assert_eq!(stats.games_kept, 77);
        runs.push((stats.games_seen, sink.seen.lock().unwrap().clone()));
    }
    assert_eq!(runs[0], runs[1]);
}

#[tokio::test]
async fn source_errors_end_the_pipeline_with_the_error() {
    let games = futures::stream::iter(vec![
        Ok(plain("aaa1", 1)),
        Ok(plain("aaa2", 1)),
        Err(DumpError::Transport("connection reset".into())),
        Ok(plain("aaa3", 1)),
    ]);
    let sink = Arc::new(Recorder::default());
    let err = run_pipeline(games, &SampleFilter::default(), &sink, &cfg(2, 1))
        .await
        .unwrap_err();
    assert_eq!(err, DumpError::Transport("connection reset".into()));
}

struct FailingMerge;

impl GameSink for FailingMerge {
    type Partial = ();

    fn observe(&self, _pgn: &str, _h: &DumpHeader, _tc: TimeClass) {}

    fn merge(&self, _parts: Vec<()>) -> BoxFuture<'_, Result<(), DumpError>> {
        Box::pin(async { Err(DumpError::Store("disk full".into())) })
    }
}

#[tokio::test]
async fn merge_errors_end_the_pipeline() {
    let games: Vec<String> = ids_of(50).iter().map(|id| plain(id, 1)).collect();
    let err = run_pipeline(
        source(games),
        &SampleFilter::default(),
        &Arc::new(FailingMerge),
        &cfg(2, 5),
    )
    .await
    .unwrap_err();
    assert_eq!(err, DumpError::Store("disk full".into()));
}

struct PanickingSink;

impl GameSink for PanickingSink {
    type Partial = ();

    fn observe(&self, _pgn: &str, h: &DumpHeader, _tc: TimeClass) {
        assert!(h.lichess_game_id != "boom7", "observe panicked");
    }

    fn merge(&self, _parts: Vec<()>) -> BoxFuture<'_, Result<(), DumpError>> {
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn a_panicking_observe_is_an_error_not_an_abort() {
    let games: Vec<String> = (0..10).map(|i| plain(&format!("boom{i}"), 1)).collect();
    let err = run_pipeline(
        source(games),
        &SampleFilter::default(),
        &Arc::new(PanickingSink),
        &cfg(2, 3),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, DumpError::Decode(ref m) if m.contains("panicked")),
        "{err:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// Sampling across the month (T19 review ruling: the dump is time-ordered)

#[test]
fn proportional_rate_is_target_over_expected_capped_at_one() {
    assert_eq!(proportional_rate(2_000_000, 100_000_000), 0.02);
    assert_eq!(proportional_rate(10, 5), 1.0);
    assert_eq!(proportional_rate(10, 0), 1.0);
}

#[tokio::test]
async fn kept_games_spread_across_the_whole_month() {
    // 30 days x 200 games, in time order like the dump. Target 300 of 6000.
    let games: Vec<String> = (0..6_000)
        .map(|i| plain(&format!("m{i:05}"), 1 + (i / 200) as u32))
        .collect();
    let target = 300;
    let filter = SampleFilter {
        max_games: Some(target * 2),
        sample_rate: proportional_rate(target, 6_000),
        seed: 3,
        ..SampleFilter::default()
    };
    let sink = Arc::new(Recorder::default());
    let stats = run_pipeline(source(games), &filter, &sink, &cfg(4, 64))
        .await
        .unwrap();
    assert!(!stats.capped, "the cap is only a safety net");
    assert_eq!(stats.games_seen, 6_000, "the whole month is read");
    let kept = stats.games_kept;
    assert!((200..=400).contains(&kept), "kept {kept}, target {target}");
    let days: BTreeSet<u8> = sink.seen.lock().unwrap().iter().map(|s| s.1).collect();
    assert_eq!(
        days.len(),
        30,
        "every day of the month is sampled: {days:?}"
    );
    // Each third of the month holds roughly a third of the sample.
    let seen = sink.seen.lock().unwrap();
    for third in 0..3u8 {
        let n = seen.iter().filter(|s| (s.1 - 1) / 10 == third).count() as u64;
        assert!(
            n * 3 > kept / 2 && n * 3 < kept * 2,
            "third {third}: {n} of {kept}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// BOT and provisional filters (T19 review rulings)

fn header(tags: &str) -> DumpHeader {
    parse_dump_headers(&game("tag1", 1, tags, "1. e4 e5")).unwrap()
}

#[test]
fn titles_and_rating_diffs_are_parsed() {
    let h = header(
        "[WhiteTitle \"BOT\"]\n[BlackTitle \"GM\"]\n[WhiteRatingDiff \"+31\"]\n[BlackRatingDiff \"-4\"]",
    );
    assert_eq!(h.white_title.as_deref(), Some("BOT"));
    assert_eq!(h.black_title.as_deref(), Some("GM"));
    assert_eq!(
        (h.white_rating_diff, h.black_rating_diff),
        (Some(31), Some(-4))
    );
    let h = header("");
    assert_eq!((h.white_title, h.black_title), (None, None));
    assert_eq!((h.white_rating_diff, h.black_rating_diff), (None, None));
}

#[test]
fn bot_games_are_excluded_by_default() {
    let f = SampleFilter::default();
    assert!(f.exclude_bots);
    assert_eq!(keep(&header(""), &f), Some(TimeClass::Blitz));
    assert_eq!(keep(&header("[WhiteTitle \"BOT\"]"), &f), None);
    assert_eq!(keep(&header("[BlackTitle \"BOT\"]"), &f), None);
    assert_eq!(
        keep(&header("[BlackTitle \"GM\"]"), &f),
        Some(TimeClass::Blitz)
    );
    let keep_bots = SampleFilter {
        exclude_bots: false,
        ..SampleFilter::default()
    };
    assert_eq!(
        keep(&header("[WhiteTitle \"BOT\"]"), &keep_bots),
        Some(TimeClass::Blitz)
    );
}

#[test]
fn provisional_heuristic_flags_large_rating_changes() {
    assert!(!looks_provisional(None));
    assert!(!looks_provisional(Some(6)));
    assert!(!looks_provisional(Some(-24)));
    assert!(looks_provisional(Some(25)));
    assert!(looks_provisional(Some(-25)));
    assert!(looks_provisional(Some(140)));
    let h = header("[WhiteRatingDiff \"+6\"]\n[BlackRatingDiff \"-160\"]");
    assert!(!h.white_provisional());
    assert!(h.black_provisional());
}

#[test]
fn provisional_games_are_dropped_only_when_asked() {
    let h = header("[WhiteRatingDiff \"+40\"]\n[BlackRatingDiff \"-6\"]");
    assert_eq!(keep(&h, &SampleFilter::default()), Some(TimeClass::Blitz));
    let strict = SampleFilter {
        exclude_provisional: true,
        ..SampleFilter::default()
    };
    assert_eq!(keep(&h, &strict), None);
    let calm = header("[WhiteRatingDiff \"+6\"]\n[BlackRatingDiff \"-6\"]");
    assert_eq!(keep(&calm, &strict), Some(TimeClass::Blitz));
}

// ---------------------------------------------------------------------------------------------
// Frames and the zstd window

async fn zstd_with(data: &[u8], params: &[CParameter]) -> Vec<u8> {
    let mut enc = ZstdEncoder::with_quality_and_params(data, Level::Default, params);
    let mut out = Vec::new();
    enc.read_to_end(&mut out).await.unwrap();
    out
}

/// A pzstd-style frame: a skippable frame holding the size of the zstd frame that follows.
fn pzstd_frame(frame: &[u8]) -> Vec<u8> {
    let mut out = vec![0x50, 0x2a, 0x4d, 0x18, 4, 0, 0, 0];
    out.extend_from_slice(&u32::try_from(frame.len()).unwrap().to_le_bytes());
    out.extend_from_slice(frame);
    out
}

async fn collect(stream: &mut DumpStream) -> Result<Vec<String>, DumpError> {
    let mut out = Vec::new();
    while let Some(g) = stream.next().await {
        out.push(g?);
    }
    Ok(out)
}

#[tokio::test]
async fn a_game_split_mid_move_across_two_pzstd_frames_is_whole() {
    let text: String = ["one1", "two2", "thr3"]
        .iter()
        .map(|id| game(id, 1, "", "1. e4 e5 2. Nf3 Nc6 3. Bb5 a6"))
        .collect();
    // Cut inside the SAN "Nf3" of the second game.
    let second = text.find("two2").unwrap();
    let cut = second + text[second..].find("Nf3").unwrap() + 2;
    assert_eq!(&text[cut - 2..cut + 1], "Nf3");
    let mut body = pzstd_frame(&zstd_with(&text.as_bytes()[..cut], &[]).await);
    body.extend(pzstd_frame(&zstd_with(&text.as_bytes()[cut..], &[]).await));
    let digest = hex::encode(Sha256::digest(&body));
    let (mut stream, sha) =
        DumpStream::from_bytes(futures::stream::iter([Ok::<_, std::io::Error>(
            Bytes::from(body),
        )]));
    let games = collect(&mut stream).await.unwrap();
    assert_eq!(games, split_games(text.lines()));
    assert!(games[1].contains("2. Nf3 Nc6"));
    assert_eq!(sha.verify(&digest).unwrap(), digest);
}

#[tokio::test]
async fn a_window_above_2_pow_24_is_refused() {
    let text = plain("win1", 1).repeat(4);
    let small = zstd_with(text.as_bytes(), &[CParameter::window_log(24)]).await;
    let (mut ok, _) = DumpStream::from_bytes(futures::stream::iter([Ok::<_, std::io::Error>(
        Bytes::from(small),
    )]));
    assert_eq!(collect(&mut ok).await.unwrap().len(), 4);
    let big = zstd_with(text.as_bytes(), &[CParameter::window_log(25)]).await;
    let (mut refused, _) =
        DumpStream::from_bytes(futures::stream::iter([Ok::<_, std::io::Error>(
            Bytes::from(big),
        )]));
    assert!(matches!(
        collect(&mut refused).await,
        Err(DumpError::Decode(_))
    ));
}

// ---------------------------------------------------------------------------------------------
// HTTP retry and in-process resume (T19 review ruling)

async fn zstd(data: &[u8]) -> Vec<u8> {
    zstd_with(data, &[]).await
}

async fn dump_body(n: usize) -> (String, Vec<u8>) {
    let text: String = ids_of(n).iter().map(|id| plain(id, 1)).collect();
    let body = zstd(text.as_bytes()).await;
    (text, body)
}

fn fast() -> RetryPolicy {
    RetryPolicy {
        connect_tries: 3,
        base_delay: Duration::from_millis(1),
        max_resumes: 3,
    }
}

/// How the raw server answers dump requests.
#[derive(Clone, Copy)]
struct RawPlan {
    /// Sent on every response (`None`: no ETag header).
    etag: Option<&'static str>,
    /// The first `cuts` plain GETs close the connection after this many body bytes.
    cut_after: usize,
    cuts: usize,
    /// Answer `Range` requests with 206 (else with a full 200).
    honour_range: bool,
    /// Every ranged response closes after `cut_after` bytes too.
    cut_ranged: bool,
}

#[derive(Default)]
struct RawLog {
    requests: Vec<String>,
}

fn header_value<'a>(req: &'a str, name: &str) -> Option<&'a str> {
    req.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.trim().eq_ignore_ascii_case(name).then(|| v.trim())
    })
}

/// A raw HTTP/1.1 server for the dump with `Content-Length`, which can close mid-body and can
/// honour `Range: bytes=<n>-` with `If-Range`.
async fn raw_server(body: Vec<u8>, plan: RawPlan) -> (String, Arc<Mutex<RawLog>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let body = Arc::new(body);
    let log = Arc::new(Mutex::new(RawLog::default()));
    let log2 = log.clone();
    let plain_gets = Arc::new(AtomicUsize::new(0));
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let (body, log, plain_gets) = (body.clone(), log2.clone(), plain_gets.clone());
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                log.lock().unwrap().requests.push(req.clone());
                let range = header_value(&req, "range")
                    .and_then(|r| r.strip_prefix("bytes="))
                    .and_then(|r| r.strip_suffix('-'))
                    .and_then(|r| r.parse::<usize>().ok());
                let if_range = header_value(&req, "if-range");
                let etag_line = plan
                    .etag
                    .map(|e| format!("etag: {e}\r\n"))
                    .unwrap_or_default();
                let (status, start, cut) = match range {
                    Some(start) if plan.honour_range && if_range == plan.etag => {
                        ("206 Partial Content", start, plan.cut_ranged)
                    }
                    _ => {
                        let k = plain_gets.fetch_add(1, Ordering::SeqCst);
                        ("200 OK", 0, k < plan.cuts)
                    }
                };
                let rest = &body[start..];
                let content_range = if start > 0 || status.starts_with("206") {
                    format!(
                        "content-range: bytes {start}-{}/{}\r\n",
                        body.len() - 1,
                        body.len()
                    )
                } else {
                    String::new()
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\ncontent-length: {}\r\n{etag_line}{content_range}\
                     connection: close\r\n\r\n",
                    rest.len()
                );
                let _ = sock.write_all(head.as_bytes()).await;
                let send = if cut {
                    &rest[..plan.cut_after.min(rest.len())]
                } else {
                    rest
                };
                let _ = sock.write_all(send).await;
                let _ = sock.flush().await;
            });
        }
    });
    (format!("http://{addr}"), log)
}

fn client() -> reqwest::Client {
    lichess_db::config::http_client().unwrap()
}

#[tokio::test]
async fn a_mid_body_close_resumes_with_range_and_verifies() {
    let (text, body) = dump_body(300).await;
    let digest = hex::encode(Sha256::digest(&body));
    let plan = RawPlan {
        etag: Some("\"v1\""),
        cut_after: body.len() / 3,
        cuts: 1,
        honour_range: true,
        cut_ranged: false,
    };
    let (base, log) = raw_server(body.clone(), plan).await;
    let (mut games, sha) = open_dump_with(&client(), &DumpSource::new(&base, MONTH), &fast())
        .await
        .unwrap();
    let got = collect(&mut games).await.unwrap();
    assert_eq!(got, split_games(text.lines()));
    assert_eq!(drain_to_end(games).await.unwrap(), 0);
    assert_eq!(sha.bytes_read(), body.len() as u64);
    assert_eq!(sha.verify(&digest).unwrap(), digest);
    let reqs = log.lock().unwrap().requests.clone();
    assert_eq!(reqs.len(), 2);
    let resume = &reqs[1];
    assert_eq!(
        header_value(resume, "range"),
        Some(format!("bytes={}-", body.len() / 3).as_str())
    );
    assert_eq!(header_value(resume, "if-range"), Some("\"v1\""));
    assert!(
        reqs.iter()
            .all(|r| r.starts_with(&format!("GET {DUMP_PATH} ")))
    );
}

#[tokio::test]
async fn a_resume_answered_with_200_is_a_transport_error() {
    let (_, body) = dump_body(300).await;
    let plan = RawPlan {
        etag: Some("\"v1\""),
        cut_after: body.len() / 3,
        cuts: 1,
        honour_range: false,
        cut_ranged: false,
    };
    let (base, _) = raw_server(body, plan).await;
    let (mut games, sha) = open_dump_with(&client(), &DumpSource::new(&base, MONTH), &fast())
        .await
        .unwrap();
    assert!(matches!(
        collect(&mut games).await,
        Err(DumpError::Transport(_))
    ));
    assert!(!sha.is_complete());
}

#[tokio::test]
async fn no_etag_means_no_resume() {
    let (_, body) = dump_body(300).await;
    let plan = RawPlan {
        etag: None,
        cut_after: body.len() / 3,
        cuts: 1,
        honour_range: true,
        cut_ranged: false,
    };
    let (base, log) = raw_server(body, plan).await;
    let (mut games, _) = open_dump_with(&client(), &DumpSource::new(&base, MONTH), &fast())
        .await
        .unwrap();
    assert!(matches!(
        collect(&mut games).await,
        Err(DumpError::Transport(_))
    ));
    assert_eq!(log.lock().unwrap().requests.len(), 1);
}

#[tokio::test]
async fn resumes_are_capped() {
    let (_, body) = dump_body(300).await;
    let plan = RawPlan {
        etag: Some("\"v1\""),
        cut_after: 64,
        cuts: 1,
        honour_range: true,
        cut_ranged: true,
    };
    let (base, log) = raw_server(body, plan).await;
    let (mut games, sha) = open_dump_with(&client(), &DumpSource::new(&base, MONTH), &fast())
        .await
        .unwrap();
    assert!(matches!(
        collect(&mut games).await,
        Err(DumpError::Transport(_))
    ));
    assert!(!sha.is_complete());
    assert_eq!(
        log.lock().unwrap().requests.len(),
        1 + 3,
        "1 GET + max_resumes"
    );
}

#[tokio::test]
async fn connect_retries_5xx_and_429_then_succeeds() {
    let (text, body) = dump_body(5).await;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(DUMP_PATH))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(DUMP_PATH))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(DUMP_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(&server)
        .await;
    let (mut games, _) = open_dump_with(&client(), &DumpSource::new(server.uri(), MONTH), &fast())
        .await
        .unwrap();
    assert_eq!(
        collect(&mut games).await.unwrap(),
        split_games(text.lines())
    );
}

#[tokio::test]
async fn connect_retries_are_bounded_to_three_tries() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(DUMP_PATH))
        .respond_with(ResponseTemplate::new(502))
        .expect(3)
        .mount(&server)
        .await;
    let err = open_dump_with(&client(), &DumpSource::new(server.uri(), MONTH), &fast())
        .await
        .unwrap_err();
    assert_eq!(err, DumpError::Http(502));
}

#[tokio::test]
async fn a_404_is_not_retried() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(DUMP_PATH))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    let err = open_dump_with(&client(), &DumpSource::new(server.uri(), MONTH), &fast())
        .await
        .unwrap_err();
    assert_eq!(err, DumpError::Http(404));
}
