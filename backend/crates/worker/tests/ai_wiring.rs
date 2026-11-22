//! R3 Task 26: the AI jobs wired into `start_workers`. The real stack (Postgres, the real
//! Stockfish pool, the queue runners) with the fake AI client or a stub sidecar `/health`; no
//! network, no key.

mod common;

#[path = "../../ai/tests/support/generate.rs"]
mod generate;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use ai::client::batches::ApiKey;
use ai::client::fake::{FakeAiClient, FakeScript};
use ai::client::port::{AiClient, AiError};
use ai::client::sidecar::SidecarToken;
use ai::db::interactive::{self, NewEval};
use ai::db::runs::ReportTrigger;
use ai::entitlements::Plan;
use ai::generate::request::build_section_request;
use ai::input::types::{CoachingInput, TimeClass};
use ai::jobs::kinds::enqueue_interactive;
use ai::jobs::providers::CoachingInputProvider;
use ai::jobs::report_job::request_report;
use ai::output::codes::SECTION_KEYS;
use ai::testing::make_input;
use common::{env_for, wait_until};
use futures::future::BoxFuture;
use queue::queue::Queue;
use queue::runner::RunnerConfig;
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;
use worker::ai_jobs::{AI_RUNNER_KINDS, AiSetup};
use worker::config::{SidecarEnv, WorkerEnv};
use worker::shutdown::{ShutdownOptions, shutdown};
use worker::start::{RunningWorkers, StartOptions, start_workers};

const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
/// Never a real key: it only has to be recognisable in a log.
const FAKE_KEY: &str = "sk-ant-api03-T26FAKEKEYNEVERLOGGED0000000000";
const FAKE_TOKEN: &str = "t26-sidecar-token-never-logged-000000000000";

/// Serves the run's own account the b2-rapid fixture.
struct FixtureInputs;

impl CoachingInputProvider for FixtureInputs {
    fn coaching_input<'a>(
        &'a self,
        account_id: Uuid,
        _tc: TimeClass,
    ) -> BoxFuture<'a, anyhow::Result<Option<Value>>> {
        Box::pin(async move { Ok(Some(make_input(json!({ "accountId": account_id })))) })
    }
}

/// The fixture as [`FixtureInputs`] serves it to `account`.
fn input_of(account: Uuid) -> CoachingInput {
    serde_json::from_value(make_input(json!({ "accountId": account }))).unwrap()
}

fn opts(ai: AiSetup) -> StartOptions {
    StartOptions {
        pool_size: Some(1),
        runner: Some(RunnerConfig::fast_for_tests()),
        with_reconciler: false,
        ai,
        inputs: Some(Arc::new(FixtureInputs)),
        ..StartOptions::default()
    }
}

async fn stop(running: RunningWorkers) {
    shutdown(running.into_shutdown_handles(), ShutdownOptions::default()).await;
}

async fn pool_of(env: &WorkerEnv) -> PgPool {
    PgPoolOptions::new()
        .max_connections(4)
        .connect(&env.database_url)
        .await
        .unwrap()
}

async fn report_request(pool: &PgPool) -> (Uuid, Uuid) {
    let account = testkit::insert_account(pool, "lichess", "t26player").await;
    (account, request_for(pool, account).await)
}

async fn request_for(pool: &PgPool, account: Uuid) -> Uuid {
    request_report(
        pool,
        &Queue::new(pool.clone()),
        account,
        TimeClass::Rapid,
        ReportTrigger::Onboarding,
        Plan::Free,
    )
    .await
    .unwrap()
}

