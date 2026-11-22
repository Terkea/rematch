//! Task 25: chat threads. A user only ever reads and writes their own account's conversations;
//! deletes are hard (messages and Agent SDK session entries go with them), usage and quota rows
//! stay.

#[path = "support/chat.rs"]
mod support;

use std::sync::Arc;

use ai::chat::anchor::ChatAnchor;
use ai::chat::job::CoachChatHandler;
use ai::chat::summary::fallback_title;
use ai::chat::threads::{
    ThreadError, create_conversation, delete_all_conversations, delete_conversation, get_thread,
    list_conversations, post_user_message,
};
use ai::client::fake::{FakeAiClient, FakeScript};
use ai::entitlements::Plan;
use ai::input::types::TimeClass;
use queue::queue::Queue;
use queue::runner::{Handler, JobContext};
use serde_json::json;
use sqlx::PgPool;
use support::{account, deps, job, statuses};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

async fn conversation(pool: &PgPool, account_id: Uuid) -> Uuid {
    create_conversation(pool, account_id, None, TimeClass::Rapid, None)
        .await
        .unwrap()
}

async fn count(pool: &PgPool, sql: &str, id: Uuid) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn session_entry(pool: &PgPool, conversation: Uuid) {
    sqlx::query(
        "INSERT INTO ai_sdk_session_entries (project_key, session_id, seq, entry) \
         VALUES ('p', $1, 1, '{}')",
    )
    .bind(conversation)
    .execute(pool)
    .await
    .unwrap();
}

