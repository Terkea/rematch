//! Back-pressure ETA (R2 Task 30): the sentence, the position in the fair order, the honest
//! unknowns, and an index-backed plan.

use std::time::Duration;

use queue::queue::Queue;
use queue::{EnqueueOptions, KIND_ANALYSE};
use scheduler::eta::{
    AccountEta, ETA_COUNTS_SQL, ETA_RATE_SQL, MIN_RATE_ROWS, account_eta, account_eta_in,
    format_eta,
};
use scheduler::priority::job_priority;
use scheduler::select::Plan;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

fn p0() -> i16 {
    job_priority(0, Plan::Free, true)
}

fn eta(done: i64, total: i64, secs: Option<u64>) -> AccountEta {
    AccountEta {
        done,
        total,
        queued_ahead: 0,
        games_per_sec: secs.map(|_| 0.1),
        eta: secs.map(Duration::from_secs),
    }
}

#[test]
fn format_says_minutes_rounded_up() {
    assert_eq!(
        format_eta(&eta(38, 50, Some(170))),
        "about 3 min, analysing 38 / 50"
    );
    assert_eq!(
        format_eta(&eta(38, 50, Some(121))),
        "about 3 min, analysing 38 / 50"
    );
    assert_eq!(
        format_eta(&eta(38, 50, Some(60))),
        "about 1 min, analysing 38 / 50"
    );
}

#[test]
fn format_says_under_a_minute_below_60_s() {
    assert_eq!(
        format_eta(&eta(38, 50, Some(45))),
        "under 1 min, analysing 38 / 50"
    );
    assert_eq!(
        format_eta(&eta(38, 50, Some(0))),
        "under 1 min, analysing 38 / 50"
    );
}

#[test]
fn format_never_invents_a_number() {
    assert_eq!(format_eta(&eta(38, 50, None)), "analysing 38 / 50");
}

async fn account(pool: &PgPool, name: &str, days_ago: i32) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO linked_accounts (platform, username, display_name, created_at) \
         VALUES ('lichess', $1, $1, now() - $2 * interval '1 day') RETURNING id",
    )
    .bind(name)
    .bind(days_ago)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// `n` games with the given status; `selected` sets `selected_at`.