/// `(run status, ai_disabled_reason, report status)` once the run is done.
async fn finished_run(pool: &PgPool, run: Uuid) -> (String, Option<String>, String) {
    wait_until(
        "the report run to finish",
        Duration::from_secs(60),
        || async {
            let row: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
                "SELECT r.status::text, r.ai_disabled_reason, p.status::text FROM report_runs r \
             LEFT JOIN reports p ON p.id = r.report_id WHERE r.id = $1",
            )
            .bind(run)
            .fetch_optional(pool)
            .await
            .unwrap();
            match row {
                Some((s, reason, Some(report))) if s == "done" || s == "failed" => {
                    Some((s, reason, report))
                }
                _ => None,
            }
        },
    )
    .await
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn no_key_runs_report_to_computed_only(_p: PgPoolOptions, opts_: PgConnectOptions) {
    let Some(env) = env_for(&opts_) else {
        return;
    };
    let running = start_workers(&env, opts(AiSetup::FromEnv)).await.unwrap();
    assert!(!running.ai_enabled);
    // Every AI kind has its runner, AI or not.
    for kind in AI_RUNNER_KINDS {
        assert!(running.runner_kinds().contains(&kind), "{kind}");
    }
    let pool = pool_of(&env).await;
    let (_account, run) = report_request(&pool).await;
    let (status, reason, report) = finished_run(&pool, run).await;
    assert_eq!(status, "done");
    assert_eq!(reason.as_deref(), Some("no_api_key"));
    assert_eq!(report, "computed_only");
    let usage: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_usage")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(usage, 0);
    pool.close().await;
    stop(running).await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn fake_ai_runs_report_to_complete_through_the_queue(
    _p: PgPoolOptions,
    opts_: PgConnectOptions,
) {
    let Some(env) = env_for(&opts_) else {
        return;
    };
    let pool = pool_of(&env).await;
    let account = testkit::insert_account(&pool, "lichess", "t26player").await;
    // Every report section answers validly; anything else (the run's change notes, whose runner
    // is wired too) is refused, so its note fails.
    let input = input_of(account);
    let fake = Arc::new(FakeAiClient::new(FakeScript {
        structured: Some(Box::new(move |req, _| {
            let task = req.messages[0].content.last();
            match SECTION_KEYS.into_iter().find(|k| {
                build_section_request(&input, *k, true).messages[0]
                    .content
                    .last()
                    == task
            }) {
                Some(k) => Ok(generate::out(generate::valid(k))),
                None => Err(AiError::Request {
                    status: 400,
                    api_error_type: None,
                }),
            }
        })),
        ..FakeScript::default()
    }));
    let client: Arc<dyn AiClient> = fake.clone();
    let running = start_workers(&env, opts(AiSetup::With(Some(client))))
        .await
        .unwrap();
    assert!(running.ai_enabled);
    let run = request_for(&pool, account).await;
    let (status, reason, report) = finished_run(&pool, run).await;
    assert_eq!((status.as_str(), reason), ("done", None));
    assert_eq!(report, "complete");
    // Five sections, each validated on its first call, each metered once.
    let usage: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ai_usage WHERE account_id = $1 AND feature = 'report'",
    )
    .bind(account)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(usage, 5);
    wait_until(
        "the generate-report job to complete",
        Duration::from_secs(30),
        || async {
            let job: String = sqlx::query_scalar(
            "SELECT state::text FROM jobs WHERE kind = 'generate-report' ORDER BY id DESC LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
            (job == "done").then_some(())
        },
    )
    .await;
    // The run enqueued its two change notes, and the explain-weakness-change runner ran them.
    let notes: Vec<String> = wait_until("the change notes", Duration::from_secs(60), || async {
        let s: Vec<String> = sqlx::query_scalar(
            "SELECT status::text FROM weakness_change_notes WHERE account_id = $1",
        )
        .bind(account)
        .fetch_all(&pool)
        .await
        .unwrap();
        (s.len() == 2 && s.iter().all(|x| x != "pending")).then_some(s)
    })
    .await;
    assert_eq!(notes, ["failed", "failed"]);
    assert!(fake.calls().len() >= 7, "5 sections and 2 notes");
    pool.close().await;
    stop(running).await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn interactive_analyse_runs_on_the_real_engine_pool(
    _p: PgPoolOptions,
    opts_: PgConnectOptions,
) {
    let Some(env) = env_for(&opts_) else {
        return;
    };
    let running = start_workers(&env, opts(AiSetup::With(None)))
        .await
        .unwrap();
    let pool = pool_of(&env).await;
    let account = testkit::insert_account(&pool, "lichess", "t26checker").await;
    let fen = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
    let row = {
        let mut conn = pool.acquire().await.unwrap();
        interactive::insert_eval(
            &mut conn,
            &NewEval {
                account_id: account,
                message_id: None,
                protocol: ai::chat::interactive::INTERACTIVE_PROTOCOL.to_owned(),
                base_fen: START_FEN.to_owned(),
                moves: vec!["e2e4".to_owned()],
                fen: fen.to_owned(),
            },
        )
        .await
        .unwrap()
    };
    enqueue_interactive(&Queue::new(pool.clone()), row.id, account)
        .await
        .unwrap();
    let lines = wait_until("the engine check", Duration::from_secs(60), || async {
        let mut conn = pool.acquire().await.unwrap();
        let r = interactive::get_eval(&mut conn, row.id)
            .await
            .unwrap()
            .unwrap();
        (r.status.as_str() == "done").then_some(r.lines)
    })
    .await
    .expect("lines");
    assert_eq!(lines.as_array().map(Vec::len), Some(3), "{lines}");
    pool.close().await;
    stop(running).await;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn the_profile_provider_has_no_input_without_analysed_games(pool: PgPool) {
    let account = testkit::insert_account(&pool, "lichess", "t26empty").await;
    let provider = worker::ai_jobs::ProfileInputProvider { pool: pool.clone() };
    let got = provider
        .coaching_input(account, TimeClass::Rapid)
        .await
        .unwrap();
    assert_eq!(got, None);
}

// --- Startup logging -----------------------------------------------------------------------

/// A `tracing` writer into a shared buffer.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        std::io::Write::write(&mut *self.0.lock().unwrap(), buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Captured;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// A sidecar stand-in answering every request with `GET /health`'s `body`.
async fn health_stub(body: Value) -> SocketAddr {
    flaky_health_stub(body, 0).await
}

/// [`health_stub`] that answers its first `fail_first` requests with a 503.
async fn flaky_health_stub(body: Value, fail_first: usize) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let body = body.to_string();
    let served = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let body = body.clone();
            let n = served.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let mut seen = Vec::new();
                while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
                    match sock.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => seen.extend_from_slice(&buf[..n]),
                    }
                }
                let status = if n < fail_first {
                    "503 Service Unavailable"
                } else {
                    "200 OK"
                };
                let resp = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
                     connection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
    addr
}

