//! R2 Task 24: Maia-only move difficulty (ADR-R2-13).
//!
//! Owner direction (fix round 1): difficulty comes only from Maia. When Maia cannot answer, the
//! outcome is `pending_maia` with a reason; opening plies are `not_applicable`/`opening_ply`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chess_core::board::Position;
use enrich::difficulty::maia::{
    DEFAULT_BATCH_SIZE, DEFAULT_CONNECT_TIMEOUT, DEFAULT_TIMEOUT, MaiaCallError, MaiaDifficulty,
};
use enrich::difficulty::select::{DifficultyEstimator, select_difficulty_provider};
use enrich::difficulty::{
    DifficultyOutcome, MAIA_FIRST_PLY, NotApplicableReason, PendingReason, PlyIndexError,
    half_move_index, is_maia_opening_ply,
};
use enrich::types::{DifficultyError, DifficultyInput, DifficultyProvider};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// A position past Maia's opening cut (fullmove 12). The mocks ignore it; only the move keys matter.
const MID: &str = "r1bq1rk1/pppp1ppp/2n2n2/2b1p3/2B1P3/2NP1N2/PPP2PPP/R1BQ1RK1 w - - 0 12";
const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const MODEL: &str = "maia2-rapid-0.11.0";

fn input(lines: &[(&str, f64)], best: Option<&str>) -> DifficultyInput {
    DifficultyInput {
        fen: MID.to_owned(),
        elo_self: Some(1500),
        elo_oppo: Some(1550),
        lines: Some(lines.iter().map(|(u, w)| ((*u).to_owned(), *w)).collect()),
        best_uci: best.map(str::to_owned),
    }
}

fn std_lines() -> DifficultyInput {
    input(
        &[("e2e4", 55.0), ("d2d4", 52.0), ("g1f3", 40.0)],
        Some("e2e4"),
    )
}

fn opening() -> DifficultyInput {
    let mut i = std_lines();
    i.fen = START.to_owned();
    i
}

fn close(actual: (f64, f64), expected: (f64, f64)) {
    assert!(
        (actual.0 - expected.0).abs() < 1e-12 && (actual.1 - expected.1).abs() < 1e-12,
        "expected {expected:?}, got {actual:?}"
    );
}

fn ok(o: &DifficultyOutcome) -> (f64, f64) {
    let d = o
        .difficulty()
        .unwrap_or_else(|| panic!("expected a difficulty, got {o:?}"));
    assert_eq!(
        d.model, MODEL,
        "every value present carries the Maia model id"
    );
    (d.p_best, d.p_ok)
}

fn pending(reason: PendingReason) -> DifficultyOutcome {
    DifficultyOutcome::PendingMaia { reason }
}

fn not_applicable(reason: NotApplicableReason) -> DifficultyOutcome {
    DifficultyOutcome::NotApplicable { reason }
}

// ---- opening plies ---------------------------------------------------------------------------

#[test]
fn opening_cut_matches_maia_training() {
    // maia2-training.yaml `first_n_moves: 10`: half-move indices 0..=9 are never trained on.
    assert_eq!(MAIA_FIRST_PLY, 10);
    assert_eq!(half_move_index(START), Ok(0));
    assert!(is_maia_opening_ply(START));
    // Index 9: black's 5th move.
    assert!(is_maia_opening_ply("8/8/8/8/8/8/8/K6k b - - 0 5"));
    // Index 10: white's 6th move is the first one Maia was trained on.
    assert_eq!(half_move_index("8/8/8/8/8/8/8/K6k w - - 0 6"), Ok(10));
    assert!(!is_maia_opening_ply("8/8/8/8/8/8/8/K6k w - - 0 6"));
    assert!(!is_maia_opening_ply(MID));
    // Unparsable FENs are not opening plies: the service rejects them.
    assert!(!is_maia_opening_ply("garbage"));
    assert!(!is_maia_opening_ply("8/8/8/8/8/8/8/K6k w - - 0 0"));
    assert_eq!(half_move_index("garbage"), Err(PlyIndexError::Unparsable));
}

