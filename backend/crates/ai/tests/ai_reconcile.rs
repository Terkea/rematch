//! Task 21: the `ai-reconcile` sweep (ADR 0059). One test per sweep: a seeded row with no job is
//! repaired once (the job then exists and the count is 1), plus the negative cases, paging,
//! the orphan session sweep and the plan guards of every sweep page.

use std::time::Duration;

use ai::db::ItemStatus;
use ai::db::batches::{self, BatchKind, NewBatchItem};
use ai::db::chat::{self, ChatRole, ChatStatus, NewConversation, NewMessage};
use ai::db::interactive::{self, EvalStatus, NewEval};
use ai::db::notes::{self, NewNote};
use ai::db::runs::{self, NewRun, ReportMode, ReportTrigger, RunStatus};
use ai::db::weekly::{self, NewWeekly};
use ai::entitlements::Plan;
use ai::jobs::kinds::{self, enqueue_chat, enqueue_interactive, enqueue_report};
use ai::jobs::reconcile::{self, AiReconcileCounts, AiReconcileHandler, reconcile_ai};
use queue::queue::Queue;
use queue::runner::{Handler, JobContext};
use queue::{Job, JobState};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

async fn account(pool: &PgPool, name: &str) -> Uuid {
    testkit::insert_account(pool, "chesscom", name).await
}

fn new_run(account_id: Uuid) -> NewRun {
    NewRun {
        account_id,
        time_class: "blitz".into(),
        trigger: ReportTrigger::Manual,
        plan: Plan::Pro,
        mode: ReportMode::Sync,
        prompt_version: "report-v1".into(),
        is_first_report: true,
        ai_disabled_reason: None,
    }
}

async fn insert_run(pool: &PgPool, account_id: Uuid) -> Uuid {
    let mut conn = pool.acquire().await.unwrap();
    runs::insert_run(&mut conn, &new_run(account_id))
        .await
        .unwrap()
}

/// Moves `col` of one row `secs` seconds into the past.
async fn backdate(pool: &PgPool, table: &str, col: &str, id: Uuid, secs: i64) {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE {table} SET {col} = now() - make_interval(secs => $2) WHERE id = $1"
    )))
    .bind(id)
    .bind(secs as f64)
    .execute(pool)
    .await
    .unwrap();
}

async fn run_status(pool: &PgPool, id: Uuid) -> RunStatus {
    let mut conn = pool.acquire().await.unwrap();
    runs::get_run(&mut conn, id).await.unwrap().unwrap().status
}

async fn live(queue: &Queue, kind: &str, key: String) -> Option<JobState> {
    queue
        .live_states(kind, std::slice::from_ref(&key))
        .await
        .unwrap()
        .get(&key)
        .copied()
}

