//! Task 25: the `coach-chat` job. One pending assistant message in, one stored, validated answer
//! out, against the T12 fake client only (no network, no API key). The allowance is reserved
//! atomically, turns on one conversation never interleave, and the call-index counter and the
//! SDK in-sync flag are stored with the conversation.

#[path = "support/chat.rs"]
mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use ai::chat::anchor::ChatAnchor;
use ai::chat::interactive::InteractiveAnalyseHandler;
use ai::chat::job::{
    CHAT_CALL_TIMEOUT, CHAT_QUOTA_LOCK_CLASS, CHAT_UNAVAILABLE_RELEASE, CoachChatHandler,
};
use ai::chat::limits::{CHAT_LIMITS, CHAT_NO_DATA_TEXT};
use ai::chat::threads::{create_conversation, post_user_message};
use ai::client::fake::{
    FakeAiClient, FakeCallKind, FakeScript, FakeStructured, FakeTurn, TurnScript,
};
use ai::client::port::{
    AiClient, AiError, BatchItem, BatchResultItem, BatchStatus, ChatTurnRequest, SchemaResolver,
    StructuredRequest, StructuredResult, ToolHandler, ToolReply, TurnEvent, UnavailableReason,
};
use ai::client::sidecar::{SidecarConfig, SidecarToken};
use ai::entitlements::Plan;
use ai::input::types::TimeClass;
use ai::jobs::kinds::CHAT_TURN_TIMEOUT;
use ai::segments::Segment;
use chess_core::EngineScore;
use engine::{AnalyseOptions, Analyser, AnalysisLine, AnalysisResult, EngineError};
use futures::StreamExt;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use queue::queue::Queue;
use queue::runner::{Handler, JobContext, Runner, RunnerConfig};
use queue::types::JobError;
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{
    OK2, OK3, account, answer, deps, events, job, seed_game, seed_move, statuses, store_input,
    usage_rows,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const AFTER_E4_E5: &str = "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2";
const BAD: &str = "You could play Qh5 here.";

fn ctx() -> JobContext {
    JobContext {
        cancel: CancellationToken::new(),
    }
}

fn fake(turns: Vec<FakeTurn>) -> Arc<FakeAiClient> {
    fake_with(turns, None, None)
}

fn fake_with(
    turns: Vec<FakeTurn>,
    structured: Option<Vec<FakeStructured>>,
    delay: Option<Duration>,
) -> Arc<FakeAiClient> {
    let turns: Vec<TurnScript> = turns
        .into_iter()
        .map(|t| -> TurnScript { Box::new(move |_, _| Ok(t.clone())) })
        .collect();
    Arc::new(FakeAiClient::new(FakeScript {
        turns,
        structured: structured.map(|s| -> ai::client::fake::StructuredScript {
            Box::new(move |_, n| {
                s.get(n)
                    .cloned()
                    .ok_or_else(|| AiError::Response("fake: no structured".into()))
            })
        }),
        delay,
        ..FakeScript::default()
    }))
}

fn unavailable() -> AiError {
    AiError::Unavailable {
        reason: UnavailableReason::Server,
        retry_after: None,
    }
}

fn turn_requests(f: &FakeAiClient) -> Vec<Value> {
    f.calls()
        .into_iter()
        .filter(|c| c.kind == FakeCallKind::Turn)
        .map(|c| c.request_json)
        .collect()
}

/// A rapid conversation with a title (so no title call is made) and a posted question.
async fn ask(pool: &PgPool, a: Uuid, question: &str) -> (Uuid, Uuid) {
    let c = create_conversation(pool, a, None, TimeClass::Rapid, None)
        .await
        .unwrap();
    set_title(pool, c).await;
    let (_, m) = post_user_message(pool, &Queue::new(pool.clone()), a, c, question, None)
        .await
        .unwrap();
    (c, m)
}

async fn set_title(pool: &PgPool, c: Uuid) {
    sqlx::query("UPDATE coach_conversations SET title = 'Chat' WHERE id = $1")
        .bind(c)
        .execute(pool)
        .await
        .unwrap();
}

async fn ask_again(pool: &PgPool, a: Uuid, c: Uuid, question: &str) -> Uuid {
    post_user_message(pool, &Queue::new(pool.clone()), a, c, question, None)
        .await
        .unwrap()
        .1
}

async fn run(
    pool: &PgPool,
    f: &Arc<FakeAiClient>,
    plan: Plan,
    msg: Uuid,
) -> Result<Value, JobError> {
    CoachChatHandler(deps(pool, Some(f.clone()), plan))
        .handle(job(msg, 1), ctx())
        .await
}

#[derive(sqlx::FromRow, Debug)]
struct Stored {
    status: String,
    text: Option<String>,
    segments: Option<Value>,
    api_messages: Option<Value>,
    ai_disclosure: Option<Value>,
    failure_reason: Option<String>,
    session_in_sync: Option<bool>,
}

async fn stored(pool: &PgPool, msg: Uuid) -> Stored {
    sqlx::query_as(
        "SELECT status::text AS status, text, segments, api_messages, ai_disclosure, \
         failure_reason, session_in_sync FROM coach_messages WHERE id = $1",
    )
    .bind(msg)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn conv_state(pool: &PgPool, c: Uuid) -> (Option<String>, Option<String>, i32, i32) {
    sqlx::query_as(
        "SELECT title, summary, summary_through_seq, next_call_index FROM coach_conversations \
         WHERE id = $1",
    )
    .bind(c)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// `n` done, AI-answered assistant turns of the account in a separate conversation, created now.
async fn seed_done_answers(pool: &PgPool, a: Uuid, n: i32) {
    let c = create_conversation(pool, a, None, TimeClass::Rapid, None)
        .await
        .unwrap();
    for i in 0..n {
        seed_turn(pool, c, 2 * i + 1, "Earlier?", OK2, Some(false)).await;
    }
}

/// A finished turn (`seq` user, `seq + 1` assistant) with its stored API messages.
async fn seed_turn(pool: &PgPool, c: Uuid, seq: i32, q: &str, a: &str, in_sync: Option<bool>) {
    let api = json!([
        {"role": "user", "content": [{"type": "text", "text": format!("<question>{q}</question>")}]},
        {"role": "assistant", "content": [{"type": "text", "text": a}]},
    ]);
    sqlx::query(
        "INSERT INTO coach_messages (conversation_id, seq, role, status, text) \
         VALUES ($1, $2, 'user', 'done', $3)",
    )
    .bind(c)
    .bind(seq)
    .bind(q)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO coach_messages (conversation_id, seq, role, status, text, segments, \
         api_messages, ai_disclosure, session_in_sync) \
         VALUES ($1, $2, 'assistant', 'done', $3, $4, $5, '{\"model\":\"m\"}', $6)",
    )
    .bind(c)
    .bind(seq + 1)
    .bind(a)
    .bind(json!([{"kind": "text", "text": a}]))
    .bind(api)
    .bind(in_sync)
    .execute(pool)
    .await
    .unwrap();
}

fn types(ev: &[(String, Value)]) -> Vec<&str> {
    ev.iter().map(|(t, _)| t.as_str()).collect()
}

// --- The happy path ---

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn job_happy_path(pool: PgPool) {
    let a = account(&pool, "alice").await;
    store_input(&pool, a).await;
    let (c, msg) = ask(&pool, a, "How do I stop hanging pieces?").await;
    let f = fake(vec![answer(&[OK2, OK3])]);
    let r = run(&pool, &f, Plan::Free, msg).await;
    assert!(r.is_ok(), "{r:?}");

    let ev = events(&pool, a).await;
    assert_eq!(
        types(&ev),
        vec!["chat.chunk", "chat.chunk", "chat.message.ready"]
    );
    assert_eq!(ev[0].1["seq"], json!(0));
    assert_eq!(ev[0].1["text"], json!(OK2));
    assert_eq!(ev[1].1["seq"], json!(1));
    assert_eq!(ev[0].1["messageId"], json!(msg));
    assert_eq!(ev[2].1, json!({"messageId": msg, "replace": false}));

    let s = stored(&pool, msg).await;
    assert_eq!(s.status, "done");
    assert_eq!(s.text.as_deref(), Some(format!("{OK2}\n\n{OK3}").as_str()));
    let api = s.api_messages.unwrap();
    assert_eq!(api.as_array().unwrap().len(), 2);
    assert_eq!(api[1]["role"], json!("assistant"));
    let d = s.ai_disclosure.unwrap();
    assert_eq!(d["label"], json!("Written by AI"));
    assert_eq!(d["promptVersion"], json!("chat@1"));
    assert_eq!(s.session_in_sync, Some(true));

    let usage = usage_rows(&pool, a).await;
    assert_eq!(usage.len(), 1, "{usage:?}");
    assert_eq!(usage[0].0, "chat");
    assert_eq!(usage[0].3.as_deref(), Some(msg.to_string().as_str()));
    assert!(usage[0].2 > 0);
    assert_eq!(conv_state(&pool, c).await.3, 0);
    // A finished message is not run again.
    assert!(run(&pool, &f, Plan::Free, msg).await.is_ok());
    assert_eq!(turn_requests(&f).len(), 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn no_ai_fails_unavailable(pool: PgPool) {
    let a = account(&pool, "alice").await;
    store_input(&pool, a).await;
    let (_, msg) = ask(&pool, a, "Why?").await;
    let r = CoachChatHandler(deps(&pool, None, Plan::Free))
        .handle(job(msg, 1), ctx())
        .await;
    assert!(r.is_ok(), "{r:?}");
    let s = stored(&pool, msg).await;
    assert_eq!(
        (s.status.as_str(), s.failure_reason.as_deref()),
        ("failed", Some("unavailable"))
    );
    let ev = events(&pool, a).await;
    assert_eq!(types(&ev), vec!["chat.message.failed"]);
    assert_eq!(ev[0].1["reason"], json!("unavailable"));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn no_coaching_input_replies_with_fixed_copy(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let (_, msg) = ask(&pool, a, "Why?").await;
    let f = fake(vec![]);
    assert!(run(&pool, &f, Plan::Free, msg).await.is_ok());
    let s = stored(&pool, msg).await;
    assert_eq!(s.status, "done");
    assert_eq!(s.text.as_deref(), Some(CHAT_NO_DATA_TEXT));
    assert_eq!(
        s.segments,
        Some(json!([{"kind": "text", "text": CHAT_NO_DATA_TEXT}]))
    );
    assert!(f.calls().is_empty());
    let ev = events(&pool, a).await;
    assert_eq!(
        ev,
        vec![(
            "chat.message.ready".into(),
            json!({"messageId": msg, "replace": false})
        )]
    );
    // Fixed copy is not an AI answer: it does not use the weekly allowance.
    store_input(&pool, a).await;
    for _ in 0..3 {
        let (_, m) = ask(&pool, a, "Why?").await;
        let f = fake(vec![answer(&[OK2])]);
        assert!(run(&pool, &f, Plan::Free, m).await.is_ok());
        assert_eq!(stored(&pool, m).await.status, "done");
    }
}

// --- Allowance ---

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn free_plan_fourth_message_this_week_fails_weekly_cap(pool: PgPool) {
    let a = account(&pool, "alice").await;
    store_input(&pool, a).await;
    seed_done_answers(&pool, a, 3).await;
    let (_, msg) = ask(&pool, a, "Why?").await;
    let f = fake(vec![answer(&[OK2])]);
    assert!(run(&pool, &f, Plan::Free, msg).await.is_ok());
    let s = stored(&pool, msg).await;
    assert_eq!(
        (s.status.as_str(), s.failure_reason.as_deref()),
        ("failed", Some("weekly_cap"))
    );
    assert!(f.calls().is_empty());
    let ev = events(&pool, a).await;
    assert_eq!(
        ev,
        vec![(
            "chat.message.failed".into(),
            json!({"messageId": msg, "reason": "weekly_cap"})
        )]
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn pro_daily_cap_fails_with_30_done_today(pool: PgPool) {
    let a = account(&pool, "alice").await;
    store_input(&pool, a).await;
    seed_done_answers(&pool, a, 29).await;
    let (_, ok) = ask(&pool, a, "Why?").await;
    let f = fake(vec![answer(&[OK2])]);
    assert!(run(&pool, &f, Plan::Pro, ok).await.is_ok());
    assert_eq!(stored(&pool, ok).await.status, "done");
    let (_, msg) = ask(&pool, a, "And?").await;
    let f = fake(vec![answer(&[OK2])]);
    assert!(run(&pool, &f, Plan::Pro, msg).await.is_ok());
    let s = stored(&pool, msg).await;
    assert_eq!(s.failure_reason.as_deref(), Some("daily_cap"));
    assert!(f.calls().is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn parallel_turns_never_overspend_the_chat_quota(pool: PgPool) {
    let a = account(&pool, "alice").await;
    store_input(&pool, a).await;
    seed_done_answers(&pool, a, 2).await;
    let (_, m1) = ask(&pool, a, "One?").await;
    let (_, m2) = ask(&pool, a, "Two?").await;
    let f = fake_with(
        vec![answer(&[OK2]), answer(&[OK2])],
        None,
        Some(Duration::from_millis(300)),
    );
    let h = CoachChatHandler(deps(&pool, Some(f.clone()), Plan::Free));
    let (r1, r2) = tokio::join!(h.handle(job(m1, 1), ctx()), h.handle(job(m2, 1), ctx()));
    assert!(r1.is_ok() && r2.is_ok(), "{r1:?} {r2:?}");
    let mut outcome = vec![
        stored(&pool, m1).await.failure_reason,
        stored(&pool, m2).await.failure_reason,
    ];
    outcome.sort();
    assert_eq!(outcome, vec![None, Some("weekly_cap".to_owned())]);
    assert_eq!(turn_requests(&f).len(), 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_reservation_waits_for_the_account_chat_lock(pool: PgPool) {
    let a = account(&pool, "alice").await;
    store_input(&pool, a).await;
    let (_, msg) = ask(&pool, a, "Why?").await;
    // Another reservation of this account holds the lock.
    let mut holder = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock($1, hashtext($2::text))")
        .bind(CHAT_QUOTA_LOCK_CLASS)
        .bind(a)
        .execute(&mut *holder)
        .await
        .unwrap();
    let f = fake(vec![answer(&[OK2])]);
    let h = CoachChatHandler(deps(&pool, Some(f.clone()), Plan::Free));
    let task = tokio::spawn(async move { h.handle(job(msg, 1), ctx()).await });
    let p = pool.clone();
    testkit::wait_for(
        "a waiting reservation",
        Duration::from_secs(5),
        Duration::from_millis(20),
        || {
            let p = p.clone();
            async move {
                let waiting: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM pg_locks WHERE locktype = 'advisory' \
                     AND classid = $1 AND NOT granted)",
                )
                .bind(CHAT_QUOTA_LOCK_CLASS)
                .fetch_one(&p)
                .await
                .unwrap();
                waiting.then_some(())
            }
        },
    )
    .await;
    assert_eq!(stored(&pool, msg).await.status, "pending");
    assert!(f.calls().is_empty());
    holder.commit().await.unwrap();
    assert!(task.await.unwrap().is_ok());
    assert_eq!(stored(&pool, msg).await.status, "done");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_later_question_waits_for_the_earlier_answer(pool: PgPool) {
    let a = account(&pool, "alice").await;
    store_input(&pool, a).await;
    let (c, first) = ask(&pool, a, "Why?").await;
    sqlx::query(
        "INSERT INTO coach_messages (conversation_id, seq, role, status, text) VALUES \
         ($1, 3, 'user', 'done', 'Again?'), ($1, 4, 'assistant', 'pending', NULL)",
    )
    .bind(c)
    .execute(&pool)
    .await
    .unwrap();
    let later: Uuid =
        sqlx::query_scalar("SELECT id FROM coach_messages WHERE conversation_id = $1 AND seq = 4")
            .bind(c)
            .fetch_one(&pool)
            .await
            .unwrap();
    let f = fake(vec![answer(&[OK2]), answer(&[OK3])]);
    let h = CoachChatHandler(deps(&pool, Some(f.clone()), Plan::Pro));
    let r = h.handle(job(later, 1), ctx()).await;
    assert!(matches!(r, Err(JobError::Release { .. })), "{r:?}");
    assert!(f.calls().is_empty());
    assert_eq!(stored(&pool, later).await.status, "pending");
    assert!(h.handle(job(first, 1), ctx()).await.is_ok());
    assert!(h.handle(job(later, 1), ctx()).await.is_ok());
    assert_eq!(stored(&pool, later).await.text.as_deref(), Some(OK3));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn budget_exhausted_fails_budget(pool: PgPool) {
    let a = account(&pool, "alice").await;
    store_input(&pool, a).await;
    sqlx::query(
        "INSERT INTO ai_usage (account_id, feature, model, input_tokens, output_tokens, \
         cache_creation_tokens, cache_read_tokens, cost_micro_usd) \
         VALUES ($1, 'report', 'm', 1, 1, 0, 0, 200000)",
    )
    .bind(a)
    .execute(&pool)
    .await
    .unwrap();
    let (_, msg) = ask(&pool, a, "Why?").await;
    let f = fake(vec![answer(&[OK2])]);
    assert!(run(&pool, &f, Plan::Free, msg).await.is_ok());
    let s = stored(&pool, msg).await;
    assert_eq!(
        (s.status.as_str(), s.failure_reason.as_deref()),
        ("failed", Some("budget"))
    );
    assert!(f.calls().is_empty());
    let ev = events(&pool, a).await;
    assert_eq!(types(&ev), vec!["chat.message.failed"]);
    assert_eq!(ev[0].1["reason"], json!("budget"));
}

// --- Errors ---

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn unavailable_first_attempt_releases_last_attempt_fails(pool: PgPool) {
    let a = account(&pool, "alice").await;
    store_input(&pool, a).await;
    let (_, msg) = ask(&pool, a, "Why?").await;
    let scripts: Vec<TurnScript> = vec![
        Box::new(|_, _| Err(unavailable())),
        Box::new(|_, _| Err(unavailable())),
    ];
    let f = Arc::new(FakeAiClient::new(FakeScript {
        turns: scripts,
        ..FakeScript::default()
    }));
    let h = CoachChatHandler(deps(&pool, Some(f.clone()), Plan::Free));
    match h.handle(job(msg, 1), ctx()).await {
        Err(JobError::Release { delay, .. }) => assert_eq!(delay, CHAT_UNAVAILABLE_RELEASE),
        other => panic!("expected a release, got {other:?}"),
    }
    assert_eq!(CHAT_UNAVAILABLE_RELEASE, Duration::from_secs(60));
    assert_eq!(stored(&pool, msg).await.status, "pending");
    assert!(events(&pool, a).await.is_empty());

    // A release gives the attempt back, so the job comes back as attempt 1 again: the turn's
    // own counter decides that this is the last try.
    assert!(h.handle(job(msg, 1), ctx()).await.is_ok());
    let s = stored(&pool, msg).await;
    assert_eq!(
        (s.status.as_str(), s.failure_reason.as_deref()),
        ("failed", Some("unavailable"))
    );
    let ev = events(&pool, a).await;
    assert_eq!(types(&ev), vec!["chat.message.failed"]);
    assert_eq!(turn_requests(&f).len(), 2);
}

#[test]
fn per_call_timeout_fits_the_turn_timeout() {
    // Up to three sequential calls per turn (main, follow-up, repair; T23 review m5).
    assert!(CHAT_CALL_TIMEOUT * 3 <= CHAT_TURN_TIMEOUT);
    let cfg = SidecarConfig::new("http://sidecar", SidecarToken::new("t"));
    assert!(cfg.chat_timeout <= CHAT_CALL_TIMEOUT);
}

// --- History ---

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn second_turn_replays_history_verbatim(pool: PgPool) {
    let a = account(&pool, "alice").await;
    store_input(&pool, a).await;
    let c = create_conversation(&pool, a, None, TimeClass::Rapid, None)
        .await
        .unwrap();
    set_title(&pool, c).await;
    seed_turn(&pool, c, 1, "First?", OK3, Some(false)).await;
    let msg = ask_again(&pool, a, c, "Second?").await;
    let f = fake(vec![answer(&[OK2])]);
    assert!(run(&pool, &f, Plan::Free, msg).await.is_ok());
    let req = &turn_requests(&f)[0];
    assert_eq!(req["resume"], json!(false));
    let msgs = req["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 4, "{msgs:?}");
    assert_eq!(
        msgs[1]["content"][0]["text"],
        json!("<question>First?</question>")
    );
    assert_eq!(msgs[2]["role"], json!("assistant"));
    assert_eq!(msgs[2]["content"][0]["text"], json!(OK3));
    assert_eq!(
        msgs[3]["content"][0]["text"],
        json!("<question>Second?</question>")
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn seventh_turn_rolls_over_into_a_summary(pool: PgPool) {
    let a = account(&pool, "alice").await;
    store_input(&pool, a).await;
    let c = create_conversation(&pool, a, None, TimeClass::Rapid, None)
        .await
        .unwrap();
    set_title(&pool, c).await;
    let n = CHAT_LIMITS.replay_turns as i32;
    for i in 0..n {
        seed_turn(&pool, c, 2 * i + 1, "Earlier?", OK3, Some(true)).await;
    }
    let msg = ask_again(&pool, a, c, "Seventh?").await;
    let summary = FakeStructured {
        parsed: Some(json!({
            "title": "Checks and captures",
            "summary": "The player asked how to stop missing checks and captures."
        })),
        ..FakeStructured::default()
    };
    let f = fake_with(vec![answer(&[OK2])], Some(vec![summary]), None);
    assert!(run(&pool, &f, Plan::Pro, msg).await.is_ok());
    let req = &turn_requests(&f)[0];
    assert_eq!(
        req["resume"],
        json!(false),
        "a rollover always starts a new session"
    );
    let msgs = req["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 2);
    let first = msgs[0]["content"].to_string();
    assert!(first.contains("<thread_summary>"), "{first}");
    assert!(first.contains("<recent_turns>"), "{first}");
    assert!(msgs.iter().all(|m| m["role"] != json!("assistant")));
    let (_, summary, through, _) = conv_state(&pool, c).await;
    assert_eq!(
        summary.as_deref(),
        Some("The player asked how to stop missing checks and captures.")
    );
    assert_eq!(through, 2 * n);
    // The summary call is metered as chat.
    assert_eq!(usage_rows(&pool, a).await.len(), 2);
    assert_eq!(stored(&pool, msg).await.status, "done");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn first_answer_sets_a_title(pool: PgPool) {
    let a = account(&pool, "alice").await;
    store_input(&pool, a).await;
    let c = create_conversation(&pool, a, None, TimeClass::Rapid, None)
        .await
        .unwrap();
    let msg = ask_again(&pool, a, c, "How do I stop hanging pieces?").await;
    let title = FakeStructured {
        parsed: Some(
            json!({"title": "Hanging pieces", "summary": "The player asked about loose pieces."}),
        ),
        ..FakeStructured::default()
    };
    let f = fake_with(vec![answer(&[OK2])], Some(vec![title]), None);
    assert!(run(&pool, &f, Plan::Free, msg).await.is_ok());
    assert_eq!(
        conv_state(&pool, c).await.0.as_deref(),
        Some("Hanging pieces")
    );
    let usage = usage_rows(&pool, a).await;
    assert_eq!(usage.len(), 2);
    assert!(usage.iter().all(|u| u.0 == "chat"));

    // An unsafe title falls back to the anchor's fixed title.
    let c2 = create_conversation(
        &pool,
        a,
        None,
        TimeClass::Rapid,
        Some(ChatAnchor::Weakness {
            weakness_id: "W1".into(),
        }),
    )
    .await
    .unwrap();
    let m2 = ask_again(&pool, a, c2, "Again?").await;
    let bad = FakeStructured {
        parsed: Some(json!({"title": "Your 3 worst games", "summary": "x"})),
        ..FakeStructured::default()
    };
    let f = fake_with(vec![answer(&[OK2])], Some(vec![bad]), None);
    assert!(run(&pool, &f, Plan::Pro, m2).await.is_ok());
    assert_eq!(
        conv_state(&pool, c2).await.0.as_deref(),
        Some("Chat about a weakness")
    );
}

// --- The SDK session and the call-index counter ---

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn in_sync_flag_and_call_index_persist_across_turns(pool: PgPool) {
    let a = account(&pool, "alice").await;
    store_input(&pool, a).await;
    let (c, m1) = ask(&pool, a, "Tell me about my first weakness.").await;
    let with_tool = FakeTurn {
        tool_calls: vec![("get_weakness".into(), json!({"weaknessId": "W1"}))],
        ..answer(&[OK2])
    };
    let f = fake(vec![with_tool.clone(), with_tool]);
    assert!(run(&pool, &f, Plan::Pro, m1).await.is_ok());
    assert_eq!(stored(&pool, m1).await.session_in_sync, Some(true));
    assert_eq!(conv_state(&pool, c).await.3, 1);

    // A new handler (a restart) reads both back from Postgres.
    let m2 = ask_again(&pool, a, c, "And the next one?").await;
    assert!(run(&pool, &f, Plan::Pro, m2).await.is_ok());
    let reqs = turn_requests(&f);
    assert_eq!(reqs[1]["resume"], json!(true));
    assert_eq!(reqs[1]["messages"].as_array().unwrap().len(), 1);
    assert!(
        reqs[1]["rebuild_context"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );
    let calls = f.calls();
    let turn2 = calls
        .iter()
        .filter(|c| c.kind == FakeCallKind::Turn)
        .nth(1)
        .unwrap();
    let reply = &turn2.tool_replies[0].2.content;
    assert!(reply.contains("tools.1."), "{reply}");
    assert!(!reply.contains("tools.0."), "{reply}");
    assert_eq!(conv_state(&pool, c).await.3, 2);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn an_out_of_sync_answer_makes_the_next_turn_send_the_full_history(pool: PgPool) {
    let a = account(&pool, "alice").await;
    store_input(&pool, a).await;
    let (c, m1) = ask(&pool, a, "Why?").await;
    // Turn 1 is repaired, so the stored answer differs from the SDK session.
    let f = fake(vec![answer(&[BAD]), answer(&[OK2]), answer(&[OK3])]);
    assert!(run(&pool, &f, Plan::Pro, m1).await.is_ok());
    let s = stored(&pool, m1).await;
    assert_eq!(s.text.as_deref(), Some(OK2));
    assert_eq!(s.session_in_sync, Some(false));
    let m2 = ask_again(&pool, a, c, "And?").await;
    assert!(run(&pool, &f, Plan::Pro, m2).await.is_ok());
    let reqs = turn_requests(&f);
    let last = reqs.last().unwrap();
    assert_eq!(last["resume"], json!(false));
    assert_eq!(last["conversation_id"], json!(c));
    let msgs = last["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 4);
    assert_eq!(msgs[2]["content"][0]["text"], json!(OK2));
}

// --- Serialisation ---

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn concurrent_jobs_on_one_conversation_never_interleave(pool: PgPool) {
    let a = account(&pool, "alice").await;
    store_input(&pool, a).await;
    let (c, msg) = ask(&pool, a, "Why?").await;
    // A second pending answer on the same conversation (bypassing the post-time check).
    sqlx::query(
        "INSERT INTO coach_messages (conversation_id, seq, role, status, text) VALUES \
         ($1, 3, 'user', 'done', 'Again?'), ($1, 4, 'assistant', 'pending', NULL)",
    )
    .bind(c)
    .execute(&pool)
    .await
    .unwrap();
    let other: Uuid =
        sqlx::query_scalar("SELECT id FROM coach_messages WHERE conversation_id = $1 AND seq = 4")
            .bind(c)
            .fetch_one(&pool)
            .await
            .unwrap();
    let f = fake_with(
        vec![answer(&[OK2]), answer(&[OK3])],
        None,
        Some(Duration::from_millis(500)),
    );
    let h = CoachChatHandler(deps(&pool, Some(f.clone()), Plan::Pro));
    // The same message twice (a redelivered job) and the other message, all at once: at most
    // one turn runs, the others are released.
    let (r1, r2, r3) = tokio::join!(
        h.handle(job(msg, 1), ctx()),
        h.handle(job(msg, 2), ctx()),
        h.handle(job(other, 1), ctx())
    );
    let released = [&r1, &r2, &r3]
        .iter()
        .filter(|r| matches!(r, Err(JobError::Release { .. })))
        .count();
    assert!(released >= 2, "{r1:?} {r2:?} {r3:?}");
    assert!(turn_requests(&f).len() <= 1);
    // The released ones run later, in seq order: the later question waits for the earlier one.
    for _ in 0..5 {
        let _ = h.handle(job(other, 1), ctx()).await;
        let _ = h.handle(job(msg, 2), ctx()).await;
    }
    assert_eq!(f.max_in_flight(), 1);
    let reqs = turn_requests(&f);
    assert_eq!(reqs.len(), 2);
    assert!(
        reqs[0]["messages"]
            .to_string()
            .contains("<question>Why?</question>")
    );
    assert!(
        reqs[1]["messages"]
            .to_string()
            .contains("<question>Again?</question>")
    );
    let st = statuses(&pool, c).await;
    assert!(st.iter().all(|s| s.2 == "done"), "{st:?}");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_conversation_deleted_mid_turn_leaves_no_rows(pool: PgPool) {
    let a = account(&pool, "alice").await;
    store_input(&pool, a).await;
    let (c, msg) = ask(&pool, a, "Why?").await;
    let f = fake_with(vec![answer(&[OK2])], None, Some(Duration::from_millis(800)));
    let h = CoachChatHandler(deps(&pool, Some(f.clone()), Plan::Pro));
    let p = pool.clone();
    let deleter = async move {
        testkit::wait_for(
            "streaming",
            Duration::from_secs(5),
            Duration::from_millis(20),
            || {
                let p = p.clone();
                async move {
                    let s: Option<String> =
                        sqlx::query_scalar("SELECT status::text FROM coach_messages WHERE id = $1")
                            .bind(msg)
                            .fetch_optional(&p)
                            .await
                            .unwrap();
                    (s.as_deref() == Some("streaming")).then_some(())
                }
            },
        )
        .await;
        ai::chat::threads::delete_conversation(&p, a, c)
            .await
            .unwrap();
        // The sidecar writes the turn's transcript after the delete.
        sqlx::query(
            "INSERT INTO ai_sdk_session_entries (project_key, session_id, seq, entry) \
             VALUES ('p', $1, 1, '{}')",
        )
        .bind(c)
        .execute(&p)
        .await
        .unwrap();
    };
    let (r, ()) = tokio::join!(h.handle(job(msg, 1), ctx()), deleter);
    assert!(r.is_ok(), "{r:?}");
    let left: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM coach_messages WHERE conversation_id = $1) + \
         (SELECT count(*) FROM ai_sdk_session_entries WHERE session_id = $1)",
    )
    .bind(c)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(left, 0);
    assert!(!types(&events(&pool, a).await).contains(&"chat.message.ready"));
    // The paid call is still metered.
    assert_eq!(usage_rows(&pool, a).await.len(), 1);
}

// --- Anchors ---

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn foreign_anchor_is_ignored(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let b = account(&pool, "bob").await;
    store_input(&pool, a).await;
    let gb = seed_game(&pool, b, "li-b-0").await;
    seed_move(&pool, gb, 3, AFTER_E4_E5, "Nf3", "g1f3", None).await;
    let anchor = ChatAnchor::Move {
        game_id: "li-b-0".into(),
        ply: 3,
    };
    let c = create_conversation(&pool, a, None, TimeClass::Rapid, Some(anchor.clone()))
        .await
        .unwrap();
    set_title(&pool, c).await;
    let (_, msg) = post_user_message(&pool, &Queue::new(pool.clone()), a, c, "Why?", Some(anchor))
        .await
        .unwrap();
    let f = fake(vec![answer(&[OK2])]);
    assert!(run(&pool, &f, Plan::Free, msg).await.is_ok());
    let req = turn_requests(&f)[0]["messages"].to_string();
    assert!(!req.contains("<anchor>"), "{req}");
    assert!(!req.contains("li-b-0"), "{req}");
    assert_eq!(stored(&pool, msg).await.status, "done");
}

// --- "What if" through the job, with the real interactive engine on a fake analyser ---

struct FakeAnalyser;

impl Analyser for FakeAnalyser {
    fn analyse<'a>(
        &'a self,
        _fen: &'a str,
        _opts: AnalyseOptions,
    ) -> BoxFuture<'a, Result<AnalysisResult, EngineError>> {
        Box::pin(async move {
            let line = |multipv, cp, pv: &[&str]| AnalysisLine {
                multipv,
                score: EngineScore::Cp(cp),
                pv: pv.iter().map(|s| (*s).to_owned()).collect(),
                depth: 20,
                nodes: 1_000_000,
            };
            Ok(AnalysisResult {
                lines: vec![line(1, 20, &["b8c6", "f1b5"]), line(2, 10, &["g8f6"])],
                wall: Duration::from_millis(10),
                cpu: None,
            })
        })
    }
}

const EVAL_SLOT: &str = "EVALID";

/// The T12 fake, with the eval id the engine returned substituted into the scripted text (the
/// script is fixed before the tools run, and the id is made by Postgres).
struct WithEvalId {
    inner: Arc<FakeAiClient>,
    eval: Arc<Mutex<Option<String>>>,
}

struct Capture {
    inner: Arc<dyn ToolHandler>,
    eval: Arc<Mutex<Option<String>>>,
}

impl ToolHandler for Capture {
    fn call<'a>(&'a self, name: &'a str, input: Value) -> BoxFuture<'a, ToolReply> {
        Box::pin(async move {
            let r = self.inner.call(name, input).await;
            if let Ok(v) = serde_json::from_str::<Value>(&r.content)
                && let Some(id) = v["evalId"].as_str()
            {
                *self.eval.lock().unwrap() = Some(id.to_owned());
            }
            r
        })
    }
}

impl AiClient for WithEvalId {
    fn structured<'a>(
        &'a self,
        req: &'a StructuredRequest,
    ) -> BoxFuture<'a, Result<StructuredResult, AiError>> {
        self.inner.structured(req)
    }

    fn stream_turn<'a>(
        &'a self,
        req: &'a ChatTurnRequest,
        tools: Option<Arc<dyn ToolHandler>>,
    ) -> BoxStream<'a, Result<TurnEvent, AiError>> {
        let tools = tools.map(|t| -> Arc<dyn ToolHandler> {
            Arc::new(Capture {
                inner: t,
                eval: self.eval.clone(),
            })
        });
        let eval = self.eval.clone();
        self.inner
            .stream_turn(req, tools)
            .map(move |e| {
                let id = eval.lock().unwrap().clone().unwrap_or_default();
                match e {
                    Ok(TurnEvent::TextDelta(t)) => {
                        Ok(TurnEvent::TextDelta(t.replace(EVAL_SLOT, &id)))
                    }
                    other => other,
                }
            })
            .boxed()
    }

    fn batch_submit<'a>(
        &'a self,
        items: &'a [BatchItem],
    ) -> BoxFuture<'a, Result<String, AiError>> {
        self.inner.batch_submit(items)
    }

    fn batch_status<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<BatchStatus, AiError>> {
        self.inner.batch_status(id)
    }

    fn batch_results<'a>(
        &'a self,
        id: &'a str,
        resolver: &'a dyn SchemaResolver,
    ) -> BoxStream<'a, Result<BatchResultItem, AiError>> {
        self.inner.batch_results(id, resolver)
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn what_if_round_trip_through_the_job(pool: PgPool) {
    let pool = testkit::dedicated_pool(&pool, 12).await;
    let a = account(&pool, "alice").await;
    store_input(&pool, a).await;
    let g = seed_game(&pool, a, "li-q-0").await;
    seed_move(&pool, g, 3, AFTER_E4_E5, "Nf3", "g1f3", Some("g1f3")).await;
    let anchor = ChatAnchor::Move {
        game_id: "li-q-0".into(),
        ply: 3,
    };
    let c = create_conversation(&pool, a, None, TimeClass::Rapid, None)
        .await
        .unwrap();
    set_title(&pool, c).await;
    let (_, msg) = post_user_message(
        &pool,
        &Queue::new(pool.clone()),
        a,
        c,
        "What if I had played d4 here?",
        Some(anchor),
    )
    .await
    .unwrap();

    let runner = Runner::spawn(
        Queue::new(pool.clone()),
        Arc::new(InteractiveAnalyseHandler {
            pool: pool.clone(),
            analyser: Arc::new(FakeAnalyser),
        }),
        RunnerConfig::fast_for_tests(),
    );
    let turn = FakeTurn {
        tool_calls: vec![(
            "analyse_position".into(),
            json!({"fen": AFTER_E4_E5, "moves": ["d2d4"]}),
        )],
        deltas: vec![format!(
            "If you had played {{{{eng:{EVAL_SLOT}:tried}}}}, the engine gives {{{{eng:{EVAL_SLOT}:1}}}} with {{{{eng:{EVAL_SLOT}:1:1}}}} next."
        )],
        ..FakeTurn::default()
    };
    let eval = Arc::new(Mutex::new(None));
    let inner = fake(vec![turn]);
    let client = Arc::new(WithEvalId {
        inner: inner.clone(),
        eval: eval.clone(),
    });
    let mut d = deps(&pool, None, Plan::Free);
    d.ai = Some(client);
    let r = CoachChatHandler(d).handle(job(msg, 1), ctx()).await;
    runner.shutdown(Duration::from_secs(2)).await;
    assert!(r.is_ok(), "{r:?}");

    // The request carried the move anchor (its FEN) in the question message.
    let req = turn_requests(&inner)[0].clone();
    let question = req["messages"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .to_string();
    assert!(question.contains("<anchor>"), "{question}");

    let eval_id = eval
        .lock()
        .unwrap()
        .clone()
        .expect("the engine returned an eval id");
    let s = stored(&pool, msg).await;
    assert_eq!(s.status, "done", "{s:?}");
    let segments: Vec<Segment> = serde_json::from_value(s.segments.unwrap()).unwrap();
    let eng: Vec<&Segment> = segments
        .iter()
        .filter(|s| matches!(s, Segment::Eng { eval_id: e, .. } if *e == eval_id))
        .collect();
    assert_eq!(eng.len(), 3, "{segments:?}");
    let linked: Option<Uuid> =
        sqlx::query_scalar("SELECT message_id FROM interactive_evals WHERE id = $1")
            .bind(Uuid::try_parse(&eval_id).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(linked, Some(msg));
    pool.close().await;
}