#[test]
fn half_move_index_overflow_is_detected() {
    let max = u32::MAX;
    // 2 * (u32::MAX - 1) overflows.
    let big = format!("8/8/8/8/8/8/8/K6k w - - 0 {max}");
    assert_eq!(half_move_index(&big), Err(PlyIndexError::Overflow));
    // The largest representable index: fullmove 2^31, black to move = 2^32 - 1.
    let edge = "8/8/8/8/8/8/8/K6k b - - 0 2147483648";
    assert_eq!(half_move_index(edge), Ok(u32::MAX));
    let past = "8/8/8/8/8/8/8/K6k w - - 0 2147483649";
    assert_eq!(half_move_index(past), Err(PlyIndexError::Overflow));
    // A move number with too many digits for u32.
    let huge = "8/8/8/8/8/8/8/K6k w - - 0 99999999999999999999";
    assert_eq!(half_move_index(huge), Err(PlyIndexError::Overflow));
    assert!(!is_maia_opening_ply(huge));
}

#[tokio::test]
async fn overflowing_move_number_is_invalid_fen_and_never_sent() {
    let server = maia_server(MODEL).await;
    Mock::given(method("POST"))
        .and(path("/v1/predict"))
        .respond_with(FixedProbs::standard())
        .expect(0)
        .mount(&server)
        .await;
    let mut i = std_lines();
    i.fen = "8/8/8/8/8/8/8/K6k w - - 0 99999999999999999999".to_owned();
    let out = estimator(&server).estimate(&[i]).await;
    assert_eq!(out, vec![not_applicable(NotApplicableReason::InvalidFen)]);
}

// ---- the only provider is Maia ---------------------------------------------------------------

/// The type names after every `impl ... DifficultyProvider for <Type>` in `text`, including
/// generic (`impl<T: X> DifficultyProvider for W<T>`), path-qualified
/// (`impl crate::types::DifficultyProvider for X`) and multi-line forms.
fn provider_impls(text: &str) -> Vec<String> {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = Vec::new();
    let needle = "DifficultyProvider for ";
    let mut from = 0;
    while let Some(pos) = flat[from..].find(needle) {
        let at = from + pos;
        // The item header starts after the previous `;`, `}` or `{`.
        let head_start = flat[..at].rfind([';', '}', '{']).map_or(0, |i| i + 1);
        let head = &flat[head_start..at];
        let is_impl = head
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|w| w == "impl");
        let rest = &flat[at + needle.len()..];
        let end = [rest.find(" {"), rest.find('{'), rest.find(" where")]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(rest.len());
        if is_impl {
            out.push(rest[..end].trim().to_owned());
        }
        from = at + needle.len();
    }
    out
}

#[test]
fn provider_impl_scanner_matches_every_form() {
    let text = "impl DifficultyProvider for A {}
        impl crate::types::DifficultyProvider for B { fn x() {} }
        impl<T: Send + Sync> enrich::types::DifficultyProvider for C<T> where T: Clone {}
        impl<'a>
    DifficultyProvider
    for D<'a> {}
        // a comment about DifficultyProvider for nothing;
        fn f(p: &dyn DifficultyProvider) {}";
    assert_eq!(provider_impls(text), vec!["A", "B", "C<T>", "D<'a>"]);
}

/// Owner direction: no simple engine-based estimate may produce a difficulty. This scans the
/// `src/` of every crate under `backend/crates`: `MaiaDifficulty` is the only
/// `DifficultyProvider`, and the enrich crate has no spread or other fallback estimator.
#[test]
fn the_only_difficulty_provider_is_maia() {
    let enrich = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let crates = enrich.parent().unwrap();
    let mut impls = Vec::new();
    let mut scanned = 0;
    for krate in std::fs::read_dir(crates).unwrap() {
        let src = krate.unwrap().path().join("src");
        if !src.is_dir() {
            continue;
        }
        let mut stack = vec![src];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let p = entry.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().is_some_and(|e| e == "rs") {
                    scanned += 1;
                    let text = std::fs::read_to_string(&p).unwrap();
                    if p.starts_with(enrich) {
                        assert!(
                            !text.to_lowercase().contains("spread"),
                            "{} mentions a spread estimator",
                            p.display()
                        );
                    }
                    for name in provider_impls(&text) {
                        impls.push(format!("{name} ({})", p.display()));
                    }
                }
            }
        }
    }
    assert!(scanned > 50, "only {scanned} files scanned");
    assert_eq!(impls.len(), 1, "{impls:?}");
    assert!(impls[0].starts_with("MaiaDifficulty ("), "{impls:?}");
}