/// Marks the conversation's pending assistant message done (as the job would).
async fn answer_pending(pool: &PgPool, conversation: Uuid) {
    sqlx::query(
        "UPDATE coach_messages SET status = 'done', text = 'ok' \
         WHERE conversation_id = $1 AND status = 'pending'",
    )
    .bind(conversation)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn post_inserts_both_rows_and_enqueues_coach_chat(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let anchor = ChatAnchor::Game {
        game_id: "li-w1-0".into(),
    };
    let c = create_conversation(&pool, a, None, TimeClass::Rapid, Some(anchor.clone()))
        .await
        .unwrap();
    let before: time::OffsetDateTime = count_ts(&pool, c).await;
    let (user, assistant) = post_user_message(&pool, &Queue::new(pool.clone()), a, c, "Why?", None)
        .await
        .unwrap();
    assert_eq!(
        statuses(&pool, c).await,
        vec![
            (1, "user".into(), "done".into(), None),
            (2, "assistant".into(), "pending".into(), None)
        ]
    );
    assert_ne!(user, assistant);
    let jobs: Vec<(String, serde_json::Value)> =
        sqlx::query_as("SELECT kind, payload FROM jobs ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        jobs,
        vec![("coach-chat".into(), json!({"messageId": assistant}))]
    );
    assert!(count_ts(&pool, c).await >= before);
}

async fn count_ts(pool: &PgPool, c: Uuid) -> time::OffsetDateTime {
    sqlx::query_scalar("SELECT last_message_at FROM coach_conversations WHERE id = $1")
        .bind(c)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_second_question_while_an_answer_is_pending_is_busy(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let c = conversation(&pool, a).await;
    let q = Queue::new(pool.clone());
    post_user_message(&pool, &q, a, c, "First?", None)
        .await
        .unwrap();
    let second = post_user_message(&pool, &q, a, c, "Second?", None).await;
    assert!(matches!(second, Err(ThreadError::Busy)), "{second:?}");
    // Two posts racing: exactly one wins.
    answer_pending(&pool, c).await;
    let (x, y) = tokio::join!(
        post_user_message(&pool, &q, a, c, "Third?", None),
        post_user_message(&pool, &q, a, c, "Fourth?", None)
    );
    assert_eq!(usize::from(x.is_ok()) + usize::from(y.is_ok()), 1);
    assert_eq!(statuses(&pool, c).await.len(), 4);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn posting_to_another_accounts_conversation_is_not_found(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let b = account(&pool, "bob").await;
    let cb = conversation(&pool, b).await;
    let r = post_user_message(&pool, &Queue::new(pool.clone()), a, cb, "Hi?", None).await;
    assert!(matches!(r, Err(ThreadError::NotFound)), "{r:?}");
    assert!(statuses(&pool, cb).await.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn list_conversations_orders_by_last_message(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let b = account(&pool, "bob").await;
    let q = Queue::new(pool.clone());
    let anchor = ChatAnchor::Weakness {
        weakness_id: "W1".into(),
    };
    let c1 = create_conversation(&pool, a, None, TimeClass::Rapid, Some(anchor.clone()))
        .await
        .unwrap();
    let c2 = conversation(&pool, a).await;
    conversation(&pool, b).await;
    sqlx::query("UPDATE coach_conversations SET last_message_at = now() - interval '1 hour'")
        .execute(&pool)
        .await
        .unwrap();
    post_user_message(&pool, &q, a, c1, "Why?", None)
        .await
        .unwrap();
    sqlx::query("UPDATE coach_conversations SET title = 'Hanging pieces' WHERE id = $1")
        .bind(c1)
        .execute(&pool)
        .await
        .unwrap();

    let list = list_conversations(&pool, a, 50).await.unwrap();
    assert_eq!(list.iter().map(|c| c.id).collect::<Vec<_>>(), vec![c1, c2]);
    assert_eq!(list[0].title, "Hanging pieces");
    assert_eq!(list[0].anchor, Some(anchor));
    assert_eq!(list[1].title, fallback_title(None));
    assert_eq!(list_conversations(&pool, a, 1).await.unwrap().len(), 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn get_thread_is_account_scoped(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let b = account(&pool, "bob").await;
    let c = conversation(&pool, a).await;
    post_user_message(&pool, &Queue::new(pool.clone()), a, c, "Why?", None)
        .await
        .unwrap();
    let t = get_thread(&pool, a, c).await.unwrap().unwrap();
    assert_eq!(t.conversation.id, c);
    assert_eq!(t.messages.len(), 2);
    assert_eq!(t.messages[0].text.as_deref(), Some("Why?"));
    let wire = serde_json::to_value(&t).unwrap();
    assert!(wire["messages"][1].get("aiDisclosure").is_some(), "{wire}");
    assert!(get_thread(&pool, b, c).await.unwrap().is_none());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn delete_conversation_removes_messages_and_is_account_scoped(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let b = account(&pool, "bob").await;
    let q = Queue::new(pool.clone());
    let ca = conversation(&pool, a).await;
    let cb = conversation(&pool, b).await;
    post_user_message(&pool, &q, a, ca, "Why?", None)
        .await
        .unwrap();
    post_user_message(&pool, &q, b, cb, "Why?", None)
        .await
        .unwrap();
    session_entry(&pool, ca).await;
    session_entry(&pool, cb).await;
    let msg: Uuid = sqlx::query_scalar(
        "SELECT id FROM coach_messages WHERE conversation_id = $1 AND role = 'assistant'",
    )
    .bind(ca)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ai_usage (account_id, feature, model, input_tokens, output_tokens, \
         cache_creation_tokens, cache_read_tokens, cost_micro_usd, ref_id) \
         VALUES ($1, 'chat', 'm', 1, 1, 0, 0, 5, $2)",
    )
    .bind(a)
    .bind(msg.to_string())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO interactive_evals (account_id, message_id, protocol, base_fen, fen) \
         VALUES ($1, $2, 'p', 'f', 'f')",
    )
    .bind(a)
    .bind(msg)
    .execute(&pool)
    .await
    .unwrap();

    // A deleting B's conversation: nothing happens.
    assert!(!delete_conversation(&pool, a, cb).await.unwrap());
    assert_eq!(statuses(&pool, cb).await.len(), 2);

    assert!(delete_conversation(&pool, a, ca).await.unwrap());
    assert!(!delete_conversation(&pool, a, ca).await.unwrap());
    assert!(statuses(&pool, ca).await.is_empty());
    let entries = "SELECT count(*) FROM ai_sdk_session_entries WHERE session_id = $1";
    assert_eq!(count(&pool, entries, ca).await, 0);
    assert_eq!(count(&pool, entries, cb).await, 1);
    // Usage (no content) and quota rows (unlinked) remain.
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM ai_usage WHERE account_id = $1",
            a
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM interactive_evals WHERE account_id = $1 AND message_id IS NULL",
            a
        )
        .await,
        1
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn delete_all_conversations_counts(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let b = account(&pool, "bob").await;
    for _ in 0..3 {
        let c = conversation(&pool, a).await;
        session_entry(&pool, c).await;
    }
    let cb = conversation(&pool, b).await;
    assert_eq!(delete_all_conversations(&pool, a).await.unwrap(), 3);
    assert_eq!(delete_all_conversations(&pool, a).await.unwrap(), 0);
    assert!(list_conversations(&pool, a, 50).await.unwrap().is_empty());
    assert_eq!(list_conversations(&pool, b, 50).await.unwrap().len(), 1);
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_sdk_session_entries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
    let _ = cb;
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn job_for_a_deleted_conversation_no_ops(pool: PgPool) {
    let a = account(&pool, "alice").await;
    let c = conversation(&pool, a).await;
    let (_, msg) = post_user_message(&pool, &Queue::new(pool.clone()), a, c, "Why?", None)
        .await
        .unwrap();
    assert!(delete_conversation(&pool, a, c).await.unwrap());
    let fake = Arc::new(FakeAiClient::new(FakeScript::default()));
    let handler = CoachChatHandler(deps(&pool, Some(fake.clone()), Plan::Free));
    let r = handler
        .handle(
            job(msg, 1),
            JobContext {
                cancel: CancellationToken::new(),
            },
        )
        .await;
    assert!(r.is_ok(), "{r:?}");
    assert!(fake.calls().is_empty());
    assert!(support::events(&pool, a).await.is_empty());
}
