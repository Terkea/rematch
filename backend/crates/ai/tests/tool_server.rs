//! The chat tool-callback endpoint (Task 11): a real axum server on `127.0.0.1:0` and a reqwest
//! caller playing the sidecar's MCP tool (`POST {url}` with `Bearer {token}`, `{name, input}`).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use ai::client::port::{ToolHandler, ToolReply};
use ai::client::tool_server::ToolServer;
use futures::future::BoxFuture;
use serde_json::{Value, json};

/// Records every call and answers with its own label.
struct Recorder {
    label: &'static str,
    calls: Mutex<Vec<(String, Value)>>,
    delay: Duration,
}

impl Recorder {
    fn new(label: &'static str) -> Arc<Self> {
        Arc::new(Recorder {
            label,
            calls: Mutex::new(Vec::new()),
            delay: Duration::ZERO,
        })
    }
}

impl ToolHandler for Recorder {
    fn call<'a>(&'a self, name: &'a str, input: Value) -> BoxFuture<'a, ToolReply> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push((name.to_owned(), input.clone()));
            if !self.delay.is_zero() {
                tokio::time::sleep(self.delay).await;
            }
            ToolReply {
                content: json!({"from": self.label, "name": name}).to_string(),
                is_error: name == "fails",
            }
        })
    }
}

/// A turn timeout long enough for every test except the TTL one.
const TTL: Duration = Duration::from_secs(180);

async fn server() -> Arc<ToolServer> {
    ToolServer::start(
        "127.0.0.1:0".parse().unwrap(),
        "http://rust-worker:7311".into(),
    )
    .await
    .unwrap()
}

fn endpoint(s: &ToolServer) -> String {
    format!("http://{}/v1/tools", s.local_addr())
}

async fn post(s: &ToolServer, token: Option<&str>, body: Value) -> (u16, Value) {
    let mut req = reqwest::Client::new().post(endpoint(s)).json(&body);
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let res = req.send().await.unwrap();
    let status = res.status().as_u16();
    let body = res.json::<Value>().await.unwrap_or(Value::Null);
    (status, body)
}

fn install_crypto() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[tokio::test]
async fn routes_by_turn_token_to_the_registered_handler() {
    install_crypto();
    let s = server().await;
    let h = Recorder::new("a");
    let reg = s.register(h.clone(), TTL);
    assert_eq!(reg.url, "http://rust-worker:7311/v1/tools");
    assert_eq!(reg.token.len(), 64);
    assert!(reg.token.bytes().all(|b| b.is_ascii_hexdigit()));
    assert!(
        !format!("{reg:?}").contains(&reg.token),
        "Debug redacts the token"
    );

    let (status, body) = post(
        &s,
        Some(&reg.token),
        json!({"name": "get_position", "input": {"gameId": "g1"}}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        body,
        json!({"content": "{\"from\":\"a\",\"name\":\"get_position\"}", "isError": false})
    );
    let (_, body) = post(&s, Some(&reg.token), json!({"name": "fails", "input": {}})).await;
    assert_eq!(body["isError"], true);
    assert_eq!(
        *h.calls.lock().unwrap(),
        vec![
            ("get_position".to_owned(), json!({"gameId": "g1"})),
            ("fails".to_owned(), json!({})),
        ]
    );
    // Two registrations get two different tokens.
    let other = s.register(Recorder::new("b"), TTL);
    assert_ne!(other.token, reg.token);
    assert_eq!(s.registrations(), 2);
}

#[tokio::test]
async fn unknown_token_is_404_and_missing_is_401() {
    install_crypto();
    let s = server().await;
    let _reg = s.register(Recorder::new("a"), TTL);
    let body = json!({"name": "x", "input": {}});
    assert_eq!(post(&s, Some(&"0".repeat(64)), body.clone()).await.0, 404);
    assert_eq!(post(&s, None, body.clone()).await.0, 401);
    let res = reqwest::Client::new()
        .post(endpoint(&s))
        .header("authorization", "Basic abc")
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 401);
    let res = reqwest::Client::new()
        .post(format!("http://{}/v1/other", s.local_addr()))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 404);
}