async fn job_count(pool: &PgPool, kind: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = $1")
        .bind(kind)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn sweep(pool: &PgPool, queue: &Queue) -> AiReconcileCounts {
    reconcile_ai(pool, queue, OffsetDateTime::now_utc())
        .await
        .unwrap()
}

async fn conversation(pool: &PgPool, account_id: Uuid) -> Uuid {
    let user_id = testkit::insert_user(pool, "owner").await;
    let mut conn = pool.acquire().await.unwrap();
    chat::create_conversation(
        &mut conn,
        &NewConversation {
            account_id,
            user_id,
            time_class: "blitz".into(),
            title: None,
            anchor: None,
        },
    )
    .await
    .unwrap()
    .id
}

async fn pending_assistant(pool: &PgPool, conversation_id: Uuid, seq: i32) -> Uuid {
    let mut conn = pool.acquire().await.unwrap();
    chat::insert_message(
        &mut conn,
        &NewMessage {
            conversation_id,
            seq,
            role: ChatRole::Assistant,
            status: ChatStatus::Pending,
            anchor: None,
            text: None,
        },
    )
    .await
    .unwrap()
    .id
}

async fn insert_eval(pool: &PgPool, account_id: Uuid) -> Uuid {
    let mut conn = pool.acquire().await.unwrap();
    interactive::insert_eval(
        &mut conn,
        &NewEval {
            account_id,
            message_id: None,
            protocol: "sf-test".into(),
            base_fen: "startpos".into(),
            moves: vec!["e2e4".into()],
            fen: "fen-after".into(),
        },
    )
    .await
    .unwrap()
    .id
}

async fn insert_weekly(pool: &PgPool, account_id: Uuid, mode: ReportMode) -> Uuid {
    let mut conn = pool.acquire().await.unwrap();
    let id = weekly::insert_weekly_if_absent(
        &mut conn,
        &NewWeekly {
            account_id,
            time_class: "blitz".into(),
            week_start: time::macros::date!(2026 - 11 - 16),
            mode,
            input: json!({}),
        },
    )
    .await
    .unwrap()
    .unwrap()
    .id;
    // Past the 5 min enqueue grace.
    backdate(pool, "weekly_reviews", "created_at", id, 6 * 60).await;
    id
}

async fn insert_note(pool: &PgPool, account_id: Uuid) -> Uuid {
    let mut conn = pool.acquire().await.unwrap();
    let id = notes::insert_note_if_absent(
        &mut conn,
        &NewNote {
            account_id,
            change_key: "ck".into(),
            series_key: "sk".into(),
            onset_date: None,
            input: json!({}),
        },
    )
    .await
    .unwrap()
    .unwrap()
    .id;
    backdate(pool, "weakness_change_notes", "created_at", id, 6 * 60).await;
    id
}

#[test]
fn sweep_thresholds_match_the_brief() {
    assert_eq!(reconcile::QUEUED_GRACE, Duration::from_secs(300));
    assert_eq!(reconcile::STALE_HEARTBEAT, Duration::from_secs(600));
    assert_eq!(reconcile::RETRY_OVERDUE, Duration::from_secs(300));
    assert_eq!(reconcile::CHAT_LOST_AFTER, Duration::from_secs(120));
    assert_eq!(reconcile::EVAL_LOST_AFTER, Duration::from_secs(120));
    assert_eq!(reconcile::LOST_REASON, "lost");
    assert_eq!(kinds::AI_RECONCILE_EVERY, Duration::from_secs(300));
    assert_eq!(
        reconcile::ORPHAN_SESSION_GRACE,
        chat::ORPHAN_SESSION_MIN_AGE
    );
    assert_eq!(reconcile::ORPHAN_SESSION_GRACE, Duration::from_secs(600));
    // Fix round 1.
    assert_eq!(reconcile::RECONCILE_CAP, 3);
    assert_eq!(reconcile::RECONCILE_CAP_ERROR, "reconcile_cap");
    assert_eq!(reconcile::PENDING_GRACE, Duration::from_secs(300));
    assert_eq!(reconcile::BATCH_SUBMIT_GRACE, Duration::from_secs(30 * 60));
    assert_eq!(kinds::CHAT_TURN_TIMEOUT, Duration::from_secs(600));
    assert_eq!(
        reconcile::STREAMING_LOST_AFTER,
        kinds::CHAT_TURN_TIMEOUT + Duration::from_secs(90)
    );
    assert_eq!(chat::ORPHAN_SESSION_MAX_AGE, Duration::from_secs(24 * 3600));
    assert_eq!(chat::ORPHAN_SWEEP_PAGE, 1000);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn queued_run_without_a_job_is_enqueued(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "queued").await;
    let old = insert_run(&pool, acct).await;
    backdate(&pool, "report_runs", "created_at", old, 6 * 60).await;
    let fresh = insert_run(&pool, acct).await;
    backdate(&pool, "report_runs", "created_at", fresh, 60).await;

    let counts = sweep(&pool, &queue).await;
    assert_eq!(counts.reports, 1, "{counts:?}");
    assert_eq!(
        live(&queue, kinds::KIND_GENERATE_REPORT, kinds::report_key(old)).await,
        Some(JobState::Queued)
    );
    assert_eq!(
        live(
            &queue,
            kinds::KIND_GENERATE_REPORT,
            kinds::report_key(fresh)
        )
        .await,
        None,
        "a run inside the 5 min grace may still have its enqueue in flight"
    );
    assert_eq!(run_status(&pool, old).await, RunStatus::Queued);

    // The job now exists: the next sweep has nothing to do.
    assert_eq!(sweep(&pool, &queue).await.reports, 0);
    assert_eq!(job_count(&pool, kinds::KIND_GENERATE_REPORT).await, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn stale_running_run_is_requeued(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "stale").await;
    let id = insert_run(&pool, acct).await;
    let mut conn = pool.acquire().await.unwrap();
    runs::claim_run(&mut conn, id).await.unwrap().unwrap();
    drop(conn);
    backdate(&pool, "report_runs", "heartbeat_at", id, 11 * 60).await;

    let counts = sweep(&pool, &queue).await;
    assert_eq!(counts.reports, 1, "{counts:?}");
    assert_eq!(run_status(&pool, id).await, RunStatus::Queued);
    assert_eq!(
        live(&queue, kinds::KIND_GENERATE_REPORT, kinds::report_key(id)).await,
        Some(JobState::Queued)
    );
    // The requeued job can claim the run again.
    let mut conn = pool.acquire().await.unwrap();
    assert!(runs::claim_run(&mut conn, id).await.unwrap().is_some());
    drop(conn);
    assert_eq!(sweep(&pool, &queue).await.reports, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn fresh_running_run_is_untouched(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "fresh").await;
    let id = insert_run(&pool, acct).await;
    backdate(&pool, "report_runs", "created_at", id, 3600).await;
    let mut conn = pool.acquire().await.unwrap();
    runs::claim_run(&mut conn, id).await.unwrap().unwrap();
    drop(conn);
    backdate(&pool, "report_runs", "heartbeat_at", id, 60).await;

    let counts = sweep(&pool, &queue).await;
    assert_eq!(counts.reports, 0, "{counts:?}");
    assert_eq!(run_status(&pool, id).await, RunStatus::Running);
    assert_eq!(job_count(&pool, kinds::KIND_GENERATE_REPORT).await, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn overdue_retry_run_is_enqueued(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "retry").await;
    let mut ids = Vec::new();
    for overdue_secs in [6 * 60, 60] {
        let id = insert_run(&pool, acct).await;
        let mut conn = pool.acquire().await.unwrap();
        runs::claim_run(&mut conn, id).await.unwrap().unwrap();
        let at = OffsetDateTime::now_utc() - time::Duration::seconds(overdue_secs);
        assert!(
            runs::set_awaiting_retry(&mut conn, id, 1, at, "rate limited")
                .await
                .unwrap()
        );
        ids.push(id);
    }
    let (overdue, recent) = (ids[0], ids[1]);

    let counts = sweep(&pool, &queue).await;
    assert_eq!(counts.reports, 1, "{counts:?}");
    assert_eq!(
        live(
            &queue,
            kinds::KIND_GENERATE_REPORT,
            kinds::report_key(overdue)
        )
        .await,
        Some(JobState::Queued)
    );
    assert_eq!(
        live(
            &queue,
            kinds::KIND_GENERATE_REPORT,
            kinds::report_key(recent)
        )
        .await,
        None,
        "a retry due less than 5 min ago is left to its released job"
    );
    assert_eq!(run_status(&pool, overdue).await, RunStatus::AwaitingRetry);
    assert_eq!(sweep(&pool, &queue).await.reports, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn awaiting_batch_and_finished_runs_are_left_to_their_owners(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "owners").await;
    let batch = insert_run(&pool, acct).await;
    let done = insert_run(&pool, acct).await;
    let mut conn = pool.acquire().await.unwrap();
    assert!(runs::set_awaiting_batch(&mut conn, batch).await.unwrap());
    assert!(runs::set_done(&mut conn, done, None).await.unwrap());
    drop(conn);
    for id in [batch, done] {
        backdate(&pool, "report_runs", "created_at", id, 3600).await;
    }
    assert_eq!(sweep(&pool, &queue).await.reports, 0);
    assert_eq!(job_count(&pool, kinds::KIND_GENERATE_REPORT).await, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn pending_weekly_review_without_a_job_is_enqueued(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "weekly").await;
    let id = insert_weekly(&pool, acct, ReportMode::Sync).await;

    let counts = sweep(&pool, &queue).await;
    assert_eq!(counts.weekly, 1, "{counts:?}");
    assert_eq!(
        live(&queue, kinds::KIND_WEEKLY_REVIEW, kinds::weekly_key(id)).await,
        Some(JobState::Queued)
    );
    assert_eq!(sweep(&pool, &queue).await.weekly, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn weekly_review_waiting_for_a_retry_or_an_open_batch_is_skipped(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let a = account(&pool, "wait-retry").await;
    let b = account(&pool, "wait-batch").await;
    let c = account(&pool, "closed-batch").await;
    let later = insert_weekly(&pool, a, ReportMode::Sync).await;
    let mut conn = pool.acquire().await.unwrap();
    let at = OffsetDateTime::now_utc() + time::Duration::minutes(30);
    assert!(
        weekly::set_weekly_retry(&mut conn, later, 1, at)
            .await
            .unwrap()
    );
    drop(conn);

    // In an open batch: the batch poller finishes it.
    let in_batch = insert_weekly(&pool, b, ReportMode::Batch).await;
    // Its batch failed: nothing else will finish it.
    let orphaned = insert_weekly(&pool, c, ReportMode::Batch).await;
    for id in [in_batch, orphaned] {
        backdate(&pool, "weekly_reviews", "created_at", id, 3600).await;
    }
    // Batch mode, 10 min old, no batch item yet: it may be mid-submit, so it is never
    // sync-enqueued inside BATCH_SUBMIT_GRACE.
    let d = account(&pool, "mid-submit").await;
    let mid_submit = insert_weekly(&pool, d, ReportMode::Batch).await;
    backdate(&pool, "weekly_reviews", "created_at", mid_submit, 10 * 60).await;
    // Sync, 1 min old: its enqueue may be in flight.
    let e = account(&pool, "fresh").await;
    let fresh = insert_weekly(&pool, e, ReportMode::Sync).await;
    backdate(&pool, "weekly_reviews", "created_at", fresh, 60).await;
    let mut conn = pool.acquire().await.unwrap();
    for (batch, weekly_id) in [("msgbatch_open", in_batch), ("msgbatch_failed", orphaned)] {
        batches::insert_batch(&mut conn, batch, BatchKind::WeeklyReview, 1)
            .await
            .unwrap();
        batches::insert_items(
            &mut conn,
            batch,
            &[NewBatchItem {
                id: Uuid::new_v4(),
                custom_id: format!("w-{weekly_id}"),
                kind: BatchKind::WeeklyReview,
                report_run_id: None,
                section: None,
                weekly_review_id: Some(weekly_id),
            }],
        )
        .await
        .unwrap();
    }
    assert!(
        batches::mark_failed(&mut conn, "msgbatch_failed")
            .await
            .unwrap()
    );
    drop(conn);

    let counts = sweep(&pool, &queue).await;
    assert_eq!(counts.weekly, 1, "{counts:?}");
    assert_eq!(
        live(
            &queue,
            kinds::KIND_WEEKLY_REVIEW,
            kinds::weekly_key(orphaned)
        )
        .await,
        Some(JobState::Queued)
    );
    assert_eq!(job_count(&pool, kinds::KIND_WEEKLY_REVIEW).await, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn pending_change_note_without_a_job_is_enqueued(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "note").await;
    let id = insert_note(&pool, acct).await;

    let counts = sweep(&pool, &queue).await;
    assert_eq!(counts.notes, 1, "{counts:?}");
    assert_eq!(
        live(&queue, kinds::KIND_EXPLAIN_CHANGE, kinds::change_key(id)).await,
        Some(JobState::Queued)
    );
    assert_eq!(sweep(&pool, &queue).await.notes, 0);

    let mut conn = pool.acquire().await.unwrap();
    let note = notes::get_note(&mut conn, id).await.unwrap().unwrap();
    assert_eq!(note.status, ItemStatus::Pending);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn lost_chat_message_fails_with_an_event(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "chat").await;
    let conv = conversation(&pool, acct).await;
    let lost = pending_assistant(&pool, conv, 2).await;
    backdate(&pool, "coach_messages", "created_at", lost, 3 * 60).await;
    let fresh = pending_assistant(&pool, conv, 4).await;
    backdate(&pool, "coach_messages", "created_at", fresh, 60).await;

    let counts = sweep(&pool, &queue).await;
    assert_eq!(counts.chats, 1, "{counts:?}");
    let mut conn = pool.acquire().await.unwrap();
    let m = chat::get_message(&mut conn, lost).await.unwrap().unwrap();
    assert_eq!(m.status, ChatStatus::Failed);
    assert_eq!(m.failure_reason.as_deref(), Some("lost"));
    let f = chat::get_message(&mut conn, fresh).await.unwrap().unwrap();
    assert_eq!(f.status, ChatStatus::Pending);
    drop(conn);

    let events: Vec<(String, Value)> =
        sqlx::query_as("SELECT type, payload FROM user_events WHERE account_id = $1 ORDER BY id")
            .bind(acct)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        events,
        [(
            "chat.message.failed".to_owned(),
            json!({"messageId": lost, "reason": "lost"})
        )]
    );
    let owners: Vec<Option<Uuid>> =
        sqlx::query_scalar("SELECT user_id FROM user_events WHERE account_id = $1")
            .bind(acct)
            .fetch_all(&pool)
            .await
            .unwrap();
    let owner: Uuid = sqlx::query_scalar("SELECT user_id FROM coach_conversations WHERE id = $1")
        .bind(conv)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        owners,
        vec![Some(owner)],
        "a chat event is for its owner only"
    );
    // A stale chat answer is never retried.
    assert_eq!(job_count(&pool, kinds::KIND_COACH_CHAT).await, 0);
    assert_eq!(sweep(&pool, &queue).await.chats, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn lost_interactive_eval_fails_and_is_refunded(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "ieval").await;
    let lost = insert_eval(&pool, acct).await;
    backdate(&pool, "interactive_evals", "created_at", lost, 3 * 60).await;
    let fresh = insert_eval(&pool, acct).await;
    backdate(&pool, "interactive_evals", "created_at", fresh, 60).await;

    let counts = sweep(&pool, &queue).await;
    assert_eq!(counts.evals, 1, "{counts:?}");
    let mut conn = pool.acquire().await.unwrap();
    let e = interactive::get_eval(&mut conn, lost)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(e.status, EvalStatus::Failed);
    assert!(!e.charged, "a lost eval is refunded");
    assert!(e.finished_at.is_some());
    let f = interactive::get_eval(&mut conn, fresh)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((f.status, f.charged), (EvalStatus::Pending, true));
    drop(conn);
    assert_eq!(job_count(&pool, kinds::KIND_INTERACTIVE_ANALYSE).await, 0);
    assert_eq!(sweep(&pool, &queue).await.evals, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_live_job_suppresses_requeue(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "live").await;
    let queued = insert_run(&pool, acct).await;
    backdate(&pool, "report_runs", "created_at", queued, 3600).await;
    enqueue_report(&queue, queued, acct).await.unwrap().unwrap();

    // A running run with a stale heartbeat whose job still holds its lease: left alone.
    let hung = insert_run(&pool, acct).await;
    let mut conn = pool.acquire().await.unwrap();
    runs::claim_run(&mut conn, hung).await.unwrap().unwrap();
    drop(conn);
    backdate(&pool, "report_runs", "heartbeat_at", hung, 3600).await;
    enqueue_report(&queue, hung, acct).await.unwrap().unwrap();

    let conv = conversation(&pool, acct).await;
    let msg = pending_assistant(&pool, conv, 2).await;
    backdate(&pool, "coach_messages", "created_at", msg, 3600).await;
    enqueue_chat(&queue, msg, acct).await.unwrap().unwrap();

    let eval = insert_eval(&pool, acct).await;
    backdate(&pool, "interactive_evals", "created_at", eval, 3600).await;
    enqueue_interactive(&queue, eval, acct)
        .await
        .unwrap()
        .unwrap();

    let counts = sweep(&pool, &queue).await;
    assert_eq!(
        (counts.reports, counts.chats, counts.evals),
        (0, 0, 0),
        "{counts:?}"
    );
    assert_eq!(run_status(&pool, hung).await, RunStatus::Running);
    assert_eq!(job_count(&pool, kinds::KIND_GENERATE_REPORT).await, 2);
    let mut conn = pool.acquire().await.unwrap();
    let m = chat::get_message(&mut conn, msg).await.unwrap().unwrap();
    assert_eq!(m.status, ChatStatus::Pending);
    let e = interactive::get_eval(&mut conn, eval)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(e.status, EvalStatus::Pending);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn paging_reaches_every_row_including_equal_timestamps(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "pages").await;
    // One transaction: every run gets the same `created_at` (now() is the transaction time).
    let mut tx = pool.begin().await.unwrap();
    for _ in 0..7 {
        runs::insert_run(&mut tx, &new_run(acct)).await.unwrap();
    }
    tx.commit().await.unwrap();
    sqlx::query("UPDATE report_runs SET created_at = now() - interval '1 hour'")
        .execute(&pool)
        .await
        .unwrap();
    // One already has its job, so every page but the last mixes live and lost rows.
    let first: Uuid = sqlx::query_scalar("SELECT id FROM report_runs ORDER BY id LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    enqueue_report(&queue, first, acct).await.unwrap().unwrap();

    let counts = reconcile::reconcile_ai_paged(&pool, &queue, OffsetDateTime::now_utc(), 2)
        .await
        .unwrap();
    assert_eq!(counts.reports, 6, "{counts:?}");
    assert_eq!(job_count(&pool, kinds::KIND_GENERATE_REPORT).await, 7);
}

async fn insert_entry(pool: &PgPool, session_id: Uuid, seq: i64, age_secs: i64) {
    sqlx::query(
        "INSERT INTO ai_sdk_session_entries (project_key, session_id, subpath, seq, entry, \
         created_at) VALUES ('p', $1, '', $2, '{}', now() - make_interval(secs => $3))",
    )
    .bind(session_id)
    .bind(seq)
    .bind(age_secs as f64)
    .execute(pool)
    .await
    .unwrap();
}

async fn entries(pool: &PgPool, session_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM ai_sdk_session_entries WHERE session_id = $1")
        .bind(session_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn job() -> Job {
    let now = OffsetDateTime::now_utc();
    Job {
        id: 1,
        kind: kinds::KIND_AI_RECONCILE.into(),
        payload: json!({}),
        dedupe_key: Some(kinds::KIND_AI_RECONCILE.into()),
        user_key: None,
        priority: 100,
        attempts: 1,
        max_attempts: 3,
        lease_token: Uuid::new_v4(),
        run_at: now,
        created_at: now,
        started_at: now,
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn every_sweep_removes_orphan_session_entries_past_the_grace(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "orphans").await;
    let conv = conversation(&pool, acct).await;
    let gone = Uuid::new_v4();
    insert_entry(&pool, gone, 1, 11 * 60).await;
    insert_entry(&pool, gone, 2, 60).await; // inside the 10 min grace
    insert_entry(&pool, conv, 1, 3600).await; // its conversation exists
    // Older than the 24 h window: an orphan cannot be that old (the delete trigger), so the
    // sweep does not look there.
    let ancient = Uuid::new_v4();
    insert_entry(&pool, ancient, 1, 25 * 3600).await;

    let counts = sweep(&pool, &queue).await;
    assert_eq!(counts.session_entries, 1, "{counts:?}");
    assert_eq!(entries(&pool, gone).await, 1);
    assert_eq!(entries(&pool, conv).await, 1);
    assert_eq!(entries(&pool, ancient).await, 1);

    // It measures from the `now` it is given.
    let later = OffsetDateTime::now_utc() + time::Duration::minutes(10);
    let counts = reconcile_ai(&pool, &queue, later).await.unwrap();
    assert_eq!(counts.session_entries, 1, "{counts:?}");
    assert_eq!(entries(&pool, gone).await, 0);

    // The handler runs the same sweep (with the orphan step) on every run.
    let handler = AiReconcileHandler {
        pool: pool.clone(),
        queue: queue.clone(),
    };
    assert_eq!(handler.kind(), kinds::KIND_AI_RECONCILE);
    let other = Uuid::new_v4();
    insert_entry(&pool, other, 1, 20 * 60).await;
    let out = handler
        .handle(
            job(),
            JobContext {
                cancel: CancellationToken::new(),
            },
        )
        .await
        .unwrap();
    assert_eq!(out["sessionEntries"], json!(1), "{out}");
    assert_eq!(entries(&pool, other).await, 0);
    assert_eq!(entries(&pool, ancient).await, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn the_orphan_sweep_pages_through_ties(pool: PgPool) {
    // 2,500 orphans with one created_at (one statement): three pages of 1,000 with an inclusive
    // cursor still reach every one, and a conversation's entries survive.
    let acct = account(&pool, "orphan-pages").await;
    let conv = conversation(&pool, acct).await;
    sqlx::query(
        "INSERT INTO ai_sdk_session_entries (project_key, session_id, subpath, seq, entry, \
         created_at) SELECT 'p', CASE WHEN i % 5 = 0 THEN $1 ELSE gen_random_uuid() END, '', i, \
         '{}', now() - interval '1 hour' FROM generate_series(1, 3125) i",
    )
    .bind(conv)
    .execute(&pool)
    .await
    .unwrap();
    let counts = sweep(&pool, &Queue::new(pool.clone())).await;
    assert_eq!(counts.session_entries, 2500, "{counts:?}");
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_sdk_session_entries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 625);
    assert_eq!(entries(&pool, conv).await, 625);
}

async fn kill_jobs(pool: &PgPool, kind: &str) {
    sqlx::query("UPDATE jobs SET state = 'dead' WHERE kind = $1")
        .bind(kind)
        .execute(pool)
        .await
        .unwrap();
}

async fn events_of(pool: &PgPool, account_id: Uuid) -> Vec<(String, Value)> {
    sqlx::query_as("SELECT type, payload FROM user_events WHERE account_id = $1 ORDER BY id")
        .bind(account_id)
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_poison_run_is_failed_past_the_crash_cap(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "poison-run").await;
    let id = insert_run(&pool, acct).await;
    // It has a report, so the cap degrades it (the computed report still shows).
    let mut tx = pool.begin().await.unwrap();
    let report = ai::db::reports::create_report_with_next_version(
        &mut tx,
        &ai::db::reports::NewReport {
            account_id: acct,
            time_class: "blitz".into(),
            report_run_id: id,
            schema_version: "report-payload-v1".into(),
            computed: json!({}),
            status: ai::db::reports::ReportStatus::ComputedOnly,
        },
    )
    .await
    .unwrap();
    assert!(runs::link_report(&mut tx, id, report.id).await.unwrap());
    tx.commit().await.unwrap();

    // Each round: a worker claims it and dies with its job; the heartbeat goes stale.
    for round in 1..=3 {
        let mut conn = pool.acquire().await.unwrap();
        runs::claim_run(&mut conn, id).await.unwrap().unwrap();
        drop(conn);
        backdate(&pool, "report_runs", "heartbeat_at", id, 11 * 60).await;
        kill_jobs(&pool, kinds::KIND_GENERATE_REPORT).await;
        let counts = sweep(&pool, &queue).await;
        assert_eq!(
            (counts.reports, counts.capped),
            (1, 0),
            "round {round}: {counts:?}"
        );
        assert_eq!(run_status(&pool, id).await, RunStatus::Queued);
    }
    let mut conn = pool.acquire().await.unwrap();
    runs::claim_run(&mut conn, id).await.unwrap().unwrap();
    drop(conn);
    backdate(&pool, "report_runs", "heartbeat_at", id, 11 * 60).await;
    kill_jobs(&pool, kinds::KIND_GENERATE_REPORT).await;
    let counts = sweep(&pool, &queue).await;
    assert_eq!((counts.reports, counts.capped), (0, 1), "{counts:?}");
    let mut conn = pool.acquire().await.unwrap();
    let run = runs::get_run(&mut conn, id).await.unwrap().unwrap();
    drop(conn);
    assert_eq!(run.status, RunStatus::Failed);
    assert_eq!(run.last_error.as_deref(), Some("reconcile_cap"));
    assert_eq!(
        events_of(&pool, acct).await,
        [(
            "report.degraded".to_owned(),
            json!({"reportId": report.id, "reason": "unavailable", "retryAt": null})
        )]
    );
    assert_eq!(
        live(&queue, kinds::KIND_GENERATE_REPORT, kinds::report_key(id)).await,
        None
    );
    // The next sweep does nothing.
    assert_eq!(sweep(&pool, &queue).await, AiReconcileCounts::default());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_lost_queued_run_without_a_report_fails_with_job_failed(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "poison-queued").await;
    let id = insert_run(&pool, acct).await;
    backdate(&pool, "report_runs", "created_at", id, 3600).await;
    for _ in 0..3 {
        assert_eq!(sweep(&pool, &queue).await.reports, 1);
        kill_jobs(&pool, kinds::KIND_GENERATE_REPORT).await;
    }
    assert_eq!(sweep(&pool, &queue).await.capped, 1);
    assert_eq!(run_status(&pool, id).await, RunStatus::Failed);
    assert_eq!(
        events_of(&pool, acct).await,
        [(
            "job.failed".to_owned(),
            json!({"kind": "generate-report", "reason": "reconcile_cap"})
        )]
    );
    assert_eq!(sweep(&pool, &queue).await, AiReconcileCounts::default());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_poison_weekly_review_and_note_are_failed_past_the_crash_cap(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "poison-items").await;
    let w = insert_weekly(&pool, acct, ReportMode::Sync).await;
    let n = insert_note(&pool, acct).await;
    for round in 1..=3 {
        let counts = sweep(&pool, &queue).await;
        assert_eq!(
            (counts.weekly, counts.notes, counts.capped),
            (1, 1, 0),
            "round {round}: {counts:?}"
        );
        kill_jobs(&pool, kinds::KIND_WEEKLY_REVIEW).await;
        kill_jobs(&pool, kinds::KIND_EXPLAIN_CHANGE).await;
    }
    let counts = sweep(&pool, &queue).await;
    assert_eq!(
        (counts.weekly, counts.notes, counts.capped),
        (0, 0, 2),
        "{counts:?}"
    );
    let mut conn = pool.acquire().await.unwrap();
    let wr = weekly::get_weekly(&mut conn, w).await.unwrap().unwrap();
    let nr = notes::get_note(&mut conn, n).await.unwrap().unwrap();
    drop(conn);
    assert_eq!(
        (wr.status, nr.status),
        (ItemStatus::Failed, ItemStatus::Failed)
    );
    let mut kinds_failed: Vec<Value> = events_of(&pool, acct)
        .await
        .into_iter()
        .map(|(ty, p)| {
            assert_eq!(ty, "job.failed");
            p
        })
        .collect();
    kinds_failed.sort_by_key(|p| p["kind"].as_str().unwrap().to_owned());
    assert_eq!(
        kinds_failed,
        [
            json!({"kind": "explain-weakness-change", "reason": "reconcile_cap"}),
            json!({"kind": "weekly-review", "reason": "reconcile_cap"}),
        ]
    );
    assert_eq!(sweep(&pool, &queue).await, AiReconcileCounts::default());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_batch_run_mid_submit_is_never_sync_enqueued(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "batch-run").await;
    let mut conn = pool.acquire().await.unwrap();
    let batch = NewRun {
        mode: ReportMode::Batch,
        ..new_run(acct)
    };
    let mid_submit = runs::insert_run(&mut conn, &batch).await.unwrap();
    let abandoned = runs::insert_run(&mut conn, &batch).await.unwrap();
    drop(conn);
    backdate(&pool, "report_runs", "created_at", mid_submit, 20 * 60).await;
    backdate(&pool, "report_runs", "created_at", abandoned, 31 * 60).await;

    let counts = sweep(&pool, &queue).await;
    assert_eq!(counts.reports, 1, "{counts:?}");
    assert_eq!(
        live(
            &queue,
            kinds::KIND_GENERATE_REPORT,
            kinds::report_key(mid_submit)
        )
        .await,
        None,
        "inside BATCH_SUBMIT_GRACE"
    );
    assert_eq!(
        live(
            &queue,
            kinds::KIND_GENERATE_REPORT,
            kinds::report_key(abandoned)
        )
        .await,
        Some(JobState::Queued)
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_running_sync_run_cannot_be_flipped_to_batch(pool: PgPool) {
    let acct = account(&pool, "flip").await;
    let id = insert_run(&pool, acct).await;
    let mut conn = pool.acquire().await.unwrap();
    runs::claim_run(&mut conn, id).await.unwrap().unwrap();
    assert!(!runs::set_awaiting_batch(&mut conn, id).await.unwrap());
    drop(conn);
    assert_eq!(run_status(&pool, id).await, RunStatus::Running);
    let queued = insert_run(&pool, acct).await;
    let mut conn = pool.acquire().await.unwrap();
    assert!(runs::set_awaiting_batch(&mut conn, queued).await.unwrap());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_lost_streaming_turn_fails_with_an_event(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "streaming").await;
    let conv = conversation(&pool, acct).await;
    let mut ids = Vec::new();
    for seq in [2, 4, 6] {
        let id = pending_assistant(&pool, conv, seq).await;
        let mut conn = pool.acquire().await.unwrap();
        assert!(chat::set_streaming(&mut conn, id).await.unwrap());
        ids.push(id);
    }
    let (lost, recent, live_job) = (ids[0], ids[1], ids[2]);
    let stamped: Option<OffsetDateTime> =
        sqlx::query_scalar("SELECT streaming_at FROM coach_messages WHERE id = $1")
            .bind(lost)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(stamped.is_some(), "set_streaming stamps streaming_at");
    // Past the turn timeout + 30 s + 60 s; inside it; past it but its job is live.
    let past = reconcile::STREAMING_LOST_AFTER.as_secs() as i64 + 10;
    backdate(&pool, "coach_messages", "streaming_at", lost, past).await;
    backdate(&pool, "coach_messages", "streaming_at", recent, past - 30).await;
    backdate(&pool, "coach_messages", "streaming_at", live_job, past).await;
    for id in ids.iter().copied() {
        backdate(&pool, "coach_messages", "created_at", id, 3600).await;
    }
    enqueue_chat(&queue, live_job, acct).await.unwrap().unwrap();

    let counts = sweep(&pool, &queue).await;
    assert_eq!(counts.chats, 1, "{counts:?}");
    let mut conn = pool.acquire().await.unwrap();
    let m = chat::get_message(&mut conn, lost).await.unwrap().unwrap();
    assert_eq!(
        (m.status, m.failure_reason.as_deref()),
        (ChatStatus::Failed, Some("lost"))
    );
    for id in [recent, live_job] {
        let m = chat::get_message(&mut conn, id).await.unwrap().unwrap();
        assert_eq!(m.status, ChatStatus::Streaming);
    }
    drop(conn);
    assert_eq!(
        events_of(&pool, acct).await,
        [(
            "chat.message.failed".to_owned(),
            json!({"messageId": lost, "reason": "lost"})
        )]
    );
    assert_eq!(sweep(&pool, &queue).await.chats, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_failed_step_does_not_stop_the_others(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "steps").await;
    let run = insert_run(&pool, acct).await;
    backdate(&pool, "report_runs", "created_at", run, 3600).await;
    let eval = insert_eval(&pool, acct).await;
    backdate(&pool, "interactive_evals", "created_at", eval, 3600).await;
    // The weekly step fails (its table is gone); the steps before and after it still run.
    sqlx::raw_sql("DROP TABLE ai_batch_items; DROP TABLE weekly_reviews")
        .execute(&pool)
        .await
        .unwrap();
    let err = reconcile_ai(&pool, &queue, OffsetDateTime::now_utc())
        .await
        .unwrap_err();
    let reconcile::AiReconcileError::Steps { counts, errors } = err else {
        panic!("{err:?}");
    };
    assert_eq!(
        errors.iter().map(|e| e.0).collect::<Vec<_>>(),
        ["pending_weekly"]
    );
    assert_eq!((counts.reports, counts.evals), (1, 1), "{counts:?}");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_cancelled_sweep_is_released(pool: PgPool) {
    let handler = AiReconcileHandler {
        pool: pool.clone(),
        queue: Queue::new(pool.clone()),
    };
    let cancel = CancellationToken::new();
    cancel.cancel();
    let err = handler
        .handle(job(), JobContext { cancel })
        .await
        .unwrap_err();
    assert!(matches!(err, queue::JobError::Release { .. }), "{err:?}");
}

/// The plans of `sql` (prepared as `name(types)`, run with `args`) under the custom and the
/// generic plan, as the cached statements of sqlx may use either.
async fn plans(pool: &PgPool, name: &str, types: &str, sql: &str, args: &str) -> Vec<String> {
    let mut conn = pool.acquire().await.unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "PREPARE {name}({types}) AS {sql}"
    )))
    .execute(&mut *conn)
    .await
    .unwrap();
    let mut out = Vec::new();
    for mode in ["force_custom_plan", "force_generic_plan"] {
        sqlx::query("SELECT set_config('plan_cache_mode', $1, false)")
            .bind(mode)
            .execute(&mut *conn)
            .await
            .unwrap();
        let plan: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "EXPLAIN EXECUTE {name}({args})"
        )))
        .fetch_all(&mut *conn)
        .await
        .unwrap();
        out.push(format!("{name} {mode} ({args}):\n{}", plan.join("\n")));
    }
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DEALLOCATE {name}; RESET plan_cache_mode"
    )))
    .execute(&mut *conn)
    .await
    .unwrap();
    out
}

/// Reads `index` in order and stops at the limit: no Seq Scan, no bitmap scan, and no full Sort
/// (an Incremental Sort on the index order, for the `id` tie-break, is fine).
fn assert_index_ordered(plan: &str, index: &str) {
    assert!(
        plan.contains(&format!("Index Scan using {index}")),
        "{plan}"
    );
    assert!(
        !plan.contains("Seq Scan") && !plan.contains("Bitmap"),
        "{plan}"
    );
    for line in plan.lines() {
        let node = line.trim_start().trim_start_matches("->").trim_start();
        assert!(!node.starts_with("Sort  ("), "full sort in\n{plan}");
    }
}

/// Every sweep page reads its partial index in (created_at, id) order from the keyset cursor,
/// in the custom and the generic plan, for the first page (NULL cursor) and a later one, on
/// tables where finished rows dominate.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn sweep_pages_are_index_ordered(pool: PgPool) {
    let acct = account(&pool, "plans").await;
    sqlx::query(
        "INSERT INTO report_runs (account_id, time_class, trigger, plan, prompt_version, \
         is_first_report, status, created_at, heartbeat_at, next_attempt_at) \
         SELECT $1, 'blitz', 'manual', 'pro', 'v', false, \
           (CASE WHEN i % 50 = 0 THEN 'queued' WHEN i % 50 = 1 THEN 'running' \
            WHEN i % 50 = 2 THEN 'awaiting_retry' WHEN i % 50 = 3 THEN 'awaiting_batch' \
            WHEN i % 50 = 4 THEN 'failed' ELSE 'done' END)::report_run_status, \
           now() - make_interval(secs => i), now() - make_interval(secs => i), \
           now() - make_interval(secs => i) \
         FROM generate_series(1, 100000) i",
    )
    .bind(acct)
    .execute(&pool)
    .await
    .unwrap();
    // Conversations and messages (10% pending and 10% streaming: many pages behind the later
    // cursor), then
    // evals, reviews and notes (2% pending).
    sqlx::query(
        "WITH c AS (INSERT INTO coach_conversations (account_id, user_id, time_class) \
           SELECT $1, $2, 'blitz' FROM generate_series(1, 1000) RETURNING id), \
         n AS (SELECT id, row_number() OVER () AS k FROM c) \
         INSERT INTO coach_messages (conversation_id, seq, role, status, created_at, \
           streaming_at) \
         SELECT n.id, s, 'assistant', \
           (CASE WHEN (n.k * 100 + s) % 10 = 0 THEN 'pending' \
            WHEN (n.k * 100 + s) % 10 = 1 THEN 'streaming' ELSE 'done' END)::chat_status, \
           now() - make_interval(secs => n.k * 100 + s), \
           now() - make_interval(secs => n.k * 100 + s) \
         FROM n, generate_series(1, 100) s",
    )
    .bind(acct)
    .bind(testkit::insert_user(&pool, "plans").await)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO interactive_evals (account_id, protocol, base_fen, fen, status, created_at) \
         SELECT $1, 'p', 'b', 'f' || i, \
           (CASE WHEN i % 50 = 0 THEN 'pending' ELSE 'done' END)::interactive_eval_status, \
           now() - make_interval(secs => i) FROM generate_series(1, 100000) i",
    )
    .bind(acct)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO weekly_reviews (account_id, time_class, week_start, status, input, \
         created_at) SELECT $1, 'blitz', date '2000-01-03' + 7 * i, \
           (CASE WHEN i % 50 = 0 THEN 'pending' ELSE 'ready' END)::ai_item_status, '{}', \
           now() - make_interval(secs => i) FROM generate_series(1, 100000) i",
    )
    .bind(acct)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO weakness_change_notes (account_id, change_key, series_key, status, input, \
         created_at) SELECT $1, 'k' || i, 's', \
           (CASE WHEN i % 50 = 0 THEN 'pending' ELSE 'ready' END)::ai_item_status, '{}', \
           now() - make_interval(secs => i) FROM generate_series(1, 100000) i",
    )
    .bind(acct)
    .execute(&pool)
    .await
    .unwrap();
    // Session entries: 98% belong to a conversation, 2% are orphans, over 55 h.
    sqlx::query(
        "WITH c AS (SELECT id, row_number() OVER (ORDER BY id) AS k FROM coach_conversations) \
         INSERT INTO ai_sdk_session_entries (project_key, session_id, subpath, seq, entry, \
           created_at) \
         SELECT 'p', CASE WHEN i % 50 = 0 THEN gen_random_uuid() ELSE c.id END, '', i, '{}', \
           now() - make_interval(secs => i * 2) \
         FROM generate_series(1, 100000) i JOIN c ON c.k = 1 + i % 1000",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(
        "ANALYZE report_runs; ANALYZE coach_conversations; ANALYZE coach_messages; \
         ANALYZE interactive_evals; ANALYZE weekly_reviews; ANALYZE weakness_change_notes; \
         ANALYZE ai_sdk_session_entries",
    )
    .execute(&pool)
    .await
    .unwrap();

    let types = "timestamptz, timestamptz, uuid, int8";
    for (name, sql, index) in reconcile::sweep_pages() {
        for cursor in [
            "NULL, NULL",
            "now() - interval '20 hours', 'ffffffff-ffff-ffff-ffff-ffffffffffff'",
        ] {
            let args = format!("now() - interval '5 minutes', {cursor}, 500");
            for plan in plans(&pool, name, types, sql, &args).await {
                assert_index_ordered(&plan, index);
            }
        }
    }
    // The orphan sweep's page: the 24 h window from the cursor, in created_at order.
    let types = "timestamptz, timestamptz, timestamptz, int8";
    for cursor in ["NULL", "now() - interval '20 hours'"] {
        let args =
            format!("now() - interval '24 hours', {cursor}, now() - interval '10 minutes', 1000");
        for plan in plans(&pool, "orphans", types, chat::ORPHAN_PAGE_SQL, &args).await {
            assert_index_ordered(&plan, "ai_sdk_session_entries_created_idx");
        }
    }
}

// --- T26 hardening (T21 re-review) ---------------------------------------------------------

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_change_note_inside_the_enqueue_grace_is_left_alone(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "note-grace").await;
    let id = insert_note(&pool, acct).await;
    // 4 min old: its enqueue may still be in flight.
    backdate(&pool, "weakness_change_notes", "created_at", id, 4 * 60).await;
    assert_eq!(sweep(&pool, &queue).await.notes, 0);
    assert_eq!(
        live(&queue, kinds::KIND_EXPLAIN_CHANGE, kinds::change_key(id)).await,
        None
    );
    backdate(&pool, "weakness_change_notes", "created_at", id, 6 * 60).await;
    assert_eq!(sweep(&pool, &queue).await.notes, 1);
}

/// Runs the orphan pages as the sweep does; returns (deleted, pages).
async fn orphan_pages(pool: &PgPool, limit: i64) -> (u64, u32) {
    let mut conn = pool.acquire().await.unwrap();
    let now = OffsetDateTime::now_utc();
    let (mut deleted, mut pages, mut after) = (0, 0, None);
    loop {
        let page = chat::sweep_orphan_session_entries_page(&mut conn, now, after, limit)
            .await
            .unwrap();
        deleted += page.deleted;
        pages += 1;
        assert!(pages < 100, "the cursor stopped advancing");
        match page.next {
            Some(next) => after = Some(next),
            None => return (deleted, pages),
        }
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn an_orphan_page_reads_at_most_its_limit_and_moves_past_live_entries(pool: PgPool) {
    // 2,500 live entries (one per second), then 3 orphans. Each page reads at most 1,000 rows
    // in time order and the cursor moves past them, so no page rescans the live ones.
    let acct = account(&pool, "orphan-live").await;
    let conv = conversation(&pool, acct).await;
    sqlx::query(
        "INSERT INTO ai_sdk_session_entries (project_key, session_id, subpath, seq, entry,          created_at) SELECT 'p', $1, '', i, '{}', now() - interval '2 hours'          + make_interval(secs => i) FROM generate_series(1, 2500) i",
    )
    .bind(conv)
    .execute(&pool)
    .await
    .unwrap();
    for (i, orphan) in [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()]
        .into_iter()
        .enumerate()
    {
        insert_entry(&pool, orphan, i as i64, 30 * 60).await;
    }
    let mut conn = pool.acquire().await.unwrap();
    let first =
        chat::sweep_orphan_session_entries_page(&mut conn, OffsetDateTime::now_utc(), None, 1000)
            .await
            .unwrap();
    assert_eq!(first.deleted, 0, "the first page holds live entries only");
    assert!(first.next.is_some());
    drop(conn);
    let (deleted, pages) = orphan_pages(&pool, 1000).await;
    assert_eq!(deleted, 3);
    assert!(pages <= 4, "{pages} pages");
    assert_eq!(entries(&pool, conv).await, 2500);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_tie_wider_than_a_page_of_live_entries_still_reaches_its_orphans(pool: PgPool) {
    // 1,500 live entries and 2 orphans with one created_at: a page of 1,000 live rows cannot
    // move the cursor, so the sweep clears that instant's orphans and steps past it.
    let acct = account(&pool, "orphan-tie").await;
    let conv = conversation(&pool, acct).await;
    sqlx::query(
        "INSERT INTO ai_sdk_session_entries (project_key, session_id, subpath, seq, entry,          created_at) SELECT 'p', CASE WHEN i <= 1500 THEN $1 ELSE gen_random_uuid() END, '', i,          '{}', date_trunc('second', now()) - interval '1 hour' FROM generate_series(1, 1502) i",
    )
    .bind(conv)
    .execute(&pool)
    .await
    .unwrap();
    let (deleted, _) = orphan_pages(&pool, 1000).await;
    assert_eq!(deleted, 2);
    assert_eq!(entries(&pool, conv).await, 1500);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_sweep_past_its_deadline_skips_the_remaining_steps(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "deadline").await;
    let run = insert_run(&pool, acct).await;
    backdate(&pool, "report_runs", "created_at", run, 6 * 60).await;
    let counts = reconcile::reconcile_ai_within(
        &pool,
        &queue,
        OffsetDateTime::now_utc(),
        reconcile::PAGE_SIZE,
        Duration::ZERO,
    )
    .await
    .unwrap();
    assert_eq!(counts.reports, 0);
    assert_eq!(counts.skipped_steps, 9, "{counts:?}");
    assert_eq!(job_count(&pool, kinds::KIND_GENERATE_REPORT).await, 0);
    // The regular sweep has a deadline below the runner's hard timeout and runs every step.
    assert!(reconcile::SWEEP_DEADLINE < Duration::from_secs(5 * 60));
    let counts = sweep(&pool, &queue).await;
    assert_eq!((counts.reports, counts.skipped_steps), (1, 0));
}
