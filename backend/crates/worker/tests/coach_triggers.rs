//! M5b Task B: the first report after a profile build (ruling G3) and the weekly-review
//! provider (ruling G2), with no AI client: nothing here calls a model.

use std::sync::Arc;

use ai::entitlements::{ConfiguredEntitlements, Entitlements, Plan};
use ai::input::types::TimeClass;
use ai::jobs::batch_jobs::WeeklyInputProvider;
use ai::jobs::kinds::{KIND_GENERATE_REPORT, report_key};
use futures::TryStreamExt;
use futures::future::BoxFuture;
use queue::queue::Queue;
use queue::runner::{Handler, JobContext};
use queue::{Job, JobError};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::OffsetDateTime;
use time::macros::date;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use worker::coach_triggers::{
    FIRST_REPORT_RETRY_AFTER, FirstReportOnBuild, ProfileWeeklyProvider, request_first_report,
};

/// Stands in for the build-profile handler: succeeds or fails, and counts its calls.
struct Build {
    ok: bool,
}

impl Handler for Build {
    fn kind(&self) -> &'static str {
        queue::kinds::KIND_BUILD_PROFILE
    }

    fn handle(&self, _job: Job, _ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        let ok = self.ok;
        Box::pin(async move {
            if ok {
                Ok(json!({"estimates": 17}))
            } else {
                Err(JobError::Retryable {
                    message: "db down".into(),
                    code: None,
                })
            }
        })
    }
}

fn job(account_id: Uuid, time_class: &str) -> Job {
    let now = OffsetDateTime::now_utc();
    Job {
        id: 1,
        kind: queue::kinds::KIND_BUILD_PROFILE.into(),
        payload: json!({ "accountId": account_id, "timeClass": time_class }),
        dedupe_key: None,
        user_key: None,
        priority: 0,
        attempts: 1,
        max_attempts: 3,
        lease_token: Uuid::new_v4(),
        run_at: now,
        created_at: now,
        started_at: now,
    }
}

fn ctx() -> JobContext {
    JobContext {
        cancel: CancellationToken::new(),
    }
}

fn wrapped(pool: &PgPool, ok: bool) -> FirstReportOnBuild {
    FirstReportOnBuild {
        inner: Arc::new(Build { ok }),
        pool: pool.clone(),
        queue: Queue::new(pool.clone()),
        entitlements: Arc::new(ConfiguredEntitlements::default()) as Arc<dyn Entitlements>,
    }
}

async fn linked_account(pool: &PgPool, name: &str) -> Uuid {
    let a = testkit::insert_account(pool, "lichess", name).await;
    let user = testkit::insert_user(pool, name).await;
    sqlx::query(
        "INSERT INTO account_links (user_id, linked_account_id, method) VALUES ($1, $2, 'username')",
    )
    .bind(user)
    .bind(a)
    .execute(pool)
    .await
    .unwrap();
    a
}

