//! Task 19: the `explain-weakness-change` job. One note per change the R2 timeline detected,
//! written on the lighter model by Task 15's generator, against the T12 fake client only.

#[path = "support/item_jobs.rs"]
mod support;

use ai::client::port::{AiError, UnavailableReason};
use ai::input::types::CoachingInput;
use ai::jobs::change_job::{ExplainChangeHandler, enqueue_missing_change_notes};
use ai::jobs::kinds::{KIND_EXPLAIN_CHANGE, change_key as job_key};
use ai::jobs::retry::RETRY_DELAYS;
use ai::output::change::WeaknessChangeInput;
use ai::testing::make_input;
use queue::queue::Queue;
use queue::runner::{Handler, JobContext};
use queue::types::JobError;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use support::{account, deps, events, fake, job, jobs, structured_calls, usage_rows};

/// b2-rapid's first change: `tactics.blunder_rate`, onset at li-c1-2.
const TACTICS: &str = "tactics.blunder_rate@li-c1-2";
const FLAG: &str = "time.flag_rate@li-c2a";
const K: &str = "tactics_blunder_rate";

fn input(a: Uuid) -> CoachingInput {
    serde_json::from_value(make_input(json!({ "accountId": a }))).unwrap()
}

fn hedged_text() -> String {
    format!(
        "Since about {{{{date:changes.{K}.onsetDate}}}} you may be missing back-rank threats about {{{{num:changes.{K}.ratioAfterBefore|dec1}}}} times as often, around the time you switched to blitz."
    )
}

fn note(text: &str) -> Value {
    json!({
        "text": text,
        "citations": [{ "gameId": "li-c1-0", "ply": 21 }, { "gameId": "li-c1-2", "ply": 25 }],
    })
}

fn ctx() -> JobContext {
    JobContext {
        cancel: CancellationToken::new(),
    }
}

fn note_job(id: Uuid) -> queue::types::Job {
    job(KIND_EXPLAIN_CHANGE, json!({ "noteId": id }))
}

async fn enqueue(pool: &PgPool, a: Uuid) -> u32 {
    enqueue_missing_change_notes(pool, &Queue::new(pool.clone()), a, &input(a))
        .await
        .unwrap()
}

#[derive(Debug, sqlx::FromRow)]
struct Row {
    id: Uuid,
    status: String,
    input: Value,
    text: Option<String>,
    segments: Option<Value>,
    citations: Value,
    model: Option<String>,
    retry_count: i32,
    ai_disclosure: Option<Value>,
}

