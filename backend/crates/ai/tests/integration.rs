//! R3 Task 26: the integration contracts with M4 (web auth and account linking).
//!
//! - Chat threads belong to `"user"` rows: deleting a user deletes their threads, and with them
//!   the Agent SDK session entries (T17's trigger). The FK migration applies only where M4's
//!   `"user"` table exists (R3 alone has none), so the test re-applies it after creating one.
//! - Chat access follows M4's `account_links` (any link, verified or not, like the web's
//!   `userAccountIds`).
//! - Every event kind that carries model text, or is otherwise about one user's thread, is
//!   stored with its owner (`user_events.user_id`); `emit` refuses those kinds.

use ai::chat::threads::{AccountAccess, AccountLinksAccess};
use ai::db::chat::{NewConversation, create_conversation};
use ai::events::{
    self, Audience, CHAT_CHUNK, CHAT_MESSAGE_FAILED, CHAT_MESSAGE_READY, ChatChunk, ChatFailed,
    ChatReady, EVENT_KINDS, EmitError, JOB_FAILED, JobFailed, NoteReady, REPORT_DEGRADED,
    REPORT_READY, REPORT_SECTION_READY, ReportDegraded, ReportReady, SectionReady,
    WEAKNESS_NOTE_READY, WEEKLY_REVIEW_READY, WeeklyReady,
};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

const FK_MIGRATION: &str =
    include_str!("../../../../db/migrations/20261121030500_integration_user_fks.sql");

