//! Queue lag and the autoscaling advice (R2 Task 30).

use std::time::Duration;

use queue::queue::Queue;
use queue::{EnqueueOptions, KIND_ANALYSE, KIND_PREVIEW, PREVIEW_PRIORITY};
use scheduler::lag::{
    DEFAULT_CPU_MS_PER_GAME, LAG_SQL, QueueLag, ScaleAdvice, ScaleConfig, queue_lag, scale_advice,
};
use scheduler::priority::job_priority;
use scheduler::select::Plan;
use serde_json::json;
use sqlx::PgPool;

const MIN: Duration = Duration::from_secs(60);

fn cfg() -> ScaleConfig {
    ScaleConfig {
        monthly_cap_eur: 100.0,
        eur_per_server_hour: 0.5,
        ..ScaleConfig::default()
    }
}

fn lag(age_min: Option<u64>, core_seconds: f64) -> QueueLag {
    QueueLag {
        oldest_p0_age: age_min.map(|m| Duration::from_secs(m * 60)),
        queued_games: 100,
        core_seconds_queued: core_seconds,
    }
}

#[test]
fn defaults_are_the_briefed_values() {
    let c = ScaleConfig::default();
    assert_eq!(c.p0_lag_threshold, 3 * MIN);
    assert_eq!(c.idle_scale_down, 15 * MIN);
    assert_eq!(c.max_extra_servers, 4);
    assert_eq!(c.engines_per_server, 48);
    assert_eq!(DEFAULT_CPU_MS_PER_GAME, 12_000.0);
}

#[test]
fn the_default_config_never_spends() {
    // The reviewer's probe: cap 0 and price 0, lagging, with a big backlog.
    let c = ScaleConfig::default();
    for core in [5000.0, 1e9] {
        assert_eq!(
            scale_advice(&lag(Some(10), core), Duration::ZERO, 0, 0.0, &c),
            ScaleAdvice::Hold
        );
    }
    for (cap, price) in [(0.0, 0.5), (100.0, 0.0), (-1.0, 0.5), (100.0, -0.5)] {
        let c = ScaleConfig {
            monthly_cap_eur: cap,
            eur_per_server_hour: price,
            ..ScaleConfig::default()
        };
        assert_eq!(
            scale_advice(&lag(Some(10), 5000.0), Duration::ZERO, 0, 0.0, &c),
            ScaleAdvice::Hold,
            "cap {cap} price {price}"
        );
    }
}

#[test]
fn zero_engines_per_server_holds() {
    let c = ScaleConfig {
        engines_per_server: 0,
        ..cfg()
    };
    assert_eq!(
        scale_advice(&lag(Some(10), 5000.0), Duration::ZERO, 0, 0.0, &c),
        ScaleAdvice::Hold
    );
}

#[test]
fn adds_one_server_when_p0_lag_passes_the_threshold() {
    // 2 000 core-seconds / (48 engines x 600 s) rounds up to 1.
    assert_eq!(
        scale_advice(&lag(Some(4), 2000.0), Duration::ZERO, 0, 0.0, &cfg()),
        ScaleAdvice::Add(1)
    );
}

#[test]
fn adds_enough_servers_for_the_backlog_capped_by_the_maximum() {
    // 48 x 600 = 28 800 core-seconds per server.
    let c = cfg();
    assert_eq!(
        scale_advice(&lag(Some(4), 60_000.0), Duration::ZERO, 0, 0.0, &c),
        ScaleAdvice::Add(3)
    );
    assert_eq!(
        scale_advice(&lag(Some(4), 1e9), Duration::ZERO, 1, 0.0, &c),
        ScaleAdvice::Add(3),
        "max 4 minus the 1 running"
    );
}

#[test]
fn holds_below_the_threshold() {
    assert_eq!(
        scale_advice(&lag(Some(2), 2000.0), Duration::ZERO, 0, 0.0, &cfg()),
        ScaleAdvice::Hold
    );
    assert_eq!(
        scale_advice(&lag(Some(3), 2000.0), Duration::ZERO, 0, 0.0, &cfg()),
        ScaleAdvice::Hold,
        "exactly the threshold is not over it"
    );
    assert_eq!(
        scale_advice(&lag(None, 0.0), Duration::ZERO, 0, 0.0, &cfg()),
        ScaleAdvice::Hold
    );
}

#[test]
fn removes_one_when_idle_long_enough() {
    assert_eq!(
        scale_advice(&lag(None, 0.0), 16 * MIN, 2, 0.0, &cfg()),
        ScaleAdvice::Remove(1)
    );
    assert_eq!(
        scale_advice(&lag(None, 0.0), 15 * MIN, 1, 0.0, &cfg()),
        ScaleAdvice::Remove(1)
    );
    assert_eq!(
        scale_advice(&lag(None, 0.0), 14 * MIN, 2, 0.0, &cfg()),
        ScaleAdvice::Hold
    );
    assert_eq!(
        scale_advice(&lag(None, 0.0), 60 * MIN, 0, 0.0, &cfg()),
        ScaleAdvice::Hold,
        "nothing extra to remove"
    );
}