#[tokio::test]
async fn estimates_carry_only_the_maia_model_id() {
    let server = maia_server(MODEL).await;
    mount_predict(&server, FixedProbs::standard()).await;
    let est = estimator(&server);
    let out = est.estimate(&[std_lines(), opening(), std_lines()]).await;
    for o in &out {
        if let Some(d) = o.difficulty() {
            assert_eq!(d.model, MODEL);
        }
    }
    assert_eq!(out.iter().filter(|o| o.difficulty().is_some()).count(), 2);
}

// ---- maia client -----------------------------------------------------------------------------

async fn maia_server(model: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/healthz"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "model": model })))
        .mount(&server)
        .await;
    server
}

/// Answers each predict item with `probs`, or `invalid_fen` for the FEN "bad".
struct FixedProbs(Value);

impl FixedProbs {
    fn standard() -> Self {
        Self(json!({ "e2e4": 0.5, "d2d4": 0.3, "g1f3": 0.2 }))
    }
}

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
                    json!({ "probs": self.0 })
                }
            })
            .collect();
        ResponseTemplate::new(200).set_body_json(json!({ "items": items }))
    }
}

/// Answers the items of one request with the given per-item JSON values, in order.
struct Items(Vec<Value>);

impl Respond for Items {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(json!({ "items": self.0 }))
    }
}

async fn mount_predict(server: &MockServer, r: impl Respond + 'static) {
    Mock::given(method("POST"))
        .and(path("/v1/predict"))
        .respond_with(r)
        .mount(server)
        .await;
}

async fn connect(server: &MockServer) -> MaiaDifficulty {
    MaiaDifficulty::connect(&server.uri(), DEFAULT_TIMEOUT, DEFAULT_BATCH_SIZE)
        .await
        .unwrap()
}

fn estimator(server: &MockServer) -> DifficultyEstimator {
    DifficultyEstimator::new(Some(&server.uri()), DEFAULT_TIMEOUT, DEFAULT_BATCH_SIZE)
}

async fn predict_bodies(server: &MockServer) -> Vec<Value> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/v1/predict")
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

#[tokio::test]
async fn maia_defaults_match_the_brief() {
    assert_eq!(DEFAULT_TIMEOUT, Duration::from_secs(30));
    assert_eq!(DEFAULT_CONNECT_TIMEOUT, Duration::from_secs(2));
    assert_eq!(DEFAULT_BATCH_SIZE, 32);
}

#[tokio::test]
async fn maia_reads_model_id_from_healthz() {
    let server = maia_server(MODEL).await;
    let maia = connect(&server).await;
    assert_eq!(maia.model(), MODEL);
    assert_eq!(maia.model_id(), MODEL);
}

#[tokio::test]
async fn maia_p_best_and_p_ok() {
    let server = maia_server(MODEL).await;
    mount_predict(&server, FixedProbs::standard()).await;
    let out = estimator(&server).estimate(&[std_lines()]).await;
    close(ok(&out[0]), (0.5, 0.8));
    // The Task 1 trait gives the same numbers.
    let trait_out = connect(&server).await.estimate(&[std_lines()]).await;
    let server_calls = predict_bodies(&server).await.len();
    assert_eq!(server_calls, 2);
    close(trait_out.unwrap()[0].unwrap(), (0.5, 0.8));
}