async fn row(pool: &PgPool, a: Uuid, key: &str) -> Row {
    sqlx::query_as(
        "SELECT id, status::text AS status, input, text, segments, citations, model, \
         retry_count, ai_disclosure FROM weakness_change_notes \
         WHERE account_id = $1 AND change_key = $2",
    )
    .bind(a)
    .bind(key)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn enqueue_missing_change_notes_is_idempotent(pool: PgPool) {
    let a = account(&pool, "ch1").await;
    assert_eq!(enqueue(&pool, a).await, 2);
    assert_eq!(enqueue(&pool, a).await, 0);

    let inp = input(a);
    let tactics = row(&pool, a, TACTICS).await;
    let flag = row(&pool, a, FLAG).await;
    assert_eq!(tactics.status, "pending");
    let stored: WeaknessChangeInput = serde_json::from_value(tactics.input).unwrap();
    assert_eq!(stored.change, inp.changes[0]);
    assert_eq!(stored.band, inp.band);
    assert_eq!(stored.time_class, inp.time_class);
    assert_eq!(stored.basis, inp.basis);

    let queued = jobs(&pool).await;
    assert_eq!(queued.len(), 2);
    assert!(queued.iter().all(|j| j.0 == KIND_EXPLAIN_CHANGE));
    let keys: Vec<Option<String>> = queued.iter().map(|j| j.1.clone()).collect();
    assert!(keys.contains(&Some(job_key(tactics.id))));
    assert!(keys.contains(&Some(job_key(flag.id))));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn only_detected_changes_get_notes_and_new_ones_are_added(pool: PgPool) {
    let a = account(&pool, "ch2").await;
    let mut one = input(a);
    one.changes.truncate(1);
    let q = Queue::new(pool.clone());
    assert_eq!(
        enqueue_missing_change_notes(&pool, &q, a, &one)
            .await
            .unwrap(),
        1
    );
    // A later input that also has the second change: only that one is new.
    assert_eq!(enqueue(&pool, a).await, 1);
    let mut none = input(a);
    none.changes.clear();
    assert_eq!(
        enqueue_missing_change_notes(&pool, &q, a, &none)
            .await
            .unwrap(),
        0
    );
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM weakness_change_notes")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 2);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn an_input_of_another_account_is_rejected(pool: PgPool) {
    let a = account(&pool, "ch3").await;
    let b = account(&pool, "ch3b").await;
    let q = Queue::new(pool.clone());
    assert!(
        enqueue_missing_change_notes(&pool, &q, a, &input(b))
            .await
            .is_err()
    );
    assert!(jobs(&pool).await.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn change_note_ready_stores_text_segments_and_citations(pool: PgPool) {
    let a = account(&pool, "ch4").await;
    enqueue(&pool, a).await;
    let id = row(&pool, a, TACTICS).await.id;
    let ai = fake(|_| Ok(note(&hedged_text())));
    let h = ExplainChangeHandler(deps(&pool, Some(ai.clone())));
    assert_eq!(h.kind(), KIND_EXPLAIN_CHANGE);
    h.handle(note_job(id), ctx()).await.unwrap();

    assert_eq!(structured_calls(&ai), 1);
    let r = row(&pool, a, TACTICS).await;
    assert_eq!(r.status, "ready");
    assert_eq!(r.text.as_deref(), Some(hedged_text().as_str()));
    assert_eq!(
        r.citations,
        json!([{ "gameId": "li-c1-0", "ply": 21 }, { "gameId": "li-c1-2", "ply": 25 }])
    );
    let segments = r.segments.expect("segments").to_string();
    assert!(segments.contains("0.6"), "{segments}");
    assert!(!segments.contains("{{"), "{segments}");
    assert_eq!(r.model.as_deref(), Some("claude-haiku-5-5"));
    let disclosure = r.ai_disclosure.expect("disclosure");
    assert_eq!(disclosure["model"], "claude-haiku-5-5");
    assert_eq!(disclosure["promptVersion"], "change@1");
    assert_eq!(
        usage_rows(&pool, a).await,
        vec![(
            "weakness_change".to_owned(),
            "claude-haiku-5-5".to_owned(),
            Some(id.to_string())
        )]
    );
    assert_eq!(
        events(&pool, a).await,
        vec![(
            "weakness.note.ready".to_owned(),
            json!({ "changeKey": TACTICS, "status": "ready" })
        )]
    );

    // A redelivery of the finished note is skipped with no call.
    let v = h.handle(note_job(id), ctx()).await.unwrap();
    assert_eq!(v["skipped"], "ready");
    assert_eq!(structured_calls(&ai), 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn causal_text_twice_is_dropped_with_event_status_dropped(pool: PgPool) {
    let a = account(&pool, "ch5").await;
    enqueue(&pool, a).await;
    let id = row(&pool, a, TACTICS).await.id;
    let causal = format!(
        "Since {{{{date:changes.{K}.onsetDate}}}} you miss back-rank threats because you switched to blitz."
    );
    let ai = fake(move |_| Ok(note(&causal)));
    let h = ExplainChangeHandler(deps(&pool, Some(ai.clone())));
    h.handle(note_job(id), ctx()).await.unwrap();

    assert_eq!(structured_calls(&ai), 2);
    let r = row(&pool, a, TACTICS).await;
    assert_eq!(r.status, "dropped");
    assert_eq!(r.text, None, "nothing unvalidated is stored");
    assert_eq!(r.segments, None);
    assert_eq!(r.citations, json!([]));
    assert_eq!(usage_rows(&pool, a).await.len(), 2, "both paid calls");
    assert_eq!(
        events(&pool, a).await,
        vec![(
            "weakness.note.ready".to_owned(),
            json!({ "changeKey": TACTICS, "status": "dropped" })
        )]
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn change_unavailable_releases_and_keeps_no_text(pool: PgPool) {
    let a = account(&pool, "ch6").await;
    enqueue(&pool, a).await;
    let id = row(&pool, a, TACTICS).await.id;
    let ai = fake(|_| {
        Err(AiError::Unavailable {
            reason: UnavailableReason::RateLimited,
            retry_after: None,
        })
    });
    let h = ExplainChangeHandler(deps(&pool, Some(ai)));
    let e = h.handle(note_job(id), ctx()).await.unwrap_err();
    assert!(
        matches!(e, JobError::Release { delay, .. } if delay == RETRY_DELAYS[0]),
        "{e:?}"
    );
    let r = row(&pool, a, TACTICS).await;
    assert_eq!(r.status, "pending");
    assert_eq!(r.retry_count, 1);
    assert_eq!(r.text, None);
    assert!(events(&pool, a).await.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn change_without_ai_fails_with_ai_disabled(pool: PgPool) {
    let a = account(&pool, "ch7").await;
    enqueue(&pool, a).await;
    let id = row(&pool, a, TACTICS).await.id;
    let h = ExplainChangeHandler(deps(&pool, None));
    h.handle(note_job(id), ctx()).await.unwrap();
    assert_eq!(row(&pool, a, TACTICS).await.status, "failed");
    assert_eq!(
        events(&pool, a).await,
        vec![(
            "job.failed".to_owned(),
            json!({ "kind": "explain-weakness-change", "reason": "ai_disabled" })
        )]
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_note_whose_input_is_not_its_change_is_failed_with_no_call(pool: PgPool) {
    let a = account(&pool, "ch8").await;
    enqueue(&pool, a).await;
    let tactics = row(&pool, a, TACTICS).await;
    let flag = row(&pool, a, FLAG).await;
    // Swap in the other change's input: the key no longer matches what the timeline detected.
    sqlx::query("UPDATE weakness_change_notes SET input = $2 WHERE id = $1")
        .bind(tactics.id)
        .bind(&flag.input)
        .execute(&pool)
        .await
        .unwrap();
    let ai = fake(|_| Ok(note(&hedged_text())));
    let h = ExplainChangeHandler(deps(&pool, Some(ai.clone())));
    h.handle(note_job(tactics.id), ctx()).await.unwrap();
    assert_eq!(structured_calls(&ai), 0);
    assert_eq!(row(&pool, a, TACTICS).await.status, "failed");
    assert_eq!(
        events(&pool, a).await,
        vec![(
            "job.failed".to_owned(),
            json!({ "kind": "explain-weakness-change", "reason": "invalid_input" })
        )]
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_cancelled_note_is_released_pending_and_meters_the_paid_call(pool: PgPool) {
    let a = account(&pool, "ch9").await;
    enqueue(&pool, a).await;
    let id = row(&pool, a, TACTICS).await.id;
    let inner = fake(|_| Ok(json!({ "nope": true })));
    let h = ExplainChangeHandler(support::deps_with(&pool, support::Hang::new(inner, 2)));
    let c = ctx();
    let cancel = c.cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        cancel.cancel();
    });
    let e = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        h.handle(note_job(id), c),
    )
    .await
    .expect("the handler stops on cancel")
    .unwrap_err();
    assert!(
        matches!(&e, JobError::Release { delay, .. } if delay.is_zero()),
        "{e:?}"
    );
    let r = row(&pool, a, TACTICS).await;
    assert_eq!(r.status, "pending");
    assert_eq!(r.text, None);
    assert_eq!(r.retry_count, 0);
    assert_eq!(usage_rows(&pool, a).await.len(), 1, "the paid first call");
    assert!(events(&pool, a).await.is_empty());
}
