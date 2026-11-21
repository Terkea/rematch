//! Port of apps/worker/test/http/platform-client.test.ts, against local test servers.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use futures::StreamExt;
use platforms::client::{ClientOptions, HttpPlatformClient, user_agent};
use platforms::{Platform, PlatformClient, PlatformError};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

fn opts() -> ClientOptions {
    ClientOptions {
        name: Platform::Lichess,
        contact_email: "ops@example.com".into(),
        retry_after: Duration::ZERO,
        ..ClientOptions::default()
    }
}

fn client(o: ClientOptions) -> HttpPlatformClient {
    HttpPlatformClient::new(o)
}

async fn collect(
    c: &HttpPlatformClient,
    url: &str,
) -> Vec<Result<serde_json::Value, PlatformError>> {
    c.get_ndjson(url, vec![]).collect().await
}

#[tokio::test]
async fn sends_identifying_user_agent() {
    assert_eq!(
        user_agent("ops@example.com"),
        "chess-improver/0.1 (+mailto:ops@example.com)"
    );
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/a"))
        .and(header(
            "user-agent",
            "chess-improver/0.1 (+mailto:ops@example.com)",
        ))
        .and(header("accept", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(opts());
    let v = c
        .get_json(
            &format!("{}/a", server.uri()),
            vec![("Accept".into(), "application/json".into())],
        )
        .await
        .unwrap();
    assert_eq!(v, json!({"ok": true}));
}

struct Tracking {
    in_flight: Arc<AtomicUsize>,
    max: Arc<AtomicUsize>,
}

impl Respond for Tracking {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.max.fetch_max(now, Ordering::SeqCst);
        // Responders are sync: release the in-flight slot after the same 10 ms the response is
        // delayed by.
        let in_flight = self.in_flight.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            in_flight.fetch_sub(1, Ordering::SeqCst);
        });
        ResponseTemplate::new(200)
            .set_body_json(json!({}))
            .set_delay(Duration::from_millis(10))
    }
}

#[tokio::test]
async fn serialises_requests() {
    let server = MockServer::start().await;
    let max = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET"))
        .respond_with(Tracking {
            in_flight: Arc::new(AtomicUsize::new(0)),
            max: max.clone(),
        })
        .mount(&server)
        .await;
    let c = client(opts());
    let (u1, u2) = (format!("{}/1", server.uri()), format!("{}/2", server.uri()));
    let (a, b) = tokio::join!(c.get_json(&u1, vec![]), c.get_json(&u2, vec![]));
    a.unwrap();
    b.unwrap();
    assert_eq!(max.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn retries_after_429() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429).set_body_string("slow down"))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"n": 1})))
        .with_priority(2)
        .mount(&server)
        .await;
    let c = client(opts());
    let v = c
        .get_json(&format!("{}/a", server.uri()), vec![])
        .await
        .unwrap();
    assert_eq!(v, json!({"n": 1}));
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn waits_retry_after_between_attempts() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .with_priority(2)
        .mount(&server)
        .await;
    let c = client(ClientOptions {
        retry_after: Duration::from_millis(50),
        ..opts()
    });
    let start = Instant::now();
    c.get_json(&format!("{}/a", server.uri()), vec![])
        .await
        .unwrap();
    assert!(start.elapsed() >= Duration::from_millis(45));
}

#[tokio::test]
async fn throws_rate_limit_error_after_four_429s() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;
    let c = client(opts());
    let err = c
        .get_json(&format!("{}/a", server.uri()), vec![])
        .await
        .unwrap_err();
    assert!(
        matches!(err, PlatformError::RateLimited { attempts: 4, .. }),
        "{err:?}"
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 4);
}

#[tokio::test]
async fn not_found_404_throws_not_found_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let c = client(opts());
    let err = c
        .get_json(&format!("{}/a", server.uri()), vec![])
        .await
        .unwrap_err();
    assert!(matches!(err, PlatformError::NotFound { .. }), "{err:?}");
}