#[tokio::test]
async fn maia_sends_fen_and_elos() {
    let server = maia_server(MODEL).await;
    mount_predict(&server, FixedProbs::standard()).await;
    let mut inp = input(&[("e2e4", 55.0), ("d2d4", 52.0)], Some("e2e4"));
    inp.elo_oppo = None;
    estimator(&server).estimate(&[inp]).await;
    assert_eq!(
        predict_bodies(&server).await,
        vec![json!({ "items": [{ "fen": MID, "eloSelf": 1500, "eloOppo": null }] })]
    );
}

#[tokio::test]
async fn maia_best_move_missing_from_probs_gives_zero() {
    let server = maia_server(MODEL).await;
    mount_predict(&server, FixedProbs::standard()).await;
    let inp = input(&[("b1c3", 55.0), ("e2e4", 54.0)], Some("b1c3"));
    let out = estimator(&server).estimate(&[inp]).await;
    close(ok(&out[0]), (0.0, 0.5));
}

#[tokio::test]
async fn maia_without_lines_p_ok_is_p_best() {
    let server = maia_server(MODEL).await;
    mount_predict(&server, FixedProbs::standard()).await;
    let mut inp = std_lines();
    inp.lines = None;
    let out = estimator(&server).estimate(&[inp]).await;
    close(ok(&out[0]), (0.5, 0.5));
}

#[tokio::test]
async fn maia_p_ok_is_clamped_to_at_least_p_best() {
    let server = maia_server(MODEL).await;
    mount_predict(&server, FixedProbs::standard()).await;
    // The best move is not among the lines: the raw p_ok (0.3) would be below p_best (0.5).
    let inp = input(&[("d2d4", 55.0), ("g1f3", 40.0)], Some("e2e4"));
    let out = estimator(&server).estimate(&[inp]).await;
    close(ok(&out[0]), (0.5, 0.5));
}

#[tokio::test]
async fn maia_p_ok_is_clamped_to_one() {
    let server = maia_server(MODEL).await;
    // Sums to 1.008: inside the 0.01 tolerance, but the raw p_ok would exceed 1.
    mount_predict(&server, FixedProbs(json!({ "e2e4": 0.6, "d2d4": 0.408 }))).await;
    let inp = input(&[("e2e4", 55.0), ("d2d4", 54.0)], Some("e2e4"));
    let out = estimator(&server).estimate(&[inp]).await;
    close(ok(&out[0]), (0.6, 1.0));
}

#[tokio::test]
async fn maia_dedupes_lines() {
    let server = maia_server(MODEL).await;
    mount_predict(&server, FixedProbs::standard()).await;
    let inp = input(
        &[
            ("e2e4", 55.0),
            ("e2e4", 55.0),
            ("d2d4", 52.0),
            ("d2d4", 51.0),
        ],
        Some("e2e4"),
    );
    let out = estimator(&server).estimate(&[inp]).await;
    close(ok(&out[0]), (0.5, 0.8));
}

#[tokio::test]
async fn maia_rejections_are_not_applicable() {
    let server = maia_server(MODEL).await;
    mount_predict(
        &server,
        Items(vec![
            json!({ "error": "invalid_fen" }),
            json!({ "error": "no_legal_moves" }),
            json!({ "probs": { "e2e4": 0.5, "d2d4": 0.3, "g1f3": 0.2 } }),
        ]),
    )
    .await;
    let out = estimator(&server)
        .estimate(&[std_lines(), std_lines(), std_lines()])
        .await;
    assert_eq!(out[0], not_applicable(NotApplicableReason::InvalidFen));
    assert_eq!(out[1], not_applicable(NotApplicableReason::NoLegalMoves));
    assert!(!out[0].is_pending() && !out[1].is_pending());
    close(ok(&out[2]), (0.5, 0.8));
}

