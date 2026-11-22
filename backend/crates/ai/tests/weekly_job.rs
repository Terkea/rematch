//! Task 19: the `weekly-review` job. One review per (account, time class, week), written on the
//! lighter model by Task 15's generator, against the T12 fake client only (no network, no key).

#[path = "support/item_jobs.rs"]
mod support;

use ai::client::port::{AiError, UnavailableReason};
use ai::input::types::TimeClass;
use ai::jobs::kinds::{KIND_WEEKLY_REVIEW, weekly_key};
use ai::jobs::retry::RETRY_DELAYS;
use ai::jobs::weekly_job::{WeeklyReviewHandler, request_weekly_review};
use ai::output::weekly::WeeklyReviewInput;
use ai::testing::load_fixture;
use queue::runner::{Handler, JobContext};
use queue::types::JobError;
use serde_json::{Value, json};
use sqlx::PgPool;
use time::{Date, Month, OffsetDateTime};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use support::{account, deps, events, fake, job, jobs, structured_calls, usage_rows};

const GOOD_OUTCOME: &str =
    "Whether the work on hanging pieces shows in your results is unclear yet.";

fn week_start() -> Date {
    Date::from_calendar_date(2026, Month::November, 16).unwrap()
}

fn weekly() -> WeeklyReviewInput {
    let b2 = load_fixture("b2-rapid");
    serde_json::from_value(json!({
        "inputVersion": "weekly-input-v1",
        "timeClass": b2["timeClass"],
        "band": b2["band"],
        "weekStart": "2026-11-16",
        "weekEnd": "2026-11-22",
        "gamesThisWeek": 14,
        "basis": b2["basis"],
        "adherence": { "minutesDone": 90, "minutesPlanned": 120 },
        "metricDeltas": [{
            "key": "blunder_rate", "area": "tactics", "label": "Blunders",
            "before": { "value": 0.2, "n": 40, "low": 0.15, "high": 0.25 },
            "after": { "value": 0.1, "n": 14, "low": null, "high": null },
            "verdict": "no_clear_change"
        }],
        "focusUpdates": [{ "weaknessId": "W1", "label": "Hanging pieces", "pImp": 0.6, "state": "active" }],
        "exemplars": [b2["weaknesses"][0]["exemplars"][0]],
        "changes": b2["changes"],
    }))
    .unwrap()
}

fn item(text: &str) -> Value {
    json!({ "text": text, "citations": [] })
}

/// A weekly review where every item passes.
fn review() -> Value {
    json!({
        "headline": item("A steady week of practice."),
        "process": item("You put in {{num:adherence.minutesDone|int}} minutes of practice this week."),
        "outcomes": [{ "text": GOOD_OUTCOME, "citations": [], "weaknessId": "W1" }],
        "exemplars": [{
            "text": "In this game {{move:li-w1-0:5}} left a piece loose.",
            "citations": [{ "gameId": "li-w1-0", "ply": 5 }],
        }],
        "nextWeek": item("Next week, look for loose pieces before you move."),
    })
}

fn ctx() -> JobContext {
    JobContext {
        cancel: CancellationToken::new(),
    }
}

fn weekly_job(id: Uuid) -> queue::types::Job {
    job(KIND_WEEKLY_REVIEW, json!({ "weeklyReviewId": id }))
}

async fn request(pool: &PgPool, a: Uuid) -> Option<Uuid> {
    let q = queue::queue::Queue::new(pool.clone());
    request_weekly_review(pool, &q, a, TimeClass::Rapid, week_start(), &weekly())
        .await
        .unwrap()
}

#[derive(Debug, sqlx::FromRow)]
struct Row {
    status: String,
    content: Option<Value>,
    dropped: Value,
    model: Option<String>,
    prompt_hash: Option<String>,
    retry_count: i32,
    next_attempt_at: Option<OffsetDateTime>,
    ai_disclosure: Option<Value>,
}

