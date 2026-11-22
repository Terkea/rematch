//! The build-profile job (R2 Task 33, TS M2 Task 25): estimates, timeline rows, KPI and area
//! snapshots, events, the follow-up build and the schedule enqueue.

mod common {
    pub mod seed;
}

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use common::seed::{
    PROTOCOL, SEED_NOW, SEED_WEEKS, SeedOptions, insert_analysed_game, insert_unanalysed_game,
    seed, seed_metrics, seed_now,
};
use profile::build_profile::{
    BUILD_PROFILE_STAGES, BuildProfileDeps, BuildProfileHandler, BuildProfileSummary,
    handle_build_profile,
};
use profile::hook_impl::enqueue_build_profile;
use profile_core::types::WeaknessKey;
use queue::queue::Queue;
use queue::runner::{Handler, JobContext};
use queue::{
    BuildProfilePayload, EnqueueOptions, KIND_BUILD_PROFILE, KIND_SCHEDULE, profile_key,
    schedule_key,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::macros::datetime;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const TEST_TIMEOUT: Duration = Duration::from_secs(120);

async fn within<T>(what: &str, fut: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(TEST_TIMEOUT, fut)
        .await
        .unwrap_or_else(|_| panic!("timed out after {TEST_TIMEOUT:?} waiting for {what}"))
}

fn deps(pool: &PgPool) -> BuildProfileDeps {
    BuildProfileDeps {
        pool: pool.clone(),
        queue: Queue::new(pool.clone()),
        now: seed_now,
    }
}

fn payload(account_id: Uuid) -> BuildProfilePayload {
    BuildProfilePayload {
        account_id,
        time_class: "blitz".to_owned(),
    }
}

async fn build(pool: &PgPool, account_id: Uuid) -> BuildProfileSummary {
    within(
        "build-profile",
        handle_build_profile(&deps(pool), payload(account_id)),
    )
    .await
    .expect("build-profile")
}

async fn count(pool: &PgPool, sql: &'static str, account_id: Uuid) -> i64 {
    sqlx::query_scalar(sql)
        .bind(account_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn snapshot_rows(pool: &PgPool, a: Uuid) -> i64 {
    count(
        pool,
        "SELECT count(*) FROM kpi_snapshot WHERE account_id = $1",
        a,
    )
    .await
}

async fn history_rows(pool: &PgPool, a: Uuid) -> i64 {
    count(
        pool,
        "SELECT count(*) FROM weakness_state_history WHERE account_id = $1",
        a,
    )
    .await
}

async fn events(pool: &PgPool, a: Uuid, ty: &str) -> Vec<Value> {
    sqlx::query_scalar(
        "SELECT payload FROM user_events WHERE account_id = $1 AND type = $2 ORDER BY id",
    )
    .bind(a)
    .bind(ty)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn live_jobs(pool: &PgPool, kind: &str, key: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = $1 AND dedupe_key = $2 \
         AND state IN ('queued', 'running', 'paused')",
    )
    .bind(kind)
    .bind(key)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn live_builds(pool: &PgPool, a: Uuid) -> i64 {
    live_jobs(pool, KIND_BUILD_PROFILE, &profile_key(a, "blitz")).await
}

fn binomial_keys() -> Vec<&'static str> {
    WeaknessKey::ALL
        .iter()
        .filter(|k| k.is_binomial())
        .map(|k| k.as_str())
        .collect()
}

// ---------------------------------------------------------------------------------------------
// TS M2 Task 25 cases

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn one_estimate_per_binomial_key_and_w1_is_active(pool: PgPool) {
    let s = seed(&pool, SeedOptions::default()).await;
    let summary = build(&pool, s.account_id).await;

    let keys: Vec<String> = sqlx::query_scalar(
        "SELECT series_key FROM weakness_estimates WHERE account_id = $1 \
         AND time_class = 'blitz' ORDER BY series_key",
    )
    .bind(s.account_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    let mut expected: Vec<String> = binomial_keys().into_iter().map(String::from).collect();
    expected.sort();
    assert_eq!(keys, expected);
    assert_eq!(summary.estimates, expected.len());

    let (confidence, state, k_unweighted, n_unweighted, band_median): (
        f64,
        String,
        f64,
        i32,
        Option<f64>,
    ) = sqlx::query_as(
        "SELECT confidence, state::text, k_unweighted, n_unweighted, band_median \
         FROM weakness_estimates WHERE account_id = $1 AND series_key = 'W1'",
    )
    .bind(s.account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(confidence > 0.7, "W1 confidence {confidence}");
    assert_eq!(state, "active");
    assert_eq!((k_unweighted, n_unweighted), (80.0, 2000));
    assert!(band_median.is_some());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn snapshots_cover_last50_and_both_seed_weeks(pool: PgPool) {
    let s = seed(&pool, SeedOptions::default()).await;
    let summary = build(&pool, s.account_id).await;

    let windows: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT window_key FROM kpi_snapshot WHERE account_id = $1 ORDER BY 1",
    )
    .bind(s.account_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    let mut expected = vec!["last50".to_owned()];
    expected.extend(SEED_WEEKS.iter().map(|w| (*w).to_owned()));
    expected.sort();
    assert_eq!(windows, expected);
    assert_eq!(
        usize::try_from(snapshot_rows(&pool, s.account_id).await).unwrap(),
        summary.snapshots
    );
    // Area scores are `area:<area>` snapshots, one per area and window.
    let areas = count(
        &pool,
        "SELECT count(*) FROM kpi_snapshot WHERE account_id = $1 AND metric LIKE 'area:%'",
        s.account_id,
    )
    .await;
    assert_eq!(areas, 7 * 3);
    let (kind, n): (String, i32) = sqlx::query_as(
        "SELECT window_kind::text, n FROM kpi_snapshot WHERE account_id = $1 \
         AND metric = 'blunders_per_game' AND window_key = 'last50'",
    )
    .bind(s.account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((kind.as_str(), n), ("last_n", 40));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn events_six_profile_updated_and_w1_changed(pool: PgPool) {
    let s = seed(&pool, SeedOptions::default()).await;
    build(&pool, s.account_id).await;

    let updated = events(&pool, s.account_id, "profile.updated").await;
    let areas: BTreeSet<String> = updated
        .iter()
        .map(|p| p["area"].as_str().unwrap().to_owned())
        .collect();
    // Mindset has no benchmark, so no change event.
    assert_eq!(updated.len(), 6, "{updated:?}");
    assert!(!areas.contains("mindset"));
    assert_eq!(areas.len(), 6);
    for p in &updated {
        let mut keys: Vec<&str> = p.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["area", "kpis", "timeClass"]);
        assert_eq!(p["timeClass"], "blitz");
        let kpis = p["kpis"].as_array().unwrap();
        assert!(!kpis.is_empty());
        for k in kpis {
            let mut keys: Vec<&str> = k.as_object().unwrap().keys().map(|k| k.as_str()).collect();
            keys.sort_unstable();
            assert_eq!(keys, ["ciHigh", "ciLow", "metric", "n", "value"]);
        }
    }
    let calc = updated.iter().find(|p| p["area"] == "calculation").unwrap();
    assert!(
        calc["kpis"]
            .as_array()
            .unwrap()
            .iter()
            .any(|k| k["metric"] == "area:calculation"),
        "the area score is part of its event"
    );

    let changed = events(&pool, s.account_id, "weakness.changed").await;
    let w1: Vec<&Value> = changed.iter().filter(|p| p["id"] == "W1").collect();
    assert_eq!(
        w1,
        [&json!({
            "id": "W1",
            "timeClass": "blitz",
            "state": "active",
            "fromState": "not_an_issue",
            "onsetAt": null,
        })]
    );
    // A series' first neutral row (`null -> not_an_issue`) is not a change.
    assert!(
        changed.iter().all(|p| p["state"] != "not_an_issue"),
        "{changed:?}"
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn second_identical_run_appends_nothing(pool: PgPool) {
    let s = seed(&pool, SeedOptions::default()).await;
    build(&pool, s.account_id).await;
    let (snaps, hist) = (
        snapshot_rows(&pool, s.account_id).await,
        history_rows(&pool, s.account_id).await,
    );
    let changed = events(&pool, s.account_id, "weakness.changed").await.len();
    let updated = events(&pool, s.account_id, "profile.updated").await.len();
    assert!(hist > 0);

    let again = build(&pool, s.account_id).await;
    assert_eq!(snapshot_rows(&pool, s.account_id).await, snaps);
    assert_eq!(history_rows(&pool, s.account_id).await, hist);
    assert_eq!((again.snapshots, again.transitions), (0, 0));
    assert_eq!(
        events(&pool, s.account_id, "weakness.changed").await.len(),
        changed
    );
    assert_eq!(
        events(&pool, s.account_id, "profile.updated").await.len(),
        updated
    );
    // Nothing was analysed after the build started: no follow-up build.
    assert_eq!(live_builds(&pool, s.account_id).await, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn game_analysed_after_start_reenqueues_exactly_once(pool: PgPool) {
    let s = seed(&pool, SeedOptions::default()).await;
    sqlx::query("UPDATE games SET analysed_at = $2 WHERE id = $1")
        .bind(s.games[39])
        .bind(SEED_NOW + time::Duration::minutes(5))
        .execute(&pool)
        .await
        .unwrap();
    build(&pool, s.account_id).await;
    assert_eq!(live_builds(&pool, s.account_id).await, 1);
    // A second run dedupes on the live follow-up.
    build(&pool, s.account_id).await;
    assert_eq!(live_builds(&pool, s.account_id).await, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn no_reference_build_gives_no_benchmark_and_not_an_issue(pool: PgPool) {
    let s = seed(
        &pool,
        SeedOptions {
            reference: false,
            ..SeedOptions::default()
        },
    )
    .await;
    let summary = build(&pool, s.account_id).await;
    assert_eq!(summary.estimates, binomial_keys().len());

    type EstimateCols = (String, Option<f64>, Option<f64>, f64, f64, String);
    let rows: Vec<EstimateCols> = sqlx::query_as(
        "SELECT series_key, band_median, percentile, confidence, priority, state::text \
         FROM weakness_estimates WHERE account_id = $1",
    )
    .bind(s.account_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), binomial_keys().len());
    for (key, median, pct, confidence, priority, state) in &rows {
        assert_eq!((median, pct), (&None, &None), "{key}");
        assert_eq!((*confidence, *priority), (0.0, 0.0), "{key}");
        assert_eq!(state, "not_an_issue", "{key}");
    }
    let states: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT to_state::text FROM weakness_state_history WHERE account_id = $1",
    )
    .bind(s.account_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(states, ["not_an_issue"]);
    assert!(
        events(&pool, s.account_id, "weakness.changed")
            .await
            .is_empty()
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn records_build_profile_stage_metrics(pool: PgPool) {
    let s = seed(&pool, SeedOptions::default()).await;
    build(&pool, s.account_id).await;
    let (stages, total): (Value, f64) = sqlx::query_as(
        "SELECT stages, total_ms FROM job_metrics WHERE kind = 'build-profile' \
         AND account_id = $1",
    )
    .bind(s.account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut keys: Vec<&str> = stages
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    keys.sort_unstable();
    let mut expected = BUILD_PROFILE_STAGES.to_vec();
    expected.sort_unstable();
    assert_eq!(keys, expected);
    assert_eq!(
        expected,
        [
            "estimates_ms",
            "kpi_ms",
            "load_ms",
            "persist_ms",
            "repertoire_ms",
            "timeline_ms"
        ]
    );
    assert!(total >= 0.0);
    assert!(
        stages
            .as_object()
            .unwrap()
            .values()
            .all(|v| v.as_f64().is_some_and(|x| x >= 0.0))
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn build_enqueues_schedule_analysis(pool: PgPool) {
    let s = seed(&pool, SeedOptions::default()).await;
    build(&pool, s.account_id).await;
    assert_eq!(
        live_jobs(&pool, KIND_SCHEDULE, &schedule_key(s.account_id)).await,
        1
    );
}

// ---------------------------------------------------------------------------------------------
// Concurrency, follow-ups and the honesty guards

/// Builds of the same account x time class serialise: two at once write what one would, and a
/// third finds nothing new.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn concurrent_builds_serialise_and_append_no_duplicates(pool: PgPool) {
    let s = seed(&pool, SeedOptions::default()).await;
    let (d1, d2) = (deps(&pool), deps(&pool));
    let (a, b) = within("two builds", async {
        tokio::join!(
            handle_build_profile(&d1, payload(s.account_id)),
            handle_build_profile(&d2, payload(s.account_id)),
        )
    })
    .await;
    let (a, b) = (a.unwrap(), b.unwrap());
    // One build wrote everything, the other found it written.
    assert_eq!(a.transitions.min(b.transitions), 0, "{a:?} {b:?}");
    assert_eq!(a.snapshots.min(b.snapshots), 0, "{a:?} {b:?}");
    let dup_history = count(
        &pool,
        "SELECT count(*) - count(DISTINCT (series_key, kind, to_state, at_game_id)) \
         FROM weakness_state_history WHERE account_id = $1",
        s.account_id,
    )
    .await;
    assert_eq!(dup_history, 0);
    let dup_snapshots = count(
        &pool,
        "SELECT count(*) - count(DISTINCT (metric, window_key)) FROM kpi_snapshot \
         WHERE account_id = $1",
        s.account_id,
    )
    .await;
    assert_eq!(dup_snapshots, 0);
    let third = build(&pool, s.account_id).await;
    assert_eq!((third.snapshots, third.transitions), (0, 0));
    // Exactly one W1 change event over both builds.
    let w1 = events(&pool, s.account_id, "weakness.changed")
        .await
        .into_iter()
        .filter(|p| p["id"] == "W1")
        .count();
    assert_eq!(w1, 1);
}

/// A game committed while the build runs, whose `analysed_at` (its transaction's start) is
/// *before* the build's `started_at`, is still picked up by a follow-up build: the check
/// compares what the build loaded with what is stored, not only timestamps.
/// Installs a trigger that commits a late game (played 2026-11-18, `analysed_at` 13:00, before
/// [`SEED_NOW`]) inside the build's own write transaction, after its loads: the race of a game
/// committing mid-build whose `analysed_at` predates `started_at`.
async fn install_late_game_trigger(pool: &PgPool, account_id: Uuid) {
    let late = insert_unanalysed_game(pool, account_id, datetime!(2026-11-18 12:00 UTC)).await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "CREATE FUNCTION t33_late_game() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN \
           INSERT INTO game_metrics (game_id, account_id, time_class, played_at, protocol, \
             enrichment_protocol, metrics) \
           VALUES ('{late}', '{account_id}', 'blitz', '2026-11-18T12:00:00Z', '{PROTOCOL}', \
             'enr-v1', '{metrics}'::jsonb) ON CONFLICT (game_id) DO NOTHING; \
           UPDATE games SET analysis_status = 'done', analysed_at = '2026-11-18T13:00:00Z' \
             WHERE id = '{late}'; \
           RETURN NULL; \
         END $$; \
         CREATE TRIGGER t33_late_game AFTER INSERT ON weakness_estimates \
           FOR EACH STATEMENT EXECUTE FUNCTION t33_late_game();",
        metrics = serde_json::to_string(&seed_metrics()).unwrap(),
    )))
    .execute(pool)
    .await
    .unwrap();
}

async fn drop_late_game_trigger(pool: &PgPool) {
    sqlx::raw_sql("DROP TRIGGER t33_late_game ON weakness_estimates")
        .execute(pool)
        .await
        .unwrap();
}

/// A game committed while the build runs, whose `analysed_at` (its transaction's start) is
/// *before* the build's `started_at`, is still picked up by a follow-up build: the check
/// compares what the build loaded with what is stored, not only timestamps.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn game_committed_mid_build_with_early_analysed_at_gets_a_follow_up(pool: PgPool) {
    let s = seed(&pool, SeedOptions::default()).await;
    install_late_game_trigger(&pool, s.account_id).await;
    build(&pool, s.account_id).await;
    drop_late_game_trigger(&pool).await;
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM game_metrics WHERE account_id = $1",
            s.account_id
        )
        .await,
        41
    );
    assert_eq!(
        live_builds(&pool, s.account_id).await,
        1,
        "the late game must get a follow-up build"
    );
}

/// While a build job runs, its dedupe key would block the hook's enqueue for a game that
/// finishes after the build's final check. The job gives the key up before that check, so the
/// hook's enqueue succeeds even before the runner marks the job done.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn running_build_job_releases_its_dedupe_key_before_its_final_check(pool: PgPool) {
    let s = seed(&pool, SeedOptions::default()).await;
    let queue = Queue::new(pool.clone());
    let key = profile_key(s.account_id, "blitz");
    queue
        .enqueue(
            KIND_BUILD_PROFILE,
            &serde_json::to_value(payload(s.account_id)).unwrap(),
            EnqueueOptions {
                dedupe_key: Some(key.clone()),
                user_key: Some(s.account_id),
                ..EnqueueOptions::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    let job = queue
        .dequeue(&[KIND_BUILD_PROFILE], "t33", 1, Duration::from_secs(60))
        .await
        .unwrap()
        .pop()
        .unwrap();
    // While it is running, the hook cannot enqueue.
    assert!(
        !enqueue_build_profile(&queue, s.account_id, chess_core::TimeClass::Blitz)
            .await
            .unwrap()
    );
    let handler = BuildProfileHandler(Arc::new(deps(&pool)));
    let out = within(
        "handler",
        handler.handle(
            job.clone(),
            JobContext {
                cancel: CancellationToken::new(),
            },
        ),
    )
    .await
    .expect("handler");
    assert!(out.get("snapshots").is_some(), "{out}");
    // The runner has not completed the job yet: it is still running...
    let state: String = sqlx::query_scalar("SELECT state::text FROM jobs WHERE id = $1")
        .bind(job.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, "running");
    // ...but a game finishing now gets its build.
    assert!(
        enqueue_build_profile(&queue, s.account_id, chess_core::TimeClass::Blitz)
            .await
            .unwrap(),
        "the running job still holds {key}"
    );
    assert_eq!(live_jobs(&pool, KIND_BUILD_PROFILE, &key).await, 1);
}

/// Every week with games is checked, not only weeks of games analysed after the last snapshot's
/// time: a game whose analysed_at predates the last snapshot but that the last build never saw
/// still gets its week.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn week_of_a_game_analysed_before_the_last_snapshot_is_still_snapshotted(pool: PgPool) {
    let s = seed(&pool, SeedOptions::default()).await;
    build(&pool, s.account_id).await;
    let at = datetime!(2026-11-03 10:00 UTC); // 2026-W45
    insert_analysed_game(&pool, s.account_id, at, datetime!(2026-11-03 11:00 UTC)).await;
    let before = snapshot_rows(&pool, s.account_id).await;
    let again = build(&pool, s.account_id).await;
    let w45 = count(
        &pool,
        "SELECT count(*) FROM kpi_snapshot WHERE account_id = $1 AND window_key = '2026-W45'",
        s.account_id,
    )
    .await;
    assert!(w45 > 0, "2026-W45 was never snapshotted");
    assert_eq!(
        usize::try_from(snapshot_rows(&pool, s.account_id).await - before).unwrap(),
        again.snapshots
    );
}

/// A stale-version game_metrics row is no evidence: it changes neither estimates nor snapshots.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn stale_metrics_rows_are_left_out(pool: PgPool) {
    let s = seed(&pool, SeedOptions::default()).await;
    sqlx::query(
        "UPDATE game_metrics SET metrics = jsonb_set(metrics, '{version}', '1') \
         WHERE game_id = $1",
    )
    .bind(s.games[0])
    .execute(&pool)
    .await
    .unwrap();
    build(&pool, s.account_id).await;
    let n: i32 = sqlx::query_scalar(
        "SELECT n FROM kpi_snapshot WHERE account_id = $1 AND metric = 'blunders_per_game' \
         AND window_key = 'last50'",
    )
    .bind(s.account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 39);
    // Its series point (written with the stale metrics) is left out too.
    let opps: i32 = sqlx::query_scalar(
        "SELECT n_unweighted FROM weakness_estimates WHERE account_id = $1 AND series_key = 'W1'",
    )
    .bind(s.account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(opps, 39 * 50);
}

// ---------------------------------------------------------------------------------------------
// Fix round 1: the handler path, the replayed history and the adjusted-value change

/// Enqueues and claims a keyed build job, as the runner would.
async fn claim_build_job(pool: &PgPool, account_id: Uuid) -> queue::Job {
    let queue = Queue::new(pool.clone());
    queue
        .enqueue(
            KIND_BUILD_PROFILE,
            &serde_json::to_value(payload(account_id)).unwrap(),
            EnqueueOptions {
                dedupe_key: Some(profile_key(account_id, "blitz")),
                user_key: Some(account_id),
                ..EnqueueOptions::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    queue
        .dequeue(&[KIND_BUILD_PROFILE], "t33", 1, Duration::from_secs(60))
        .await
        .unwrap()
        .pop()
        .unwrap()
}

async fn run_handler(pool: &PgPool, job: &queue::Job) -> Value {
    let handler = BuildProfileHandler(Arc::new(deps(pool)));
    within(
        "handler",
        handler.handle(
            job.clone(),
            JobContext {
                cancel: CancellationToken::new(),
            },
        ),
    )
    .await
    .expect("handler")
}

/// Keyed build jobs still queued (the follow-up), not the running job itself.
async fn queued_keyed_builds(pool: &PgPool, a: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = $1 AND dedupe_key = $2 AND state = 'queued'",
    )
    .bind(KIND_BUILD_PROFILE)
    .bind(profile_key(a, "blitz"))
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Through the runner's handler: the job releases its key *before* it enqueues the follow-up,
/// or the follow-up would dedupe against the job itself and be lost.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn handler_game_analysed_after_start_leaves_one_queued_follow_up(pool: PgPool) {
    let s = seed(&pool, SeedOptions::default()).await;
    sqlx::query("UPDATE games SET analysed_at = $2 WHERE id = $1")
        .bind(s.games[39])
        .bind(SEED_NOW + time::Duration::minutes(5))
        .execute(&pool)
        .await
        .unwrap();
    let job = claim_build_job(&pool, s.account_id).await;
    run_handler(&pool, &job).await;
    assert_eq!(queued_keyed_builds(&pool, s.account_id).await, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn handler_game_committed_mid_build_leaves_one_queued_follow_up(pool: PgPool) {
    let s = seed(&pool, SeedOptions::default()).await;
    install_late_game_trigger(&pool, s.account_id).await;
    let job = claim_build_job(&pool, s.account_id).await;
    run_handler(&pool, &job).await;
    drop_late_game_trigger(&pool).await;
    assert_eq!(queued_keyed_builds(&pool, s.account_id).await, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn handler_without_changes_leaves_no_follow_up(pool: PgPool) {
    let s = seed(&pool, SeedOptions::default()).await;
    let job = claim_build_job(&pool, s.account_id).await;
    run_handler(&pool, &job).await;
    assert_eq!(queued_keyed_builds(&pool, s.account_id).await, 0);
}

type HistoryRow = (String, String, Option<String>, String, Option<Uuid>);

async fn history(pool: &PgPool, a: Uuid) -> Vec<HistoryRow> {
    sqlx::query_as(
        "SELECT series_key, kind::text, from_state::text, to_state::text, at_game_id \
         FROM weakness_state_history WHERE account_id = $1 \
         ORDER BY series_key, at_played_at, id",
    )
    .bind(a)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Older games analysed after the profile was built (the scheduler's P2 backfill) move the
/// replay's rows to other games. History is the current replay: each series keeps exactly one
/// first (`from NULL`) row, its transitions chain, and an unchanged current state publishes no
/// event.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn backfilled_older_games_keep_one_consistent_history(pool: PgPool) {
    let s = seed(&pool, SeedOptions::default()).await;
    build(&pool, s.account_id).await;
    let w1_events = |evs: Vec<Value>| evs.into_iter().filter(|p| p["id"] == "W1").count();
    assert_eq!(
        w1_events(events(&pool, s.account_id, "weakness.changed").await),
        1
    );
    // 20 older games (2026-W44/45), analysed after the first build but before SEED_NOW.
    for i in 0..20 {
        let at = datetime!(2026-10-28 10:00 UTC) + time::Duration::hours(12 * i);
        insert_analysed_game(&pool, s.account_id, at, datetime!(2026-11-19 10:00 UTC)).await;
    }
    build(&pool, s.account_id).await;

    let rows = history(&pool, s.account_id).await;
    let mut firsts: std::collections::BTreeMap<&str, usize> = Default::default();
    for (key, _, from, _, _) in &rows {
        if from.is_none() {
            *firsts.entry(key.as_str()).or_default() += 1;
        }
    }
    assert!(!firsts.is_empty());
    assert!(firsts.values().all(|&n| n == 1), "{rows:#?}");
    // Transitions chain: each row starts where the previous one of its series ended.
    let mut last: std::collections::BTreeMap<&str, &str> = Default::default();
    for (key, kind, from, to, _) in &rows {
        if kind != "transition" {
            continue;
        }
        if let Some(prev) = last.get(key.as_str()) {
            assert_eq!(from.as_deref(), Some(*prev), "{key}: {rows:#?}");
        }
        last.insert(key.as_str(), to.as_str());
    }
    let state: String = sqlx::query_scalar(
        "SELECT state::text FROM weakness_estimates WHERE account_id = $1 AND series_key = 'W1'",
    )
    .bind(s.account_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, "active");
    // W1 was active before and after: no second "W1 became active".
    assert_eq!(
        w1_events(events(&pool, s.account_id, "weakness.changed").await),
        1
    );
    // A third build changes nothing.
    let third = build(&pool, s.account_id).await;
    assert_eq!(third.transitions, 0);
    assert_eq!(history(&pool, s.account_id).await, rows);
}

async fn latest_snapshot_id(pool: &PgPool, a: Uuid, metric: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT max(id) FROM kpi_snapshot WHERE account_id = $1 AND metric = $2 \
         AND window_key = 'last50'",
    )
    .bind(a)
    .bind(metric)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// A change of only `value_adjusted` or of the interval (for example after a new reference)
/// is a change: the latest stored row would otherwise stay stale.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn adjusted_value_or_interval_change_appends_a_row(pool: PgPool) {
    let s = seed(&pool, SeedOptions::default()).await;
    build(&pool, s.account_id).await;
    let adj = latest_snapshot_id(&pool, s.account_id, "weakness:W1").await;
    let ci = latest_snapshot_id(&pool, s.account_id, "blunders_per_game").await;
    let (adjusted, high): (Option<f64>, Option<f64>) = sqlx::query_as(
        "SELECT (SELECT value_adjusted FROM kpi_snapshot WHERE id = $1), \
                (SELECT ci_high FROM kpi_snapshot WHERE id = $2)",
    )
    .bind(adj)
    .bind(ci)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        adjusted.is_some() && high.is_some(),
        "{adjusted:?} {high:?}"
    );
    sqlx::query("UPDATE kpi_snapshot SET value_adjusted = value_adjusted + 0.25 WHERE id = $1")
        .bind(adj)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE kpi_snapshot SET ci_high = ci_high + 0.25 WHERE id = $1")
        .bind(ci)
        .execute(&pool)
        .await
        .unwrap();
    let again = build(&pool, s.account_id).await;
    assert_eq!(again.snapshots, 2);
    assert!(latest_snapshot_id(&pool, s.account_id, "weakness:W1").await > adj);
    assert!(latest_snapshot_id(&pool, s.account_id, "blunders_per_game").await > ci);
}