#[tokio::test]
async fn dropped_registration_is_404() {
    install_crypto();
    let s = server().await;
    let reg = s.register(Recorder::new("a"), TTL);
    let token = reg.token.clone();
    let body = json!({"name": "x", "input": {}});
    assert_eq!(post(&s, Some(&token), body.clone()).await.0, 200);
    drop(reg);
    assert_eq!(s.registrations(), 0);
    assert_eq!(post(&s, Some(&token), body).await.0, 404);
}

#[tokio::test]
async fn slow_handler_times_out_as_engine_busy() {
    install_crypto();
    let s = ToolServer::start_with_timeout(
        "127.0.0.1:0".parse().unwrap(),
        "http://rust-worker:7311".into(),
        Duration::from_millis(50),
    )
    .await
    .unwrap();
    let h = Arc::new(Recorder {
        label: "slow",
        calls: Mutex::new(Vec::new()),
        delay: Duration::from_secs(5),
    });
    let reg = s.register(h, TTL);
    let started = std::time::Instant::now();
    let (status, body) = post(
        &s,
        Some(&reg.token),
        json!({"name": "get_position", "input": {}}),
    )
    .await;
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(status, 200);
    assert_eq!(body["isError"], true);
    let content: Value = serde_json::from_str(body["content"].as_str().unwrap()).unwrap();
    assert_eq!(
        content,
        json!({"available": false, "reason": "engine busy"})
    );
}

#[tokio::test]
async fn oversized_body_is_413() {
    install_crypto();
    let s = server().await;
    let h = Recorder::new("a");
    let reg = s.register(h.clone(), TTL);
    let big = "x".repeat(65 * 1024);
    let (status, _) = post(
        &s,
        Some(&reg.token),
        json!({"name": "x", "input": {"pad": big}}),
    )
    .await;
    assert_eq!(status, 413);
    assert!(h.calls.lock().unwrap().is_empty());
    // Just under the limit is accepted.
    let ok = "x".repeat(60 * 1024);
    let (status, _) = post(
        &s,
        Some(&reg.token),
        json!({"name": "x", "input": {"pad": ok}}),
    )
    .await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn two_turns_never_cross() {
    install_crypto();
    let s = server().await;
    let a = Recorder::new("a");
    let b = Recorder::new("b");
    let ra = s.register(a.clone(), TTL);
    let rb = s.register(b.clone(), TTL);
    let mut calls = Vec::new();
    for i in 0..10 {
        let (reg, name) = if i % 2 == 0 {
            (&ra, "for_a")
        } else {
            (&rb, "for_b")
        };
        calls.push(post(
            &s,
            Some(&reg.token),
            json!({"name": name, "input": {"i": i}}),
        ));
    }
    let replies = futures::future::join_all(calls).await;
    for (i, (status, body)) in replies.into_iter().enumerate() {
        assert_eq!(status, 200);
        let content: Value = serde_json::from_str(body["content"].as_str().unwrap()).unwrap();
        assert_eq!(content["from"], if i % 2 == 0 { "a" } else { "b" });
    }
    assert!(a.calls.lock().unwrap().iter().all(|(n, _)| n == "for_a"));
    assert!(b.calls.lock().unwrap().iter().all(|(n, _)| n == "for_b"));
    assert_eq!(a.calls.lock().unwrap().len(), 5);
    assert_eq!(b.calls.lock().unwrap().len(), 5);
}

#[tokio::test]
async fn registration_expires_after_its_ttl() {
    install_crypto();
    let s = server().await;
    let h = Recorder::new("a");
    // Never dropped: the TTL (the turn timeout) still ends it.
    let reg = s.register(h.clone(), Duration::from_millis(100));
    let body = json!({"name": "x", "input": {}});
    assert_eq!(post(&s, Some(&reg.token), body.clone()).await.0, 200);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(post(&s, Some(&reg.token), body).await.0, 404);
    assert_eq!(s.registrations(), 0);
    assert_eq!(h.calls.lock().unwrap().len(), 1);
    drop(reg); // dropping an expired registration is harmless
    let live = s.register(Recorder::new("b"), TTL);
    assert_eq!(s.registrations(), 1);
    drop(live);
}
