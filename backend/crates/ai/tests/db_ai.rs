//! Task 17: AI tables, repositories, job kinds and event types (TS plan C1 tests, ported).

use std::time::Duration;

use ai::db::batches::{self, BatchKind, NewBatchItem};
use ai::db::chat::{self, ChatRole, ChatStatus, MessageFinish, NewConversation, NewMessage};
use ai::db::interactive::{self, NewEval};
use ai::db::notes::{self, NewNote};
use ai::db::reports::{self, NewReport, ReportStatus, SectionWrite, SectionWriteStatus};
use ai::db::runs::{self, NewRun, ReportMode, ReportTrigger, RunStatus};
use ai::db::usage::{self, AiFeature, UsageRow};
use ai::db::weekly::{self, NewWeekly};
use ai::db::{ItemStatus, SessionPath};
use ai::entitlements::Plan;
use ai::events::{self, SectionReady};
use ai::jobs::kinds::{self, enqueue_report, ensure_ai_recurring};
use ai::jobs::retry::{RETRY_DELAYS, retry_delay};
use ai::output::codes::SectionKey;
use queue::queue::Queue;
use serde_json::{Value, json};
use sqlx::{Acquire, PgPool};
use time::OffsetDateTime;
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

async fn run_and_report(pool: &PgPool, account_id: Uuid) -> (Uuid, reports::ReportRow) {
    let mut tx = pool.begin().await.unwrap();
    let run_id = runs::insert_run(&mut tx, &new_run(account_id))
        .await
        .unwrap();
    let report = reports::create_report_with_next_version(
        &mut tx,
        &NewReport {
            account_id,
            time_class: "blitz".into(),
            report_run_id: run_id,
            schema_version: "report-payload-v1".into(),
            computed: json!({"k": 1}),
            status: ReportStatus::ComputedOnly,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    (run_id, report)
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "23505")
}

fn is_check_violation(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "23514")
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn report_versions_are_unique_and_sequential(pool: PgPool) {
    let acct = account(&pool, "versions").await;
    let (_, first) = run_and_report(&pool, acct).await;
    let (_, second) = run_and_report(&pool, acct).await;
    assert_eq!((first.version, second.version), (1, 2));
    assert_eq!(first.status, ReportStatus::ComputedOnly);

    // Another time class starts at 1 again.
    let mut conn = pool.acquire().await.unwrap();
    let run = runs::insert_run(
        &mut conn,
        &NewRun {
            time_class: "rapid".into(),
            ..new_run(acct)
        },
    )
    .await
    .unwrap();
    let rapid = reports::create_report_with_next_version(
        &mut conn,
        &NewReport {
            account_id: acct,
            time_class: "rapid".into(),
            report_run_id: run,
            schema_version: "report-payload-v1".into(),
            computed: json!({}),
            status: ReportStatus::ComputedOnly,
        },
    )
    .await
    .unwrap();
    assert_eq!(rapid.version, 1);

    // A duplicate (account, time class, version) is a unique violation.
    let run3 = runs::insert_run(&mut conn, &new_run(acct)).await.unwrap();
    let err = sqlx::query(
        "INSERT INTO reports (account_id, time_class, version, report_run_id, schema_version, \
         computed, status) VALUES ($1, 'blitz', 2, $2, 'v', '{}', 'partial')",
    )
    .bind(acct)
    .bind(run3)
    .execute(&mut *conn)
    .await
    .unwrap_err();
    assert!(is_unique_violation(&err), "{err:?}");

    let latest = reports::latest_report(&mut conn, acct, "blitz")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(latest.id, second.id);
    let at = runs::latest_report_at(&mut conn, acct, "blitz")
        .await
        .unwrap();
    assert_eq!(at, Some(second.created_at));
    assert_eq!(
        runs::latest_report_at(&mut conn, acct, "bullet")
            .await
            .unwrap(),
        None
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn concurrent_first_reports_get_distinct_versions(pool: PgPool) {
    let acct = account(&pool, "racing").await;
    // A creates version 1 and holds its transaction (and the advisory lock) open.
    let mut a = pool.begin().await.unwrap();
    let run_a = runs::insert_run(&mut a, &new_run(acct)).await.unwrap();
    let first = reports::create_report_with_next_version(
        &mut a,
        &NewReport {
            account_id: acct,
            time_class: "blitz".into(),
            report_run_id: run_a,
            schema_version: "report-payload-v1".into(),
            computed: json!({}),
            status: ReportStatus::ComputedOnly,
        },
    )
    .await
    .unwrap();
    assert_eq!(first.version, 1);

    // B starts and must block on the lock, not read max() = 0.
    let b = tokio::spawn({
        let pool = pool.clone();
        async move { run_and_report(&pool, acct).await.1.version }
    });
    testkit::wait_for(
        "B to wait on the version lock",
        Duration::from_secs(10),
        Duration::from_millis(20),
        || {
            let pool = pool.clone();
            async move {
                let waiting: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND NOT granted \
                     AND classid = 30001",
                )
                .fetch_one(&pool)
                .await
                .unwrap();
                (waiting == 1).then_some(())
            }
        },
    )
    .await;
    assert!(!b.is_finished());

    a.commit().await.unwrap();
    assert_eq!(b.await.unwrap(), 2);
}

fn ready(content: Value) -> SectionWrite {
    SectionWrite {
        section: SectionKey::Overview,
        status: SectionWriteStatus::Ready,
        content: Some(content),
        dropped: json!([]),
        attempts: 1,
        model: "claude-sonnet-5-5".into(),
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn upsert_section_reports_changed_only_on_real_change(pool: PgPool) {
    let acct = account(&pool, "upsert").await;
    let (_, report) = run_and_report(&pool, acct).await;
    let mut conn = pool.acquire().await.unwrap();

    let first = reports::upsert_section(&mut conn, report.id, &ready(json!({"a": 1, "b": 2})))
        .await
        .unwrap();
    assert!(first.0, "a new section is a change");
    // Identical content (key order is irrelevant for jsonb) is not a change.
    let same = reports::upsert_section(&mut conn, report.id, &ready(json!({"b": 2, "a": 1})))
        .await
        .unwrap();
    assert!(!same.0);
    // A different attempt count alone is not a change either (TS: status or content).
    let attempts = reports::upsert_section(
        &mut conn,
        report.id,
        &SectionWrite {
            attempts: 2,
            ..ready(json!({"a": 1, "b": 2}))
        },
    )
    .await
    .unwrap();
    assert!(!attempts.0);
    // A status change is.
    let dropped = reports::upsert_section(
        &mut conn,
        report.id,
        &SectionWrite {
            status: SectionWriteStatus::Dropped,
            ..ready(json!({"a": 1, "b": 2}))
        },
    )
    .await
    .unwrap();
    assert!(dropped.0);
    // And a content change.
    let other = reports::upsert_section(&mut conn, report.id, &ready(json!({"a": 9})))
        .await
        .unwrap();
    assert!(other.0);

    reports::upsert_section(
        &mut conn,
        report.id,
        &SectionWrite {
            section: SectionKey::ClimbPlan,
            ..ready(json!({}))
        },
    )
    .await
    .unwrap();
    reports::upsert_section(
        &mut conn,
        report.id,
        &SectionWrite {
            section: SectionKey::Openings,
            status: SectionWriteStatus::Dropped,
            content: None,
            ..ready(json!({}))
        },
    )
    .await
    .unwrap();
    let keys = reports::ready_sections(&mut conn, report.id).await.unwrap();
    assert_eq!(keys, [SectionKey::Overview, SectionKey::ClimbPlan]);

    let stored = reports::load_report_rows(&mut conn, report.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.report.id, report.id);
    assert_eq!(stored.sections.len(), 3);
    assert_eq!(stored.sections[0].section, SectionKey::Overview);
    assert_eq!(stored.sections[0].attempts, 1);
    assert_eq!(stored.sections[0].content, Some(json!({"a": 9})));

    assert!(
        reports::set_status(
            &mut conn,
            report.id,
            ReportStatus::Partial,
            Some(&json!({"label": "Written by AI"}))
        )
        .await
        .unwrap()
    );
    let r = reports::get_report(&mut conn, report.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r.status, ReportStatus::Partial);
    assert!(r.updated_at >= r.created_at);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn deleting_a_report_cascades_sections(pool: PgPool) {
    let acct = account(&pool, "cascade").await;
    let (_, report) = run_and_report(&pool, acct).await;
    let mut conn = pool.acquire().await.unwrap();
    reports::upsert_section(&mut conn, report.id, &ready(json!({})))
        .await
        .unwrap();
    sqlx::query("DELETE FROM reports WHERE id = $1")
        .bind(report.id)
        .execute(&mut *conn)
        .await
        .unwrap();
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM report_sections WHERE report_id = $1")
        .bind(report.id)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

fn usage_row(account_id: Uuid, cost: i64) -> UsageRow {
    UsageRow {
        account_id,
        feature: AiFeature::Report,
        model: "claude-sonnet-5-5".into(),
        batch: false,
        input_tokens: 10,
        output_tokens: 20,
        cache_creation_tokens: 0,
        cache_read_tokens: 5,
        cost_micro_usd: cost,
        ref_id: Some("ref".into()),
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn spend_since_sums_two_accounts_and_ignores_older_rows(pool: PgPool) {
    let a = account(&pool, "spend-a").await;
    let b = account(&pool, "spend-b").await;
    let c = account(&pool, "spend-c").await;
    let mut conn = pool.acquire().await.unwrap();
    usage::record_usage(&mut conn, &usage_row(a, 100))
        .await
        .unwrap();
    usage::record_usage(&mut conn, &usage_row(b, 250))
        .await
        .unwrap();
    usage::record_usage(&mut conn, &usage_row(c, 1000))
        .await
        .unwrap();
    let old = usage::record_usage(&mut conn, &usage_row(a, 7000))
        .await
        .unwrap();
    sqlx::query("UPDATE ai_usage SET created_at = now() - interval '40 days' WHERE id = $1")
        .bind(old)
        .execute(&mut *conn)
        .await
        .unwrap();
    let since = OffsetDateTime::now_utc() - time::Duration::days(30);
    assert_eq!(
        usage::spend_since(&mut conn, &[a, b], since).await.unwrap(),
        350
    );
    assert_eq!(usage::spend_since(&mut conn, &[], since).await.unwrap(), 0);
    assert_eq!(
        usage::global_spend_since(&mut conn, since).await.unwrap(),
        1350
    );
}

async fn conversation(pool: &PgPool, account_id: Uuid) -> chat::ConversationRow {
    let mut conn = pool.acquire().await.unwrap();
    chat::create_conversation(
        &mut conn,
        &NewConversation {
            account_id,
            user_id: Uuid::new_v4(),
            time_class: "blitz".into(),
            title: Some("Why do I lose endgames?".into()),
            anchor: None,
        },
    )
    .await
    .unwrap()
}

fn user_msg(conversation_id: Uuid, seq: i32) -> NewMessage {
    NewMessage {
        conversation_id,
        seq,
        role: ChatRole::User,
        status: ChatStatus::Done,
        anchor: None,
        text: Some("hello".into()),
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn duplicate_conversation_seq_is_rejected(pool: PgPool) {
    let acct = account(&pool, "seq").await;
    let conv = conversation(&pool, acct).await;
    let mut conn = pool.acquire().await.unwrap();
    assert_eq!(chat::next_seq(&mut conn, conv.id).await.unwrap(), 1);
    chat::insert_message(&mut conn, &user_msg(conv.id, 1))
        .await
        .unwrap();
    assert_eq!(chat::next_seq(&mut conn, conv.id).await.unwrap(), 2);
    let err = chat::insert_message(&mut conn, &user_msg(conv.id, 1))
        .await
        .unwrap_err();
    assert!(is_unique_violation(&err), "{err:?}");

    // The assistant turn: pending, then streaming, then done with the session path.
    let reply = chat::insert_message(
        &mut conn,
        &NewMessage {
            role: ChatRole::Assistant,
            status: ChatStatus::Pending,
            text: None,
            ..user_msg(conv.id, 2)
        },
    )
    .await
    .unwrap();
    assert!(chat::set_streaming(&mut conn, reply.id).await.unwrap());
    assert!(
        chat::finish_message(
            &mut conn,
            reply.id,
            &MessageFinish {
                text: "Try this.".into(),
                segments: json!([{"kind": "text", "text": "Try this."}]),
                citations: json!([]),
                api_messages: Some(json!([])),
                dropped_count: 0,
                ai_disclosure: Some(json!({"label": "Written by AI"})),
                session: Some(SessionPath::Resumed),
                session_model_usage: Some(json!({"claude-sonnet-5-5": {"inputTokens": 10}})),
            }
        )
        .await
        .unwrap()
    );
    let msgs = chat::list_messages(&mut conn, conv.id).await.unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[1].status, ChatStatus::Done);
    assert_eq!(msgs[1].session, Some(SessionPath::Resumed));
    assert_eq!(msgs[1].role, ChatRole::Assistant);
    // A finished message cannot fail afterwards.
    assert!(
        !chat::fail_message(&mut conn, reply.id, "late")
            .await
            .unwrap()
    );

    assert!(
        chat::set_summary(&mut conn, conv.id, "they asked about endgames", 2)
            .await
            .unwrap()
    );
    let c = chat::get_conversation(&mut conn, conv.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(c.summary_through_seq, 2);
    assert_eq!(c.title.as_deref(), Some("Why do I lose endgames?"));
    assert!(c.last_message_at >= conv.last_message_at);
    assert_eq!(
        chat::list_conversations(&mut conn, acct, 10)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn claim_run_is_atomic(pool: PgPool) {
    let acct = account(&pool, "claim").await;
    let mut conn = pool.acquire().await.unwrap();
    let id = runs::insert_run(&mut conn, &new_run(acct)).await.unwrap();
    drop(conn);
    let claims = (0..2).map(|_| {
        let pool = pool.clone();
        tokio::spawn(async move {
            let mut conn = pool.acquire().await.unwrap();
            runs::claim_run(&mut conn, id).await.unwrap()
        })
    });
    let mut won = 0;
    for c in claims {
        if let Some(run) = c.await.unwrap() {
            won += 1;
            assert_eq!(run.status, RunStatus::Running);
            assert!(run.started_at.is_some() && run.heartbeat_at.is_some());
        }
    }
    assert_eq!(won, 1);

    let mut conn = pool.acquire().await.unwrap();
    assert!(runs::heartbeat(&mut conn, id).await.unwrap());
    assert!(
        runs::snapshot_input(&mut conn, id, &json!({"x": 1}), "ih", "ph")
            .await
            .unwrap()
    );
    let at = OffsetDateTime::now_utc() + time::Duration::minutes(5);
    assert!(
        runs::set_awaiting_retry(&mut conn, id, 1, at, "unavailable")
            .await
            .unwrap()
    );
    let r = runs::get_run(&mut conn, id).await.unwrap().unwrap();
    assert_eq!((r.status, r.retry_count), (RunStatus::AwaitingRetry, 1));
    assert_eq!(r.input, Some(json!({"x": 1})));
    assert_eq!(r.last_error.as_deref(), Some("unavailable"));
    // awaiting_retry can be claimed again.
    assert!(runs::claim_run(&mut conn, id).await.unwrap().is_some());
    let report = uuid_report(&pool, acct, id).await;
    assert!(runs::set_done(&mut conn, id, Some(report)).await.unwrap());
    let r = runs::get_run(&mut conn, id).await.unwrap().unwrap();
    assert_eq!(r.status, RunStatus::Done);
    assert_eq!(r.report_id, Some(report));
    assert!(r.finished_at.is_some());
    // A finished run is never claimed again.
    assert!(runs::claim_run(&mut conn, id).await.unwrap().is_none());
    assert!(!runs::set_failed(&mut conn, id, "late").await.unwrap());
}

async fn uuid_report(pool: &PgPool, account_id: Uuid, run_id: Uuid) -> Uuid {
    let mut conn = pool.acquire().await.unwrap();
    reports::create_report_with_next_version(
        &mut conn,
        &NewReport {
            account_id,
            time_class: "blitz".into(),
            report_run_id: run_id,
            schema_version: "report-payload-v1".into(),
            computed: json!({}),
            status: ReportStatus::ComputedOnly,
        },
    )
    .await
    .unwrap()
    .id
}

async fn event_count(pool: &PgPool, account_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM user_events WHERE account_id = $1")
        .bind(account_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn emit_inserts_an_event_visible_after_commit_only(pool: PgPool) {
    let acct = account(&pool, "emit").await;
    let payload = SectionReady {
        report_id: Uuid::nil(),
        report_run_id: Uuid::nil(),
        section: "area:tactics".into(),
        narrative: true,
    };

    let mut tx = pool.begin().await.unwrap();
    events::emit(&mut tx, acct, events::REPORT_SECTION_READY, &payload)
        .await
        .unwrap();
    assert_eq!(
        event_count(&pool, acct).await,
        0,
        "not visible before commit"
    );
    tx.rollback().await.unwrap();
    assert_eq!(event_count(&pool, acct).await, 0, "rolled back");

    let mut tx = pool.begin().await.unwrap();
    let id = events::emit(&mut tx, acct, events::REPORT_SECTION_READY, &payload)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let (ty, body): (String, Value) =
        sqlx::query_as("SELECT type, payload FROM user_events WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(ty, "report.section.ready");
    assert_eq!(
        body,
        json!({"reportId": Uuid::nil(), "reportRunId": Uuid::nil(), "section": "area:tactics", "narrative": true})
    );
}

#[test]
fn event_payloads_are_camel_case() {
    use ai::events::*;
    assert_eq!(
        serde_json::to_value(ReportDegraded {
            report_id: Uuid::nil(),
            reason: DegradedReason::AiDisabled,
            retry_at: None
        })
        .unwrap(),
        json!({"reportId": Uuid::nil(), "reason": "ai_disabled", "retryAt": null})
    );
    assert_eq!(
        serde_json::to_value(ReportReady {
            report_id: Uuid::nil(),
            status: ReportStatus::ComputedOnly
        })
        .unwrap(),
        json!({"reportId": Uuid::nil(), "status": "computed_only"})
    );
    assert_eq!(
        serde_json::to_value(ChatChunk {
            message_id: Uuid::nil(),
            seq: 3,
            text: "hi".into(),
            segments: vec![],
            citations: vec![],
            interim: true
        })
        .unwrap(),
        json!({"messageId": Uuid::nil(), "seq": 3, "text": "hi", "segments": [], "citations": [], "interim": true})
    );
    assert_eq!(
        serde_json::to_value(NoteReady {
            change_key: "k".into(),
            status: ItemStatus::Dropped
        })
        .unwrap(),
        json!({"changeKey": "k", "status": "dropped"})
    );
    assert_eq!(
        serde_json::to_value(ChatReady {
            message_id: Uuid::nil(),
            replace: false
        })
        .unwrap(),
        json!({"messageId": Uuid::nil(), "replace": false})
    );
    assert_eq!(
        serde_json::to_value(JobFailed {
            kind: "coach-chat".into(),
            reason: "x".into()
        })
        .unwrap(),
        json!({"kind": "coach-chat", "reason": "x"})
    );
    assert_eq!(
        [
            REPORT_SECTION_READY,
            REPORT_READY,
            REPORT_DEGRADED,
            WEEKLY_REVIEW_READY,
            WEAKNESS_NOTE_READY,
            CHAT_CHUNK,
            CHAT_MESSAGE_READY,
            CHAT_MESSAGE_FAILED,
            JOB_FAILED
        ],
        [
            "report.section.ready",
            "report.ready",
            "report.degraded",
            "weekly_review.ready",
            "weakness.note.ready",
            "chat.chunk",
            "chat.message.ready",
            "chat.message.failed",
            "job.failed"
        ]
    );
    let _ = (
        WeeklyReady { id: Uuid::nil() },
        ChatFailed {
            message_id: Uuid::nil(),
            reason: "r".into(),
        },
    );
}

#[test]
fn retry_delay_schedule() {
    let s = Duration::from_secs;
    assert_eq!(RETRY_DELAYS, [s(300), s(1800), s(7200), s(21600), s(86400)]);
    assert_eq!(retry_delay(1, None), Some(s(300)));
    assert_eq!(retry_delay(2, None), Some(s(1800)));
    assert_eq!(retry_delay(5, None), Some(s(86400)));
    assert_eq!(retry_delay(6, None), None);
    assert_eq!(retry_delay(1, Some(s(3600))), Some(s(3600)));
    // A shorter retry-after never shortens the schedule, and a huge one is capped at 24 h.
    assert_eq!(retry_delay(2, Some(s(10))), Some(s(1800)));
    assert_eq!(retry_delay(1, Some(s(10 * 86400))), Some(s(86400)));
}

async fn job_rows(
    pool: &PgPool,
    kind: &str,
) -> Vec<(Option<String>, Option<Uuid>, i16, i32, Value)> {
    sqlx::query_as(
        "SELECT dedupe_key, user_key, priority, max_attempts, payload FROM jobs WHERE kind = $1",
    )
    .bind(kind)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn enqueue_report_twice_is_one_job(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let run = Uuid::new_v4();
    let acct = Uuid::new_v4();
    assert!(enqueue_report(&q, run, acct).await.unwrap().is_some());
    assert!(enqueue_report(&q, run, acct).await.unwrap().is_none());
    let rows = job_rows(&pool, kinds::KIND_GENERATE_REPORT).await;
    assert_eq!(
        rows,
        [(
            Some(format!("report:{run}")),
            Some(acct),
            50,
            3,
            json!({"reportRunId": run})
        )]
    );

    let msg = Uuid::new_v4();
    kinds::enqueue_chat(&q, msg, acct).await.unwrap();
    assert_eq!(
        job_rows(&pool, kinds::KIND_COACH_CHAT).await,
        [(
            Some(format!("chat:{msg}")),
            Some(acct),
            10,
            2,
            json!({"messageId": msg})
        )]
    );
    let note = Uuid::new_v4();
    kinds::enqueue_change(&q, note, acct).await.unwrap();
    assert_eq!(
        job_rows(&pool, kinds::KIND_EXPLAIN_CHANGE).await,
        [(
            Some(format!("wchange:{note}")),
            Some(acct),
            100,
            3,
            json!({"noteId": note})
        )]
    );
    let wk = Uuid::new_v4();
    kinds::enqueue_weekly(&q, wk, acct).await.unwrap();
    assert_eq!(
        job_rows(&pool, kinds::KIND_WEEKLY_REVIEW).await,
        [(
            Some(format!("weekly:{wk}")),
            Some(acct),
            100,
            3,
            json!({"weeklyReviewId": wk})
        )]
    );
    let ev = Uuid::new_v4();
    kinds::enqueue_interactive(&q, ev, acct).await.unwrap();
    assert_eq!(
        job_rows(&pool, kinds::KIND_INTERACTIVE_ANALYSE).await,
        [(
            Some(format!("ieval:{ev}")),
            Some(acct),
            1,
            1,
            json!({"evalId": ev})
        )]
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn ensure_ai_recurring_is_idempotent(pool: PgPool) {
    let q = Queue::new(pool.clone());
    ensure_ai_recurring(&q).await.unwrap();
    ensure_ai_recurring(&q).await.unwrap();
    let rows: Vec<(String, f64, OffsetDateTime)> = sqlx::query_as(
        "SELECT kind, extract(epoch FROM recur_every)::float8, run_at FROM jobs ORDER BY kind",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let kinds: Vec<_> = rows.iter().map(|r| (r.0.as_str(), r.1)).collect();
    assert_eq!(
        kinds,
        [
            ("ai-batch-poll", 600.0),
            ("ai-reconcile", 300.0),
            ("report-batch-schedule", 86400.0),
            ("weekly-batch-schedule", 604800.0),
        ]
    );
    let report = rows[2].2;
    assert_eq!((report.hour(), report.minute(), report.second()), (3, 0, 0));
    let weekly = rows[3].2;
    assert_eq!(weekly.weekday(), time::Weekday::Monday);
    assert_eq!((weekly.hour(), weekly.minute()), (4, 0));
}

#[test]
fn next_run_times() {
    use time::macros::datetime;
    // Wednesday 2026-11-25.
    let now = datetime!(2026-11-25 02:59 UTC);
    assert_eq!(
        kinds::next_daily_at(now, 3),
        datetime!(2026-11-25 03:00 UTC)
    );
    let now = datetime!(2026-11-25 03:00 UTC);
    assert_eq!(
        kinds::next_daily_at(now, 3),
        datetime!(2026-11-26 03:00 UTC)
    );
    assert_eq!(
        kinds::next_monday_at(now, 4),
        datetime!(2026-11-30 04:00 UTC)
    );
    let monday_early = datetime!(2026-11-30 03:00 UTC);
    assert_eq!(
        kinds::next_monday_at(monday_early, 4),
        datetime!(2026-11-30 04:00 UTC)
    );
    let monday_late = datetime!(2026-11-30 04:00 UTC);
    assert_eq!(
        kinds::next_monday_at(monday_late, 4),
        datetime!(2026-12-07 04:00 UTC)
    );
}

fn eval(account_id: Uuid, message_id: Option<Uuid>) -> NewEval {
    NewEval {
        account_id,
        message_id,
        protocol: "sf-test".into(),
        base_fen: "startpos".into(),
        moves: vec!["e2e4".into()],
        fen: "fen-after".into(),
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn interactive_evals_accept_only_the_interactive_source(pool: PgPool) {
    let acct = account(&pool, "source").await;
    let mut conn = pool.acquire().await.unwrap();
    let e = interactive::insert_eval(&mut conn, &eval(acct, None))
        .await
        .unwrap();
    assert_eq!(e.source, "interactive");
    let err = sqlx::query(
        "INSERT INTO interactive_evals (account_id, source, protocol, base_fen, fen) \
         VALUES ($1, 'stockfish', 'p', 'b', 'f')",
    )
    .bind(acct)
    .execute(&mut *conn)
    .await
    .unwrap_err();
    assert!(is_check_violation(&err), "{err:?}");

    // Quota, completion, reuse and refunds.
    let since = OffsetDateTime::now_utc() - time::Duration::hours(1);
    assert_eq!(
        interactive::charged_since(&mut conn, acct, since)
            .await
            .unwrap(),
        1
    );
    assert!(
        interactive::reusable(
            &mut conn,
            acct,
            "fen-after",
            "sf-test",
            interactive::REUSE_WITHIN
        )
        .await
        .unwrap()
        .is_none(),
        "a pending eval is not reusable"
    );
    assert!(
        interactive::complete_eval(&mut conn, e.id, &json!([{"pv": []}]))
            .await
            .unwrap()
    );
    let hit = interactive::reusable(
        &mut conn,
        acct,
        "fen-after",
        "sf-test",
        interactive::REUSE_WITHIN,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(hit.id, e.id);
    assert!(
        interactive::reusable(
            &mut conn,
            acct,
            "fen-after",
            "other",
            interactive::REUSE_WITHIN
        )
        .await
        .unwrap()
        .is_none()
    );
    let f = interactive::insert_eval(&mut conn, &eval(acct, None))
        .await
        .unwrap();
    assert!(
        interactive::fail_eval(&mut conn, f.id, "engine crashed", true)
            .await
            .unwrap()
    );
    assert_eq!(
        interactive::charged_since(&mut conn, acct, since)
            .await
            .unwrap(),
        1
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn deleting_a_conversation_cascades_messages_and_detaches_evals(pool: PgPool) {
    let acct = account(&pool, "delete").await;
    let other = account(&pool, "other").await;
    let conv = conversation(&pool, acct).await;
    let keep = conversation(&pool, acct).await;
    let foreign = conversation(&pool, other).await;
    let mut conn = pool.acquire().await.unwrap();
    let msg = chat::insert_message(&mut conn, &user_msg(conv.id, 1))
        .await
        .unwrap();
    let e = interactive::insert_eval(&mut conn, &eval(acct, None))
        .await
        .unwrap();
    assert_eq!(
        interactive::link_message(&mut conn, &[e.id], msg.id)
            .await
            .unwrap(),
        1
    );
    for (session, n) in [(conv.id, 3), (keep.id, 1), (foreign.id, 2)] {
        for seq in 0..n {
            sqlx::query(
                "INSERT INTO ai_sdk_session_entries (project_key, session_id, subpath, seq, entry) \
                 VALUES ('p', $1, '', $2, '{}')",
            )
            .bind(session)
            .bind(seq)
            .execute(&mut *conn)
            .await
            .unwrap();
        }
    }
    sqlx::query(
        "INSERT INTO ai_sdk_session_entries (project_key, session_id, subpath, seq, entry) \
         VALUES ('p', $1, 'subagents/x', 0, '{}')",
    )
    .bind(conv.id)
    .execute(&mut *conn)
    .await
    .unwrap();

    // Another account cannot delete it.
    assert!(
        !chat::delete_conversation(&mut conn, other, conv.id)
            .await
            .unwrap()
    );
    let mut tx = conn.begin().await.unwrap();
    assert!(
        chat::delete_conversation(&mut tx, acct, conv.id)
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();

    let entries = |id: Uuid| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM ai_sdk_session_entries WHERE session_id = $1",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert_eq!(entries(conv.id).await, 0);
    assert_eq!(entries(keep.id).await, 1);
    let msgs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM coach_messages WHERE conversation_id = $1")
            .bind(conv.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(msgs, 0);
    let (message_id, charged): (Option<Uuid>, bool) =
        sqlx::query_as("SELECT message_id, charged FROM interactive_evals WHERE id = $1")
            .bind(e.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((message_id, charged), (None, true));
    let since = OffsetDateTime::now_utc() - time::Duration::hours(1);
    assert_eq!(
        interactive::charged_since(&mut conn, acct, since)
            .await
            .unwrap(),
        1,
        "deleting does not reset the quota"
    );

    // Delete all: only this account's conversations and their session entries go.
    assert_eq!(
        chat::delete_all_conversations(&mut conn, acct)
            .await
            .unwrap(),
        1
    );
    assert_eq!(entries(keep.id).await, 0);
    assert_eq!(entries(foreign.id).await, 2);
    assert_eq!(
        chat::list_conversations(&mut conn, acct, 10)
            .await
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        chat::list_conversations(&mut conn, other, 10)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn notes_weekly_and_batches_round_trip(pool: PgPool) {
    let acct = account(&pool, "misc").await;
    let mut conn = pool.acquire().await.unwrap();

    let note = notes::insert_note_if_absent(
        &mut conn,
        &NewNote {
            account_id: acct,
            change_key: "ck".into(),
            series_key: "sk".into(),
            onset_date: Some(time::macros::date!(2026 - 11 - 01)),
            input: json!({}),
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(note.status, ItemStatus::Pending);
    assert!(
        notes::insert_note_if_absent(
            &mut conn,
            &NewNote {
                account_id: acct,
                change_key: "ck".into(),
                series_key: "sk".into(),
                onset_date: None,
                input: json!({}),
            },
        )
        .await
        .unwrap()
        .is_none()
    );
    assert!(
        notes::set_note_ready(
            &mut conn,
            note.id,
            &notes::NoteContent {
                text: "t".into(),
                segments: json!([]),
                citations: json!([]),
                model: "claude-haiku-5-5".into(),
                ai_disclosure: json!({}),
            }
        )
        .await
        .unwrap()
    );
    let n = notes::get_note_by_key(&mut conn, acct, "ck")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(n.status, ItemStatus::Ready);
    assert_eq!(n.text.as_deref(), Some("t"));

    let wk = weekly::insert_weekly_if_absent(
        &mut conn,
        &NewWeekly {
            account_id: acct,
            time_class: "blitz".into(),
            week_start: time::macros::date!(2026 - 11 - 16),
            mode: ReportMode::Batch,
            input: json!({}),
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        (wk.status, wk.mode),
        (ItemStatus::Pending, ReportMode::Batch)
    );
    let at = OffsetDateTime::now_utc() - time::Duration::minutes(1);
    assert!(
        weekly::set_weekly_retry(&mut conn, wk.id, 1, at)
            .await
            .unwrap()
    );
    let due = weekly::pending_weekly_due(&mut conn, 10).await.unwrap();
    assert_eq!(due.len(), 1);
    assert!(weekly::set_weekly_failed(&mut conn, wk.id).await.unwrap());
    assert!(
        weekly::pending_weekly_due(&mut conn, 10)
            .await
            .unwrap()
            .is_empty()
    );

    let run = runs::insert_run(&mut conn, &new_run(acct)).await.unwrap();
    batches::insert_batch(&mut conn, "msgbatch_1", BatchKind::Report, 1)
        .await
        .unwrap();
    batches::insert_items(
        &mut conn,
        "msgbatch_1",
        &[NewBatchItem {
            id: Uuid::new_v4(),
            custom_id: "r-1".into(),
            kind: BatchKind::Report,
            report_run_id: Some(run),
            section: Some(SectionKey::Overview),
            weekly_review_id: None,
        }],
    )
    .await
    .unwrap();
    assert_eq!(batches::open_batches(&mut conn).await.unwrap().len(), 1);
    let item = batches::item_by_custom_id(&mut conn, "r-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(item.section, Some(SectionKey::Overview));
    assert!(
        batches::mark_item_processed(&mut conn, item.id)
            .await
            .unwrap()
    );
    assert!(
        !batches::mark_item_processed(&mut conn, item.id)
            .await
            .unwrap()
    );
    assert!(
        batches::pending_items(&mut conn, "msgbatch_1")
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        batches::mark_processed(&mut conn, "msgbatch_1")
            .await
            .unwrap()
    );
    assert!(batches::open_batches(&mut conn).await.unwrap().is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn duplicate_session_entry_uuid_is_a_no_op(pool: PgPool) {
    let session = Uuid::new_v4();
    let insert = |seq: i64, subpath: &'static str, entry: Value| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO ai_sdk_session_entries (project_key, session_id, subpath, seq, entry) \
                 VALUES ('p', $1, $2, $3, $4) ON CONFLICT DO NOTHING",
            )
            .bind(session)
            .bind(subpath)
            .bind(seq)
            .bind(entry)
            .execute(&pool)
            .await
            .unwrap()
            .rows_affected()
        }
    };
    assert_eq!(insert(0, "", json!({"uuid": "u1", "n": 1})).await, 1);
    assert_eq!(
        insert(1, "", json!({"uuid": "u1", "n": 2})).await,
        0,
        "same uuid"
    );
    assert_eq!(insert(1, "", json!({"uuid": "u2"})).await, 1);
    assert_eq!(
        insert(2, "sub", json!({"uuid": "u1"})).await,
        1,
        "other subpath"
    );
    assert_eq!(
        insert(3, "", json!({"type": "x"})).await,
        1,
        "no uuid: not deduped"
    );
    assert_eq!(insert(4, "", json!({"type": "x"})).await, 1);
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM ai_sdk_session_entries WHERE session_id = $1")
            .bind(session)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(n, 5);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn sidecar_role_reaches_only_the_session_table(pool: PgPool) {
    // Every privilege the role holds on any relation or sequence in `public` (direct or through
    // PUBLIC), checked with has_*_privilege.
    let held: Vec<(String, String)> = sqlx::query_as(
        "SELECT c.relname::text, p.priv FROM pg_class c \
         JOIN pg_namespace n ON n.oid = c.relnamespace AND n.nspname = 'public' \
         CROSS JOIN LATERAL unnest(CASE WHEN c.relkind = 'S' \
           THEN ARRAY['USAGE', 'SELECT', 'UPDATE'] \
           ELSE ARRAY['SELECT', 'INSERT', 'UPDATE', 'DELETE', 'TRUNCATE', 'REFERENCES', \
                      'TRIGGER', 'MAINTAIN'] END) AS p(priv) \
         WHERE c.relkind IN ('r', 'p', 'v', 'm', 'f', 'S') \
         AND CASE WHEN c.relkind = 'S' THEN has_sequence_privilege('ai_sidecar', c.oid, p.priv) \
                  ELSE has_table_privilege('ai_sidecar', c.oid, p.priv) END \
         ORDER BY 1, 2",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let relations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = 'public' AND c.relkind IN ('r', 'p', 'v', 'm', 'f', 'S')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        relations > 20,
        "the check saw the schema ({relations} relations)"
    );
    let session = |p: &str| ("ai_sdk_session_entries".to_owned(), p.to_owned());
    assert_eq!(
        held,
        [session("DELETE"), session("INSERT"), session("SELECT")]
    );
    let usage: bool =
        sqlx::query_scalar("SELECT has_schema_privilege('ai_sidecar', 'public', 'USAGE')")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(usage);
}

async fn add_entries(pool: &PgPool, session: Uuid, n: i64) {
    for seq in 0..n {
        sqlx::query(
            "INSERT INTO ai_sdk_session_entries (project_key, session_id, subpath, seq, entry) \
             VALUES ('p', $1, '', $2, '{}')",
        )
        .bind(session)
        .bind(seq)
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn session_entries(pool: &PgPool, session: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM ai_sdk_session_entries WHERE session_id = $1")
        .bind(session)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn every_conversation_delete_path_removes_session_entries(pool: PgPool) {
    let acct = account(&pool, "paths").await;
    let rust = conversation(&pool, acct).await;
    let raw = conversation(&pool, acct).await;
    let cascaded = conversation(&pool, acct).await;
    for c in [&rust, &raw, &cascaded] {
        add_entries(&pool, c.id, 2).await;
    }

    let mut conn = pool.acquire().await.unwrap();
    assert!(
        chat::delete_conversation(&mut conn, acct, rust.id)
            .await
            .unwrap()
    );
    assert_eq!(session_entries(&pool, rust.id).await, 0, "Rust delete");

    sqlx::query("DELETE FROM coach_conversations WHERE id = $1")
        .bind(raw.id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(session_entries(&pool, raw.id).await, 0, "raw SQL delete");

    // Unlinking the account cascades to its conversations, and the trigger fires for each.
    // (Report runs etc. cascade too; nothing else references this account.)
    sqlx::query("DELETE FROM linked_accounts WHERE id = $1")
        .bind(acct)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        session_entries(&pool, cascaded.id).await,
        0,
        "linked_accounts cascade"
    );
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_sdk_session_entries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);

    // A rolled-back delete keeps the entries.
    let acct2 = account(&pool, "rollback").await;
    let kept = conversation(&pool, acct2).await;
    add_entries(&pool, kept.id, 1).await;
    let mut tx = pool.begin().await.unwrap();
    assert!(
        chat::delete_conversation(&mut tx, acct2, kept.id)
            .await
            .unwrap()
    );
    tx.rollback().await.unwrap();
    assert_eq!(session_entries(&pool, kept.id).await, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn orphan_sweep_removes_only_old_entries_without_a_conversation(pool: PgPool) {
    let acct = account(&pool, "sweep").await;
    let live = conversation(&pool, acct).await;
    let old_orphan = Uuid::new_v4();
    let new_orphan = Uuid::new_v4();
    for id in [live.id, old_orphan, new_orphan] {
        add_entries(&pool, id, 2).await;
    }
    sqlx::query(
        "UPDATE ai_sdk_session_entries SET created_at = now() - interval '11 minutes' \
         WHERE session_id = ANY($1)",
    )
    .bind(vec![live.id, old_orphan])
    .execute(&pool)
    .await
    .unwrap();
    let mut conn = pool.acquire().await.unwrap();
    let swept = chat::sweep_orphan_session_entries(&mut conn, OffsetDateTime::now_utc())
        .await
        .unwrap();
    assert_eq!(swept, 2);
    assert_eq!(session_entries(&pool, old_orphan).await, 0);
    assert_eq!(
        session_entries(&pool, new_orphan).await,
        2,
        "younger than 10 min"
    );
    assert_eq!(
        session_entries(&pool, live.id).await,
        2,
        "has a conversation"
    );
    assert_eq!(chat::ORPHAN_SESSION_MIN_AGE, Duration::from_secs(600));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn previous_session_model_usage_is_the_last_metered_assistant_turn(pool: PgPool) {
    let acct = account(&pool, "usage").await;
    let conv = conversation(&pool, acct).await;
    let mut conn = pool.acquire().await.unwrap();
    let assistant = |seq| NewMessage {
        role: ChatRole::Assistant,
        status: ChatStatus::Pending,
        text: None,
        ..user_msg(conv.id, seq)
    };
    let finish = |usage: Option<Value>| MessageFinish {
        text: "t".into(),
        segments: json!([]),
        citations: json!([]),
        api_messages: None,
        dropped_count: 0,
        ai_disclosure: None,
        session: Some(SessionPath::Resumed),
        session_model_usage: usage,
    };
    assert_eq!(
        chat::previous_session_model_usage(&mut conn, conv.id, 1)
            .await
            .unwrap(),
        None
    );
    chat::insert_message(&mut conn, &user_msg(conv.id, 1))
        .await
        .unwrap();
    let a2 = chat::insert_message(&mut conn, &assistant(2))
        .await
        .unwrap();
    let u2 = json!({"claude-sonnet-5-5": {"inputTokens": 100, "outputTokens": 10}});
    chat::finish_message(&mut conn, a2.id, &finish(Some(u2.clone())))
        .await
        .unwrap();
    chat::insert_message(&mut conn, &user_msg(conv.id, 3))
        .await
        .unwrap();
    let a4 = chat::insert_message(&mut conn, &assistant(4))
        .await
        .unwrap();
    // A failed turn stores nothing; the previous value is still turn 2's.
    chat::fail_message(&mut conn, a4.id, "unavailable")
        .await
        .unwrap();
    chat::insert_message(&mut conn, &user_msg(conv.id, 5))
        .await
        .unwrap();
    let a6 = chat::insert_message(&mut conn, &assistant(6))
        .await
        .unwrap();
    assert_eq!(
        chat::previous_session_model_usage(&mut conn, conv.id, 6)
            .await
            .unwrap(),
        Some(u2.clone())
    );
    let u6 = json!({"claude-sonnet-5-5": {"inputTokens": 250, "outputTokens": 30}});
    chat::finish_message(&mut conn, a6.id, &finish(Some(u6.clone())))
        .await
        .unwrap();
    assert_eq!(
        chat::previous_session_model_usage(&mut conn, conv.id, 7)
            .await
            .unwrap(),
        Some(u6.clone())
    );
    assert_eq!(
        chat::previous_session_model_usage(&mut conn, conv.id, 6)
            .await
            .unwrap(),
        Some(u2)
    );
    let stored = chat::get_message(&mut conn, a6.id).await.unwrap().unwrap();
    assert_eq!(stored.session_model_usage, Some(u6));
}

/// The plans of `sql`, prepared as `name(types)` and run with the literal `args`, under the
/// custom and the generic plan (sqlx caches its statements, and Postgres may switch a cached
/// statement to the generic plan), with sequential and bitmap scans disabled: on empty test
/// tables the planner would otherwise pick a seq scan or a bitmap scan plus a sort whatever the
/// indexes.
async fn plans(pool: &PgPool, name: &str, types: &str, sql: &str, args: &str) -> Vec<String> {
    let mut tx = pool.begin().await.unwrap();
    for off in ["enable_seqscan", "enable_bitmapscan"] {
        sqlx::query(sqlx::AssertSqlSafe(format!("SET LOCAL {off} = off")))
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    let types = if types.is_empty() {
        String::new()
    } else {
        format!("({types})")
    };
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "PREPARE {name}{types} AS {sql}"
    )))
    .execute(&mut *tx)
    .await
    .unwrap();
    let args = if args.is_empty() {
        String::new()
    } else {
        format!("({args})")
    };
    let mut out = Vec::new();
    for mode in ["force_custom_plan", "force_generic_plan"] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "SET LOCAL plan_cache_mode = {mode}"
        )))
        .execute(&mut *tx)
        .await
        .unwrap();
        let plan: Vec<String> =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("EXPLAIN EXECUTE {name}{args}")))
                .fetch_all(&mut *tx)
                .await
                .unwrap();
        out.push(format!("{name} {mode}:\n{}", plan.join("\n")));
    }
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DEALLOCATE {name}")))
        .execute(&mut *tx)
        .await
        .unwrap();
    out
}

/// The plan reads `index` in order: no sort and no seq scan.
fn assert_ordered_by(plan: &str, index: &str) {
    assert!(
        plan.contains(&format!("Index Scan using {index}")),
        "expected {index} in\n{plan}"
    );
    assert!(
        !plan.contains("Sort") && !plan.contains("Seq Scan"),
        "{plan}"
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn reconciler_scans_use_their_partial_indexes(pool: PgPool) {
    // All live statuses (literals): one ordered read of the partial index, in both plans.
    for p in plans(&pool, "live", "int8", &runs::live_runs_sql(), "50").await {
        assert_ordered_by(&p, "report_runs_live_idx");
    }
    // Each status alone, generic plan included: an index, never a seq scan. The literal implies
    // the live partial predicate, but on these empty tables `status_idx` plus a small sort is
    // as cheap; the reconciler's keyset pages (`ai_reconcile.rs`) are guarded on real volumes.
    for status in [
        RunStatus::Queued,
        RunStatus::Running,
        RunStatus::AwaitingRetry,
        RunStatus::AwaitingBatch,
        RunStatus::Done,
        RunStatus::Failed,
    ] {
        let sql = runs::runs_with_status_sql(status);
        for p in plans(&pool, "one", "int8", &sql, "50").await {
            assert!(
                p.contains("Index Scan using report_runs_live_idx")
                    || p.contains("Index Scan using report_runs_status_idx"),
                "{p}"
            );
            assert!(!p.contains("Seq Scan"), "{p}");
        }
    }
    let sql = interactive::stale_pending_sql();
    for p in plans(&pool, "evals", "float8, int8", &sql, "120, 50").await {
        assert_ordered_by(&p, "interactive_evals_pending_idx");
    }
    for p in plans(&pool, "batches", "", &batches::open_batches_sql(), "").await {
        assert_ordered_by(&p, "ai_batches_open_idx");
    }
    assert!(
        !batches::open_batches_sql().contains("  "),
        "{}",
        batches::open_batches_sql()
    );
    assert!(
        !runs::live_runs_sql().contains("  "),
        "{}",
        runs::live_runs_sql()
    );

    // And the queries still filter.
    let acct = account(&pool, "plans").await;
    let mut conn = pool.acquire().await.unwrap();
    let id = runs::insert_run(&mut conn, &new_run(acct)).await.unwrap();
    let found = runs::runs_with_status(&mut conn, RunStatus::Queued, 10)
        .await
        .unwrap();
    assert_eq!(found.iter().map(|r| r.id).collect::<Vec<_>>(), [id]);
    assert_eq!(
        runs::live_runs(&mut conn, 10)
            .await
            .unwrap()
            .iter()
            .map(|r| r.id)
            .collect::<Vec<_>>(),
        [id]
    );
    assert!(runs::set_done(&mut conn, id, None).await.unwrap());
    assert!(
        runs::runs_with_status(&mut conn, RunStatus::Queued, 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(runs::live_runs(&mut conn, 10).await.unwrap().is_empty());
    let done = runs::runs_with_status(&mut conn, RunStatus::Done, 10)
        .await
        .unwrap();
    assert_eq!(done.len(), 1);
}
