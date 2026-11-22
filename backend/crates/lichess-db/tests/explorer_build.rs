//! Opening explorer build from a dump (R2 Task 20, TS M2 Task 11): counts, pruning, supersede,
//! verification (R-9), determinism across thread counts, a failed stream, and stage metrics.
//! The dumps are small synthetic `.pgn.zst` bodies served by wiremock or a raw local server.

use std::sync::Arc;
use std::time::Duration;

use async_compression::tokio::bufread::ZstdEncoder;
use chess_core::TimeClass;
use chess_core::board::Position;
use chess_core::fen::fen_key;
use lichess_db::config::{DumpSource, http_client};
use lichess_db::download::{DumpError, RetryPolicy};
use lichess_db::explorer_build::{BuildOptions, Verify, build, explorer_band, safety_cap};
use lichess_db::pipeline::PipelineConfig;
use profile_core::types::Band;
use refdata::builds::{BuildKind, BuildStatus, active_build, get_build};
use refdata::explorer::query_explorer;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn blitz(id: &str, white: i32, black: i32, result: &str, moves: &str) -> String {
    format!(
        "[Event \"Rated Blitz game\"]\n[Site \"https://lichess.org/{id}\"]\n\
         [White \"FixtureW\"]\n[Black \"FixtureB\"]\n[Result \"{result}\"]\n\
         [UTCDate \"2026.10.02\"]\n[UTCTime \"10:00:00\"]\n[WhiteElo \"{white}\"]\n\
         [BlackElo \"{black}\"]\n[WhiteRatingDiff \"+5\"]\n[BlackRatingDiff \"-5\"]\n\
         [TimeControl \"180+0\"]\n[Termination \"Normal\"]\n\n{moves} {result}\n\n"
    )
}

/// TS M2 Task 11: three B2 blitz games `1.e4 e5` (1-0), `1.e4 c5` (0-1), `1.d4 d5` (½).
fn three_games() -> String {
    [
        blitz("tsA1", 1550, 1450, "1-0", "1. e4 e5"),
        blitz("tsB2", 1600, 1500, "0-1", "1. e4 c5"),
        blitz("tsC3", 1500, 1480, "1/2-1/2", "1. d4 d5"),
    ]
    .concat()
}

async fn zstd(data: &[u8]) -> Vec<u8> {
    let mut enc = ZstdEncoder::new(data);
    let mut out = Vec::new();
    enc.read_to_end(&mut out).await.unwrap();
    out
}

fn sha_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

fn dump_path(month: &str) -> String {
    format!("/standard/lichess_db_standard_rated_{month}.pgn.zst")
}

