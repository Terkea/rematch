//! Task 21: the `ai-reconcile` sweep (ADR-R3-3). One test per sweep: a seeded row with no job is
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
    let mut conn = pool.acquire().await.unwrap();
    chat::create_conversation(
        &mut conn,
        &NewConversation {
            account_id,
            user_id: None,
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
    weekly::insert_weekly_if_absent(
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
    .id
}

async fn insert_note(pool: &PgPool, account_id: Uuid) -> Uuid {
    let mut conn = pool.acquire().await.unwrap();
    notes::insert_note_if_absent(
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
    .id
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

    let counts = sweep(&pool, &queue).await;
    assert_eq!(counts.session_entries, 1, "{counts:?}");
    assert_eq!(entries(&pool, gone).await, 1);
    assert_eq!(entries(&pool, conv).await, 1);

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
    assert_eq!(entries(&pool, gone).await, 1);
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
    // Conversations and messages (a 10% backlog: many pages behind the later cursor), then
    // evals, reviews and notes (2% pending).
    sqlx::query(
        "WITH c AS (INSERT INTO coach_conversations (account_id, time_class) \
           SELECT $1, 'blitz' FROM generate_series(1, 1000) RETURNING id), \
         n AS (SELECT id, row_number() OVER () AS k FROM c) \
         INSERT INTO coach_messages (conversation_id, seq, role, status, created_at) \
         SELECT n.id, s, 'assistant', \
           (CASE WHEN (n.k * 100 + s) % 10 = 0 THEN 'pending' ELSE 'done' END)::chat_status, \
           now() - make_interval(secs => n.k * 100 + s) \
         FROM n, generate_series(1, 100) s",
    )
    .bind(acct)
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
    sqlx::raw_sql(
        "ANALYZE report_runs; ANALYZE coach_conversations; ANALYZE coach_messages; \
         ANALYZE interactive_evals; ANALYZE weekly_reviews; ANALYZE weakness_change_notes",
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
}
