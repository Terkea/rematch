//! Port of apps/worker/test/recovery/breaker-delay.test.ts.

mod common;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use analysis::jobs::import::{ImportAccountDeps, enqueue_import};
use common::{event_types, wait_until};
use db::import_runs::list_runs;
use futures::StreamExt;
use futures::stream::BoxStream;
use platforms::breaker::{BreakerOptions, CircuitBreaker, CircuitState};
use platforms::{
    FetchOptions, GameSource, NormalizedGame, Platform, PlatformClient, PlatformError, Player,
};
use queue::queue::Queue;
use queue::runner::RunnerConfig;
use queue::{KIND_IMPORT, import_key};
use serde_json::json;
use sqlx::PgPool;
use testkit::fake_platform::FakeClient;
use time::macros::datetime;
use worker::start::spawn_import_runner;

const PROBE: &str = "https://example.invalid/probe";

fn game() -> NormalizedGame {
    NormalizedGame {
        platform: Platform::Lichess,
        platform_game_id: "patient1".to_owned(),
        pgn: testkit::SCHOLARS.to_owned(),
        played_at: datetime!(2026-03-04 12:00 UTC),
        time_class: chess_core::TimeClass::Blitz,
        base_seconds: Some(300),
        increment_seconds: Some(0),
        rated: true,
        white: Player {
            name: "Patient".to_owned(),
            rating: Some(1),
        },
        black: Player {
            name: "Villain".to_owned(),
            rating: Some(1),
        },
        result: "1-0".to_owned(),
        termination: None,
        eco: None,
        opening_name: None,
        lichess_evals: None,
    }
}

/// Probes the platform once, then streams one game.
struct ProbeThenGame;

impl GameSource for ProbeThenGame {
    fn fetch<'a>(
        &'a self,
        client: Arc<dyn PlatformClient>,
        _username: &'a str,
        _opts: FetchOptions,
    ) -> BoxStream<'a, Result<NormalizedGame, PlatformError>> {
        futures::stream::once(async move {
            client.get_json(PROBE, Default::default()).await?;
            Ok(game())
        })
        .boxed()
    }
}

async fn job_row(pool: &PgPool, acct: uuid::Uuid) -> (String, i32, bool) {
    sqlx::query_as(
        "SELECT state::text, attempts, run_at > now() FROM jobs \
         WHERE kind = $1 AND dedupe_key = $2",
    )
    .bind(KIND_IMPORT)
    .bind(import_key(acct))
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn delays_the_job_without_using_an_attempt_then_resumes_once_the_breaker_closes(
    pool: PgPool,
) {
    let acct = testkit::insert_account(&pool, "lichess", "patient").await;
    let breaker = Arc::new(CircuitBreaker::new(BreakerOptions {
        failure_threshold: 1,
        cooldown: Duration::from_millis(700),
        probe_timeout: Duration::from_millis(100),
        ..BreakerOptions::default()
    }));
    let _ = breaker
        .exec(|| async {
            Err::<(), _>(PlatformError::Timeout {
                url: PROBE.to_owned(),
                ms: 1,
            })
        })
        .await;
    assert_eq!(breaker.state(), CircuitState::Open);

    let mut client = FakeClient::default();
    client.json.insert(PROBE.to_owned(), json!({}));
    let client: Arc<dyn PlatformClient> = Arc::new(client);
    let source: Arc<dyn GameSource> = Arc::new(ProbeThenGame);
    let queue = Queue::new(pool.clone());
    let mut deps = ImportAccountDeps::new(pool.clone(), queue.clone());
    deps.sources = HashMap::from([
        (Platform::Lichess, source.clone()),
        (Platform::Chesscom, source),
    ]);
    deps.clients = HashMap::from([
        (Platform::Lichess, client.clone()),
        (Platform::Chesscom, client),
    ]);
    deps.breakers = HashMap::from([
        (Platform::Lichess, breaker.clone()),
        (Platform::Chesscom, breaker.clone()),
    ]);
    deps.breaker_min_delay = Duration::from_millis(100);
    deps.probe_timeout = Duration::from_millis(100);
    let runner = spawn_import_runner(queue.clone(), deps, &RunnerConfig::fast_for_tests());
    assert!(enqueue_import(&queue, acct, None).await.unwrap());

    // Delayed: queued again in the future, with the attempt given back.
    let delayed = wait_until("the job to be delayed", Duration::from_secs(15), || {
        let pool = pool.clone();
        async move {
            let row = job_row(&pool, acct).await;
            (row.0 == "queued" && row.2).then_some(row)
        }
    })
    .await;
    assert_eq!(delayed.1, 0);
    let runs = list_runs(&pool, acct).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status.as_str(), "paused");
    assert_eq!(runs[0].error_code.as_deref(), Some("circuit_open"));

    let done = wait_until(
        "the job to complete after the cooldown",
        Duration::from_secs(15),
        || {
            let pool = pool.clone();
            async move {
                let row = job_row(&pool, acct).await;
                (row.0 == "done").then_some(row)
            }
        },
    )
    .await;
    assert!(done.1 <= 1, "{done:?}");
    assert_eq!(breaker.state(), CircuitState::Closed);
    let games: i64 = sqlx::query_scalar("SELECT count(*) FROM games WHERE account_id = $1")
        .bind(acct)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(games, 1);
    // One run: the paused run is reused (T21), unlike TS's ["done", "paused"].
    let statuses: Vec<String> = list_runs(&pool, acct)
        .await
        .unwrap()
        .iter()
        .map(|r| r.status.as_str().to_owned())
        .collect();
    assert_eq!(statuses, ["done"]);
    assert!(
        !event_types(&pool, acct)
            .await
            .contains(&"job.failed".to_owned())
    );
    runner.shutdown(Duration::from_secs(5)).await;
}