fn ai_env(env: WorkerEnv, sidecar: SocketAddr) -> WorkerEnv {
    WorkerEnv {
        anthropic_api_key: Some(ApiKey::new(FAKE_KEY)),
        ai_sidecar: Some(SidecarEnv {
            url: format!("http://{sidecar}"),
            token: SidecarToken::new(FAKE_TOKEN),
        }),
        ai_tool_bind: "127.0.0.1:0".parse().unwrap(),
        ai_tool_public_url: Some("http://127.0.0.1:7311".to_owned()),
        ..env
    }
}

/// Every log line of this test binary, at every level (one global subscriber: a thread-local
/// one misses events whose callsite interest another test thread cached first).
fn logs() -> &'static Captured {
    static LOGS: OnceLock<Captured> = OnceLock::new();
    LOGS.get_or_init(|| {
        let captured = Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(captured.clone())
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .finish();
        tracing::subscriber::set_global_default(subscriber).expect("one global subscriber");
        captured
    })
}

/// Starts the worker from `env` (its `Debug` is logged too), then stops it. Returns whether AI
/// is enabled and every line logged so far by any test of this binary.
async fn start_logged(env: &WorkerEnv) -> (bool, String) {
    let captured = logs();
    tracing::info!(env = ?env, "test: worker env");
    let running = start_workers(env, opts(AiSetup::FromEnv)).await.unwrap();
    let enabled = running.ai_enabled;
    stop(running).await;
    (enabled, captured.text())
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn startup_logs_never_contain_the_key(_p: PgPoolOptions, opts_: PgConnectOptions) {
    let Some(env) = env_for(&opts_) else {
        return;
    };
    let stub = health_stub(json!({"ok": true, "auth": "api_key", "sdkVersion": "0.3.325"})).await;
    let (enabled, logs) = start_logged(&ai_env(env, stub)).await;
    assert!(enabled);
    assert!(logs.contains("ai: enabled (sidecar, batches on)"), "{logs}");
    assert!(
        logs.contains("ai: scheduled regenerations via batches"),
        "{logs}"
    );
    assert!(!logs.contains(FAKE_KEY), "the API key was logged:\n{logs}");
    assert!(
        !logs.contains("T26FAKEKEY"),
        "part of the API key was logged:\n{logs}"
    );
    assert!(
        !logs.contains(FAKE_TOKEN),
        "the sidecar token was logged:\n{logs}"
    );
}

/// M5b Task B ruling: no `ANTHROPIC_API_KEY`, but the sidecar has a credential (the owner's dev
/// OAuth token): AI is on, and the schedules use the sync jobs. The start says so once.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn without_a_key_the_schedules_run_sync_through_the_sidecar(
    _p: PgPoolOptions,
    opts_: PgConnectOptions,
) {
    let Some(env) = env_for(&opts_) else {
        return;
    };
    let stub = health_stub(json!({"ok": true, "auth": "oauth", "sdkVersion": "0.3.325"})).await;
    let env = WorkerEnv {
        anthropic_api_key: None,
        ..ai_env(env, stub)
    };
    let (enabled, logs) = start_logged(&env).await;
    assert!(enabled);
    assert!(
        logs.contains("ai: enabled (sidecar, batches off)"),
        "{logs}"
    );
    assert!(
        logs.contains("ai: scheduled regenerations via sync jobs (sidecar)"),
        "{logs}"
    );
    assert!(!logs.contains(FAKE_TOKEN), "the sidecar token was logged");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_transient_health_failure_is_retried(_p: PgPoolOptions, opts_: PgConnectOptions) {
    let Some(env) = env_for(&opts_) else {
        return;
    };
    let stub = flaky_health_stub(json!({"ok": true, "auth": "api_key"}), 1).await;
    let (enabled, _) = start_logged(&ai_env(env, stub)).await;
    assert!(enabled, "one failed probe must not disable AI");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn sidecar_without_credentials_disables_ai(_p: PgPoolOptions, opts_: PgConnectOptions) {
    let Some(env) = env_for(&opts_) else {
        return;
    };
    let stub = health_stub(json!({"ok": true, "auth": "none"})).await;
    let (enabled, logs) = start_logged(&ai_env(env, stub)).await;
    assert!(!enabled);
    assert!(
        logs.contains("ai: disabled (sidecar has no credential)"),
        "{logs}"
    );
    assert!(!logs.contains(FAKE_KEY), "{logs}");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn no_sidecar_logs_ai_disabled(_p: PgPoolOptions, opts_: PgConnectOptions) {
    let Some(env) = env_for(&opts_) else {
        return;
    };
    let env = WorkerEnv {
        anthropic_api_key: Some(ApiKey::new(FAKE_KEY)),
        ..env
    };
    let (enabled, logs) = start_logged(&env).await;
    assert!(!enabled);
    assert!(
        logs.contains("ai: disabled (AI_SIDECAR_URL not set)"),
        "{logs}"
    );
    assert!(!logs.contains(FAKE_KEY), "{logs}");
}
