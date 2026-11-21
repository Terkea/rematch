//! R2 Task 24: difficulty providers (spread fallback, Maia HTTP client, selection).
//! Port of the TS M2 Task 21 tests (`apps/worker/test/difficulty/*.test.ts` in the plan).

use std::time::Duration;

use enrich::difficulty::maia::{DEFAULT_BATCH_SIZE, DEFAULT_TIMEOUT, MaiaDifficulty};
use enrich::difficulty::select::select_difficulty_provider;
use enrich::difficulty::spread::SpreadDifficulty;
use enrich::types::{DifficultyError, DifficultyInput, DifficultyProvider};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

fn input(lines: &[(&str, f64)], best: Option<&str>) -> DifficultyInput {
    DifficultyInput {
        fen: START.to_owned(),
        elo_self: Some(1500),
        elo_oppo: Some(1550),
        lines: Some(lines.iter().map(|(u, w)| ((*u).to_owned(), *w)).collect()),
        best_uci: best.map(str::to_owned),
    }
}

fn wins(ws: &[f64]) -> DifficultyInput {
    let ucis = ["e2e4", "d2d4", "g1f3", "c2c4"];
    let lines: Vec<(&str, f64)> = ws.iter().enumerate().map(|(i, w)| (ucis[i], *w)).collect();
    input(&lines, Some("e2e4"))
}

fn close(actual: (f64, f64), expected: (f64, f64)) {
    assert!(
        (actual.0 - expected.0).abs() < 1e-12 && (actual.1 - expected.1).abs() < 1e-12,
        "expected {expected:?}, got {actual:?}"
    );
}

// ---- spread ----------------------------------------------------------------------------------

#[tokio::test]
async fn spread_model_id() {
    assert_eq!(SpreadDifficulty.model(), "spread-v1");
}

#[tokio::test]
async fn spread_two_playable_of_three() {
    let out = SpreadDifficulty
        .estimate(&[wins(&[60.0, 58.0, 40.0])])
        .await
        .unwrap();
    close(out[0].unwrap(), (0.5, 2.0 / 3.0));
}

#[tokio::test]
async fn spread_one_playable_of_three() {
    let out = SpreadDifficulty
        .estimate(&[wins(&[60.0, 30.0, 20.0])])
        .await
        .unwrap();
    close(out[0].unwrap(), (1.0, 1.0 / 3.0));
}

#[tokio::test]
async fn spread_needs_two_lines() {
    let mut no_lines = wins(&[]);
    no_lines.lines = None;
    let out = SpreadDifficulty
        .estimate(&[wins(&[60.0]), no_lines])
        .await
        .unwrap();
    assert_eq!(out, vec![None, None]);
}

#[tokio::test]
async fn spread_threshold_is_inclusive() {
    // 55 = 60 - 5 is still playable.
    let out = SpreadDifficulty
        .estimate(&[wins(&[60.0, 55.0, 54.9])])
        .await
        .unwrap();
    close(out[0].unwrap(), (0.5, 2.0 / 3.0));
}

// ---- maia ------------------------------------------------------------------------------------

async fn maia_server(model: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/healthz"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "model": model })))
        .mount(&server)
        .await;
    server
}

/// Answers every predict item with the same probabilities, or `invalid_fen` for "bad".
struct FixedProbs;

impl Respond for FixedProbs {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&req.body).unwrap();
        let items: Vec<Value> = body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|it| {
                if it["fen"] == "bad" {
                    json!({ "error": "invalid_fen" })
                } else {
                    json!({ "probs": { "e2e4": 0.5, "d2d4": 0.3, "g1f3": 0.2 } })
                }
            })
            .collect();
        ResponseTemplate::new(200).set_body_json(json!({ "items": items }))
    }
}

async fn connect(server: &MockServer) -> Result<MaiaDifficulty, DifficultyError> {
    MaiaDifficulty::connect(&server.uri(), DEFAULT_TIMEOUT, DEFAULT_BATCH_SIZE).await
}

#[tokio::test]
async fn maia_defaults_match_the_brief() {
    assert_eq!(DEFAULT_TIMEOUT, Duration::from_secs(10));
    assert_eq!(DEFAULT_BATCH_SIZE, 64);
}

