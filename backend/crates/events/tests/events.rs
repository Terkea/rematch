//! Port of apps/worker/test/recovery/events.test.ts, plus the NOTIFY, reset and size rules of
//! ADR 0034.

use std::time::Duration;

use events::listener::{EventListener, LiveEvent};
use events::store::{
    CHANNEL, EVENT_RETENTION, USER_EVENTS_LOCK_KEY, UserEventRow, UserEventType, insert_user_event,
    list_events_after, prune_events, publish,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgListener;
use testkit::{insert_account, wait_for};
use time::OffsetDateTime;
use uuid::Uuid;

const WAIT: Duration = Duration::from_secs(10);
const TICK: Duration = Duration::from_millis(20);

async fn next_event(listener: &mut EventListener) -> LiveEvent {
    tokio::time::timeout(WAIT, listener.next())
        .await
        .expect("a live event within 10 s")
        .expect("listener.next")
}

async fn row(pool: &PgPool, id: i64) -> UserEventRow {
    sqlx::query_as(
        "SELECT id, account_id, type AS ty, payload, created_at FROM user_events WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("select user_events row")
}

fn ids_of(rows: &[UserEventRow]) -> Vec<i64> {
    rows.iter().map(|r| r.id).collect()
}

#[test]
fn constants_match_the_ts_reference() {
    assert_eq!(USER_EVENTS_LOCK_KEY, 7_262_011);
    assert_eq!(CHANNEL, "user_events");
    assert_eq!(EVENT_RETENTION, Duration::from_secs(7 * 24 * 3600));
    assert_eq!(UserEventType::ImportProgress.as_str(), "import.progress");
    assert_eq!(UserEventType::GameAnalysed.as_str(), "game.analysed");
    assert_eq!(UserEventType::JobFailed.as_str(), "job.failed");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn events_are_ordered_and_replayable(pool: PgPool) {
    let acct = insert_account(&pool, "lichess", "eventful").await;
    let mut listener = EventListener::connect(&pool).await.unwrap();

    let ids = vec![
        publish(
            &pool,
            acct,
            UserEventType::ImportProgress,
            json!({"imported": 50}),
        )
        .await
        .unwrap(),
        publish(
            &pool,
            acct,
            UserEventType::GameAnalysed,
            json!({"gameId": "g1", "accuracy": 81.5}),
        )
        .await
        .unwrap(),
        publish(
            &pool,
            acct,
            UserEventType::JobFailed,
            json!({"kind": "analyse-game", "reason": "x"}),
        )
        .await
        .unwrap(),
    ];
    assert!(ids[0] < ids[1]);
    assert!(ids[1] < ids[2]);

    let mut received = Vec::new();
    for _ in 0..3 {
        received.push(next_event(&mut listener).await);
    }
    assert_eq!(received.iter().map(|e| e.id).collect::<Vec<_>>(), ids);
    assert_eq!(
        received.iter().map(|e| e.ty.as_str()).collect::<Vec<_>>(),
        ["import.progress", "game.analysed", "job.failed"]
    );
    assert!(received.iter().all(|e| e.account_id == acct));
    assert_eq!(
        received[1].payload,
        Some(json!({"gameId": "g1", "accuracy": 81.5}))
    );

    // Every notified id is already durable, so a reconnecting client can replay after it.
    let replay = list_events_after(&pool, &[acct], ids[0], 100)
        .await
        .unwrap();
    assert!(!replay.reset);
    assert_eq!(ids_of(&replay.events), ids[1..]);
    assert_eq!(
        replay
            .events
            .iter()
            .map(|e| e.ty.as_str())
            .collect::<Vec<_>>(),
        ["game.analysed", "job.failed"]
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn notify_payload_matches_row(pool: PgPool) {
    let acct = insert_account(&pool, "lichess", "notified").await;
    let mut listener = EventListener::connect(&pool).await.unwrap();
    // A raw listener sees the exact NOTIFY JSON.
    let mut raw = PgListener::connect_with(&pool).await.unwrap();
    raw.listen(CHANNEL).await.unwrap();

    let payload = json!({"gameId": "g7", "accuracy": 92.25});
    let id = publish(&pool, acct, UserEventType::GameAnalysed, payload.clone())
        .await
        .unwrap();
    let stored = row(&pool, id).await;

    let live = next_event(&mut listener).await;
    assert_eq!(live.id, id);
    assert_eq!(live.account_id, acct);
    assert_eq!(live.ty, "game.analysed");
    assert_eq!(live.payload, Some(payload.clone()));
    // Same instant as the stored row, to the microsecond (returned by the INSERT, not now()).
    assert_eq!(live.created_at, stored.created_at);

    let note = tokio::time::timeout(WAIT, raw.recv())
        .await
        .unwrap()
        .unwrap();
    let wire: Value = serde_json::from_str(note.payload()).unwrap();
    let obj = wire.as_object().unwrap();
    let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["accountId", "createdAt", "id", "payload", "type"]);
    assert_eq!(wire["id"], json!(id));
    assert_eq!(wire["accountId"], json!(acct.to_string()));
    assert_eq!(wire["type"], json!("game.analysed"));
    assert_eq!(wire["payload"], payload);
    let created = OffsetDateTime::parse(
        wire["createdAt"].as_str().unwrap(),
        &time::format_description::well_known::Rfc3339,
    )
    .unwrap();
    assert_eq!(created, stored.created_at);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn notify_is_sent_only_on_commit(pool: PgPool) {
    let acct = insert_account(&pool, "lichess", "rolledback").await;
    let mut listener = EventListener::connect(&pool).await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    insert_user_event(&mut tx, acct, "import.progress", &json!({"imported": 1}))
        .await
        .unwrap();
    tx.rollback().await.unwrap();

    let nothing = tokio::time::timeout(Duration::from_millis(500), listener.next()).await;
    assert!(nothing.is_err(), "a rolled-back insert must not notify");

    // The listener is live: a committed event does arrive.
    let id = publish(
        &pool,
        acct,
        UserEventType::ImportProgress,
        json!({"imported": 2}),
    )
    .await
    .unwrap();
    assert_eq!(next_event(&mut listener).await.id, id);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn large_payload_is_notified_without_payload(pool: PgPool) {
    let acct = insert_account(&pool, "lichess", "bulky").await;
    let mut listener = EventListener::connect(&pool).await.unwrap();
    let mut raw = PgListener::connect_with(&pool).await.unwrap();
    raw.listen(CHANNEL).await.unwrap();

    let big = json!({"blob": "x".repeat(10_000)});
    let id = publish(&pool, acct, UserEventType::ImportProgress, big.clone())
        .await
        .unwrap();

    let note = tokio::time::timeout(WAIT, raw.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        note.payload().len() < 8000,
        "NOTIFY payload must fit Postgres's 8000-byte limit"
    );
    let wire: Value = serde_json::from_str(note.payload()).unwrap();
    assert_eq!(wire["id"], json!(id));
    assert_eq!(wire["truncated"], json!(true));
    assert!(wire.get("payload").is_none());

    // The listener fetches the row by id.
    let live = next_event(&mut listener).await;
    assert_eq!(live.id, id);
    assert_eq!(live.payload, Some(big));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn replay_never_skips_a_lower_id_that_commits_after_a_higher_one_was_read(pool: PgPool) {
    let acct = insert_account(&pool, "lichess", "ordered").await;

    // Transaction A takes an id, then stays open.
    let mut tx_a = pool.begin().await.unwrap();
    let id_a = insert_user_event(&mut tx_a, acct, "a", &json!({}))
        .await
        .unwrap()
        .id;

    // Transaction B starts later and would commit first.
    let pool_b = pool.clone();
    let tx_b = tokio::spawn(async move {
        let mut tx = pool_b.begin().await.unwrap();
        let id = insert_user_event(&mut tx, acct, "b", &json!({}))
            .await
            .unwrap()
            .id;
        tx.commit().await.unwrap();
        id
    });

    // B is really blocked behind A's advisory lock (not merely not started yet).
    wait_for("B to wait on the user-events lock", WAIT, TICK, || {
        let pool = pool.clone();
        async move {
            let waiting: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_locks l JOIN pg_database d ON d.oid = l.database \
                 WHERE d.datname = current_database() AND l.locktype = 'advisory' \
                 AND l.objid = $1 AND NOT l.granted",
            )
            .bind(USER_EVENTS_LOCK_KEY)
            .fetch_one(&pool)
            .await
            .unwrap();
            (waiting == 1).then_some(())
        }
    })
    .await;

    // A reader polls repeatedly, advancing its cursor like an SSE client.
    let mut seen: Vec<i64> = Vec::new();
    let mut cursor = 0;
    async fn poll(pool: &PgPool, acct: Uuid, seen: &mut Vec<i64>, cursor: &mut i64) {
        let page = list_events_after(pool, &[acct], *cursor, 100)
            .await
            .unwrap();
        assert!(!page.reset);
        for e in page.events {
            seen.push(e.id);
            *cursor = e.id;
        }
    }
    for _ in 0..5 {
        poll(&pool, acct, &mut seen, &mut cursor).await;
        tokio::time::sleep(TICK).await;
    }
    // Nothing is visible while A is open: B is ordered behind A rather than slipping past it.
    assert_eq!(seen, Vec::<i64>::new());

    tx_a.commit().await.unwrap();
    let id_b = tokio::time::timeout(WAIT, tx_b).await.unwrap().unwrap();
    poll(&pool, acct, &mut seen, &mut cursor).await;
    poll(&pool, acct, &mut seen, &mut cursor).await;
    assert!(id_a < id_b);
    assert_eq!(seen, [id_a, id_b]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn list_events_after_filters_by_account_orders_by_id_and_honours_the_limit(pool: PgPool) {
    let one = insert_account(&pool, "lichess", "list-one").await;
    let two = insert_account(&pool, "lichess", "list-two").await;
    let mut ids = Vec::new();
    for a in [one, two, one, two, one] {
        let mut tx = pool.begin().await.unwrap();
        let e = insert_user_event(&mut tx, a, "t", &json!({"a": a}))
            .await
            .unwrap();
        tx.commit().await.unwrap();
        ids.push(e.id);
    }
    let mine = list_events_after(&pool, &[one], 0, 10).await.unwrap();
    assert_eq!(ids_of(&mine.events), [ids[0], ids[2], ids[4]]);
    assert!(mine.events.iter().all(|e| e.payload == json!({"a": one})));
    let both = list_events_after(&pool, &[one, two], ids[1], 2)
        .await
        .unwrap();
    assert_eq!(ids_of(&both.events), [ids[2], ids[3]]);
    let none = list_events_after(&pool, &[], 0, 10).await.unwrap();
    assert!(none.events.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn insert_returns_the_stored_created_at(pool: PgPool) {
    let acct = insert_account(&pool, "lichess", "stamped").await;
    let mut tx = pool.begin().await.unwrap();
    let stored = insert_user_event(&mut tx, acct, "t", &json!({}))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(row(&pool, stored.id).await.created_at, stored.created_at);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn list_events_after_signals_reset_after_prune(pool: PgPool) {
    let acct = insert_account(&pool, "lichess", "pruned").await;
    let mut ids = Vec::new();
    for n in 1..=3 {
        ids.push(
            publish(
                &pool,
                acct,
                UserEventType::ImportProgress,
                json!({"imported": n}),
            )
            .await
            .unwrap(),
        );
    }
    let (id1, id2, id3) = (ids[0], ids[1], ids[2]);
    sqlx::query("UPDATE user_events SET created_at = now() - interval '8 days' WHERE id = ANY($1)")
        .bind([id1, id2])
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(prune_events(&pool, EVENT_RETENTION).await.unwrap(), 2);
    let watermark: i64 = sqlx::query_scalar("SELECT pruned_through_id FROM user_events_watermark")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(watermark, id2);

    // The client's cursor predates what was pruned: it missed id2 and must refetch.
    let stale = list_events_after(&pool, &[acct], id1, 100).await.unwrap();
    assert!(stale.reset);
    assert_eq!(ids_of(&stale.events), [id3]);

    // A cursor at the watermark missed nothing.
    let fresh = list_events_after(&pool, &[acct], id2, 100).await.unwrap();
    assert!(!fresh.reset);
    assert_eq!(ids_of(&fresh.events), [id3]);

    // Pruning again with nothing old keeps the watermark (never moves backwards).
    assert_eq!(prune_events(&pool, EVENT_RETENTION).await.unwrap(), 0);
    let again: i64 = sqlx::query_scalar("SELECT pruned_through_id FROM user_events_watermark")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(again, id2);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn empty_account_list_returns_empty_page(pool: PgPool) {
    let acct = insert_account(&pool, "lichess", "lonely").await;
    publish(&pool, acct, UserEventType::JobFailed, json!({}))
        .await
        .unwrap();
    let page = list_events_after(&pool, &[], 0, 10).await.unwrap();
    assert!(!page.reset);
    assert!(page.events.is_empty());
}