#[tokio::test]
async fn maia_validation_failure_is_pending_for_that_item_only() {
    let server = maia_server(MODEL).await;
    let good = json!({ "probs": { "e2e4": 0.5, "d2d4": 0.3, "g1f3": 0.2 } });
    let bad = [
        json!({ "probs": { "e2e4": 1.5, "d2d4": -0.5 } }), // outside [0, 1], sums to 1
        json!({ "probs": { "e2e4": 0.5, "d2d4": 0.3 } }),  // sums to 0.8
        json!({ "probs": { "e2e4": 0.7, "d2d4": 0.32 } }), // sums to 1.02
        json!({ "probs": { "e2e4": "x" } }),               // not a number
        json!({ "probs": {} }),                            // empty
        json!({ "probs": { "castle": 1.0 } }),             // not a UCI move
        json!({ "error": "something_else" }),              // unknown error
        json!({ "something": "else" }),                    // neither probs nor error
    ];
    let mut items = vec![good.clone()];
    items.extend(bad.iter().cloned());
    items.push(good);
    mount_predict(&server, Items(items)).await;
    let inputs: Vec<DifficultyInput> = (0..10).map(|_| std_lines()).collect();
    let out = estimator(&server).estimate(&inputs).await;
    close(ok(&out[0]), (0.5, 0.8));
    for o in &out[1..9] {
        assert_eq!(*o, pending(PendingReason::InvalidResponse));
        assert!(o.is_pending());
    }
    close(ok(&out[9]), (0.5, 0.8));
}

#[tokio::test]
async fn maia_batches_130_inputs_into_5_requests() {
    let server = maia_server(MODEL).await;
    Mock::given(method("POST"))
        .and(path("/v1/predict"))
        .respond_with(FixedProbs::standard())
        .expect(5)
        .mount(&server)
        .await;
    let inputs: Vec<DifficultyInput> = (0..130).map(|_| std_lines()).collect();
    let out = estimator(&server).estimate(&inputs).await;
    assert_eq!(out.len(), 130);
    assert!(out.iter().all(|o| o.difficulty().is_some()));
    let sizes: Vec<usize> = predict_bodies(&server)
        .await
        .iter()
        .map(|b| b["items"].as_array().unwrap().len())
        .collect();
    assert_eq!(sizes, vec![32, 32, 32, 32, 2]);
}

#[tokio::test]
async fn opening_plies_are_not_applicable_and_never_sent() {
    let server = maia_server(MODEL).await;
    mount_predict(&server, FixedProbs::standard()).await;
    let out = estimator(&server)
        .estimate(&[opening(), std_lines(), opening()])
        .await;
    assert_eq!(out[0], not_applicable(NotApplicableReason::OpeningPly));
    assert!(!out[0].is_pending());
    close(ok(&out[1]), (0.5, 0.8));
    assert_eq!(out[2], not_applicable(NotApplicableReason::OpeningPly));
    let bodies = predict_bodies(&server).await;
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0]["items"].as_array().unwrap().len(), 1);
    assert_eq!(bodies[0]["items"][0]["fen"], MID);
}

#[tokio::test]
async fn opening_plies_are_never_pending_even_when_maia_is_down() {
    let est = select_difficulty_provider(Some("http://127.0.0.1:1"));
    let out = est.estimate(&[opening()]).await;
    assert_eq!(out, vec![not_applicable(NotApplicableReason::OpeningPly)]);
}

#[tokio::test]
async fn no_best_move_is_not_applicable() {
    let server = maia_server(MODEL).await;
    Mock::given(method("POST"))
        .and(path("/v1/predict"))
        .respond_with(FixedProbs::standard())
        .expect(0)
        .mount(&server)
        .await;
    let out = estimator(&server)
        .estimate(&[input(&[("e2e4", 55.0)], None)])
        .await;
    assert_eq!(out, vec![not_applicable(NotApplicableReason::NoBestMove)]);
}

#[tokio::test]
async fn empty_input_sends_nothing() {
    let server = maia_server(MODEL).await;
    Mock::given(method("POST"))
        .and(path("/v1/predict"))
        .respond_with(FixedProbs::standard())
        .expect(0)
        .mount(&server)
        .await;
    assert!(estimator(&server).estimate(&[]).await.is_empty());
}