#[test]
fn holds_when_the_monthly_cap_leaves_no_room_for_an_hour() {
    let c = cfg(); // cap 100, 0.5 per server-hour
    assert_eq!(
        scale_advice(&lag(Some(10), 5000.0), Duration::ZERO, 0, 99.6, &c),
        ScaleAdvice::Hold
    );
    assert_eq!(
        scale_advice(&lag(Some(10), 5000.0), Duration::ZERO, 0, 100.0, &c),
        ScaleAdvice::Hold
    );
    assert_eq!(
        scale_advice(&lag(Some(10), 5000.0), Duration::ZERO, 0, 99.5, &c),
        ScaleAdvice::Add(1),
        "exactly one hour of room"
    );
}

#[test]
fn the_cap_limits_how_many_servers_are_added() {
    // Room for two server-hours only (cap 100, spent 99, 0.5 each) though 3 are wanted.
    assert_eq!(
        scale_advice(&lag(Some(10), 60_000.0), Duration::ZERO, 0, 99.0, &cfg()),
        ScaleAdvice::Add(2)
    );
}

#[test]
fn holds_at_the_maximum() {
    assert_eq!(
        scale_advice(&lag(Some(10), 5000.0), Duration::ZERO, 4, 0.0, &cfg()),
        ScaleAdvice::Hold
    );
}

#[test]
fn bad_numbers_never_produce_nan_or_a_server_storm() {
    let c = cfg();
    for core in [f64::NAN, f64::INFINITY, -5.0] {
        let a = scale_advice(&lag(Some(10), core), Duration::ZERO, 0, 0.0, &c);
        assert!(
            matches!(a, ScaleAdvice::Add(n) if (1..=4).contains(&n)),
            "{core}: {a:?}"
        );
    }
    let free = ScaleConfig {
        eur_per_server_hour: 0.0,
        ..cfg()
    };
    assert_eq!(
        scale_advice(&lag(Some(10), 5000.0), Duration::ZERO, 0, 0.0, &free),
        ScaleAdvice::Hold,
        "a zero price is not a licence to spend"
    );
    let nan_cap = ScaleConfig {
        monthly_cap_eur: f64::NAN,
        ..cfg()
    };
    assert_eq!(
        scale_advice(&lag(Some(10), 5000.0), Duration::ZERO, 0, 0.0, &nan_cap),
        ScaleAdvice::Hold,
        "an unusable cap spends nothing"
    );
    assert_eq!(
        scale_advice(&lag(Some(10), 5000.0), Duration::ZERO, 0, f64::NAN, &cfg()),
        ScaleAdvice::Hold,
        "unknown spend spends nothing"
    );
}

