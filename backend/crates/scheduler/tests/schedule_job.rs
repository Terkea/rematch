//! The schedule-analysis job (R2 Task 29; TS M2 Task 23 DB cases): `imported` games are promoted
//! to `pending` with their tier and priority in one transaction, then enqueued with that priority;
//! the first run of an account also enqueues previews. Synthetic data and fakes only.

use std::sync::Arc;
use std::time::Duration;

use analysis::eval_cache::DbEvalCache;
use analysis::jobs::analyse::{AnalyseGameDeps, AnalyseGameHandler};
use analysis::jobs::import::enqueue_schedule;
use db::games::{AnalysisStatus, NewGame, upsert_game};
use queue::queue::Queue;
use queue::runner::{Runner, RunnerConfig};
use queue::{
    JobError, KIND_ANALYSE, KIND_PREVIEW, KIND_SCHEDULE, PREVIEW_PRIORITY, SchedulePayload,
    analyse_key, schedule_key,
};
use scheduler::fair::FairRoundRobin;
use scheduler::preview::{PreviewDeps, PreviewHandler};
use scheduler::priority::job_priority;
use scheduler::schedule_job::{ScheduleDeps, ScheduleHandler, ScheduleSummary, handle_schedule};
use scheduler::select::{PREVIEW_GAMES, Plan};
use serde_json::json;
use sqlx::PgPool;
use testkit::fake_engine::FakeAnalyser;
use testkit::{SCHOLARS, insert_account};
use time::OffsetDateTime;
use uuid::Uuid;

/// 12 plies: long enough to be selected (`MIN_PLIES` is 10).
const ITALIAN: &str = "1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 4. c3 Nf6 5. d4 exd4 6. cxd4 Bb4+ *";

fn deps(pool: &PgPool) -> ScheduleDeps {
    ScheduleDeps {
        pool: pool.clone(),
        queue: Queue::new(pool.clone()),
        now: OffsetDateTime::now_utc,
    }
}

fn payload(account_id: Uuid, plan: Option<&str>) -> SchedulePayload {
    SchedulePayload {
        account_id,
        plan: plan.map(str::to_owned),
    }
}

async fn schedule(pool: &PgPool, account: Uuid, plan: Option<&str>) -> ScheduleSummary {
    handle_schedule(&deps(pool), payload(account, plan))
        .await
        .expect("schedule")
}

#[derive(Clone)]
struct Seed {
    time_class: &'static str,
    days_ago: f64,
    rated: bool,
    status: &'static str,
    ply_count: Option<i32>,
    evals: bool,
    tier: Option<i16>,
    selected_days_ago: Option<f64>,
}

impl Default for Seed {
    fn default() -> Self {
        Seed {
            time_class: "blitz",
            days_ago: 1.0,
            rated: true,
            status: "imported",
            ply_count: Some(40),
            evals: false,
            tier: None,
            selected_days_ago: None,
        }
    }
}

async fn seed(pool: &PgPool, account: Uuid, s: Seed) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO games (account_id, platform, platform_game_id, pgn, played_at, time_class, \
           rated, player_color, white_name, black_name, result, lichess_evals, analysis_status, \
           ply_count, analysis_tier, analysis_priority, selected_at, pending_since) \
         SELECT a.id, a.platform, $2, $3, now() - make_interval(secs => $4 * 86400), \
           $5::time_class, $6, 'white', 'Hero', 'Villain', '1-0', \
           CASE WHEN $7 THEN '[]'::jsonb END, $8::analysis_status, $9, $10, \
           CASE WHEN $10::int2 IS NOT NULL THEN 20::int2 END, \
           CASE WHEN $11::float8 IS NOT NULL THEN now() - make_interval(secs => $11 * 86400) END, \
           now() - interval '1 hour' \
         FROM linked_accounts a WHERE a.id = $1 RETURNING id",
    )
    .bind(account)
    .bind(Uuid::new_v4().simple().to_string())
    .bind(SCHOLARS)
    .bind(s.days_ago)
    .bind(s.time_class)
    .bind(s.rated)
    .bind(s.evals)
    .bind(s.status)
    .bind(s.ply_count)
    .bind(s.tier)
    .bind(s.selected_days_ago)
    .fetch_one(pool)
    .await
    .expect("seed game")
}