#[tokio::test]
async fn non_404_error_status_is_http_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let c = client(opts());
    let err = c
        .get_json(&format!("{}/a", server.uri()), vec![])
        .await
        .unwrap_err();
    assert!(
        matches!(err, PlatformError::Http { status: 503, .. }),
        "{err:?}"
    );
}

/// Raw server that writes the given chunks (HTTP/1.1 chunked), then ends or hangs.
async fn raw_chunked_server(chunks: Vec<&'static str>, hang: bool) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let chunks = chunks.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await;
                let _ = sock
                    .write_all(b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n")
                    .await;
                for c in chunks {
                    let _ = sock
                        .write_all(format!("{:x}\r\n{}\r\n", c.len(), c).as_bytes())
                        .await;
                    let _ = sock.flush().await;
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                if hang {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                } else {
                    let _ = sock.write_all(b"0\r\n\r\n").await;
                }
            });
        }
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn get_ndjson_yields_three_objects_from_a_body_split_mid_line() {
    let base = raw_chunked_server(vec!["{\"a\":1}\n{\"a\"", ":2}\n{\"a\":3", "}\n"], false).await;
    let c = client(opts());
    let out: Vec<_> = collect(&c, &format!("{base}/s"))
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect();
    assert_eq!(out, vec![json!({"a":1}), json!({"a":2}), json!({"a":3})]);
}

#[tokio::test]
async fn times_out_a_hung_fetch_and_the_next_queued_call_still_proceeds() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/hang"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/next"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": 2})))
        .mount(&server)
        .await;
    let c = client(ClientOptions {
        request_timeout: Duration::from_millis(30),
        ..opts()
    });
    let (u1, u2) = (
        format!("{}/hang", server.uri()),
        format!("{}/next", server.uri()),
    );
    let first = c.get_json(&u1, vec![]);
    let second = c.get_json(&u2, vec![]);
    let (a, b) = tokio::join!(first, second);
    assert!(matches!(a.unwrap_err(), PlatformError::Timeout { .. }));
    assert_eq!(b.unwrap(), json!({"ok": 2}));
}

#[tokio::test]
async fn times_out_a_stream_that_goes_idle_between_chunks() {
    let base = raw_chunked_server(vec!["{\"a\":1}\n"], true).await;
    let c = client(ClientOptions {
        stream_idle_timeout: Duration::from_millis(30),
        ..opts()
    });
    let out = collect(&c, &format!("{base}/s")).await;
    assert_eq!(out.len(), 2, "{out:?}");
    assert_eq!(out[0].as_ref().unwrap(), &json!({"a": 1}));
    assert!(
        matches!(out[1], Err(PlatformError::Timeout { .. })),
        "{:?}",
        out[1]
    );

    // The lock was released: the same client serves another request.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": 1})))
        .mount(&server)
        .await;
    let v = tokio::time::timeout(Duration::from_secs(2), c.get_json(&server.uri(), vec![]))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(v, json!({"ok": 1}));
}

#[tokio::test]
async fn releases_the_lock_on_early_drop_of_get_ndjson() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/s"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/next"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": 1})))
        .mount(&server)
        .await;
    let c = client(opts());
    {
        let url = format!("{}/s", server.uri());
        let mut s = c.get_ndjson(&url, vec![]);
        let first = s.next().await.unwrap().unwrap();
        assert_eq!(first, json!({"a": 1}));
    }
    let v = tokio::time::timeout(
        Duration::from_secs(2),
        c.get_json(&format!("{}/next", server.uri()), vec![]),
    )
    .await
    .expect("lock released")
    .unwrap();
    assert_eq!(v, json!({"ok": 1}));
}

#[tokio::test]
async fn yields_a_final_line_without_a_trailing_newline() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"a\":1}\n{\"a\":2}"))
        .mount(&server)
        .await;
    let c = client(opts());
    let out: Vec<_> = collect(&c, &server.uri())
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect();
    assert_eq!(out, vec![json!({"a":1}), json!({"a":2})]);
}