async fn row(pool: &PgPool, id: Uuid) -> Row {
    sqlx::query_as(
        "SELECT status::text AS status, content, dropped, model, prompt_hash, retry_count, \
         next_attempt_at, ai_disclosure FROM weekly_reviews WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn weekly_run_writes_a_haiku_row_and_an_event(pool: PgPool) {
    let a = account(&pool, "wk1").await;
    let id = request(&pool, a).await.expect("a new review");
    let queued = jobs(&pool).await;
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].0, KIND_WEEKLY_REVIEW);
    assert_eq!(queued[0].1.as_deref(), Some(weekly_key(id).as_str()));
    assert_eq!(queued[0].2, json!({ "weeklyReviewId": id }));

    let ai = fake(|_| Ok(review()));
    let h = WeeklyReviewHandler(deps(&pool, Some(ai.clone())));
    assert_eq!(h.kind(), KIND_WEEKLY_REVIEW);
    h.handle(weekly_job(id), ctx()).await.unwrap();

    assert_eq!(structured_calls(&ai), 1);
    let r = row(&pool, id).await;
    assert_eq!(r.status, "ready");
    assert_eq!(r.model.as_deref(), Some("claude-haiku-5-5"));
    assert!(r.prompt_hash.is_some());
    let content = r.content.expect("content");
    assert_eq!(content["headline"]["text"], "A steady week of practice.");
    assert_eq!(content["outcomes"][0]["weaknessId"], "W1");
    assert_eq!(r.dropped, json!([]));
    let disclosure = r.ai_disclosure.expect("disclosure");
    assert_eq!(disclosure["model"], "claude-haiku-5-5");
    assert_eq!(disclosure["promptVersion"], "weekly@1");
    assert_eq!(
        usage_rows(&pool, a).await,
        vec![(
            "weekly_review".to_owned(),
            "claude-haiku-5-5".to_owned(),
            Some(id.to_string())
        )]
    );
    assert_eq!(
        events(&pool, a).await,
        vec![("weekly_review.ready".to_owned(), json!({ "id": id }))]
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn second_request_same_week_is_none_with_zero_calls(pool: PgPool) {
    let a = account(&pool, "wk2").await;
    let id = request(&pool, a).await.expect("a new review");
    assert_eq!(request(&pool, a).await, None);
    assert_eq!(jobs(&pool).await.len(), 1, "no second job");

    let ai = fake(|_| Ok(review()));
    let h = WeeklyReviewHandler(deps(&pool, Some(ai.clone())));
    h.handle(weekly_job(id), ctx()).await.unwrap();
    assert_eq!(structured_calls(&ai), 1);

    // After it is ready: a second request is still None, and a redelivered job is skipped
    // with zero calls, no metering and no event.
    assert_eq!(request(&pool, a).await, None);
    let v = h.handle(weekly_job(id), ctx()).await.unwrap();
    assert_eq!(v["skipped"], "ready");
    assert_eq!(structured_calls(&ai), 1);
    assert_eq!(usage_rows(&pool, a).await.len(), 1);
    assert_eq!(events(&pool, a).await.len(), 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn request_rejects_an_input_for_another_week(pool: PgPool) {
    let a = account(&pool, "wk2b").await;
    let q = queue::queue::Queue::new(pool.clone());
    let other = Date::from_calendar_date(2026, Month::November, 9).unwrap();
    let r = request_weekly_review(&pool, &q, a, TimeClass::Rapid, other, &weekly()).await;
    assert!(r.is_err());
    let r = request_weekly_review(&pool, &q, a, TimeClass::Blitz, week_start(), &weekly()).await;
    assert!(r.is_err());
    assert!(jobs(&pool).await.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn weekly_unavailable_releases_and_keeps_no_content(pool: PgPool) {
    let a = account(&pool, "wk3").await;
    let id = request(&pool, a).await.unwrap();
    let ai = fake(|_| {
        Err(AiError::Unavailable {
            reason: UnavailableReason::Overloaded,
            retry_after: None,
        })
    });
    let h = WeeklyReviewHandler(deps(&pool, Some(ai.clone())));
    let before = OffsetDateTime::now_utc();
    let e = h.handle(weekly_job(id), ctx()).await.unwrap_err();
    match e {
        JobError::Release { delay, .. } => assert_eq!(delay, RETRY_DELAYS[0]),
        other => panic!("expected a release, got {other:?}"),
    }
    let r = row(&pool, id).await;
    assert_eq!(r.status, "pending");
    assert_eq!(r.content, None);
    assert_eq!(r.dropped, json!([]));
    assert_eq!(r.retry_count, 1);
    let next = r.next_attempt_at.expect("next attempt");
    let d = next - before;
    assert!(
        d >= time::Duration::seconds(299) && d <= time::Duration::seconds(320),
        "{d}"
    );
    assert!(events(&pool, a).await.is_empty());

    // The second retry uses the second delay.
    let e = h.handle(weekly_job(id), ctx()).await.unwrap_err();
    assert!(matches!(e, JobError::Release { delay, .. } if delay == RETRY_DELAYS[1]));
    assert_eq!(row(&pool, id).await.retry_count, 2);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn weekly_retries_exhausted_fails_with_an_event(pool: PgPool) {
    let a = account(&pool, "wk4").await;
    let id = request(&pool, a).await.unwrap();
    sqlx::query("UPDATE weekly_reviews SET retry_count = 5 WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let ai = fake(|_| {
        Err(AiError::Unavailable {
            reason: UnavailableReason::Overloaded,
            retry_after: None,
        })
    });
    let h = WeeklyReviewHandler(deps(&pool, Some(ai)));
    h.handle(weekly_job(id), ctx()).await.unwrap();
    let r = row(&pool, id).await;
    assert_eq!(r.status, "failed");
    assert_eq!(r.content, None);
    assert_eq!(
        events(&pool, a).await,
        vec![(
            "job.failed".to_owned(),
            json!({ "kind": "weekly-review", "reason": "ai_unavailable" })
        )]
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn weekly_without_ai_fails_with_ai_disabled(pool: PgPool) {
    let a = account(&pool, "wk5").await;
    let id = request(&pool, a).await.unwrap();
    let h = WeeklyReviewHandler(deps(&pool, None));
    h.handle(weekly_job(id), ctx()).await.unwrap();
    assert_eq!(row(&pool, id).await.status, "failed");
    assert_eq!(
        events(&pool, a).await,
        vec![(
            "job.failed".to_owned(),
            json!({ "kind": "weekly-review", "reason": "ai_disabled" })
        )]
    );
    assert!(usage_rows(&pool, a).await.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_paid_first_call_is_metered_when_the_repair_is_unavailable(pool: PgPool) {
    let a = account(&pool, "wk6").await;
    let id = request(&pool, a).await.unwrap();
    // Attempt 1 is unusable (a schema failure), so a repair is needed; the repair is unavailable.
    let ai = fake(|n| {
        if n == 1 {
            Ok(json!({ "nope": true }))
        } else {
            Err(AiError::Unavailable {
                reason: UnavailableReason::Overloaded,
                retry_after: None,
            })
        }
    });
    let h = WeeklyReviewHandler(deps(&pool, Some(ai.clone())));
    let e = h.handle(weekly_job(id), ctx()).await.unwrap_err();
    assert!(matches!(e, JobError::Release { .. }), "{e:?}");
    assert_eq!(structured_calls(&ai), 2);
    assert_eq!(usage_rows(&pool, a).await.len(), 1, "the paid first call");
    assert_eq!(row(&pool, id).await.content, None);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn budget_spent_fails_the_review_with_no_call(pool: PgPool) {
    let a = account(&pool, "wk7").await;
    let id = request(&pool, a).await.unwrap();
    sqlx::query(
        "INSERT INTO ai_usage (account_id, feature, model, batch, input_tokens, output_tokens, \
         cache_creation_tokens, cache_read_tokens, cost_micro_usd) \
         VALUES ($1, 'report', 'claude-sonnet-5-5', false, 0, 0, 0, 0, 1000000000)",
    )
    .bind(a)
    .execute(&pool)
    .await
    .unwrap();
    let ai = fake(|_| Ok(review()));
    let h = WeeklyReviewHandler(deps(&pool, Some(ai.clone())));
    h.handle(weekly_job(id), ctx()).await.unwrap();
    assert_eq!(structured_calls(&ai), 0);
    assert_eq!(row(&pool, id).await.status, "failed");
    assert_eq!(
        events(&pool, a).await,
        vec![(
            "job.failed".to_owned(),
            json!({ "kind": "weekly-review", "reason": "budget" })
        )]
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn stored_dropped_items_carry_no_model_text(pool: PgPool) {
    let a = account(&pool, "wk8").await;
    let id = request(&pool, a).await.unwrap();
    // The headline makes a marketing claim on both attempts; the rest is fine.
    let ai = fake(|_| {
        let mut r = review();
        r["headline"] = item("Upgrade to premium for your weekly plan.");
        Ok(r)
    });
    let h = WeeklyReviewHandler(deps(&pool, Some(ai.clone())));
    h.handle(weekly_job(id), ctx()).await.unwrap();
    let r = row(&pool, id).await;
    assert_eq!(r.status, "ready");
    let content = r.content.unwrap();
    assert_eq!(content["headline"], Value::Null);
    let dropped = r.dropped.to_string();
    assert!(dropped.contains("headline"), "{dropped}");
    assert!(!dropped.contains("detail"), "{dropped}");
    assert!(!dropped.contains("premium"), "{dropped}");
    assert_eq!(usage_rows(&pool, a).await.len(), 2, "both paid calls");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn nothing_usable_is_stored_as_dropped_with_an_event(pool: PgPool) {
    let a = account(&pool, "wk9").await;
    let id = request(&pool, a).await.unwrap();
    let ai = fake(|_| Ok(json!({ "nope": true })));
    let h = WeeklyReviewHandler(deps(&pool, Some(ai.clone())));
    h.handle(weekly_job(id), ctx()).await.unwrap();
    let r = row(&pool, id).await;
    assert_eq!(r.status, "dropped");
    assert_eq!(r.content, None);
    assert_ne!(r.dropped, json!([]));
    assert_eq!(usage_rows(&pool, a).await.len(), 2);
    assert_eq!(
        events(&pool, a).await,
        vec![("weekly_review.ready".to_owned(), json!({ "id": id }))]
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_batch_mode_review_is_accepted_and_a_missing_one_skipped(pool: PgPool) {
    let a = account(&pool, "wk10").await;
    let id = request(&pool, a).await.unwrap();
    sqlx::query("UPDATE weekly_reviews SET mode = 'batch' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let ai = fake(|_| Ok(review()));
    let h = WeeklyReviewHandler(deps(&pool, Some(ai.clone())));
    h.handle(weekly_job(id), ctx()).await.unwrap();
    assert_eq!(row(&pool, id).await.status, "ready");

    let v = h.handle(weekly_job(Uuid::new_v4()), ctx()).await.unwrap();
    assert_eq!(v["skipped"], "missing");
    assert_eq!(structured_calls(&ai), 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_dead_job_fails_its_review(pool: PgPool) {
    let a = account(&pool, "wk11").await;
    let id = request(&pool, a).await.unwrap();
    let h = WeeklyReviewHandler(deps(&pool, None));
    h.on_dead(&weekly_job(id), "boom").await;
    assert_eq!(row(&pool, id).await.status, "failed");
    assert_eq!(
        events(&pool, a).await,
        vec![(
            "job.failed".to_owned(),
            json!({ "kind": "weekly-review", "reason": "job_dead" })
        )]
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_cancelled_review_is_released_pending_and_meters_the_paid_call(pool: PgPool) {
    let a = account(&pool, "wk12").await;
    let id = request(&pool, a).await.unwrap();
    // Attempt 1 is paid but unusable; the repair is still in flight when the job is cancelled.
    let inner = fake(|_| Ok(json!({ "nope": true })));
    let h = WeeklyReviewHandler(support::deps_with(&pool, support::Hang::new(inner, 2)));
    let c = ctx();
    let cancel = c.cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        cancel.cancel();
    });
    let e = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        h.handle(weekly_job(id), c),
    )
    .await
    .expect("the handler stops on cancel")
    .unwrap_err();
    assert!(
        matches!(&e, JobError::Release { delay, .. } if delay.is_zero()),
        "{e:?}"
    );
    let r = row(&pool, id).await;
    assert_eq!(r.status, "pending");
    assert_eq!(r.content, None);
    assert_eq!(r.retry_count, 0, "a cancel is not an AI retry");
    assert_eq!(usage_rows(&pool, a).await.len(), 1, "the paid first call");
    assert!(events(&pool, a).await.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn an_already_cancelled_review_makes_no_call(pool: PgPool) {
    let a = account(&pool, "wk13").await;
    let id = request(&pool, a).await.unwrap();
    let ai = fake(|_| Ok(review()));
    let h = WeeklyReviewHandler(deps(&pool, Some(ai.clone())));
    let c = ctx();
    c.cancel.cancel();
    let e = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        h.handle(weekly_job(id), c),
    )
    .await
    .expect("the handler stops on cancel")
    .unwrap_err();
    assert!(matches!(e, JobError::Release { .. }), "{e:?}");
    assert_eq!(structured_calls(&ai), 0);
    assert_eq!(row(&pool, id).await.status, "pending");
}

/// Odd calls are paid but unusable; even calls (the repairs) are unavailable: every delivery
/// pays exactly one call and then releases.
fn paid_then_unavailable() -> std::sync::Arc<ai::client::fake::FakeAiClient> {
    fake(|n| {
        if n % 2 == 1 {
            Ok(json!({ "nope": true }))
        } else {
            Err(AiError::Unavailable {
                reason: UnavailableReason::Overloaded,
                retry_after: None,
            })
        }
    })
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn each_delivery_meters_its_own_paid_call(pool: PgPool) {
    let a = account(&pool, "wk14").await;
    let id = request(&pool, a).await.unwrap();
    let h = WeeklyReviewHandler(deps(&pool, Some(paid_then_unavailable())));
    // Two deliveries (each with its own lease token), each paying one call.
    for _ in 0..2 {
        let e = h.handle(weekly_job(id), ctx()).await.unwrap_err();
        assert!(matches!(e, JobError::Release { .. }), "{e:?}");
    }
    assert_eq!(usage_rows(&pool, a).await.len(), 2, "one row per paid call");
    assert_eq!(row(&pool, id).await.retry_count, 2);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_delivery_metered_twice_is_counted_once(pool: PgPool) {
    let a = account(&pool, "wk15").await;
    let id = request(&pool, a).await.unwrap();
    let h = WeeklyReviewHandler(deps(&pool, Some(paid_then_unavailable())));
    // The same delivery (one lease token) run twice: its metering key repeats, so the second
    // write of call 0 adds nothing.
    let j = weekly_job(id);
    for _ in 0..2 {
        h.handle(j.clone(), ctx()).await.unwrap_err();
    }
    assert_eq!(usage_rows(&pool, a).await.len(), 1);
}
