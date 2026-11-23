//! Syzygy fetch: verified download, mismatch cleanup and idempotent re-run. Local wiremock only;
//! the real tablebases are never downloaded by a test.

use std::path::{Path, PathBuf};

use lichess_db::config::http_client;
use lichess_db::download::DumpError;
use lichess_db::syzygy::{FetchLimits, Manifest, fetch_syzygy, fetch_syzygy_with};
use sha2::{Digest, Sha256};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const A: &[u8] = b"syzygy fixture A: KPvK wdl";
const B: &[u8] = b"syzygy fixture B: KQvK dtz, a little longer";

fn sha(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

fn manifest_text() -> String {
    format!(
        "{}  3-4-5-wdl/KPvK.rtbw\n{}  3-4-5-dtz/KQvK.rtbz\n",
        sha(A),
        sha(B)
    )
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

async fn serve(server: &MockServer, p: &str, body: &[u8]) {
    Mock::given(method("GET"))
        .and(path(p))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(server)
        .await;
}

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ci-syzygy-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn manifest_parses_hash_and_file_lines() {
    let m = Manifest::parse(&format!(
        "# comment\n\n{}  a/b.rtbw\n{} *c.rtbz\nnot a line\n",
        sha(A),
        sha(B)
    ));
    assert_eq!(m.entries().len(), 2);
    assert_eq!(m.entries()[0].file, "a/b.rtbw");
    assert_eq!(m.entries()[0].sha256, sha(A));
    assert_eq!(m.entries()[1].file, "c.rtbz");
}

#[test]
fn the_pinned_manifest_is_complete() {
    let m = Manifest::pinned();
    assert_eq!(m.entries().len(), 290);
    let wdl = m
        .entries()
        .iter()
        .filter(|e| e.file.starts_with("3-4-5-wdl/") && e.file.ends_with(".rtbw"))
        .count();
    let dtz = m
        .entries()
        .iter()
        .filter(|e| e.file.starts_with("3-4-5-dtz/") && e.file.ends_with(".rtbz"))
        .count();
    assert_eq!((wdl, dtz), (145, 145));
}

#[tokio::test]
async fn writes_both_files() {
    let server = MockServer::start().await;
    serve(&server, "/3-4-5-wdl/KPvK.rtbw", A).await;
    serve(&server, "/3-4-5-dtz/KQvK.rtbz", B).await;
    let dest = tempdir("both");
    let client = http_client().unwrap();
    let s = fetch_syzygy(
        &client,
        &server.uri(),
        &Manifest::parse(&manifest_text()),
        &dest,
    )
    .await
    .unwrap();
    assert_eq!((s.files, s.skipped), (2, 0));
    assert_eq!(s.bytes, (A.len() + B.len()) as u64);
    assert_eq!(std::fs::read(dest.join("KPvK.rtbw")).unwrap(), A);
    assert_eq!(std::fs::read(dest.join("KQvK.rtbz")).unwrap(), B);
    assert_eq!(names(&dest), ["KPvK.rtbw", "KQvK.rtbz"]);
}

#[tokio::test]
async fn requests_carry_the_project_user_agent() {
    let server = MockServer::start().await;
    serve(&server, "/3-4-5-wdl/KPvK.rtbw", A).await;
    let dest = tempdir("ua");
    let client = http_client().unwrap();
    let m = Manifest::parse(&format!("{}  3-4-5-wdl/KPvK.rtbw\n", sha(A)));
    fetch_syzygy(&client, &server.uri(), &m, &dest)
        .await
        .unwrap();
    let reqs = server.received_requests().await.unwrap();
    let ua = reqs[0]
        .headers
        .get("user-agent")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        ua,
        "rematch/0.1 (+https://github.com/Terkea/rematch)"
    );
    assert!(!ua.contains('@'), "{ua}");
}

#[tokio::test]
async fn corrupted_body_is_a_mismatch_and_leaves_nothing() {
    let server = MockServer::start().await;
    serve(&server, "/3-4-5-wdl/KPvK.rtbw", b"corrupted body").await;
    let dest = tempdir("bad");
    let client = http_client().unwrap();
    let m = Manifest::parse(&format!("{}  3-4-5-wdl/KPvK.rtbw\n", sha(A)));
    let err = fetch_syzygy(&client, &server.uri(), &m, &dest)
        .await
        .unwrap_err();
    match err {
        DumpError::ChecksumMismatch { expected, actual } => {
            assert_eq!(expected, sha(A));
            assert_eq!(actual, sha(b"corrupted body"));
        }
        other => panic!("expected ChecksumMismatch, got {other:?}"),
    }
    assert!(names(&dest).is_empty(), "left {:?}", names(&dest));
}

#[tokio::test]
async fn rerun_makes_no_body_requests() {
    let server = MockServer::start().await;
    serve(&server, "/3-4-5-wdl/KPvK.rtbw", A).await;
    serve(&server, "/3-4-5-dtz/KQvK.rtbz", B).await;
    let dest = tempdir("rerun");
    let client = http_client().unwrap();
    let m = Manifest::parse(&manifest_text());
    fetch_syzygy(&client, &server.uri(), &m, &dest)
        .await
        .unwrap();
    let before = server.received_requests().await.unwrap().len();
    let s = fetch_syzygy(&client, &server.uri(), &m, &dest)
        .await
        .unwrap();
    assert_eq!((s.files, s.bytes, s.skipped), (0, 0, 2));
    assert_eq!(server.received_requests().await.unwrap().len(), before);
}

#[tokio::test]
async fn a_present_file_with_the_wrong_hash_is_refetched() {
    let server = MockServer::start().await;
    serve(&server, "/3-4-5-wdl/KPvK.rtbw", A).await;
    let dest = tempdir("stale");
    std::fs::write(dest.join("KPvK.rtbw"), b"truncated").unwrap();
    let client = http_client().unwrap();
    let m = Manifest::parse(&format!("{}  3-4-5-wdl/KPvK.rtbw\n", sha(A)));
    let s = fetch_syzygy(&client, &server.uri(), &m, &dest)
        .await
        .unwrap();
    assert_eq!((s.files, s.skipped), (1, 0));
    assert_eq!(std::fs::read(dest.join("KPvK.rtbw")).unwrap(), A);
}

#[tokio::test]
async fn http_errors_leave_no_part_file() {
    let server = MockServer::start().await; // every path 404s
    let dest = tempdir("404");
    let client = http_client().unwrap();
    let m = Manifest::parse(&format!("{}  x/KPvK.rtbw\n", sha(A)));
    let err = fetch_syzygy(&client, &server.uri(), &m, &dest)
        .await
        .unwrap_err();
    assert_eq!(err, DumpError::Http(404));
    assert!(names(&dest).is_empty());
}

#[tokio::test]
async fn unsafe_manifest_names_are_rejected_before_any_request() {
    let server = MockServer::start().await;
    let dest = tempdir("unsafe");
    let client = http_client().unwrap();
    let m = Manifest::parse(&format!("{}  ../../evil\n", sha(A)));
    let err = fetch_syzygy(&client, &server.uri(), &m, &dest)
        .await
        .unwrap_err();
    assert!(matches!(err, DumpError::Decode(_)), "{err:?}");
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// Runs the real CLI with CONTACT_EMAIL set and checks the header it sends.
async fn cli_user_agents(args: Vec<String>) -> Vec<String> {
    let server = MockServer::start().await; // every path 404s; only the headers matter
    let mut args = args;
    args.push("--base-url".into());
    args.push(server.uri());
    tokio::task::spawn_blocking(move || {
        std::process::Command::new(env!("CARGO_BIN_EXE_lichess-db"))
            .args(&args)
            .env("CONTACT_EMAIL", "someone@example.com")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| {
            r.headers
                .get("user-agent")
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned()
        })
        .collect()
}

#[tokio::test]
async fn contact_email_never_reaches_the_cli_user_agent() {
    let dest = tempdir("cli");
    let syz = cli_user_agents(vec![
        "syzygy".into(),
        "--dest".into(),
        dest.display().to_string(),
    ])
    .await;
    let scan = cli_user_agents(vec!["scan".into(), "--month".into(), "2026-10".into()]).await;
    assert!(!syz.is_empty() && !scan.is_empty());
    for ua in syz.iter().chain(&scan) {
        assert_eq!(
            ua,
            "rematch/0.1 (+https://github.com/Terkea/rematch)"
        );
        assert!(!ua.contains('@'));
    }
}

#[tokio::test]
async fn an_oversized_file_is_aborted_and_deleted() {
    let server = MockServer::start().await;
    serve(&server, "/3-4-5-wdl/KPvK.rtbw", &[7u8; 4096]).await;
    let dest = tempdir("big");
    let client = http_client().unwrap();
    let m = Manifest::parse(&format!("{}  3-4-5-wdl/KPvK.rtbw\n", sha(A)));
    let limits = FetchLimits {
        max_file_bytes: 1024,
        ..FetchLimits::default()
    };
    let err = fetch_syzygy_with(&client, &server.uri(), &m, &dest, limits)
        .await
        .unwrap_err();
    assert!(
        matches!(err, DumpError::Decode(ref m) if m.contains("exceeds")),
        "{err:?}"
    );
    assert!(names(&dest).is_empty(), "left {:?}", names(&dest));
}

#[tokio::test]
async fn a_slow_file_times_out_and_leaves_nothing() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/3-4-5-wdl/KPvK.rtbw"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(A.to_vec())
                .set_delay(std::time::Duration::from_secs(5)),
        )
        .mount(&server)
        .await;
    let dest = tempdir("slow");
    let client = http_client().unwrap();
    let m = Manifest::parse(&format!("{}  3-4-5-wdl/KPvK.rtbw\n", sha(A)));
    let limits = FetchLimits {
        file_timeout: std::time::Duration::from_millis(200),
        ..FetchLimits::default()
    };
    let err = fetch_syzygy_with(&client, &server.uri(), &m, &dest, limits)
        .await
        .unwrap_err();
    assert!(
        matches!(err, DumpError::Transport(ref m) if m.contains("timed out")),
        "{err:?}"
    );
    assert!(names(&dest).is_empty());
}

#[tokio::test]
async fn stale_part_files_are_removed_at_start() {
    let server = MockServer::start().await;
    serve(&server, "/3-4-5-wdl/KPvK.rtbw", A).await;
    let dest = tempdir("stalepart");
    std::fs::write(dest.join("OLD.rtbw.part"), b"half").unwrap();
    let client = http_client().unwrap();
    let m = Manifest::parse(&format!("{}  3-4-5-wdl/KPvK.rtbw\n", sha(A)));
    fetch_syzygy(&client, &server.uri(), &m, &dest)
        .await
        .unwrap();
    assert_eq!(names(&dest), ["KPvK.rtbw"]);
}

#[tokio::test]
async fn rerun_reports_rehash_time() {
    let server = MockServer::start().await;
    serve(&server, "/3-4-5-wdl/KPvK.rtbw", A).await;
    let dest = tempdir("rehash");
    let client = http_client().unwrap();
    let m = Manifest::parse(&format!("{}  3-4-5-wdl/KPvK.rtbw\n", sha(A)));
    fetch_syzygy(&client, &server.uri(), &m, &dest)
        .await
        .unwrap();
    let s = fetch_syzygy(&client, &server.uri(), &m, &dest)
        .await
        .unwrap();
    assert_eq!(s.skipped, 1);
    assert!(s.rehash > std::time::Duration::ZERO);
}