// ---- Maia unavailable: pending_maia, never blocking ------------------------------------------

#[tokio::test]
async fn not_configured_is_pending() {
    for url in [None, Some(""), Some("  ")] {
        let est = select_difficulty_provider(url);
        assert!(!est.is_configured());
        let out = est.estimate(&[std_lines()]).await;
        assert_eq!(out, vec![pending(PendingReason::MaiaNotConfigured)]);
    }
}

#[tokio::test]
async fn unreachable_at_startup_is_pending_without_blocking() {
    let est = select_difficulty_provider(Some("http://127.0.0.1:1"));
    let started = Instant::now();
    let out = est.estimate(&[std_lines(), std_lines()]).await;
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(out, vec![pending(PendingReason::MaiaUnavailable); 2]);
}

#[tokio::test]
async fn healthz_failure_is_pending() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/healthz"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let err = MaiaDifficulty::connect(&server.uri(), DEFAULT_TIMEOUT, DEFAULT_BATCH_SIZE)
        .await
        .unwrap_err();
    assert!(matches!(err, MaiaCallError::Unavailable(_)), "{err:?}");
    let out = estimator(&server).estimate(&[std_lines()]).await;
    assert_eq!(out, vec![pending(PendingReason::MaiaUnavailable)]);
}

#[tokio::test]
async fn maia_down_at_startup_then_healthy_is_picked_up() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/healthz"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/healthz"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "model": MODEL })))
        .mount(&server)
        .await;
    mount_predict(&server, FixedProbs::standard()).await;
    let est = estimator(&server);
    assert_eq!(
        est.estimate(&[std_lines()]).await,
        vec![pending(PendingReason::MaiaUnavailable)]
    );
    close(ok(&est.estimate(&[std_lines()]).await[0]), (0.5, 0.8));
}

#[tokio::test]
async fn maia_down_mid_batch_leaves_the_rest_pending() {
    let server = maia_server(MODEL).await;
    // The first request succeeds; then Maia goes down.
    Mock::given(method("POST"))
        .and(path("/v1/predict"))
        .respond_with(FixedProbs::standard())
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    mount_predict(&server, ResponseTemplate::new(503)).await;
    let inputs: Vec<DifficultyInput> = (0..130).map(|_| std_lines()).collect();
    let out = estimator(&server).estimate(&inputs).await;
    assert_eq!(out.len(), 130);
    for o in &out[..32] {
        close(ok(o), (0.5, 0.8));
    }
    for o in &out[32..] {
        assert_eq!(*o, pending(PendingReason::MaiaUnavailable));
    }
    // After the failure the rest of the call is pending without another request.
    assert_eq!(predict_bodies(&server).await.len(), 2);
}

#[tokio::test]
async fn slow_answer_times_out_as_pending() {
    let server = maia_server(MODEL).await;
    mount_predict(
        &server,
        ResponseTemplate::new(200)
            .set_body_json(json!({ "items": [] }))
            .set_delay(Duration::from_secs(5)),
    )
    .await;
    let est = DifficultyEstimator::new(Some(&server.uri()), Duration::from_millis(300), 64);
    let started = Instant::now();
    let out = est.estimate(&[std_lines()]).await;
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(out, vec![pending(PendingReason::Timeout)]);
}

#[tokio::test]
async fn item_count_mismatch_is_pending() {
    let server = maia_server(MODEL).await;
    mount_predict(
        &server,
        ResponseTemplate::new(200).set_body_json(json!({ "items": [] })),
    )
    .await;
    let out = estimator(&server).estimate(&[std_lines()]).await;
    assert_eq!(out, vec![pending(PendingReason::InvalidResponse)]);
}

#[tokio::test]
async fn trait_estimate_reports_a_503_as_unavailable() {
    let server = maia_server(MODEL).await;
    mount_predict(&server, ResponseTemplate::new(503)).await;
    let err = connect(&server)
        .await
        .estimate(&[std_lines()])
        .await
        .unwrap_err();
    assert!(matches!(err, DifficultyError::Unavailable(_)), "{err:?}");
    // The trait skips opening plies (None) without a request.
    let out = connect(&server).await.estimate(&[opening()]).await.unwrap();
    assert_eq!(out, vec![None]);
}