async fn account(pool: &PgPool, name: &str) -> uuid::Uuid {
    sqlx::query_scalar(
        "INSERT INTO linked_accounts (platform, username, display_name) \
         VALUES ('lichess', $1, $1) RETURNING id",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn job(q: &Queue, kind: &str, priority: i16, key: &str) -> i64 {
    q.enqueue(
        kind,
        &json!({ "k": key }),
        EnqueueOptions {
            priority,
            dedupe_key: Some(key.to_string()),
            ..EnqueueOptions::default()
        },
    )
    .await
    .unwrap()
    .unwrap()
}

async fn age(pool: &PgPool, id: i64, minutes: i32) {
    sqlx::query(
        "UPDATE jobs SET created_at = now() - $2 * interval '1 minute', \
         run_at = now() - $2 * interval '1 minute' WHERE id = $1",
    )
    .bind(id)
    .bind(minutes)
    .execute(pool)
    .await
    .unwrap();
}

async fn age_secs(pool: &PgPool, id: i64, secs: i32) {
    sqlx::query(
        "UPDATE jobs SET created_at = now() - $2 * interval '1 second', \
         run_at = now() - $2 * interval '1 second' WHERE id = $1",
    )
    .bind(id)
    .bind(secs)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn p0_is_priority_5_and_below_not_6(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let six = job(&q, KIND_ANALYSE, 6, "six").await;
    age_secs(&pool, six, 3600).await;
    assert_eq!(queue_lag(&pool).await.unwrap().oldest_p0_age, None);
    let five = job(&q, KIND_ANALYSE, 5, "five").await;
    age_secs(&pool, five, 300).await;
    let a = queue_lag(&pool).await.unwrap().oldest_p0_age.unwrap();
    assert!(
        a >= 5 * MIN && a < 10 * MIN,
        "{a:?}: only the priority-5 job counts"
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn empty_queue_has_no_lag(pool: PgPool) {
    let l = queue_lag(&pool).await.unwrap();
    assert_eq!(l.oldest_p0_age, None);
    assert_eq!(l.queued_games, 0);
    assert_eq!(l.core_seconds_queued, 0.0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn oldest_p0_age_counts_only_tiers_up_to_2(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let p0 = job(&q, KIND_ANALYSE, job_priority(0, Plan::Free, true), "p0").await;
    age_secs(&pool, p0, 4 * 60 + 30).await;
    // Tier 5 (priority 10 / 2) is not P0, however old.
    let p2 = job(&q, KIND_ANALYSE, job_priority(2, Plan::Free, true), "p2").await;
    age(&pool, p2, 60).await;
    let l = queue_lag(&pool).await.unwrap();
    let a = l.oldest_p0_age.unwrap();
    assert!(a >= 4 * MIN && a < 6 * MIN, "{a:?}");
    assert_eq!(l.queued_games, 2);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_delayed_p0_job_is_not_lagging_and_other_kinds_do_not_count(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let later = job(&q, KIND_ANALYSE, 3, "later").await;
    sqlx::query("UPDATE jobs SET run_at = now() + interval '1 hour' WHERE id = $1")
        .bind(later)
        .execute(&pool)
        .await
        .unwrap();
    let other = job(&q, "build-profile", 3, "other").await;
    age(&pool, other, 30).await;
    let l = queue_lag(&pool).await.unwrap();
    assert_eq!(l.oldest_p0_age, None);
    assert_eq!(l.queued_games, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn preview_jobs_count_as_p0(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let acc = account(&pool, "a").await;
    let id = q
        .enqueue(
            KIND_PREVIEW,
            &json!({ "account_id": acc }),
            EnqueueOptions {
                user_key: Some(acc),
                priority: PREVIEW_PRIORITY,
                ..EnqueueOptions::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    age(&pool, id, 5).await;
    let l = queue_lag(&pool).await.unwrap();
    assert!(l.oldest_p0_age.unwrap() >= 5 * MIN);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn core_seconds_use_the_recent_mean_cpu_per_game(pool: PgPool) {
    let q = Queue::new(pool.clone());
    for i in 0..10 {
        job(&q, KIND_ANALYSE, 20, &format!("j{i}")).await;
    }
    // No data: the default 12 000 ms per game.
    let l = queue_lag(&pool).await.unwrap();
    assert!((l.core_seconds_queued - 10.0 * 12.0).abs() < 1e-9);

    // Mean of 4 000 and 8 000 is 6 000 ms; NULL cpu, old rows and other kinds are ignored.
    sqlx::raw_sql(
        "INSERT INTO job_metrics (kind, total_ms, engine_cpu_ms, created_at) VALUES \
           ('analyse-game', 1, 4000, now()), ('analyse-game', 1, 8000, now()), \
           ('analyse-game', 1, NULL, now()), \
           ('analyse-game', 1, 900000, now() - interval '25 hours'), \
           ('preview', 1, 900000, now())",
    )
    .execute(&pool)
    .await
    .unwrap();
    let l = queue_lag(&pool).await.unwrap();
    assert!((l.core_seconds_queued - 10.0 * 6.0).abs() < 1e-9, "{l:?}");
    assert!(l.core_seconds_queued.is_finite());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn lag_read_is_index_backed(pool: PgPool) {
    sqlx::raw_sql(
        // A realistic table: most rows are finished, 5% are queued, 1% of those P0.
        "INSERT INTO jobs (kind, payload, priority, user_key, state, finished_at) \
         SELECT 'analyse-game', '{}', CASE WHEN g % 20 = 0 THEN 2 ELSE 20 END, \
           gen_random_uuid(), \
           CASE WHEN g % 20 = 0 THEN 'queued' ELSE 'done' END::job_state, \
           CASE WHEN g % 20 = 0 THEN NULL ELSE now() END \
         FROM generate_series(1, 20000) g; \
         INSERT INTO job_metrics (kind, total_ms, engine_cpu_ms, created_at) \
         SELECT (ARRAY['analyse-game','preview'])[1 + (g % 2)], 1000, 5000, \
           now() - (g % 2880) * interval '1 minute' FROM generate_series(1, 20000) g; \
         ANALYZE jobs; ANALYZE job_metrics;",
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut conn = pool.acquire().await.unwrap();
    for mode in ["force_custom_plan", "force_generic_plan"] {
        sqlx::query("SELECT set_config('plan_cache_mode', $1, false)")
            .bind(mode)
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "PREPARE lag_{mode}_q(text, text) AS {LAG_SQL}"
        )))
        .execute(&mut *conn)
        .await
        .unwrap();
        let plan: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "EXPLAIN EXECUTE lag_{mode}_q('analyse-game', 'preview')"
        )))
        .fetch_all(&mut *conn)
        .await
        .unwrap();
        let plan = plan.join("\n");
        assert!(!plan.contains("Seq Scan"), "{mode}:\n{plan}");
    }
}
