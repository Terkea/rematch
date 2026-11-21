//! Port of apps/worker/test/recovery/reconciler.test.ts, plus paused-run revival, the recurring
//! schedule and `job_metrics` retention.
//!
//! The TS tests injected a clock; here the database clock (`now()`) is the only one, so rows are
//! back-dated instead. Every margin is at least a minute, well above the ~3 s back-steps of the
//! Docker VM clock (ADR 0032).

use std::time::Duration;

use analysis::jobs::reconcile::{
    EXHAUSTED_REASON, MAX_ANALYSIS_ATTEMPTS, ReconcileOptions, ReconcileResult,
    enqueue_analysis_page, ensure_reconcile_scheduled, reconcile,
};
use queue::queue::Queue;
use queue::{
    EnqueueOptions, JobState, KIND_ANALYSE, KIND_IMPORT, KIND_RECONCILE, RECONCILE_KEY,
    analyse_key, import_key,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use testkit::{GameSeed, insert_account, insert_game};
use uuid::Uuid;

const ZERO: ReconcileResult = ReconcileResult {
    requeued_pending: 0,
    reset_stale: 0,
    resumed_imports: 0,
    revived_paused: 0,
    hung_active: 0,
    exhausted: 0,
    pruned_events: 0,
    cleaned_jobs: 0,
};

/// The DDL of T19's `20261121000400_job_metrics.sql`, for branches where it has not landed yet.
const JOB_METRICS_DDL: &str = "\
CREATE TABLE IF NOT EXISTS job_metrics (
  id bigserial PRIMARY KEY, job_id bigint, kind text NOT NULL, game_id uuid, account_id uuid,
  queue_wait_ms double precision, parse_ms double precision, cache_ms double precision,
  engine_wall_ms double precision, engine_cpu_ms double precision, persist_ms double precision,
  total_ms double precision NOT NULL, positions integer, engine_positions integer,
  cache_hits integer, lichess_used integer, created_at timestamptz NOT NULL DEFAULT now())";

fn opts() -> ReconcileOptions {
    ReconcileOptions::default()
}

async fn sweep(pool: &PgPool) -> ReconcileResult {
    reconcile(pool, &Queue::new(pool.clone()), &opts())
        .await
        .expect("reconcile")
}

async fn game(pool: &PgPool, account: Uuid, pgn: &str) -> Uuid {
    insert_game(
        pool,
        account,
        GameSeed {
            pgn: pgn.to_owned(),
            ..GameSeed::default()
        },
    )
    .await
}

/// Sets a game to `analysing` with its heartbeat `mins` minutes ago.
async fn analysing(pool: &PgPool, id: Uuid, mins: f64, attempts: i32) {
    sqlx::query(
        "UPDATE games SET analysis_status = 'analysing', analysis_attempts = $3, \
         analysis_started_at = now() - interval '1 hour', \
         heartbeat_at = now() - make_interval(secs => $2 * 60) WHERE id = $1",
    )
    .bind(id)
    .bind(mins)
    .bind(attempts)
    .execute(pool)
    .await
    .unwrap();
}