async fn games(pool: &PgPool, acc: Uuid, n: i32, status: &str, selected: bool) -> Vec<Uuid> {
    sqlx::query_scalar(
        "INSERT INTO games (account_id, platform, platform_game_id, pgn, played_at, time_class, \
           rated, player_color, white_name, black_name, result, analysis_status, selected_at) \
         SELECT $1, 'lichess', gen_random_uuid()::text, '1. e4 *', now(), 'blitz', true, \
           'white', 'w', 'b', '*', $3::analysis_status, \
           CASE WHEN $4 THEN now() ELSE NULL END \
         FROM generate_series(1, $2) RETURNING id",
    )
    .bind(acc)
    .bind(n)
    .bind(status)
    .bind(selected)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn enqueue(q: &Queue, user: Uuid, games: &[Uuid]) -> Vec<i64> {
    let mut ids = Vec::new();
    for g in games {
        let id = q
            .enqueue(
                KIND_ANALYSE,
                &json!({ "game_id": g }),
                EnqueueOptions {
                    user_key: Some(user),
                    priority: p0(),
                    ..EnqueueOptions::default()
                },
            )
            .await
            .unwrap()
            .unwrap();
        ids.push(id);
    }
    ids
}

async fn metrics(pool: &PgPool, n: i32, minutes_ago: i32) {
    sqlx::query(
        "INSERT INTO job_metrics (kind, total_ms, created_at) \
         SELECT 'analyse-game', 1000, now() - $2 * interval '1 minute' \
         FROM generate_series(1, $1)",
    )
    .bind(n)
    .bind(minutes_ago)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn counts_only_selected_pending_analysing_or_done(pool: PgPool) {
    let a = account(&pool, "a", 1).await;
    games(&pool, a, 3, "done", true).await;
    games(&pool, a, 2, "analysing", true).await;
    games(&pool, a, 4, "pending", true).await;
    games(&pool, a, 7, "pending", false).await; // not selected
    games(&pool, a, 5, "failed", true).await;
    games(&pool, a, 6, "skipped", true).await;
    let e = account_eta(&pool, a).await.unwrap();
    assert_eq!((e.done, e.total), (3, 9));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn second_account_waits_behind_the_first_ones_slices(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let a = account(&pool, "a", 2).await;
    let b = account(&pool, "b", 1).await;
    let ga = games(&pool, a, 20, "pending", true).await;
    enqueue(&q, a, &ga).await;
    let gb = games(&pool, b, 10, "pending", true).await;
    enqueue(&q, b, &gb).await;
    metrics(&pool, 60, 5).await; // 60 / 600 s = 0.1 games per second

    let e = account_eta(&pool, b).await.unwrap();
    // A5 B5 A5 B5: B's last job is the 20th.
    assert_eq!(e.queued_ahead, 20);
    assert_eq!((e.done, e.total), (0, 10));
    let rate = e.games_per_sec.unwrap();
    assert!((rate - 0.1).abs() < 1e-9, "{rate}");
    let secs = e.eta.unwrap().as_secs_f64();
    assert!((secs - 200.0).abs() < 1e-6, "{secs}");
    assert_eq!(
        format_eta(&e),
        "about 4 min, analysing 0 / 10",
        "ceil(200 s / 60)"
    );

    // A's 20th job comes after all of B's 10: 30 jobs are ahead of or at it.
    let ea = account_eta(&pool, a).await.unwrap();
    assert_eq!(ea.queued_ahead, 30);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn too_little_throughput_data_means_unknown(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let a = account(&pool, "a", 1).await;
    let ga = games(&pool, a, 6, "pending", true).await;
    enqueue(&q, a, &ga).await;
    assert_eq!(MIN_RATE_ROWS, 5);
    metrics(&pool, 4, 1).await; // a literal: the constant must not move the guard
    // Rows older than 10 minutes do not count either.
    metrics(&pool, 50, 11).await;
    let e = account_eta(&pool, a).await.unwrap();
    assert_eq!(e.queued_ahead, 6);
    assert_eq!(e.games_per_sec, None);
    assert_eq!(e.eta, None);
    assert_eq!(format_eta(&e), "analysing 0 / 6");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn exactly_five_rows_is_enough(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let a = account(&pool, "a", 1).await;
    let ga = games(&pool, a, 6, "pending", true).await;
    enqueue(&q, a, &ga).await;
    metrics(&pool, 5, 1).await;
    let e = account_eta(&pool, a).await.unwrap();
    let rate = e.games_per_sec.unwrap();
    assert!((rate - 5.0 / 600.0).abs() < 1e-12);
    assert!((e.eta.unwrap().as_secs_f64() - 6.0 / rate).abs() < 1e-6);
    assert!(rate.is_finite() && rate > 0.0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn nothing_queued_is_a_known_zero_even_without_a_rate(pool: PgPool) {
    let a = account(&pool, "a", 1).await;
    games(&pool, a, 4, "done", true).await;
    let e = account_eta(&pool, a).await.unwrap();
    assert_eq!((e.done, e.total, e.queued_ahead), (4, 4, 0));
    assert_eq!(e.games_per_sec, None);
    assert_eq!(e.eta, Some(Duration::ZERO));
    assert_eq!(format_eta(&e), "under 1 min, analysing 4 / 4");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn nothing_queued_but_games_outstanding_is_unknown(pool: PgPool) {
    let a = account(&pool, "a", 1).await;
    games(&pool, a, 3, "done", true).await;
    games(&pool, a, 2, "pending", true).await; // not scheduled yet
    games(&pool, a, 1, "analysing", true).await; // running, not queued
    metrics(&pool, 60, 1).await;
    let e = account_eta(&pool, a).await.unwrap();
    assert_eq!((e.done, e.total, e.queued_ahead), (3, 6, 0));
    assert_eq!(e.eta, None, "pending or running games are not a known zero");
    assert_eq!(format_eta(&e), "analysing 3 / 6");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_target_entirely_past_the_window_is_unknown(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let a = account(&pool, "a", 2).await;
    let b = account(&pool, "b", 1).await;
    let ga = games(&pool, a, 30, "pending", true).await;
    enqueue(&q, a, &ga).await;
    let gb = games(&pool, b, 3, "pending", true).await;
    enqueue(&q, b, &gb).await;
    metrics(&pool, 60, 1).await;
    // Order: A5 B3 A5 A5 ...: B's last job is the 8th, past a window of 6.
    let e = account_eta_in(&pool, b, 6).await.unwrap();
    assert_eq!(e.eta, None);
    let wide = account_eta_in(&pool, b, 20).await.unwrap();
    assert_eq!(wide.queued_ahead, 8);
    assert!(wide.eta.is_some());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_position_beyond_the_window_is_unknown_not_guessed(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let a = account(&pool, "a", 1).await;
    let ga = games(&pool, a, 12, "pending", true).await;
    enqueue(&q, a, &ga).await;
    metrics(&pool, 60, 1).await;
    let e = account_eta_in(&pool, a, 6).await.unwrap();
    assert_eq!(e.eta, None, "the last job is past the exact window");
    assert_eq!(
        e.queued_ahead, 6,
        "a lower bound, as far as the window sees"
    );
    let full = account_eta_in(&pool, a, 100).await.unwrap();
    assert_eq!(full.queued_ahead, 12);
    assert!(full.eta.is_some());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_delayed_job_makes_the_position_unknown(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let a = account(&pool, "a", 1).await;
    let ga = games(&pool, a, 6, "pending", true).await;
    let ids = enqueue(&q, a, &ga).await;
    metrics(&pool, 60, 1).await;
    sqlx::query("UPDATE jobs SET run_at = now() + interval '1 hour' WHERE id = $1")
        .bind(ids[5])
        .execute(&pool)
        .await
        .unwrap();
    let e = account_eta(&pool, a).await.unwrap();
    assert_eq!(e.eta, None);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn running_jobs_are_not_queued_ahead(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let a = account(&pool, "a", 1).await;
    let ga = games(&pool, a, 3, "pending", true).await;
    let ids = enqueue(&q, a, &ga).await;
    metrics(&pool, 60, 1).await;
    sqlx::query("UPDATE jobs SET state = 'running' WHERE id = $1")
        .bind(ids[0])
        .execute(&pool)
        .await
        .unwrap();
    let e = account_eta(&pool, a).await.unwrap();
    assert_eq!(e.queued_ahead, 2);
}

// The count and rate reads must be index-backed at scale (Task 3 review).
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn eta_reads_are_index_backed(pool: PgPool) {
    sqlx::raw_sql(
        "INSERT INTO linked_accounts (platform, username, display_name) \
         SELECT 'lichess', 'u' || g, 'u' || g FROM generate_series(1, 300) g; \
         INSERT INTO games (account_id, platform, platform_game_id, pgn, played_at, time_class, \
           rated, player_color, white_name, black_name, result, analysis_status, selected_at) \
         SELECT a.id, 'lichess', gen_random_uuid()::text, '1. e4 *', now(), 'blitz', true, \
           'white', 'w', 'b', '*', \
           (ARRAY['pending','analysing','done','failed'])[1 + (g % 4)]::analysis_status, \
           CASE WHEN g % 3 = 0 THEN NULL ELSE now() END \
         FROM linked_accounts a CROSS JOIN generate_series(1, 100) g; \
         INSERT INTO job_metrics (kind, total_ms, created_at) \
         SELECT (ARRAY['analyse-game','preview'])[1 + (g % 2)], 1000, \
           now() - (g % 1440) * interval '1 minute' FROM generate_series(1, 20000) g; \
         ANALYZE games; ANALYZE job_metrics;",
    )
    .execute(&pool)
    .await
    .unwrap();
    let acc: Uuid = sqlx::query_scalar("SELECT id FROM linked_accounts LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut conn = pool.acquire().await.unwrap();
    for mode in ["force_custom_plan", "force_generic_plan"] {
        sqlx::query("SELECT set_config('plan_cache_mode', $1, false)")
            .bind(mode)
            .execute(&mut *conn)
            .await
            .unwrap();
        for (name, sql, param, arg) in [
            ("counts", ETA_COUNTS_SQL, "uuid", format!("'{acc}'")),
            ("rate", ETA_RATE_SQL, "text", "'analyse-game'".to_string()),
        ] {
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                "PREPARE {name}_{mode}_q({param}) AS {sql}"
            )))
            .execute(&mut *conn)
            .await
            .unwrap();
            let plan: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "EXPLAIN EXECUTE {name}_{mode}_q({arg})"
            )))
            .fetch_all(&mut *conn)
            .await
            .unwrap();
            let plan = plan.join("\n");
            assert!(!plan.contains("Seq Scan"), "{name} {mode}:\n{plan}");
        }
    }
}