#[tokio::test]
async fn maia_reads_model_id_from_healthz() {
    let server = maia_server("maia2-rapid-0.11.0").await;
    let maia = connect(&server).await.unwrap();
    assert_eq!(maia.model(), "maia2-rapid-0.11.0");
}

#[tokio::test]
async fn maia_p_best_and_p_ok() {
    let server = maia_server("maia2-rapid-0.11.0").await;
    Mock::given(method("POST"))
        .and(path("/v1/predict"))
        .respond_with(FixedProbs)
        .expect(1)
        .mount(&server)
        .await;
    let maia = connect(&server).await.unwrap();
    let inp = input(
        &[("e2e4", 55.0), ("d2d4", 52.0), ("g1f3", 40.0)],
        Some("e2e4"),
    );
    let out = maia.estimate(&[inp]).await.unwrap();
    close(out[0].unwrap(), (0.5, 0.8));
}

#[tokio::test]
async fn maia_sends_fen_and_elos() {
    let server = maia_server("m").await;
    Mock::given(method("POST"))
        .and(path("/v1/predict"))
        .respond_with(FixedProbs)
        .mount(&server)
        .await;
    let maia = connect(&server).await.unwrap();
    let mut inp = input(&[("e2e4", 55.0), ("d2d4", 52.0)], Some("e2e4"));
    inp.elo_oppo = None;
    maia.estimate(&[inp]).await.unwrap();
    let reqs = server.received_requests().await.unwrap();
    let post = reqs.iter().find(|r| r.url.path() == "/v1/predict").unwrap();
    let body: Value = serde_json::from_slice(&post.body).unwrap();
    assert_eq!(
        body,
        json!({ "items": [{ "fen": START, "eloSelf": 1500, "eloOppo": null }] })
    );
}

#[tokio::test]
async fn maia_best_move_missing_from_probs_gives_zero() {
    let server = maia_server("m").await;
    Mock::given(method("POST"))
        .and(path("/v1/predict"))
        .respond_with(FixedProbs)
        .mount(&server)
        .await;
    let maia = connect(&server).await.unwrap();
    let inp = input(&[("b1c3", 55.0), ("e2e4", 54.0)], Some("b1c3"));
    let out = maia.estimate(&[inp]).await.unwrap();
    close(out[0].unwrap(), (0.0, 0.5));
}

#[tokio::test]
async fn maia_invalid_fen_item_is_none() {
    let server = maia_server("m").await;
    Mock::given(method("POST"))
        .and(path("/v1/predict"))
        .respond_with(FixedProbs)
        .mount(&server)
        .await;
    let maia = connect(&server).await.unwrap();
    let mut bad = input(&[("e2e4", 55.0), ("d2d4", 52.0)], Some("e2e4"));
    bad.fen = "bad".to_owned();
    let good = input(&[("e2e4", 55.0), ("d2d4", 52.0)], Some("e2e4"));
    let out = maia.estimate(&[bad, good]).await.unwrap();
    assert_eq!(out[0], None);
    close(out[1].unwrap(), (0.5, 0.8));
}

#[tokio::test]
async fn maia_batches_130_inputs_into_3_requests() {
    let server = maia_server("m").await;
    Mock::given(method("POST"))
        .and(path("/v1/predict"))
        .respond_with(FixedProbs)
        .expect(3)
        .mount(&server)
        .await;
    let maia = connect(&server).await.unwrap();
    let inputs: Vec<DifficultyInput> = (0..130)
        .map(|_| input(&[("e2e4", 55.0), ("d2d4", 52.0)], Some("e2e4")))
        .collect();
    let out = maia.estimate(&inputs).await.unwrap();
    assert_eq!(out.len(), 130);
    assert!(out.iter().all(Option::is_some));
    let sizes: Vec<usize> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/v1/predict")
        .map(|r| {
            let b: Value = serde_json::from_slice(&r.body).unwrap();
            b["items"].as_array().unwrap().len()
        })
        .collect();
    assert_eq!(sizes, vec![64, 64, 2]);
}