async fn status(pool: &PgPool, id: Uuid) -> (String, Option<String>) {
    sqlx::query_as("SELECT analysis_status::text, analysis_error FROM games WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Live jobs of `kind` with `key`: (state, payload).
async fn live_jobs(pool: &PgPool, kind: &str, key: &str) -> Vec<(String, Value)> {
    sqlx::query_as(
        "SELECT state::text, payload FROM jobs WHERE kind = $1 AND dedupe_key = $2 \
         AND state IN ('queued', 'running', 'paused')",
    )
    .bind(kind)
    .bind(key)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn enqueue_analysis(queue: &Queue, game: Uuid, account: Uuid) -> Option<i64> {
    queue
        .enqueue(
            KIND_ANALYSE,
            &json!({ "gameId": game }),
            EnqueueOptions {
                dedupe_key: Some(analyse_key(game)),
                user_key: Some(account),
                max_attempts: 3,
                ..EnqueueOptions::default()
            },
        )
        .await
        .unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn reconciler_requeues_lost_pending(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = insert_account(&pool, "lichess", "lost").await;
    let id = game(&pool, acct, "1. e4 e5 2. Nf3 *").await;
    // The job was lost (the TS test dropped it from Redis).
    enqueue_analysis(&queue, id, acct).await.unwrap();
    sqlx::query("DELETE FROM jobs WHERE dedupe_key = $1")
        .bind(analyse_key(id))
        .execute(&pool)
        .await
        .unwrap();

    // Freshly pending games are left alone: their enqueue may simply not have happened yet.
    assert_eq!(sweep(&pool).await, ZERO);

    sqlx::query("UPDATE games SET pending_since = now() - interval '3 minutes' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        sweep(&pool).await,
        ReconcileResult {
            requeued_pending: 1,
            ..ZERO
        }
    );
    let jobs = live_jobs(&pool, KIND_ANALYSE, &analyse_key(id)).await;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].1, json!({ "gameId": id }));
    let (user_key, max_attempts): (Option<Uuid>, i32) =
        sqlx::query_as("SELECT user_key, max_attempts FROM jobs WHERE dedupe_key = $1")
            .bind(analyse_key(id))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((user_key, max_attempts), (Some(acct), 3));

    // Idempotent: the job is now live, so a second sweep does nothing.
    assert_eq!(sweep(&pool).await.requeued_pending, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn reconciler_resets_stale_analysing(pool: PgPool) {
    let acct = insert_account(&pool, "lichess", "stale").await;
    let stale = game(&pool, acct, "1. d4 d5 *").await;
    let fresh = game(&pool, acct, "1. c4 c5 *").await;
    analysing(&pool, stale, 11.0, 1).await;
    analysing(&pool, fresh, 1.0, 1).await;

    assert_eq!(
        sweep(&pool).await,
        ReconcileResult {
            reset_stale: 1,
            ..ZERO
        }
    );
    assert_eq!(status(&pool, stale).await.0, "pending");
    let pending_recent: bool = sqlx::query_scalar(
        "SELECT pending_since > now() - interval '1 minute' FROM games WHERE id = $1",
    )
    .bind(stale)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(pending_recent, "the reset sets pending_since = now()");
    assert_eq!(
        live_jobs(&pool, KIND_ANALYSE, &analyse_key(stale))
            .await
            .len(),
        1
    );
    assert_eq!(status(&pool, fresh).await.0, "analysing");
    assert!(
        live_jobs(&pool, KIND_ANALYSE, &analyse_key(fresh))
            .await
            .is_empty()
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn honours_stale_after(pool: PgPool) {
    let acct = insert_account(&pool, "lichess", "custom").await;
    let g = game(&pool, acct, "1. Nf3 Nf6 *").await;
    analysing(&pool, g, 2.0, 1).await;
    let res = reconcile(
        &pool,
        &Queue::new(pool.clone()),
        &ReconcileOptions {
            stale_after: Duration::from_secs(90),
            ..opts()
        },
    )
    .await
    .unwrap();
    assert_eq!(res.reset_stale, 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn fails_a_game_that_exhausted_its_attempts_instead_of_re_enqueueing_it_forever(
    pool: PgPool,
) {
    assert_eq!(MAX_ANALYSIS_ATTEMPTS, 6);
    assert_eq!(
        EXHAUSTED_REASON,
        "exceeded max attempts (stalled or crashed worker)"
    );
    let acct = insert_account(&pool, "lichess", "exhausted").await;
    let stuck = game(&pool, acct, "1. b3 e5 *").await;
    analysing(&pool, stuck, 11.0, 6).await;
    let lost = insert_game(
        &pool,
        acct,
        GameSeed {
            pgn: "1. b4 e5 *".to_owned(),
            pending_since_ago: Some(Duration::from_secs(180)),
            ..GameSeed::default()
        },
    )
    .await;
    sqlx::query("UPDATE games SET analysis_attempts = 7 WHERE id = $1")
        .bind(lost)
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(
        sweep(&pool).await,
        ReconcileResult {
            exhausted: 2,
            ..ZERO
        }
    );
    for g in [stuck, lost] {
        assert_eq!(
            status(&pool, g).await,
            ("failed".to_owned(), Some(EXHAUSTED_REASON.to_owned()))
        );
        assert!(
            live_jobs(&pool, KIND_ANALYSE, &analyse_key(g))
                .await
                .is_empty()
        );
    }
    let mut events: Vec<(Uuid, String, Value)> =
        sqlx::query_as("SELECT account_id, type, payload FROM user_events")
            .fetch_all(&pool)
            .await
            .unwrap();
    events.sort_by_key(|e| e.2["gameId"].as_str().unwrap_or_default().to_owned());
    let mut expected: Vec<(Uuid, String, Value)> = [stuck, lost]
        .iter()
        .map(|g| {
            (
                acct,
                "job.failed".to_owned(),
                json!({ "kind": "analyse-game", "gameId": g, "reason": EXHAUSTED_REASON }),
            )
        })
        .collect();
    expected.sort_by_key(|e| e.2["gameId"].as_str().unwrap_or_default().to_owned());
    assert_eq!(events, expected);

    // Given up once: a second sweep neither counts nor publishes again.
    assert_eq!(sweep(&pool).await, ZERO);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM user_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 2);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn leaves_a_stale_game_whose_job_is_still_running_alone_and_counts_it_as_hung_active(
    pool: PgPool,
) {
    let queue = Queue::new(pool.clone());
    let acct = insert_account(&pool, "lichess", "hung").await;
    let hung = game(&pool, acct, "1. g3 d5 *").await;
    analysing(&pool, hung, 11.0, 1).await;
    enqueue_analysis(&queue, hung, acct).await.unwrap();
    // A worker holds it with a live lease, but has stopped heartbeating the game.
    let claimed = queue
        .dequeue(&[KIND_ANALYSE], "hanging", 1, Duration::from_secs(600))
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);

    assert_eq!(
        sweep(&pool).await,
        ReconcileResult {
            hung_active: 1,
            ..ZERO
        }
    );
    assert_eq!(status(&pool, hung).await.0, "analysing");
    let states = queue
        .live_states(KIND_ANALYSE, &[analyse_key(hung)])
        .await
        .unwrap();
    assert_eq!(states.get(&analyse_key(hung)), Some(&JobState::Running));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn resumes_import_runs_whose_heartbeat_went_stale(pool: PgPool) {
    let acct = insert_account(&pool, "chesscom", "importer").await;
    let stale_run: Uuid = sqlx::query_scalar(
        "INSERT INTO import_runs (account_id, status, started_at, heartbeat_at) \
         VALUES ($1, 'running', now() - interval '1 hour', now() - interval '11 minutes') \
         RETURNING id",
    )
    .bind(acct)
    .fetch_one(&pool)
    .await
    .unwrap();
    let live_run: Uuid = sqlx::query_scalar(
        "INSERT INTO import_runs (account_id, status) VALUES ($1, 'running') RETURNING id",
    )
    .bind(acct)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(sweep(&pool).await.resumed_imports, 1);
    let jobs = live_jobs(&pool, KIND_IMPORT, &import_key(acct)).await;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].1, json!({ "accountId": acct }));

    let run = |id: Uuid| {
        let pool = pool.clone();
        async move {
            sqlx::query_as::<_, (String, Option<String>, Option<String>, bool)>(
                "SELECT status::text, last_error, error_code, finished_at IS NOT NULL \
                 FROM import_runs WHERE id = $1",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    let (st, err, code, finished) = run(stale_run).await;
    assert_eq!(st, "failed");
    assert_eq!(
        err.as_deref(),
        Some("stale heartbeat: resumed by the reconciler")
    );
    assert_eq!(code.as_deref(), Some("timeout"));
    assert!(finished);
    assert_eq!(run(live_run).await.0, "running");

    // The stale run is closed, so it is not resumed twice.
    assert_eq!(sweep(&pool).await.resumed_imports, 0);
    assert_eq!(
        live_jobs(&pool, KIND_IMPORT, &import_key(acct)).await.len(),
        1
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn prunes_user_events_older_than_7_days(pool: PgPool) {
    let acct = insert_account(&pool, "lichess", "pruned").await;
    sqlx::query(
        "INSERT INTO user_events (account_id, type, payload, created_at) VALUES \
         ($1, 'x', '{}', now() - interval '8 days'), ($1, 'y', '{}', now())",
    )
    .bind(acct)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(sweep(&pool).await.pruned_events, 1);
    let left: Vec<String> =
        sqlx::query_scalar("SELECT type FROM user_events WHERE account_id = $1")
            .bind(acct)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(left, ["y"]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn revives_paused_run_without_live_job(pool: PgPool) {
    let acct = insert_account(&pool, "chesscom", "paused").await;
    sqlx::query(
        "INSERT INTO import_runs (account_id, status, error_code) \
         VALUES ($1, 'paused', 'circuit_open')",
    )
    .bind(acct)
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        sweep(&pool).await,
        ReconcileResult {
            revived_paused: 1,
            ..ZERO
        }
    );
    assert_eq!(
        live_jobs(&pool, KIND_IMPORT, &import_key(acct)).await.len(),
        1
    );
    // The run itself is left for the import job to reuse.
    let st: String =
        sqlx::query_scalar("SELECT status::text FROM import_runs WHERE account_id = $1")
            .bind(acct)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(st, "paused");

    assert_eq!(sweep(&pool).await.revived_paused, 0);
    assert_eq!(
        live_jobs(&pool, KIND_IMPORT, &import_key(acct)).await.len(),
        1
    );
}

/// Only the account's newest run counts (the import job reuses only that one, T21): an old
/// paused run under a newer finished one must not trigger an import on every sweep.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn ignores_a_paused_run_that_is_not_the_newest(pool: PgPool) {
    let acct = insert_account(&pool, "chesscom", "superseded").await;
    sqlx::query(
        "INSERT INTO import_runs (account_id, status, started_at) VALUES \
         ($1, 'paused', now() - interval '2 hours'), ($1, 'done', now() - interval '1 hour')",
    )
    .bind(acct)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(sweep(&pool).await, ZERO);
    assert!(
        live_jobs(&pool, KIND_IMPORT, &import_key(acct))
            .await
            .is_empty()
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn ensure_reconcile_scheduled_is_idempotent(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    for _ in 0..3 {
        ensure_reconcile_scheduled(&queue).await.unwrap();
    }
    let rows: Vec<(i16, i32, bool)> = sqlx::query_as(
        "SELECT priority, max_attempts, recur_every = interval '300 seconds' FROM jobs \
         WHERE kind = $1 AND dedupe_key = $2 AND state IN ('queued', 'running', 'paused')",
    )
    .bind(KIND_RECONCILE)
    .bind(RECONCILE_KEY)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows, [(0, 1, true)]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn prunes_job_metrics_older_than_30_days(pool: PgPool) {
    sqlx::raw_sql(JOB_METRICS_DDL).execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO job_metrics (kind, total_ms, created_at) VALUES \
         ('analyse-game', 1, now() - interval '31 days'), ('analyse-game', 2, now() - interval '29 days')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sweep(&pool).await;
    let left: Vec<f64> = sqlx::query_scalar("SELECT total_ms FROM job_metrics")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(left, [2.0]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn cleans_finished_jobs(pool: PgPool) {
    sqlx::query(
        "INSERT INTO jobs (kind, payload, state, finished_at) VALUES \
         ('analyse-game', '{}', 'done', now() - interval '2 days'), \
         ('analyse-game', '{}', 'done', now())",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(sweep(&pool).await.cleaned_jobs, 1);
}

/// The pending sweep enqueues a page in one statement; its rows must equal what
/// `Queue::enqueue` writes with the analyse options, and it must respect live dedupe keys.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn page_enqueue_matches_queue_enqueue(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = insert_account(&pool, "lichess", "page").await;
    let single = game(&pool, acct, "1. e4 *").await;
    let paged = game(&pool, acct, "1. d4 *").await;
    enqueue_analysis(&queue, single, acct).await.unwrap();

    let mut listener = sqlx::postgres::PgListener::connect_with(&pool)
        .await
        .unwrap();
    listener.listen(queue::queue::JOBS_CHANNEL).await.unwrap();
    // `single` already has a live job: only `paged` is new.
    let n = enqueue_analysis_page(&queue, &[(single, acct), (paged, acct)])
        .await
        .unwrap();
    assert_eq!(n, 1);
    let note = tokio::time::timeout(Duration::from_secs(5), listener.recv())
        .await
        .expect("a NOTIFY")
        .unwrap();
    assert_eq!(note.payload(), KIND_ANALYSE);

    type Row = (
        String,
        Value,
        Option<String>,
        Option<Uuid>,
        i16,
        i32,
        i32,
        i32,
        String,
        bool,
        bool,
    );
    let row = |id: Uuid| {
        let pool = pool.clone();
        async move {
            sqlx::query_as::<_, Row>(
                "SELECT kind, payload, dedupe_key, user_key, priority, max_attempts, attempts, \
                 max_stalls, state::text, recur_every IS NULL, run_at <= now() FROM jobs \
                 WHERE dedupe_key = $1",
            )
            .bind(analyse_key(id))
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    let (a, b) = (row(single).await, row(paged).await);
    let strip = |r: Row, id: Uuid| {
        assert_eq!(r.1, json!({ "gameId": id }));
        assert_eq!(r.2.as_deref(), Some(analyse_key(id).as_str()));
        (r.0, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10)
    };
    assert_eq!(strip(a, single), strip(b, paged));
    assert_eq!(enqueue_analysis_page(&queue, &[]).await.unwrap(), 0);
}