#[test]
fn outcome_json_shape() {
    let ok = DifficultyOutcome::Ok {
        difficulty: profile_core::types::Difficulty {
            p_best: 0.5,
            p_ok: 0.8,
            model: MODEL.to_owned(),
        },
    };
    assert_eq!(
        serde_json::to_value(&ok).unwrap(),
        json!({ "status": "ok", "difficulty": { "pBest": 0.5, "pOk": 0.8, "model": MODEL } })
    );
    assert_eq!(
        serde_json::to_value(pending(PendingReason::MaiaUnavailable)).unwrap(),
        json!({ "status": "pending_maia", "reason": "maia_unavailable" })
    );
    assert_eq!(
        serde_json::to_value(not_applicable(NotApplicableReason::OpeningPly)).unwrap(),
        json!({ "status": "not_applicable", "reason": "opening_ply" })
    );
}

// ---- live contract (only with MAIA_URL) ------------------------------------------------------

/// Ruy Lopez Exchange, 1.e4 e5 2.Nf3 Nc6 3.Bb5 a6 4.Bxc6 dxc6 5.O-O f6 6.d4 exd4 7.Nxd4 c5
/// 8.Nb3 Qxd1: white must recapture the queen (9.Rxd1, `f1d1`). Half-move index 16.
const LIVE_RECAPTURE: &str = "r1b1kbnr/1pp3pp/p4p2/2p5/4P3/1N6/PPP2PPP/RNBq1RK1 w kq - 0 9";
/// Italian, 1.e4 e5 2.Nf3 Nc6 3.Bc4 Bc5 4.c3 Nf6 5.d3 d6 6.Nbd2: black to move, both sides
/// may castle. Half-move index 11.
const LIVE_BLACK: &str = "r1bqk2r/ppp2ppp/2np1n2/2b1p3/2B1P3/2PP1N2/PP1N1PPP/R1BQK2R b KQkq - 1 6";
/// Same game after 6...a6: white to move, `e1g1` legal. Half-move index 12.
const LIVE_CASTLE: &str =
    "r1bqk2r/1pp2ppp/p1np1n2/2b1p3/2B1P3/2PP1N2/PP1N1PPP/R1BQK2R w KQkq - 0 7";
/// White pawn on e7 with e8 empty: `e7e8q` (and underpromotions) legal.
const LIVE_PROMO: &str = "8/4P3/8/8/8/8/k7/6K1 w - - 0 50";
/// Fool's mate (move number raised past the opening cut): white is checkmated.
const LIVE_MATE: &str = "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 30";
/// Black to move, stalemated.
const LIVE_STALEMATE: &str = "7k/5Q2/6K1/8/8/8/8/8 b - - 0 60";

fn live_input(fen: &str, elo: i32, lines: &[(&str, f64)], best: &str) -> DifficultyInput {
    DifficultyInput {
        fen: fen.to_owned(),
        elo_self: Some(elo),
        elo_oppo: Some(elo),
        lines: Some(lines.iter().map(|(u, w)| ((*u).to_owned(), *w)).collect()),
        best_uci: Some(best.to_owned()),
    }
}