/// M4's tables where this branch lacks them, with the columns the tests touch. On the
/// integrated schema they already exist and these are no-ops.
async fn m4_tables(pool: &PgPool) {
    sqlx::raw_sql(
        "CREATE TABLE IF NOT EXISTS \"user\" (id uuid PRIMARY KEY, name text NOT NULL, \
           email text NOT NULL); \
         CREATE TABLE IF NOT EXISTS account_links (id uuid PRIMARY KEY DEFAULT gen_random_uuid(), \
           user_id uuid NOT NULL REFERENCES \"user\"(id) ON DELETE CASCADE, \
           linked_account_id uuid NOT NULL REFERENCES linked_accounts(id) ON DELETE CASCADE, \
           method text NOT NULL, verified_at timestamptz, \
           UNIQUE (user_id, linked_account_id));",
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn conversation(pool: &PgPool, account: Uuid, user: Uuid) -> Result<Uuid, sqlx::Error> {
    let mut conn = pool.acquire().await.unwrap();
    let row = create_conversation(
        &mut conn,
        &NewConversation {
            account_id: account,
            user_id: user,
            time_class: "rapid".to_owned(),
            title: None,
            anchor: None,
        },
    )
    .await?;
    Ok(row.id)
}

async fn count(pool: &PgPool, sql: &str, id: Uuid) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn without_the_user_table_the_fk_migration_adds_nothing(pool: PgPool) {
    let has_user: bool = sqlx::query_scalar("SELECT to_regclass('public.\"user\"') IS NOT NULL")
        .fetch_one(&pool)
        .await
        .unwrap();
    let fk: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_constraint WHERE conname = 'coach_conversations_user_id_fk'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    // R3 alone: no table, no FK. Integrated with M4 (whose migrations sort first): both.
    assert_eq!(fk, i64::from(has_user));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn deleting_a_user_deletes_their_threads_and_session_entries(pool: PgPool) {
    m4_tables(&pool).await;
    sqlx::raw_sql(FK_MIGRATION).execute(&pool).await.unwrap();
    // Idempotent: running it again changes nothing.
    sqlx::raw_sql(FK_MIGRATION).execute(&pool).await.unwrap();
    let account = testkit::insert_account(&pool, "lichess", "t26fk").await;
    let user = testkit::insert_user(&pool, "owner").await;
    let id = conversation(&pool, account, user).await.unwrap();
    sqlx::query(
        "INSERT INTO ai_sdk_session_entries (project_key, session_id, seq, entry) \
         VALUES ('p', $1, 1, '{}')",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    // A thread for a user that does not exist is refused.
    let err = conversation(&pool, account, Uuid::new_v4())
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("coach_conversations_user_id_fk"),
        "{err}"
    );

    sqlx::query("DELETE FROM \"user\" WHERE id = $1")
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    let sql = "SELECT count(*) FROM coach_conversations WHERE id = $1";
    assert_eq!(count(&pool, sql, id).await, 0);
    let sql = "SELECT count(*) FROM ai_sdk_session_entries WHERE session_id = $1";
    assert_eq!(count(&pool, sql, id).await, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn account_links_access_follows_the_m4_links(pool: PgPool) {
    m4_tables(&pool).await;
    let account = testkit::insert_account(&pool, "chesscom", "t26shared").await;
    let other_account = testkit::insert_account(&pool, "chesscom", "t26other").await;
    let verified = testkit::insert_user(&pool, "verified").await;
    let pending = testkit::insert_user(&pool, "pending").await;
    let stranger = testkit::insert_user(&pool, "stranger").await;
    for (user, verified_at) in [(verified, "now()"), (pending, "NULL")] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO account_links (user_id, linked_account_id, method, verified_at) \
             VALUES ($1, $2, 'location_code', {verified_at})"
        )))
        .bind(user)
        .bind(account)
        .execute(&pool)
        .await
        .unwrap();
    }
    let access = AccountLinksAccess(pool.clone());
    // Any link counts, verified or not (as the web's userAccountIds).
    assert!(access.is_linked(verified, account).await.unwrap());
    assert!(access.is_linked(pending, account).await.unwrap());
    assert!(!access.is_linked(stranger, account).await.unwrap());
    assert!(!access.is_linked(verified, other_account).await.unwrap());
    // The M1 column is not consulted.
    sqlx::query("UPDATE linked_accounts SET user_id = $1 WHERE id = $2")
        .bind(stranger)
        .bind(other_account)
        .execute(&pool)
        .await
        .ok();
    assert!(!access.is_linked(stranger, other_account).await.unwrap());
}

/// The event kinds about one user's chat thread. `chat.chunk` carries model text.
const USER_KINDS: [&str; 3] = [CHAT_CHUNK, CHAT_MESSAGE_READY, CHAT_MESSAGE_FAILED];

/// A sample payload of every kind, as the jobs emit it.
fn sample(kind: &str) -> Value {
    let id = Uuid::from_u128(7);
    let v = match kind {
        REPORT_SECTION_READY => serde_json::to_value(SectionReady {
            report_id: id,
            report_run_id: id,
            section: "overview".to_owned(),
            narrative: true,
        }),
        REPORT_READY => serde_json::to_value(ReportReady {
            report_id: id,
            status: ai::db::reports::ReportStatus::Complete,
        }),
        REPORT_DEGRADED => serde_json::to_value(ReportDegraded {
            report_id: id,
            reason: events::DegradedReason::Budget,
            retry_at: Some("2026-11-22T10:00:00.000Z".to_owned()),
        }),
        WEEKLY_REVIEW_READY => serde_json::to_value(WeeklyReady { id }),
        WEAKNESS_NOTE_READY => serde_json::to_value(NoteReady {
            change_key: "hanging_pieces:2026-11-01".to_owned(),
            status: ai::db::ItemStatus::Ready,
        }),
        CHAT_CHUNK => serde_json::to_value(ChatChunk {
            message_id: id,
            seq: 0,
            text: "model text".to_owned(),
            segments: Vec::new(),
            citations: Vec::new(),
            interim: true,
        }),
        CHAT_MESSAGE_READY => serde_json::to_value(ChatReady {
            message_id: id,
            replace: false,
        }),
        CHAT_MESSAGE_FAILED => serde_json::to_value(ChatFailed {
            message_id: id,
            reason: "unavailable".to_owned(),
        }),
        JOB_FAILED => serde_json::to_value(JobFailed {
            kind: "weekly-review".to_owned(),
            reason: "ai_disabled".to_owned(),
        }),
        other => panic!("no sample for {other}"),
    };
    v.unwrap()
}

/// The payload keys whose string values are codes, ids or times, never model text.
const CODE_KEYS: [&str; 9] = [
    "reportId",
    "reportRunId",
    "section",
    "status",
    "reason",
    "retryAt",
    "id",
    "changeKey",
    "kind",
];

#[test]
fn every_event_kind_is_classified() {
    // Every `pub const …: &str = "…";` in events.rs is an event kind, and each is in the table.
    let src = include_str!("../src/events.rs");
    let mut declared: Vec<&str> = src
        .lines()
        .filter(|l| l.starts_with("pub const ") && l.contains(": &str = \""))
        .filter_map(|l| l.split('"').nth(1))
        .collect();
    declared.sort_unstable();
    let mut classified: Vec<&str> = EVENT_KINDS.iter().map(|(k, _)| *k).collect();
    classified.sort_unstable();
    assert_eq!(declared, classified);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn every_event_kind_carrying_model_text_is_user_owned(pool: PgPool) {
    let account = testkit::insert_account(&pool, "lichess", "t26events").await;
    let owner = testkit::insert_user(&pool, "owner").await;
    for (kind, audience) in EVENT_KINDS {
        let payload = sample(kind);
        let free_text: Vec<&str> = payload
            .as_object()
            .unwrap()
            .iter()
            .filter(|(k, v)| v.is_string() && !CODE_KEYS.contains(&k.as_str()))
            .map(|(k, _)| k.as_str())
            .collect();
        if !free_text.is_empty() || USER_KINDS.contains(&kind) {
            // Model text (or a user's thread): only ever stored for its owner.
            assert_eq!(audience, Audience::User, "{kind} carries {free_text:?}");
            let mut conn = pool.acquire().await.unwrap();
            match events::emit(&mut conn, account, kind, &payload).await {
                Err(EmitError::NeedsOwner(k)) => assert_eq!(k, kind),
                other => panic!("{kind}: emit without an owner gave {other:?}"),
            }
            let id = events::emit_for_user(&mut conn, account, owner, kind, &payload)
                .await
                .unwrap();
            let stored: Option<Uuid> =
                sqlx::query_scalar("SELECT user_id FROM user_events WHERE id = $1")
                    .bind(id)
                    .fetch_one(&mut *conn)
                    .await
                    .unwrap();
            assert_eq!(stored, Some(owner), "{kind}");
        } else {
            // Ids and codes about the account (reports, weekly reviews, notes): every user
            // linked to it gets them, and reads the text through the account's own rows.
            assert_eq!(audience, Audience::Account, "{kind}");
        }
    }
    // Nothing reached user_events without an owner.
    let ownerless: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM user_events WHERE user_id IS NULL AND type::text = ANY($1)",
    )
    .bind(USER_KINDS.map(str::to_owned).to_vec())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ownerless, 0);
}