#[derive(Debug, sqlx::FromRow)]
struct Row {
    status: String,
    tier: Option<i16>,
    priority: Option<i16>,
    selected: bool,
    pending_recent: bool,
}

async fn row(pool: &PgPool, id: Uuid) -> Row {
    sqlx::query_as(
        "SELECT analysis_status::text AS status, analysis_tier AS tier, \
           analysis_priority AS priority, selected_at IS NOT NULL AS selected, \
           coalesce(pending_since > now() - interval '1 minute', false) AS pending_recent \
         FROM games WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Live jobs of `kind` owned by the account: `(dedupe_key, priority)`, sorted.
async fn live(pool: &PgPool, kind: &str, account: Uuid) -> Vec<(String, i16)> {
    let mut v: Vec<(String, i16)> = sqlx::query_as(
        "SELECT dedupe_key, priority FROM jobs WHERE kind = $1 AND user_key = $2 \
         AND state IN ('queued', 'running', 'paused')",
    )
    .bind(kind)
    .bind(account)
    .fetch_all(pool)
    .await
    .unwrap();
    v.sort();
    v
}

async fn scheduled_at(pool: &PgPool, account: Uuid) -> Option<OffsetDateTime> {
    sqlx::query_scalar("SELECT analysis_scheduled_at FROM linked_accounts WHERE id = $1")
        .bind(account)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn schedule_sets_pending_with_tier_and_priority(pool: PgPool) {
    let acct = insert_account(&pool, "lichess", "tiers").await;
    let plain = seed(&pool, acct, Seed::default()).await;
    let with_evals = seed(
        &pool,
        acct,
        Seed {
            days_ago: 2.0,
            evals: true,
            ..Seed::default()
        },
    )
    .await;
    let short = seed(
        &pool,
        acct,
        Seed {
            ply_count: Some(5),
            ..Seed::default()
        },
    )
    .await;

    let s = schedule(&pool, acct, Some("free")).await;
    assert_eq!(s.selected, 2);
    assert_eq!(s.by_tier, [2, 0, 0, 0]);

    for (id, evals) in [(plain, false), (with_evals, true)] {
        let r = row(&pool, id).await;
        let want = job_priority(0, Plan::Free, evals);
        assert_eq!(r.status, "pending", "{r:?}");
        assert_eq!((r.tier, r.priority), (Some(0), Some(want)), "{r:?}");
        assert!(r.selected && r.pending_recent, "{r:?}");
    }
    // Too short to analyse: stays imported, never selected.
    let r = row(&pool, short).await;
    assert_eq!(
        (r.status.as_str(), r.tier, r.selected),
        ("imported", None, false)
    );
    // The scheduler has seen it: its "waiting for the scheduler" marker is cleared, so the
    // reconciler does not schedule the account again for it.
    let marker: Option<OffsetDateTime> =
        sqlx::query_scalar("SELECT pending_since FROM games WHERE id = $1")
            .bind(short)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(marker, None);

    // Each live analyse job carries the game's stored priority.
    let mut want = vec![
        (analyse_key(plain), job_priority(0, Plan::Free, false)),
        (analyse_key(with_evals), job_priority(0, Plan::Free, true)),
    ];
    want.sort();
    assert_eq!(live(&pool, KIND_ANALYSE, acct).await, want);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn pro_plan_from_the_payload_sets_pro_priorities(pool: PgPool) {
    let acct = insert_account(&pool, "lichess", "pro").await;
    let id = seed(&pool, acct, Seed::default()).await;
    let s = schedule(&pool, acct, Some("pro")).await;
    assert_eq!(s.by_tier[0], 1);
    let want = job_priority(0, Plan::Pro, false);
    assert_eq!(row(&pool, id).await.priority, Some(want));
    assert_eq!(
        live(&pool, KIND_ANALYSE, acct).await,
        vec![(analyse_key(id), want)]
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn an_unknown_plan_is_unrecoverable(pool: PgPool) {
    let acct = insert_account(&pool, "lichess", "badplan").await;
    seed(&pool, acct, Seed::default()).await;
    let err = handle_schedule(&deps(&pool), payload(acct, Some("platinum")))
        .await
        .unwrap_err();
    assert!(matches!(err, JobError::Unrecoverable { .. }), "{err:?}");
    assert!(live(&pool, KIND_ANALYSE, acct).await.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_re_run_selects_nothing(pool: PgPool) {
    let acct = insert_account(&pool, "lichess", "rerun").await;
    for d in 1..=3 {
        seed(
            &pool,
            acct,
            Seed {
                days_ago: f64::from(d),
                ..Seed::default()
            },
        )
        .await;
    }
    assert_eq!(schedule(&pool, acct, None).await.selected, 3);
    let jobs = live(&pool, KIND_ANALYSE, acct).await;
    assert_eq!(
        schedule(&pool, acct, None).await,
        ScheduleSummary {
            selected: 0,
            by_tier: [0; 4],
            previews: 0
        }
    );
    assert_eq!(live(&pool, KIND_ANALYSE, acct).await, jobs);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn legacy_pending_games_are_never_re_selected(pool: PgPool) {
    // R1 imported games as `pending` with no tier; they already have analyse jobs.
    let acct = insert_account(&pool, "lichess", "legacy").await;
    let legacy = seed(
        &pool,
        acct,
        Seed {
            status: "pending",
            ..Seed::default()
        },
    )
    .await;
    let s = schedule(&pool, acct, None).await;
    assert_eq!(s.selected, 0);
    let r = row(&pool, legacy).await;
    assert_eq!((r.status.as_str(), r.tier), ("pending", None));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn first_schedule_enqueues_20_previews(pool: PgPool) {
    let acct = insert_account(&pool, "lichess", "previews").await;
    let mut ids = Vec::new();
    for d in 0..30 {
        ids.push(
            seed(
                &pool,
                acct,
                Seed {
                    days_ago: 0.5 + f64::from(d),
                    ..Seed::default()
                },
            )
            .await,
        );
    }
    assert_eq!(scheduled_at(&pool, acct).await, None);
    let s = schedule(&pool, acct, None).await;
    assert_eq!(s.by_tier[0], 30);
    assert_eq!(s.previews, PREVIEW_GAMES);
    assert_eq!(PREVIEW_GAMES, 20);
    assert!(scheduled_at(&pool, acct).await.is_some());

    // The 20 most recent tier-0 games, at the preview priority.
    let mut want: Vec<(String, i16)> = ids[..20]
        .iter()
        .map(|id| (format!("preview:{id}"), PREVIEW_PRIORITY))
        .collect();
    want.sort();
    assert_eq!(live(&pool, KIND_PREVIEW, acct).await, want);

    // The second run is not the first: no previews, and the first-run time is kept.
    let first = scheduled_at(&pool, acct).await;
    for d in 0..3 {
        seed(
            &pool,
            acct,
            Seed {
                days_ago: 0.1 + f64::from(d) * 0.01,
                ..Seed::default()
            },
        )
        .await;
    }
    let s = schedule(&pool, acct, None).await;
    assert_eq!(s.previews, 0);
    assert_eq!(s.selected, 3);
    assert_eq!(scheduled_at(&pool, acct).await, first);
    assert_eq!(live(&pool, KIND_PREVIEW, acct).await.len(), 20);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn precise_time_classes_get_no_backfill(pool: PgPool) {
    // Pro: blitz is the primary (P0); rapid is active (>= 20 recent games), so it gets P1 unless
    // its estimates are already precise.
    let mut rapid_tier1 = Vec::new();
    for (name, precise) in [("loose", false), ("precise", true)] {
        let acct = insert_account(&pool, "lichess", name).await;
        for d in 0..30 {
            seed(
                &pool,
                acct,
                Seed {
                    days_ago: 0.5 + f64::from(d),
                    ..Seed::default()
                },
            )
            .await;
        }
        for d in 0..22 {
            seed(
                &pool,
                acct,
                Seed {
                    time_class: "rapid",
                    days_ago: 0.7 + f64::from(d),
                    ..Seed::default()
                },
            )
            .await;
        }
        if precise {
            sqlx::query(
                "INSERT INTO weakness_estimates (account_id, time_class, series_key, k, n, \
                   n_unweighted, k_unweighted, alpha, beta, mean, ci_low, ci_high, band_median, \
                   confidence, priority, state) \
                 VALUES ($1, 'rapid', 'W1', 10, 2000, 2000, 10, 1, 1, 0.005, 0.004, 0.006, \
                   0.005, 0.9, 1, 'active')",
            )
            .bind(acct)
            .execute(&pool)
            .await
            .unwrap();
        }
        let s = schedule(&pool, acct, Some("pro")).await;
        assert_eq!(s.by_tier[0], 30, "{name}");
        rapid_tier1.push(s.by_tier[1]);
    }
    assert_eq!(rapid_tier1, vec![22, 0]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn in_flight_backfill_waits_for_the_batch(pool: PgPool) {
    let acct = insert_account(&pool, "lichess", "inflight").await;
    for d in 0..25 {
        seed(
            &pool,
            acct,
            Seed {
                days_ago: 0.5 + f64::from(d),
                ..Seed::default()
            },
        )
        .await;
    }
    for d in 0..21 {
        seed(
            &pool,
            acct,
            Seed {
                time_class: "rapid",
                days_ago: 0.7 + f64::from(d),
                ..Seed::default()
            },
        )
        .await;
    }
    // One rapid backfill game is still being analysed: the next rapid batch waits for it.
    seed(
        &pool,
        acct,
        Seed {
            time_class: "rapid",
            status: "analysing",
            tier: Some(1),
            selected_days_ago: Some(0.1),
            ..Seed::default()
        },
    )
    .await;
    let s = schedule(&pool, acct, Some("pro")).await;
    assert_eq!(s.by_tier, [25, 0, 0, 0]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn p3_budget_is_a_rolling_seven_day_window(pool: PgPool) {
    // Free: P0 is full (50 tier-0 games), the first run was 30 days ago, 15 tier-3 games were
    // selected 3 days ago (in the window) and 10 eleven days ago (out of it): 20 - 15 = 5 left.
    let acct = insert_account(&pool, "lichess", "p3").await;
    sqlx::query(
        "UPDATE linked_accounts SET analysis_scheduled_at = now() - interval '30 days' \
         WHERE id = $1",
    )
    .bind(acct)
    .execute(&pool)
    .await
    .unwrap();
    for d in 0..50 {
        seed(
            &pool,
            acct,
            Seed {
                days_ago: 40.0 + f64::from(d),
                status: "done",
                tier: Some(0),
                selected_days_ago: Some(30.0),
                ..Seed::default()
            },
        )
        .await;
    }
    for (n, ago) in [(15, 3.0), (10, 11.0)] {
        for _ in 0..n {
            seed(
                &pool,
                acct,
                Seed {
                    days_ago: ago + 1.0,
                    status: "done",
                    tier: Some(3),
                    selected_days_ago: Some(ago),
                    ..Seed::default()
                },
            )
            .await;
        }
    }
    for d in 0..10 {
        seed(
            &pool,
            acct,
            Seed {
                days_ago: 0.1 + f64::from(d) * 0.01,
                ..Seed::default()
            },
        )
        .await;
    }
    let s = schedule(&pool, acct, Some("free")).await;
    assert_eq!(s.by_tier, [0, 0, 0, 5]);
    assert_eq!(s.previews, 0, "not the first run");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn schedule_handler_runs_the_job(pool: PgPool) {
    let acct = insert_account(&pool, "lichess", "handler").await;
    seed(&pool, acct, Seed::default()).await;
    let queue = Queue::new(pool.clone());
    assert!(enqueue_schedule(&queue, acct).await.unwrap());
    // One live schedule job per account.
    assert!(!enqueue_schedule(&queue, acct).await.unwrap());
    assert_eq!(live(&pool, KIND_SCHEDULE, acct).await.len(), 1);
    let runner = Runner::spawn(
        queue.clone(),
        Arc::new(ScheduleHandler(Arc::new(deps(&pool)))),
        RunnerConfig {
            idle_poll: Duration::from_millis(50),
            ..RunnerConfig::default()
        },
    );
    testkit::wait_for(
        "the schedule job to finish",
        Duration::from_secs(20),
        Duration::from_millis(50),
        || async {
            let done: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM jobs WHERE kind = $1 AND dedupe_key = $2 AND state = 'done'",
            )
            .bind(KIND_SCHEDULE)
            .bind(schedule_key(acct))
            .fetch_one(&pool)
            .await
            .unwrap();
            (done == 1).then_some(())
        },
    )
    .await;
    runner.stop();
    runner.wait_idle().await;
    assert_eq!(live(&pool, KIND_ANALYSE, acct).await.len(), 1);
    assert_eq!(live(&pool, KIND_PREVIEW, acct).await.len(), 1);
    // The job's result, as the runner stores it in the logs and job metrics.
    let summary = ScheduleSummary {
        selected: 1,
        by_tier: [1, 0, 0, 0],
        previews: 1,
    };
    assert_eq!(
        serde_json::to_value(summary).unwrap(),
        json!({ "selected": 1, "byTier": [1, 0, 0, 0], "previews": 1 })
    );
}

/// The CLI's flow end to end with fakes: games stored as the importer stores them, the schedule
/// job the importer enqueues, then the analyse runner (fair round robin) with a fake engine.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn cli_style_flow_still_finishes(pool: PgPool) {
    let acct = insert_account(&pool, "lichess", "cliflow").await;
    let now = OffsetDateTime::now_utc();
    let mut ids = Vec::new();
    for i in 0..60_i64 {
        let res = upsert_game(
            &pool,
            &NewGame {
                account_id: acct,
                platform: "lichess".to_owned(),
                platform_game_id: format!("cli-{i}"),
                pgn: ITALIAN.to_owned(),
                played_at: now - time::Duration::hours(i + 1),
                time_class: "blitz".to_owned(),
                base_seconds: Some(180),
                increment_seconds: Some(0),
                rated: true,
                player_color: "white".to_owned(),
                white_name: "cliflow".to_owned(),
                black_name: "Villain".to_owned(),
                white_rating: Some(1500),
                black_rating: Some(1500),
                result: "1-0".to_owned(),
                termination: None,
                eco: None,
                opening_name: None,
                lichess_evals: None,
                status: AnalysisStatus::Imported,
                ply_count: Some(12),
            },
        )
        .await
        .unwrap();
        ids.push(res.id);
    }
    let queue = Queue::new(pool.clone());
    assert!(enqueue_schedule(&queue, acct).await.unwrap());

    let fast = RunnerConfig {
        idle_poll: Duration::from_millis(50),
        ..RunnerConfig::default()
    };
    let fake = Arc::new(FakeAnalyser::new());
    let runners = [
        Runner::spawn(
            queue.clone(),
            Arc::new(ScheduleHandler(Arc::new(deps(&pool)))),
            fast.clone(),
        ),
        Runner::spawn(
            queue.clone(),
            Arc::new(AnalyseGameHandler(Arc::new(AnalyseGameDeps::new(
                pool.clone(),
                fake.clone(),
                Arc::new(DbEvalCache::new(pool.clone())),
            )))),
            RunnerConfig {
                concurrency: 4,
                strategy: Arc::new(FairRoundRobin::default()),
                ..fast.clone()
            },
        ),
        Runner::spawn(
            queue.clone(),
            Arc::new(PreviewHandler(Arc::new(PreviewDeps {
                pool: pool.clone(),
                analyser: fake.clone(),
                concurrency: 2,
            }))),
            fast.clone(),
        ),
    ];
    testkit::wait_for(
        "every selected game to finish",
        Duration::from_secs(60),
        Duration::from_millis(100),
        || async {
            let open: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM jobs WHERE user_key = $1 AND kind = ANY($2) \
                 AND state IN ('queued', 'running', 'paused')",
            )
            .bind(acct)
            .bind(vec![KIND_SCHEDULE, KIND_ANALYSE, KIND_PREVIEW])
            .fetch_one(&pool)
            .await
            .unwrap();
            let done_jobs: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM jobs WHERE user_key = $1 AND kind = $2 AND state = 'done'",
            )
            .bind(acct)
            .bind(KIND_SCHEDULE)
            .fetch_one(&pool)
            .await
            .unwrap();
            (open == 0 && done_jobs == 1).then_some(())
        },
    )
    .await;
    for r in &runners {
        r.stop();
    }
    for r in &runners {
        r.wait_idle().await;
    }
    // The beta plan (the default until M7) selects every imported game.
    let counts: Vec<(String, Option<i16>, i64)> = sqlx::query_as(
        "SELECT analysis_status::text, analysis_tier, count(*) FROM games WHERE account_id = $1 \
         GROUP BY 1, 2 ORDER BY 1, 2",
    )
    .bind(acct)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(counts, vec![("done".to_owned(), Some(0), 60)]);
    for id in &ids {
        assert_eq!(row(&pool, *id).await.status, "done");
    }
}

/// Stores a game the way the importer does (`imported`, with its ply count).
async fn store_imported(
    pool: &PgPool,
    account: Uuid,
    n: usize,
    time_class: &str,
    played_at: OffsetDateTime,
) -> Uuid {
    upsert_game(
        pool,
        &NewGame {
            account_id: account,
            platform: "lichess".to_owned(),
            platform_game_id: format!("beta-{account}-{n}"),
            pgn: ITALIAN.to_owned(),
            played_at,
            time_class: time_class.to_owned(),
            base_seconds: Some(180),
            increment_seconds: Some(0),
            rated: !n.is_multiple_of(7),
            player_color: "white".to_owned(),
            white_name: "beta".to_owned(),
            black_name: "Villain".to_owned(),
            white_rating: Some(1500),
            black_rating: Some(1500),
            result: "1-0".to_owned(),
            termination: None,
            eco: None,
            opening_name: None,
            lichess_evals: None,
            status: AnalysisStatus::Imported,
            ply_count: Some(12),
        },
    )
    .await
    .unwrap()
    .id
}

/// Owner ruling (ADR 0041, ADR-R2-16): until plans exist (M7) every account is on the beta
/// plan, with no plan limit. The CLI's `--max 500` import of 500 games is followed by the
/// schedule job the importer enqueues (no plan in the payload), and it schedules all 500, in
/// tier-priority order: the primary time class first.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn cli_max_500_on_500_imported_games_schedules_all_500(pool: PgPool) {
    let acct = insert_account(&pool, "lichess", "beta500").await;
    let now = OffsetDateTime::now_utc();
    let mut by_tc: Vec<(Uuid, &str)> = Vec::new();
    for n in 0..500_usize {
        // 300 blitz (the primary), 150 rapid, 50 bullet, spread over three years.
        let tc = match n % 10 {
            0..=5 => "blitz",
            6..=8 => "rapid",
            _ => "bullet",
        };
        let played = now - time::Duration::hours(i64::try_from(n).unwrap() * 50 + 1);
        by_tc.push((store_imported(&pool, acct, n, tc, played).await, tc));
    }
    let queue = Queue::new(pool.clone());
    assert!(enqueue_schedule(&queue, acct).await.unwrap());
    let job_payload: serde_json::Value =
        sqlx::query_scalar("SELECT payload FROM jobs WHERE kind = $1 AND user_key = $2")
            .bind(KIND_SCHEDULE)
            .bind(acct)
            .fetch_one(&pool)
            .await
            .unwrap();
    let p: SchedulePayload = serde_json::from_value(job_payload).unwrap();

    let s = handle_schedule(&deps(&pool), p).await.unwrap();
    assert_eq!(s.selected, 500);
    assert_eq!(s.by_tier.iter().sum::<usize>(), 500);
    assert_eq!(s.by_tier[0], 300, "{s:?}");
    assert_eq!(live(&pool, KIND_ANALYSE, acct).await.len(), 500);
    let imported: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM games WHERE account_id = $1 AND analysis_status <> 'pending'",
    )
    .bind(acct)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(imported, 0);
    // Tier-priority order: every primary game runs before every other game.
    let worst_primary = by_tc.iter().filter(|g| g.1 == "blitz").map(|g| g.0);
    let mut max_primary = i16::MIN;
    for id in worst_primary {
        max_primary = max_primary.max(row(&pool, id).await.priority.unwrap());
    }
    for (id, tc) in &by_tc {
        if *tc != "blitz" {
            assert!(row(&pool, *id).await.priority.unwrap() > max_primary);
        }
    }
}
