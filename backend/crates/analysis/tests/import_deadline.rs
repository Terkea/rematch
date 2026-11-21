//! Port of the import case of apps/worker/test/recovery/deadline.test.ts.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use analysis::deadline::IMPORT_IDLE_DEADLINE;
use analysis::jobs::import::{ImportAccountDeps, enqueue_import, handle_import_account};
use db::import_runs::{ImportRunStatus, list_runs};
use futures::StreamExt;
use futures::stream::BoxStream;
use platforms::{
    FetchOptions, GameSource, NormalizedGame, Platform, PlatformClient, PlatformError, Player,
};
use queue::queue::Queue;
use queue::{JobError, KIND_IMPORT};
use sqlx::PgPool;
use testkit::fake_platform::FakeClient;
use time::macros::datetime;
use tokio_util::sync::CancellationToken;

/// Yields one game, then goes silent for ever.
struct SilentAfterOne(NormalizedGame);

impl GameSource for SilentAfterOne {
    fn fetch<'a>(
        &'a self,
        _client: Arc<dyn PlatformClient>,
        _username: &'a str,
        _opts: FetchOptions,
    ) -> BoxStream<'a, Result<NormalizedGame, PlatformError>> {
        futures::stream::once(futures::future::ready(Ok(self.0.clone())))
            .chain(futures::stream::pending())
            .boxed()
    }
}

#[test]
fn uses_the_brief_s_import_idle_deadline_of_30_minutes() {
    assert_eq!(IMPORT_IDLE_DEADLINE, Duration::from_secs(30 * 60));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn an_import_with_no_progress_for_the_idle_deadline_fails_as_a_timeout(pool: PgPool) {
    let acct = testkit::insert_account(&pool, "lichess", "idle").await;
    let first = NormalizedGame {
        platform: Platform::Lichess,
        platform_game_id: "idle0001".to_owned(),
        pgn: testkit::SCHOLARS.to_owned(),
        played_at: datetime!(2026-01-01 00:00 UTC),
        time_class: chess_core::TimeClass::Blitz,
        base_seconds: Some(300),
        increment_seconds: Some(0),
        rated: true,
        white: Player {
            name: "Idle".to_owned(),
            rating: Some(1),
        },
        black: Player {
            name: "Villain".to_owned(),
            rating: Some(1500),
        },
        result: "1-0".to_owned(),
        termination: None,
        eco: None,
        opening_name: None,
        lichess_evals: None,
    };
    let source: Arc<dyn GameSource> = Arc::new(SilentAfterOne(first));
    let client: Arc<dyn PlatformClient> = Arc::new(FakeClient::default());
    let mut d = ImportAccountDeps::new(pool.clone(), Queue::new(pool.clone()));
    d.sources = HashMap::from([(Platform::Lichess, source)]);
    d.clients = HashMap::from([(Platform::Lichess, client)]);
    d.idle_deadline = Duration::from_millis(200);

    enqueue_import(&d.queue, acct, None).await.unwrap();
    // Back-dated so a small clock step between transactions cannot hide the job.
    sqlx::query("UPDATE jobs SET run_at = now() - interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();
    let job = d
        .queue
        .dequeue(&[KIND_IMPORT], "test", 1, Duration::from_secs(60))
        .await
        .unwrap()
        .pop()
        .unwrap();
    let payload = serde_json::from_value(job.payload.clone()).unwrap();

    let started = Instant::now();
    let res = tokio::time::timeout(
        Duration::from_secs(10),
        handle_import_account(&d, &job, payload, CancellationToken::new()),
    )
    .await
    .expect("the idle deadline ends the import");
    // Generous bound: at least 3x the 200 ms deadline plus DB time.
    assert!(started.elapsed() < Duration::from_secs(5));
    match &res {
        Err(JobError::Retryable { code, .. }) => assert_eq!(code.as_deref(), Some("timeout")),
        other => panic!("expected a retryable timeout, got {other:?}"),
    }
    let runs = list_runs(&pool, acct).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, ImportRunStatus::Failed);
    assert_eq!(runs[0].error_code.as_deref(), Some("timeout"));
    assert_eq!(runs[0].imported, 1);
}