/// A wiremock server with one dump per month and a `sha256sums.txt` listing `sums` (month, hex).
async fn server(dumps: &[(&str, Vec<u8>)], sums: &[(&str, String)]) -> MockServer {
    let server = MockServer::start().await;
    let text: String = sums
        .iter()
        .map(|(m, h)| format!("{h}  lichess_db_standard_rated_{m}.pgn.zst\n"))
        .collect();
    Mock::given(method("GET"))
        .and(path("/standard/sha256sums.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string(text))
        .mount(&server)
        .await;
    for (month, body) in dumps {
        Mock::given(method("GET"))
            .and(path(dump_path(month)))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
            .mount(&server)
            .await;
    }
    server
}

fn opts(month: &str) -> BuildOptions {
    BuildOptions {
        max_ply: 2,
        min_row_count: 1,
        sample_rate: Some(1.0),
        retry: RetryPolicy {
            connect_tries: 3,
            base_delay: Duration::from_millis(1),
            max_resumes: 3,
        },
        pipeline: PipelineConfig {
            threads: 2,
            batch_games: 2,
            channel_batches: 4,
        },
        ..BuildOptions::new(month)
    }
}

fn start_key() -> String {
    fen_key(&Position::start().fen())
}

async fn rows_of(
    pool: &PgPool,
    build: Uuid,
) -> Vec<(String, String, String, String, i32, i32, i32)> {
    sqlx::query_as(
        "SELECT fen_key, band::text, time_class::text, uci, white, draws, black \
         FROM opening_explorer WHERE build_id = $1 \
         ORDER BY fen_key, band, time_class, uci",
    )
    .bind(build)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn status(pool: &PgPool, id: Uuid) -> BuildStatus {
    get_build(pool, id).await.unwrap().unwrap().status
}

#[test]
fn explorer_band_is_the_band_of_the_mean_rating() {
    assert_eq!(explorer_band(1550, 1450), Band::B2);
    assert_eq!(explorer_band(1250, 1340), Band::B1, "mean 1295");
    assert_eq!(explorer_band(2400, 1600), Band::B4);
    assert_eq!(explorer_band(800, 900), Band::B0);
}

#[test]
fn build_options_defaults_match_adr_0035() {
    let o = BuildOptions::new("2026-10");
    assert_eq!(o.month, "2026-10");
    assert_eq!(o.max_games, 2_000_000);
    assert_eq!(o.max_ply, 30);
    assert_eq!(o.min_row_count, 3);
    assert_eq!(o.flush_every_games, 50_000);
    assert_eq!(o.verify, Verify::Full);
    assert!(o.explorer);
    assert!(!o.band_reference);
    assert_eq!(o.sample_rate, None, "derived from the expected month size");
    assert_eq!(safety_cap(2_000_000), 2_500_000);
    assert!(
        !o.exclude_provisional,
        "the provisional heuristic filters partly by result: opt-in only"
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn three_b2_blitz_games_count_the_start_position(pool: PgPool) {
    let body = zstd(three_games().as_bytes()).await;
    let srv = server(&[("2026-10", body.clone())], &[("2026-10", sha_hex(&body))]).await;
    let out = build(
        &pool,
        &http_client().unwrap(),
        &DumpSource::new(srv.uri(), "2026-10"),
        &opts("2026-10"),
    )
    .await
    .unwrap();
    let id = out.explorer_build.unwrap();
    assert_eq!(out.reference_build, None);
    assert_eq!(status(&pool, id).await, BuildStatus::Active);
    let row = get_build(&pool, id).await.unwrap().unwrap();
    assert_eq!(row.source_sha256, sha_hex(&body));
    assert_eq!(row.games_scanned, 3);
    assert_eq!((out.stats.games_seen, out.stats.games_kept), (3, 3));
    assert_eq!(out.stats.bytes_compressed, body.len() as u64);

    let moves = query_explorer(&pool, &start_key(), Band::B2, TimeClass::Blitz)
        .await
        .unwrap();
    let got: Vec<(&str, i64, i64, i64, i64)> = moves
        .iter()
        .map(|m| (m.uci.as_str(), m.white, m.draws, m.black, m.total))
        .collect();
    assert_eq!(got, [("e2e4", 1, 0, 1, 2), ("d2d4", 0, 1, 0, 1)]);
    // The reply after 1.e4 is counted too (max_ply 2), and nothing beyond it.
    let after_e4 = Position::start().play_uci("e2e4").unwrap();
    let replies = query_explorer(&pool, &fen_key(&after_e4.fen()), Band::B2, TimeClass::Blitz)
        .await
        .unwrap();
    let ucis: Vec<&str> = replies.iter().map(|m| m.uci.as_str()).collect();
    assert_eq!(ucis, ["c7c5", "e7e5"]);
    assert_eq!(rows_of(&pool, id).await.len(), 5);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn min_row_count_2_prunes_d2d4(pool: PgPool) {
    let body = zstd(three_games().as_bytes()).await;
    let srv = server(&[("2026-10", body.clone())], &[("2026-10", sha_hex(&body))]).await;
    let o = BuildOptions {
        min_row_count: 2,
        ..opts("2026-10")
    };
    build(
        &pool,
        &http_client().unwrap(),
        &DumpSource::new(srv.uri(), "2026-10"),
        &o,
    )
    .await
    .unwrap();
    let moves = query_explorer(&pool, &start_key(), Band::B2, TimeClass::Blitz)
        .await
        .unwrap();
    let ucis: Vec<&str> = moves.iter().map(|m| m.uci.as_str()).collect();
    assert_eq!(ucis, ["e2e4"]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_second_build_supersedes_the_first(pool: PgPool) {
    let first = zstd(three_games().as_bytes()).await;
    let second_text = [
        blitz("nwA1", 1550, 1450, "1-0", "1. c4 e5"),
        blitz("nwB2", 1550, 1450, "1-0", "1. c4 c5"),
    ]
    .concat();
    let second = zstd(second_text.as_bytes()).await;
    let srv = server(
        &[("2026-09", first.clone()), ("2026-10", second.clone())],
        &[("2026-09", sha_hex(&first)), ("2026-10", sha_hex(&second))],
    )
    .await;
    let client = http_client().unwrap();
    let a = build(
        &pool,
        &client,
        &DumpSource::new(srv.uri(), "2026-09"),
        &opts("2026-09"),
    )
    .await
    .unwrap()
    .explorer_build
    .unwrap();
    let b = build(
        &pool,
        &client,
        &DumpSource::new(srv.uri(), "2026-10"),
        &opts("2026-10"),
    )
    .await
    .unwrap()
    .explorer_build
    .unwrap();
    assert_eq!(status(&pool, a).await, BuildStatus::Superseded);
    assert_eq!(status(&pool, b).await, BuildStatus::Active);
    assert!(rows_of(&pool, a).await.is_empty());
    let moves = query_explorer(&pool, &start_key(), Band::B2, TimeClass::Blitz)
        .await
        .unwrap();
    let got: Vec<(&str, i64)> = moves.iter().map(|m| (m.uci.as_str(), m.total)).collect();
    assert_eq!(got, [("c2c4", 2)]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn sha_mismatch_marks_build_failed_and_keeps_previous_active(pool: PgPool) {
    let first = zstd(three_games().as_bytes()).await;
    let second = zstd(blitz("bad1", 1550, 1450, "1-0", "1. c4 e5").as_bytes()).await;
    let srv = server(
        &[("2026-09", first.clone()), ("2026-10", second.clone())],
        &[
            ("2026-09", sha_hex(&first)),
            ("2026-10", sha_hex(b"not the dump")),
        ],
    )
    .await;
    let client = http_client().unwrap();
    let a = build(
        &pool,
        &client,
        &DumpSource::new(srv.uri(), "2026-09"),
        &opts("2026-09"),
    )
    .await
    .unwrap()
    .explorer_build
    .unwrap();
    let before = rows_of(&pool, a).await;
    let err = build(
        &pool,
        &client,
        &DumpSource::new(srv.uri(), "2026-10"),
        &opts("2026-10"),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, DumpError::ChecksumMismatch { .. }), "{err:?}");
    let failed: (Uuid, Option<String>, i64) = sqlx::query_as(
        "SELECT id, error, (SELECT count(*) FROM opening_explorer o WHERE o.build_id = b.id) \
         FROM reference_builds b WHERE source_month = '2026-10'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status(&pool, failed.0).await, BuildStatus::Failed);
    assert!(failed.1.unwrap().contains("checksum mismatch"));
    assert_eq!(failed.2, 0, "a failed build keeps no rows");
    assert_eq!(
        active_build(&pool, BuildKind::OpeningExplorer)
            .await
            .unwrap(),
        Some(a)
    );
    assert_eq!(rows_of(&pool, a).await, before);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn verify_none_leaves_build_unverified(pool: PgPool) {
    let body = zstd(three_games().as_bytes()).await;
    // No sha256sums.txt at all: --verify none never asks for it.
    let srv = server(&[("2026-10", body)], &[]).await;
    let o = BuildOptions {
        verify: Verify::None,
        ..opts("2026-10")
    };
    let id = build(
        &pool,
        &http_client().unwrap(),
        &DumpSource::new(srv.uri(), "2026-10"),
        &o,
    )
    .await
    .unwrap()
    .explorer_build
    .unwrap();
    assert_eq!(status(&pool, id).await, BuildStatus::Unverified);
    assert_eq!(
        rows_of(&pool, id).await.len(),
        5,
        "rows stay for inspection"
    );
    assert_eq!(
        active_build(&pool, BuildKind::OpeningExplorer)
            .await
            .unwrap(),
        None
    );
    assert!(
        query_explorer(&pool, &start_key(), Band::B2, TimeClass::Blitz)
            .await
            .unwrap()
            .is_empty()
    );
}

/// Varied games: the five fixture games and the three TS games, each repeated with distinct
/// sites, ratings across every band and rotating results.
fn varied_games(n: usize) -> String {
    let fixture = testkit::fixture("lichess-db/sample.pgn");
    let base: Vec<String> = lichess_db::split::split_games(fixture.lines())
        .into_iter()
        .chain(lichess_db::split::split_games(three_games().lines()))
        .collect();
    let mut out = String::new();
    for i in 0..n {
        let g = &base[i % base.len()];
        let site_start = g.find("[Site \"").unwrap() + 7;
        let site_end = site_start + g[site_start..].find('"').unwrap();
        let elo = 700 + (i * 137) % 1900;
        let mut g = format!(
            "{}https://lichess.org/v{i:05}{}",
            &g[..site_start],
            &g[site_end..]
        );
        for tag in ["WhiteElo", "BlackElo"] {
            let s = g.find(&format!("[{tag} \"")).unwrap() + tag.len() + 3;
            let e = s + g[s..].find('"').unwrap();
            g = format!(
                "{}{}{}",
                &g[..s],
                elo + if tag == "BlackElo" { 40 } else { 0 },
                &g[e..]
            );
        }
        out.push_str(&g);
        if !out.ends_with("\n\n") {
            out.push('\n');
        }
    }
    out
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn pipeline_is_deterministic_across_thread_counts(pool: PgPool) {
    let body = zstd(varied_games(400).as_bytes()).await;
    let srv = server(&[("2026-10", body)], &[]).await;
    let client = http_client().unwrap();
    let mut rows = Vec::new();
    for threads in [1, 4] {
        let o = BuildOptions {
            verify: Verify::None,
            max_ply: 30,
            min_row_count: 1,
            flush_every_games: 37,
            // A cap inside the stream, so the kept set depends on merge order. Sums commute and
            // these batches finish nearly in order, so the merge-order guard proper is the
            // pipeline tests (cap and order), which fail under `buffer_unordered`.
            max_games: 200,
            pipeline: PipelineConfig {
                threads,
                batch_games: 9,
                channel_batches: 2 * threads,
            },
            ..opts("2026-10")
        };
        let id = build(&pool, &client, &DumpSource::new(srv.uri(), "2026-10"), &o)
            .await
            .unwrap()
            .explorer_build
            .unwrap();
        rows.push(rows_of(&pool, id).await);
    }
    assert!(rows[0].len() > 200, "{} rows", rows[0].len());
    let bands: std::collections::BTreeSet<&str> = rows[0].iter().map(|r| r.1.as_str()).collect();
    assert!(bands.len() >= 4, "{bands:?}");
    assert_eq!(rows[0], rows[1]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_capped_full_build_still_drains_verifies_and_activates(pool: PgPool) {
    let body = zstd(varied_games(400).as_bytes()).await;
    let srv = server(&[("2026-10", body.clone())], &[("2026-10", sha_hex(&body))]).await;
    let o = BuildOptions {
        max_games: 40,
        min_row_count: 1,
        ..opts("2026-10")
    };
    let out = build(
        &pool,
        &http_client().unwrap(),
        &DumpSource::new(srv.uri(), "2026-10"),
        &o,
    )
    .await
    .unwrap();
    assert!(out.stats.capped);
    assert_eq!(out.stats.games_kept, safety_cap(40));
    assert!(out.stats.games_seen < 400);
    assert_eq!(
        out.stats.bytes_compressed,
        body.len() as u64,
        "drained to the end"
    );
    let id = out.explorer_build.unwrap();
    let row = get_build(&pool, id).await.unwrap().unwrap();
    assert_eq!(row.status, BuildStatus::Active);
    assert_eq!(row.source_sha256, sha_hex(&body));
    assert_eq!(row.params["capped"], serde_json::json!(true));
}

/// A raw HTTP server: the sums file, and a dump body that closes after half its bytes (no
/// ETag, so no resume is possible).
async fn cut_server(month: &str, body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let sums = format!(
        "{}  lichess_db_standard_rated_{month}.pgn.zst\n",
        sha_hex(&body)
    );
    let body = Arc::new(body);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let (body, sums) = (body.clone(), sums.clone());
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                if req.starts_with("GET /standard/sha256sums.txt") {
                    let head = format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n", sums.len());
                    let _ = sock.write_all(head.as_bytes()).await;
                    let _ = sock.write_all(sums.as_bytes()).await;
                } else {
                    let head = format!(
                        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = sock.write_all(head.as_bytes()).await;
                    let _ = sock.write_all(&body[..body.len() / 2]).await;
                }
                let _ = sock.flush().await;
            });
        }
    });
    format!("http://{addr}")
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn failed_stream_marks_build_failed(pool: PgPool) {
    let first = zstd(three_games().as_bytes()).await;
    let srv = server(
        &[("2026-09", first.clone())],
        &[("2026-09", sha_hex(&first))],
    )
    .await;
    let client = http_client().unwrap();
    let a = build(
        &pool,
        &client,
        &DumpSource::new(srv.uri(), "2026-09"),
        &opts("2026-09"),
    )
    .await
    .unwrap()
    .explorer_build
    .unwrap();
    let body = zstd(varied_games(300).as_bytes()).await;
    let base = cut_server("2026-10", body).await;
    let err = build(
        &pool,
        &client,
        &DumpSource::new(&base, "2026-10"),
        &opts("2026-10"),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, DumpError::Transport(_)), "{err:?}");
    let (id, n): (Uuid, i64) = sqlx::query_as(
        "SELECT id, (SELECT count(*) FROM opening_explorer o WHERE o.build_id = b.id) \
         FROM reference_builds b WHERE source_month = '2026-10'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status(&pool, id).await, BuildStatus::Failed);
    assert_eq!(n, 0);
    assert_eq!(
        active_build(&pool, BuildKind::OpeningExplorer)
            .await
            .unwrap(),
        Some(a)
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn build_records_stage_metrics(pool: PgPool) {
    let body = zstd(three_games().as_bytes()).await;
    let srv = server(&[("2026-10", body.clone())], &[("2026-10", sha_hex(&body))]).await;
    build(
        &pool,
        &http_client().unwrap(),
        &DumpSource::new(srv.uri(), "2026-10"),
        &opts("2026-10"),
    )
    .await
    .unwrap();
    let rows: Vec<(String, f64, serde_json::Value)> =
        sqlx::query_as("SELECT kind, total_ms, stages FROM job_metrics")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 1);
    let (kind, total, stages) = &rows[0];
    assert_eq!(kind, "lichess-db:build");
    assert!(*total > 0.0);
    for stage in [
        "read_ms",
        "parse_ms",
        "aggregate_ms",
        "flush_ms",
        "drain_ms",
        "prune_ms",
    ] {
        assert!(
            stages[stage].as_f64().is_some(),
            "{stage} missing in {stages}"
        );
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn without_a_rate_the_sample_is_proportional_to_the_previous_month(pool: PgPool) {
    // Previous month: 400 games scanned. Target 100 → rate 0.25.
    let first = zstd(varied_games(400).as_bytes()).await;
    let second = zstd(varied_games(400).as_bytes()).await;
    let srv = server(
        &[("2026-09", first.clone()), ("2026-10", second.clone())],
        &[("2026-09", sha_hex(&first)), ("2026-10", sha_hex(&second))],
    )
    .await;
    let client = http_client().unwrap();
    let full = build(
        &pool,
        &client,
        &DumpSource::new(srv.uri(), "2026-09"),
        &opts("2026-09"),
    )
    .await
    .unwrap();
    assert_eq!(full.stats.games_seen, 400);
    let o = BuildOptions {
        sample_rate: None,
        max_games: 100,
        ..opts("2026-10")
    };
    let out = build(&pool, &client, &DumpSource::new(srv.uri(), "2026-10"), &o)
        .await
        .unwrap();
    assert_eq!(out.sample_rate, 0.25);
    assert_eq!(
        out.stats.games_seen, 400,
        "the whole month is read, not the first 100"
    );
    assert!(!out.stats.capped);
    assert!(out.stats.games_kept <= 125, "kept {}", out.stats.games_kept);
    assert!(out.stats.games_kept > 50, "kept {}", out.stats.games_kept);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn nothing_to_build_is_refused(pool: PgPool) {
    let o = BuildOptions {
        explorer: false,
        band_reference: false,
        ..opts("2026-10")
    };
    let err = build(
        &pool,
        &http_client().unwrap(),
        &DumpSource::new("http://127.0.0.1:9", "2026-10"),
        &o,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, DumpError::Config(_)), "{err:?}");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM reference_builds")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}