fn assert_distribution(fen: &str, probs: &HashMap<String, f64>) {
    assert!(!probs.is_empty(), "{fen}: no moves");
    let sum: f64 = probs.values().sum();
    assert!((sum - 1.0).abs() <= 1e-3, "{fen}: sum {sum}");
    let pos = Position::from_fen(fen).unwrap();
    for (uci, p) in probs {
        assert!(
            p.is_finite() && (0.0..=1.0).contains(p),
            "{fen}: {uci} = {p}"
        );
        let played = pos
            .clone()
            .push_uci(uci)
            .unwrap_or_else(|e| panic!("{fen}: {uci} is not legal: {e}"));
        // Canonical UCI: castling as the king's two-square step, promotions with a suffix.
        assert_eq!(&played.uci, uci, "{fen}: non-canonical key");
    }
}

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

    // Model id: maia2-(rapid|blitz)-<major.minor.patch>.
    let id = maia.model();
    let rest = id
        .strip_prefix("maia2-rapid-")
        .or_else(|| id.strip_prefix("maia2-blitz-"))
        .unwrap_or_else(|| panic!("unexpected model id {id}"));
    let parts: Vec<&str> = rest.split('.').collect();
    assert!(
        parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())),
        "unexpected model id {id}"
    );

    let positions = [LIVE_RECAPTURE, LIVE_BLACK, LIVE_CASTLE, LIVE_PROMO];
    for fen in positions {
        assert!(
            !is_maia_opening_ply(fen),
            "{fen} must be past the opening cut"
        );
    }
    let probe = |fen: &str| live_input(fen, 1500, &[("a1a1", 50.0)], "a1a1");
    let invalid = probe("not a fen");
    let inputs = vec![
        probe(LIVE_RECAPTURE),
        probe(LIVE_CASTLE),
        probe(LIVE_PROMO),
        probe(LIVE_MATE),
        probe(LIVE_STALEMATE),
        invalid,
    ];
    let probs = maia.probabilities(&inputs).await.unwrap();

    // Middlegame: the obvious recapture is Maia's top move, with most of the mass.
    let recapture = probs[0].as_ref().expect("recapture position");
    assert_distribution(LIVE_RECAPTURE, recapture);
    let (top, p_top) = recapture.iter().max_by(|a, b| a.1.total_cmp(b.1)).unwrap();
    assert_eq!(top, "f1d1", "{recapture:?}");
    assert!(*p_top >= 0.5, "p(f1d1) = {p_top}");

    let castle = probs[1].as_ref().expect("castling position");
    assert_distribution(LIVE_CASTLE, castle);
    assert!(
        castle.contains_key("e1g1") && !castle.contains_key("e1h1"),
        "{castle:?}"
    );

    let promo = probs[2].as_ref().expect("promotion position");
    assert_distribution(LIVE_PROMO, promo);
    assert!(promo.contains_key("e7e8q"), "{promo:?}");
    assert!(!promo.contains_key("e7e8"), "{promo:?}");

    assert_eq!(probs[3], None, "checkmate must give None");
    assert_eq!(probs[4], None, "stalemate must give None");
    assert_eq!(probs[5], None, "garbage FEN must give None");

    // Black to move. Its half-move index (11) is past the cut, so it is sent.
    let black = maia
        .probabilities(&[probe(LIVE_BLACK)])
        .await
        .unwrap()
        .remove(0)
        .expect("black-to-move position");
    assert_distribution(LIVE_BLACK, &black);
    assert!(black.contains_key("e8g8"), "{black:?}");

    // A full batch completes inside the request timeout.
    let batch: Vec<DifficultyInput> = (0..DEFAULT_BATCH_SIZE)
        .map(|_| probe(LIVE_RECAPTURE))
        .collect();
    let started = Instant::now();
    let out = maia.estimate(&batch).await.unwrap();
    assert!(
        started.elapsed() < DEFAULT_TIMEOUT,
        "{:?}",
        started.elapsed()
    );
    assert!(out.iter().all(Option::is_some));

    // Elo changes the distribution.
    let low = maia
        .probabilities(&[live_input(LIVE_CASTLE, 1000, &[], "e1g1")])
        .await
        .unwrap()
        .remove(0)
        .unwrap();
    let high = maia
        .probabilities(&[live_input(LIVE_CASTLE, 2100, &[], "e1g1")])
        .await
        .unwrap()
        .remove(0)
        .unwrap();
    let l1: f64 = low
        .iter()
        .map(|(m, p)| (p - high.get(m).copied().unwrap_or(0.0)).abs())
        .sum();
    assert!(l1 > 0.02, "Elo 1000 vs 2100 L1 distance {l1}");
}