#[tokio::test]
async fn maia_503_is_unavailable() {
    let server = maia_server("m").await;
    Mock::given(method("POST"))
        .and(path("/v1/predict"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let maia = connect(&server).await.unwrap();
    let err = maia
        .estimate(&[input(&[("e2e4", 55.0), ("d2d4", 52.0)], Some("e2e4"))])
        .await
        .unwrap_err();
    assert!(matches!(err, DifficultyError::Unavailable(_)), "{err:?}");
}

#[tokio::test]
async fn maia_item_count_mismatch_is_unavailable() {
    let server = maia_server("m").await;
    Mock::given(method("POST"))
        .and(path("/v1/predict"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
        .mount(&server)
        .await;
    let maia = connect(&server).await.unwrap();
    let err = maia
        .estimate(&[input(&[("e2e4", 55.0), ("d2d4", 52.0)], Some("e2e4"))])
        .await
        .unwrap_err();
    assert!(matches!(err, DifficultyError::Unavailable(_)), "{err:?}");
}

#[tokio::test]
async fn maia_empty_input_sends_nothing() {
    let server = maia_server("m").await;
    Mock::given(method("POST"))
        .and(path("/v1/predict"))
        .respond_with(FixedProbs)
        .expect(0)
        .mount(&server)
        .await;
    let maia = connect(&server).await.unwrap();
    assert!(maia.estimate(&[]).await.unwrap().is_empty());
}

#[tokio::test]
async fn maia_connect_fails_when_healthz_is_down() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/healthz"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let err = connect(&server).await.unwrap_err();
    assert!(matches!(err, DifficultyError::Unavailable(_)), "{err:?}");
}

// ---- selection -------------------------------------------------------------------------------

#[tokio::test]
async fn select_without_url_is_spread() {
    assert_eq!(select_difficulty_provider(None).await.model(), "spread-v1");
    assert_eq!(
        select_difficulty_provider(Some("")).await.model(),
        "spread-v1"
    );
}

#[tokio::test]
async fn select_unreachable_url_is_spread() {
    let p = select_difficulty_provider(Some("http://127.0.0.1:1")).await;
    assert_eq!(p.model(), "spread-v1");
}

#[tokio::test]
async fn select_healthy_maia() {
    let server = maia_server("maia2-blitz-0.11.0").await;
    let p = select_difficulty_provider(Some(&server.uri())).await;
    assert_eq!(p.model(), "maia2-blitz-0.11.0");
}

// ---- live contract (only with MAIA_URL) ------------------------------------------------------

/// Runs against a real `services/maia` when `MAIA_URL` is set (e.g. `http://maia:8080`). With
/// `IN_DOCKER=1` and `MAIA_URL` set, an unreachable service fails the test instead of skipping it.
#[tokio::test]
async fn maia_live_contract() {
    let url = match std::env::var("MAIA_URL") {
        Ok(u) if !u.is_empty() => u,
        _ => return,
    };
    let maia = match MaiaDifficulty::connect(&url, DEFAULT_TIMEOUT, DEFAULT_BATCH_SIZE).await {
        Ok(m) => m,
        Err(e) if std::env::var("IN_DOCKER").as_deref() == Ok("1") => {
            panic!("MAIA_URL={url} is set but unreachable: {e}")
        }
        Err(_) => return,
    };
    assert!(maia.model().starts_with("maia2-"), "{}", maia.model());
    let lines = [
        ("e2e4", 54.0),
        ("d2d4", 54.0),
        ("g1f3", 53.0),
        ("c2c4", 53.0),
    ];
    let mut bad = input(&lines, Some("e2e4"));
    bad.fen = "not a fen".to_owned();
    let out = maia
        .estimate(&[input(&lines, Some("e2e4")), bad])
        .await
        .unwrap();
    let (p_best, p_ok) = out[0].unwrap();
    assert!(p_best > 0.05 && p_best <= 1.0, "p_best {p_best}");
    // The four main openings carry most of Maia's mass at the start position.
    assert!(p_ok > 0.5 && p_ok <= 1.0 + 1e-6, "p_ok {p_ok}");
    assert_eq!(out[1], None);
}