/// (run id, trigger, plan, status, is_first_report) of every run, oldest first.
async fn runs(pool: &PgPool) -> Vec<(Uuid, String, String, String, bool)> {
    sqlx::query_as(
        "SELECT id, trigger::text, plan::text, status::text, is_first_report FROM report_runs \
         ORDER BY created_at, id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

/// (payload, dedupe key) of every generate-report job.
async fn report_jobs(pool: &PgPool) -> Vec<(Value, Option<String>)> {
    sqlx::query_as("SELECT payload, dedupe_key FROM jobs WHERE kind = $1 ORDER BY id")
        .bind(KIND_GENERATE_REPORT)
        .fetch_all(pool)
        .await
        .unwrap()
}

// ------------------------------------------------------------------ G3: the first report

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_completed_build_without_a_report_enqueues_the_first_report(pool: PgPool) {
    let a = linked_account(&pool, "firstrep").await;
    let h = wrapped(&pool, true);
    assert_eq!(h.kind(), queue::kinds::KIND_BUILD_PROFILE);
    let out = h.handle(job(a, "blitz"), ctx()).await.unwrap();
    // The build's own result is passed through.
    assert_eq!(out, json!({"estimates": 17}));
    let r = runs(&pool).await;
    assert_eq!(r.len(), 1);
    let (run, trigger, plan, status, first) = r[0].clone();
    assert_eq!(
        (trigger.as_str(), plan.as_str(), status.as_str(), first),
        ("onboarding", "beta", "queued", true)
    );
    let tc: (Uuid, String) =
        sqlx::query_as("SELECT account_id, time_class::text FROM report_runs WHERE id = $1")
            .bind(run)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(tc, (a, "blitz".to_owned()));
    // Exactly T18's payload and dedupe key.
    assert_eq!(
        report_jobs(&pool).await,
        vec![(json!({ "reportRunId": run }), Some(report_key(run)))]
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_later_build_does_not_re_enqueue_while_the_first_is_pending(pool: PgPool) {
    let a = linked_account(&pool, "again").await;
    let h = wrapped(&pool, true);
    h.handle(job(a, "blitz"), ctx()).await.unwrap();
    h.handle(job(a, "blitz"), ctx()).await.unwrap();
    for status in ["running", "awaiting_retry", "awaiting_batch"] {
        sqlx::query("UPDATE report_runs SET status = $1::report_run_status")
            .bind(status)
            .execute(&pool)
            .await
            .unwrap();
        h.handle(job(a, "blitz"), ctx()).await.unwrap();
    }
    assert_eq!(runs(&pool).await.len(), 1);
    assert_eq!(report_jobs(&pool).await.len(), 1);
    // Another time class is its own first report.
    h.handle(job(a, "rapid"), ctx()).await.unwrap();
    assert_eq!(runs(&pool).await.len(), 2);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn no_first_report_when_a_report_exists(pool: PgPool) {
    let a = linked_account(&pool, "hasone").await;
    let h = wrapped(&pool, true);
    h.handle(job(a, "blitz"), ctx()).await.unwrap();
    let (run, ..) = runs(&pool).await[0].clone();
    // The run finished with a report (for example a computed-only one).
    sqlx::query(
        "INSERT INTO reports (account_id, time_class, version, report_run_id, schema_version, \
           computed, status) VALUES ($1, 'blitz', 1, $2, 'report-payload-v1', '{}', 'computed_only')",
    )
    .bind(a)
    .bind(run)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE report_runs SET status = 'done'")
        .execute(&pool)
        .await
        .unwrap();
    h.handle(job(a, "blitz"), ctx()).await.unwrap();
    assert_eq!(runs(&pool).await.len(), 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_failed_first_run_is_retried_only_after_a_day(pool: PgPool) {
    let a = linked_account(&pool, "failed").await;
    let h = wrapped(&pool, true);
    h.handle(job(a, "blitz"), ctx()).await.unwrap();
    sqlx::query("UPDATE report_runs SET status = 'failed', finished_at = now()")
        .execute(&pool)
        .await
        .unwrap();
    // A failed run (no input yet, say) is not re-requested on every build.
    h.handle(job(a, "blitz"), ctx()).await.unwrap();
    assert_eq!(runs(&pool).await.len(), 1);
    sqlx::query(
        "UPDATE report_runs SET created_at = created_at - $1, finished_at = finished_at - $1",
    )
    .bind(FIRST_REPORT_RETRY_AFTER + time::Duration::minutes(1))
    .execute(&pool)
    .await
    .unwrap();
    h.handle(job(a, "blitz"), ctx()).await.unwrap();
    let r = runs(&pool).await;
    assert_eq!(r.len(), 2);
    assert_eq!(r[1].3, "queued");
    assert_eq!(FIRST_REPORT_RETRY_AFTER, time::Duration::days(1));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn an_unlinked_account_gets_no_report(pool: PgPool) {
    let a = testkit::insert_account(&pool, "lichess", "nobody").await;
    wrapped(&pool, true)
        .handle(job(a, "blitz"), ctx())
        .await
        .unwrap();
    assert!(runs(&pool).await.is_empty());
    assert!(report_jobs(&pool).await.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_failed_build_requests_nothing(pool: PgPool) {
    let a = linked_account(&pool, "broken").await;
    let res = wrapped(&pool, false).handle(job(a, "blitz"), ctx()).await;
    assert!(matches!(res, Err(JobError::Retryable { .. })), "{res:?}");
    assert!(runs(&pool).await.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn concurrent_builds_request_one_first_report(pool: PgPool) {
    let a = linked_account(&pool, "racer").await;
    let queue = Queue::new(pool.clone());
    let got = futures::future::join_all(
        (0..6).map(|_| request_first_report(&pool, &queue, a, TimeClass::Blitz, Plan::Beta)),
    )
    .await;
    let made: Vec<Uuid> = got.into_iter().filter_map(|r| r.unwrap()).collect();
    assert_eq!(made.len(), 1);
    assert_eq!(runs(&pool).await.len(), 1);
    assert_eq!(report_jobs(&pool).await.len(), 1);
}

// ------------------------------------------------------------------ G2: the weekly provider

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn the_weekly_provider_has_nothing_due_without_analysed_games(pool: PgPool) {
    let a = linked_account(&pool, "weekly").await;
    // testkit plays every game on Thursday 2026-01-01, in the week of Monday 2025-12-29. The
    // game is imported but has no metrics yet.
    testkit::insert_game(&pool, a, testkit::GameSeed::default()).await;
    let p = ProfileWeeklyProvider { pool: pool.clone() };
    let due: Vec<_> = p.due(date!(2025 - 12 - 29)).try_collect().await.unwrap();
    assert!(due.is_empty(), "{due:?}");
    let due: Vec<_> = p.due(date!(2026 - 11 - 16)).try_collect().await.unwrap();
    assert!(due.is_empty());
}
