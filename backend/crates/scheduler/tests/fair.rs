//! Fair round-robin dequeue (R2 Task 27, R-12): tier, then slices of 5 per account (counting the
//! games it started in the last hour), then join time; reference games last. Claim correctness
//! and fairness under several concurrent workers, and an index-backed plan at scale.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use queue::queue::{Queue, QueueError};
use queue::runner::{Handler, JobContext, Runner, RunnerConfig};
use queue::strategy::SelectStrategy;
use queue::{EnqueueOptions, Job, JobError};
use scheduler::fair::{
    FairRoundRobin, NotAServedKind, ServedKind, fair_claim_sql, first_candidates,
};
use scheduler::priority::job_priority;
use scheduler::select::Plan;
use serde_json::{Value, json};
use sqlx::PgPool;
use testkit::wait::wait_for;
use uuid::Uuid;

const KIND: &str = "analyse-game";
const LEASE: Duration = Duration::from_secs(60);

fn p0() -> i16 {
    job_priority(0, Plan::Free, true)
}

fn p2() -> i16 {
    job_priority(2, Plan::Free, true)
}

/// A linked account that joined `days_ago` days ago.
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

/// `n` games of `acc`; `started_ago_min` sets `analysis_started_at` that many minutes ago.
async fn games(pool: &PgPool, acc: Uuid, n: i32, started_ago_min: Option<i32>) -> Vec<Uuid> {
    sqlx::query_scalar(
        "INSERT INTO games (account_id, platform, platform_game_id, pgn, played_at, time_class, \
           rated, player_color, white_name, black_name, result, analysis_status, \
           analysis_started_at) \
         SELECT $1, 'lichess', gen_random_uuid()::text, '1. e4 *', now(), 'blitz', true, \
           'white', 'w', 'b', '*', 'pending', \
           CASE WHEN $3::int4 IS NULL THEN NULL ELSE now() - $3 * interval '1 minute' END \
         FROM generate_series(1, $2) RETURNING id",
    )
    .bind(acc)
    .bind(n)
    .bind(started_ago_min)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// One queued analyse job per game, through `Queue::enqueue`, in order.
async fn enqueue(q: &Queue, user: Option<Uuid>, games: &[Uuid], priority: i16) -> Vec<i64> {
    let mut ids = Vec::new();
    for g in games {
        let id = q
            .enqueue(
                KIND,
                &json!({ "game_id": g }),
                EnqueueOptions {
                    user_key: user,
                    priority,
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

/// What the analyse handler does when it starts: the game counts as served.
async fn start_game(pool: &PgPool, job: &Job) {
    let Some(game) = job.payload.get("game_id").and_then(Value::as_str) else {
        return;
    };
    sqlx::query("UPDATE games SET analysis_started_at = now() WHERE id = $1::uuid")
        .bind(game)
        .execute(pool)
        .await
        .unwrap();
}

/// Claims one job at a time with the fair strategy (each started and completed), `n` times.
async fn claim_in_turn(q: &Queue, n: usize) -> Vec<Job> {
    let fair = FairRoundRobin::default();
    let mut out = Vec::new();
    for _ in 0..n {
        let jobs = fair.dequeue(q, &[KIND], "w", 1, LEASE).await.unwrap();
        assert_eq!(jobs.len(), 1, "claim {} of {n} got {jobs:?}", out.len());
        let job = jobs.into_iter().next().unwrap();
        start_game(q.pool(), &job).await;
        assert!(q.complete(&job).await.unwrap());
        out.push(job);
    }
    out
}

fn users(jobs: &[Job]) -> Vec<Option<Uuid>> {
    jobs.iter().map(|j| j.user_key).collect()
}

#[test]
fn defaults_are_slices_of_5_over_one_hour() {
    let f = FairRoundRobin::default();
    assert_eq!(f.slice, 5);
    assert_eq!(f.served_window, Duration::from_secs(3600));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn round_robin_in_slices_of_5(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let a = account(&pool, "a", 2).await;
    let b = account(&pool, "b", 1).await;
    // A's jobs are all older than B's, so FIFO would serve A twenty times first.
    let ga = games(&pool, a, 20, None).await;
    enqueue(&q, Some(a), &ga, p0()).await;
    let gb = games(&pool, b, 20, None).await;
    enqueue(&q, Some(b), &gb, p0()).await;

    let got = users(&claim_in_turn(&q, 40).await);
    let mut want = Vec::new();
    for _ in 0..4 {
        want.extend([Some(a); 5]);
        want.extend([Some(b); 5]);
    }
    assert_eq!(got, want);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn newcomer_gets_first_10_before_anyone_gets_11th(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let a = account(&pool, "heavy", 30).await;
    games(&pool, a, 10, Some(20)).await; // served in the last hour
    games(&pool, a, 30, Some(120)).await; // outside the window: not counted
    let ga = games(&pool, a, 40, None).await;
    enqueue(&q, Some(a), &ga, p0()).await;
    let b = account(&pool, "newcomer", 0).await;
    let gb = games(&pool, b, 50, None).await;
    enqueue(&q, Some(b), &gb, p0()).await;

    let got = users(&claim_in_turn(&q, 11).await);
    assert_eq!(&got[..10], &[Some(b); 10]);
    assert_eq!(got[10], Some(a), "A's 11th comes once B has had 10");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn tier_orders_before_slice(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let a = account(&pool, "a", 2).await;
    let b = account(&pool, "b", 1).await;
    // B has been served a lot (slice 10), A not at all, but A's jobs are P2.
    games(&pool, b, 50, Some(5)).await;
    let ga = games(&pool, a, 20, None).await;
    enqueue(&q, Some(a), &ga, p2()).await;
    let gb = games(&pool, b, 10, None).await;
    enqueue(&q, Some(b), &gb, p0()).await;
    assert_eq!(p2(), 20);

    let got = claim_in_turn(&q, 12).await;
    assert_eq!(&users(&got)[..10], &[Some(b); 10]);
    assert!(got[..10].iter().all(|j| j.priority == p0()));
    assert_eq!(&users(&got)[10..], &[Some(a); 2]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn reference_jobs_come_last(pool: PgPool) {
    let q = Queue::new(pool.clone());
    // Reference games: no user, priority 200, enqueued first.
    let refs: Vec<Uuid> = (0..7).map(|_| Uuid::new_v4()).collect();
    enqueue(&q, None, &refs, 200).await;
    let a = account(&pool, "a", 2).await;
    let ga = games(&pool, a, 6, None).await;
    enqueue(&q, Some(a), &ga, p2()).await;
    // A job whose account is gone (no linked_accounts row) still runs, after every account.
    let ghost = Uuid::new_v4();
    enqueue(&q, Some(ghost), &[Uuid::new_v4()], p2()).await;

    let got = claim_in_turn(&q, 14).await;
    let u = users(&got);
    // The ghost has no join time (infinity): it waits for A's first slice, then goes first.
    assert_eq!(&u[..5], &[Some(a); 5]);
    assert_eq!(u[5], Some(ghost));
    assert_eq!(u[6], Some(a));
    assert_eq!(
        &u[7..],
        &[None; 7],
        "the reference pseudo-user is served last, in full"
    );
    assert!(got[7..].iter().all(|j| j.priority == 200));
    let none = FairRoundRobin::default()
        .dequeue(&q, &[KIND], "w", 5, LEASE)
        .await
        .unwrap();
    assert!(none.is_empty());
}

/// A claim over several kinds ranks each account across all of them (no `kind = ANY` sort).
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn several_kinds_share_one_round_robin(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let a = account(&pool, "a", 2).await;
    let b = account(&pool, "b", 1).await;
    for (kind, user, n) in [("x", a, 3), ("y", a, 3), ("x", b, 6)] {
        for _ in 0..n {
            q.enqueue(
                kind,
                &json!({}),
                EnqueueOptions {
                    user_key: Some(user),
                    priority: p0(),
                    ..EnqueueOptions::default()
                },
            )
            .await
            .unwrap()
            .unwrap();
        }
    }
    // FairRoundRobin only claims served kinds, so the statement itself is driven here.
    let mut got = Vec::new();
    for _ in 0..12 {
        let claimed = q
            .claim_with(
                fair_claim_sql(),
                &["x", "y"],
                &[first_candidates(1), 3_600_000, 5, 1],
                "w",
                LEASE,
            )
            .await
            .unwrap();
        let job = claimed.jobs.into_iter().next().expect("a job");
        // Served is counted from games; mark one served game per claim.
        games(&pool, job.user_key.unwrap(), 1, Some(0)).await;
        got.push(job.user_key);
    }
    let mut want = vec![Some(a); 5];
    want.extend([Some(b); 5]);
    want.extend([Some(a), Some(b)]);
    assert_eq!(got, want);
}

/// R1's due rule (ADR 0032): a delayed job waits; a job whose `run_at` is ahead of `now()` only
/// because the clock stepped back since its enqueue (`run_at <= created_at`) is due.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn due_rule_matches_r1(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let a = account(&pool, "a", 1).await;
    q.enqueue(
        KIND,
        &json!({}),
        EnqueueOptions {
            user_key: Some(a),
            priority: p0(),
            run_at: Some(time::OffsetDateTime::now_utc() + time::Duration::hours(1)),
            ..EnqueueOptions::default()
        },
    )
    .await
    .unwrap()
    .unwrap();
    let fair = FairRoundRobin::default();
    assert!(
        fair.dequeue(&q, &[KIND], "w", 5, LEASE)
            .await
            .unwrap()
            .is_empty()
    );
    let stepped: i64 = sqlx::query_scalar(
        "INSERT INTO jobs (kind, payload, user_key, priority, run_at, created_at)          VALUES ($1, '{}', $2, 4, now() + interval '10 seconds', now() + interval '10 seconds')          RETURNING id",
    )
    .bind(KIND)
    .bind(a)
    .fetch_one(&pool)
    .await
    .unwrap();
    let got = fair.dequeue(&q, &[KIND], "w", 5, LEASE).await.unwrap();
    assert_eq!(got.iter().map(|j| j.id).collect::<Vec<_>>(), vec![stepped]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn concurrent_fair_dequeue_is_disjoint(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let a = account(&pool, "a", 2).await;
    let b = account(&pool, "b", 1).await;
    let fair = FairRoundRobin::default();
    for round in 0..10 {
        let ga = games(&pool, a, 5, None).await;
        let mut ids = enqueue(&q, Some(a), &ga, p0()).await;
        let gb = games(&pool, b, 5, None).await;
        ids.extend(enqueue(&q, Some(b), &gb, p0()).await);
        let (x, y) = tokio::join!(
            fair.dequeue(&q, &[KIND], "w1", 5, LEASE),
            fair.dequeue(&q, &[KIND], "w2", 5, LEASE)
        );
        let (x, y) = (x.unwrap(), y.unwrap());
        assert_eq!((x.len(), y.len()), (5, 5), "round {round}");
        let xs: HashSet<i64> = x.iter().map(|j| j.id).collect();
        let ys: HashSet<i64> = y.iter().map(|j| j.id).collect();
        assert!(xs.is_disjoint(&ys), "round {round}");
        let all: HashSet<i64> = xs.union(&ys).copied().collect();
        assert_eq!(all, ids.iter().copied().collect::<HashSet<_>>());
        for j in x.iter().chain(&y) {
            assert_eq!(j.attempts, 1);
            assert!(q.complete(j).await.unwrap());
        }
    }
}

/// Many claimers racing over several users: every job is claimed exactly once.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn racing_fair_claimers_never_share_a_job(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let mut expected = HashSet::new();
    for u in 0..6 {
        let acc = account(&pool, &format!("u{u}"), 10 - u).await;
        let g = games(&pool, acc, 50, None).await;
        expected.extend(enqueue(&q, Some(acc), &g, p0()).await);
    }
    let claimed = Arc::new(Mutex::new(Vec::new()));
    let mut tasks = Vec::new();
    for w in 0..8 {
        let (q, claimed) = (q.clone(), claimed.clone());
        tasks.push(tokio::spawn(async move {
            let fair = FairRoundRobin::default();
            let owner = format!("w{w}");
            loop {
                let jobs = fair.dequeue(&q, &[KIND], &owner, 3, LEASE).await.unwrap();
                if jobs.is_empty() {
                    let queued: i64 =
                        sqlx::query_scalar("SELECT count(*) FROM jobs WHERE state = 'queued'")
                            .fetch_one(q.pool())
                            .await
                            .unwrap();
                    if queued == 0 {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                for j in jobs {
                    start_game(q.pool(), &j).await;
                    assert!(q.complete(&j).await.unwrap(), "lease of {} lost", j.id);
                    claimed.lock().unwrap().push(j.id);
                }
            }
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    let claimed = claimed.lock().unwrap().clone();
    let unique: HashSet<i64> = claimed.iter().copied().collect();
    assert_eq!(unique.len(), claimed.len(), "a job was claimed twice");
    assert_eq!(unique, expected);
}

struct Recording {
    pool: PgPool,
    order: Arc<Mutex<Vec<Uuid>>>,
}

impl Handler for Recording {
    fn kind(&self) -> &'static str {
        KIND
    }

    fn handle(&self, job: Job, _ctx: JobContext) -> BoxFuture<'_, Result<Value, JobError>> {
        Box::pin(async move {
            start_game(&self.pool, &job).await;
            self.order.lock().unwrap().push(job.user_key.unwrap());
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(Value::Null)
        })
    }
}

/// Three runners (2 jobs each at once) over four users whose jobs were enqueued user by user:
/// FIFO would serve each user 20 times in a row. With the fair strategy no user is ever more
/// than one slice plus the in-flight jobs ahead of another.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn runners_with_fair_strategy_interleave_users(pool: PgPool) {
    const USERS: i32 = 4;
    const PER_USER: usize = 20;
    const RUNNERS: usize = 3;
    const CONCURRENCY: usize = 2;
    let q = Queue::new(pool.clone());
    let mut accounts = Vec::new();
    for u in 0..USERS {
        let acc = account(&pool, &format!("u{u}"), 10 - u).await;
        let g = games(&pool, acc, PER_USER as i32, None).await;
        enqueue(&q, Some(acc), &g, p0()).await;
        accounts.push(acc);
    }
    let order = Arc::new(Mutex::new(Vec::new()));
    let handles: Vec<_> = (0..RUNNERS)
        .map(|_| {
            let cfg = RunnerConfig {
                concurrency: CONCURRENCY,
                lease: Duration::from_secs(6),
                strategy: Arc::new(FairRoundRobin::default()),
                ..RunnerConfig::fast_for_tests()
            };
            let h = Recording {
                pool: pool.clone(),
                order: order.clone(),
            };
            Runner::spawn(q.clone(), Arc::new(h), cfg)
        })
        .collect();
    let total = USERS as i64 * PER_USER as i64;
    wait_for(
        "every job done",
        Duration::from_secs(60),
        Duration::from_millis(25),
        || async {
            let n: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE state = 'done'")
                .fetch_one(&pool)
                .await
                .unwrap();
            (n == total).then_some(())
        },
    )
    .await;
    for h in &handles {
        assert!(h.shutdown(Duration::from_secs(5)).await);
    }
    let attempts: Vec<i32> = sqlx::query_scalar("SELECT attempts FROM jobs")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(attempts.iter().all(|&a| a == 1), "{attempts:?}");
    let order = order.lock().unwrap().clone();
    assert_eq!(order.len(), total as usize);
    let bound = 5 + RUNNERS * CONCURRENCY;
    let mut seen: HashMap<Uuid, usize> = accounts.iter().map(|a| (*a, 0)).collect();
    for (i, u) in order.iter().enumerate() {
        *seen.get_mut(u).unwrap() += 1;
        let max = *seen.values().max().unwrap();
        let min = *seen.values().min().unwrap();
        // A user with nothing left cannot fall behind any further.
        let min_open = seen
            .values()
            .filter(|&&n| n < PER_USER)
            .min()
            .copied()
            .unwrap_or(min);
        assert!(
            max - min_open.min(max) <= bound,
            "after {} claims the counts are {seen:?}",
            i + 1
        );
    }
}

/// 50 000 queued jobs over 500 accounts (100 each), 20 served games per account in the window
/// (10 000 rows), and 2 000 reference jobs, then `VACUUM ANALYZE`.
const SCALE_SEED_SQL: &str = r#"
INSERT INTO linked_accounts (platform, username, display_name, created_at)
  SELECT 'lichess', 'p' || a, 'p' || a, now() - a * interval '1 hour'
  FROM generate_series(1, 500) a;
INSERT INTO games (account_id, platform, platform_game_id, pgn, played_at, time_class, rated,
                   player_color, white_name, black_name, result, analysis_status,
                   analysis_started_at)
  SELECT a.id, 'lichess', a.username || '-' || g, '1. e4 *', now(), 'blitz', true,
         'white', 'w', 'b', '*', 'pending',
         CASE WHEN g <= 20 THEN now() - g * interval '1 minute'
              WHEN g <= 40 THEN now() - interval '1 day' END
  FROM linked_accounts a, generate_series(1, 140) g;
-- Enqueued over the last 50 minutes, so run_at (= created_at) values are distinct, as from
-- separate enqueue transactions.
INSERT INTO jobs (kind, payload, user_key, priority, run_at, created_at)
  SELECT 'analyse-game', jsonb_build_object('game_id', g.id), g.account_id,
         (ARRAY[2, 4, 10, 20])[1 + (hashtext(g.platform_game_id) & 3)]::int2, g.at, g.at
  FROM (SELECT id, account_id, platform_game_id, now() - random() * interval '50 minutes' AS at
        FROM games WHERE analysis_started_at IS NULL) g;
INSERT INTO jobs (kind, payload, priority)
  SELECT 'analyse-reference-game', '{}', 200 FROM generate_series(1, 2000);
INSERT INTO jobs (kind, payload, user_key, priority, state)
  SELECT 'analyse-game', '{}', g.account_id, 4, 'done'
  FROM games g WHERE g.analysis_started_at IS NOT NULL;
"#;

async fn seed_scale(pool: &PgPool) {
    sqlx::raw_sql(SCALE_SEED_SQL).execute(pool).await.unwrap();
    for table in ["jobs", "games", "linked_accounts"] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("VACUUM ANALYZE {table}")))
            .execute(pool)
            .await
            .unwrap();
    }
    let queued: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE state = 'queued' AND kind = 'analyse-game'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(queued, 50_000);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn fair_dequeue_scales(pool: PgPool) {
    seed_scale(&pool).await;
    let q = Queue::new(pool.clone());
    let fair = FairRoundRobin::default();
    // Warm the connection and plan cache once, then time a claim.
    fair.dequeue(&q, &[KIND], "warm", 1, LEASE).await.unwrap();
    let t = Instant::now();
    let jobs = fair.dequeue(&q, &[KIND], "w", 16, LEASE).await.unwrap();
    let took = t.elapsed();
    assert_eq!(jobs.len(), 16);
    assert!(took < Duration::from_secs(1), "claim took {took:?}");
    // Every account has 20 served games (slice 4 for its first job), so the first tier with
    // work (priority 2/3, tier 1) is served by join time, one slice per account.
    assert!(jobs.iter().all(|j| j.priority / 2 == 1), "{jobs:?}");
    let users: HashSet<_> = jobs.iter().map(|j| j.user_key).collect();
    assert!(users.len() >= 4, "{users:?}");
}

/// Every plan node that reads `jobs`: (node, index, `Actual Rows × Actual Loops`).
fn jobs_reads(node: &Value, out: &mut Vec<(String, String, f64)>) {
    if node["Relation Name"] == "jobs" {
        // Rows returned plus rows read and thrown away by a filter, over every loop.
        let per_loop = [
            "Actual Rows",
            "Rows Removed by Filter",
            "Rows Removed by Index Recheck",
        ]
        .iter()
        .map(|k| node[*k].as_f64().unwrap_or(0.0))
        .sum::<f64>();
        let rows = per_loop * node["Actual Loops"].as_f64().unwrap_or(1.0);
        out.push((
            node["Node Type"].as_str().unwrap_or("").to_owned(),
            node["Index Name"].as_str().unwrap_or("").to_owned(),
            rows,
        ));
    }
    for child in node["Plans"].as_array().into_iter().flatten() {
        jobs_reads(child, out);
    }
}

/// The fair claim is index-backed in both the custom and the generic (cached) plan: users are
/// found by a skip scan of `jobs_user_ready_idx`, each user's next jobs by an index range on it,
/// `served` by a range on `games_started_account_idx`, join times by the accounts' primary key.
/// No Seq Scan anywhere, and the rows read from `jobs` are bounded by
/// `2 × users + kinds × (4 × limit)²`, not by the backlog.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn fair_claim_plan_is_index_backed(pool: PgPool) {
    seed_scale(&pool).await;
    let mut conn = pool.acquire().await.unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "PREPARE fair_claim(text[], int8, int8, int8, int8, text, int8) AS {}",
        fair_claim_sql()
    )))
    .execute(&mut *conn)
    .await
    .unwrap();
    let cases = [
        ("{analyse-game}", 1.0),
        ("{analyse-game,analyse-reference-game}", 2.0),
    ];
    for ((kinds, n_kinds), mode) in cases
        .iter()
        .flat_map(|c| ["force_custom_plan", "force_generic_plan"].map(|m| (*c, m)))
    {
        let execute =
            format!("EXECUTE fair_claim('{kinds}', 64, 3600000, 5, 16, 'w', 60000)");
        sqlx::query("SELECT set_config('plan_cache_mode', $1, false)")
            .bind(mode)
            .execute(&mut *conn)
            .await
            .unwrap();
        let plan: Vec<String> =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("EXPLAIN {execute}")))
                .fetch_all(&mut *conn)
                .await
                .unwrap();
        let plan = plan.join("\n");
        for index in [
            "jobs_user_ready_idx",
            "games_started_account_idx",
            "linked_accounts_pkey",
            "jobs_pkey",
        ] {
            assert!(plan.contains(index), "{kinds} {mode}: no {index}:\n{plan}");
        }
        assert!(!plan.contains("Seq Scan"), "{mode}:\n{plan}");

        sqlx::raw_sql("BEGIN").execute(&mut *conn).await.unwrap();
        let json: Value = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "EXPLAIN (ANALYZE, FORMAT JSON) {execute}"
        )))
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        sqlx::raw_sql("ROLLBACK").execute(&mut *conn).await.unwrap();
        let root = &json[0]["Plan"];
        let mut reads = Vec::new();
        jobs_reads(root, &mut reads);
        let read: f64 = reads.iter().map(|r| r.2).sum();
        // Heads: the skip scan reads one row per account plus a closing probe per kind, and one
        // probe per account and kind (and the reference pseudo-user) reads its head. Expand: the
        // best 4 × limit accounts read at most 4 × limit rows per kind. The lock, its key lookup
        // and the update touch at most `limit` rows each.
        let (users, limit): (f64, f64) = (500.0, 16.0);
        let bound = 2.0 * (users + n_kinds) + n_kinds * (4.0 * limit).powi(2) + 3.0 * limit;
        assert!(
            read <= bound,
            "{kinds} {mode}: read {read} rows of jobs (bound {bound}): {reads:?}"
        );
        assert!(read >= 16.0, "{mode}: {read}");
        let ms = json[0]["Execution Time"].as_f64().unwrap();
        eprintln!("{kinds} {mode}: {read} rows of jobs read (bound {bound}), {ms:.1} ms");
        assert!(ms < 1000.0, "{mode}: {ms} ms");
    }
    sqlx::raw_sql("DEALLOCATE fair_claim; RESET plan_cache_mode")
        .execute(&mut *conn)
        .await
        .unwrap();
}

// Fix round 1 ---------------------------------------------------------------------------------

/// Review counterexample: one account, 10 jobs with equal priority and `run_at` whose ids are
/// out of heap order. With `limit 1` the claim must return the smallest id (claim order is
/// `(priority, run_at, id)`), not whichever tuple the index range meets first.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn equal_priority_and_run_at_claims_the_smallest_id(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let a = account(&pool, "a", 1).await;
    // Inserted largest id first, so heap (and insertion) order is descending by id.
    let ids: Vec<i64> = sqlx::query_scalar(
        "INSERT INTO jobs (id, kind, payload, user_key, priority, run_at, created_at) \
         SELECT 1000 - i, $1, '{}', $2, 4, t, t \
         FROM generate_series(1, 10) i, (SELECT now() - interval '1 minute' AS t) c \
         ORDER BY i RETURNING id",
    )
    .bind(KIND)
    .bind(a)
    .fetch_all(&pool)
    .await
    .unwrap();
    let smallest = *ids.iter().min().unwrap();
    assert_eq!(smallest, 990);
    let fair = FairRoundRobin::default();
    let got = fair.dequeue(&q, &[KIND], "w", 1, LEASE).await.unwrap();
    assert_eq!(got.iter().map(|j| j.id).collect::<Vec<_>>(), vec![smallest]);
    // And then in id order, one at a time.
    let mut order = vec![got[0].id];
    for _ in 0..9 {
        let got = fair.dequeue(&q, &[KIND], "w", 1, LEASE).await.unwrap();
        order.push(got[0].id);
    }
    assert_eq!(order, (990..1000).collect::<Vec<i64>>());
}

#[test]
fn only_served_kinds_build_a_fair_strategy() {
    assert_eq!(ServedKind::ALL, &[ServedKind::AnalyseGame]);
    assert_eq!(ServedKind::AnalyseGame.as_str(), KIND);
    assert_eq!(FairRoundRobin::default().kinds(), &[ServedKind::AnalyseGame]);
    assert!(FairRoundRobin::for_kinds(&[KIND]).is_ok());
    for bad in [
        queue::KIND_PREVIEW,
        queue::KIND_REFERENCE_GAME,
        queue::KIND_BUILD_PROFILE,
        queue::KIND_IMPORT,
        "x",
    ] {
        assert_eq!(
            FairRoundRobin::for_kinds(&[KIND, bad]),
            Err(NotAServedKind(bad.to_owned()))
        );
    }
    let fair = FairRoundRobin::default();
    assert!(fair.check(&[KIND]).is_ok());
    assert_eq!(
        fair.check(&[KIND, queue::KIND_PREVIEW]),
        Err(NotAServedKind(queue::KIND_PREVIEW.to_owned()))
    );
}

/// Asked for a kind outside its allow-list, the strategy errors and claims nothing.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn fair_dequeue_refuses_kinds_that_do_not_record_served(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let a = account(&pool, "a", 1).await;
    for kind in [KIND, queue::KIND_BUILD_PROFILE] {
        q.enqueue(
            kind,
            &json!({}),
            EnqueueOptions {
                user_key: Some(a),
                ..EnqueueOptions::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    }
    let fair = FairRoundRobin::default();
    for kinds in [
        vec![queue::KIND_BUILD_PROFILE],
        vec![KIND, queue::KIND_BUILD_PROFILE],
    ] {
        let err = fair.dequeue(&q, &kinds, "w", 5, LEASE).await.unwrap_err();
        assert!(matches!(err, QueueError::InvalidOptions(_)), "{err:?}");
    }
    let queued: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE state = 'queued'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(queued, 2, "nothing was claimed");
}

/// Eight claimers racing (limit 1) over one heavy and three light users: a claim may come back
/// empty only when the other seven could hold every remaining job (at most 7 locks), never
/// while more due work remains. The first candidate window (4) can be fully held by the other
/// seven; the retry's window (40) cannot.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn contended_claimers_never_come_back_empty_while_work_remains(pool: PgPool) {
    const CLAIMERS: usize = 8;
    const LIMIT: i64 = 1;
    let q = Queue::new(pool.clone());
    let mut total = 0;
    for u in 0..4 {
        let acc = account(&pool, &format!("u{u}"), 10 - u).await;
        let g = games(&pool, acc, if u == 0 { 150 } else { 30 }, None).await;
        total += enqueue(&q, Some(acc), &g, p0()).await.len();
    }
    let spurious = Arc::new(Mutex::new(Vec::new()));
    let claimed = Arc::new(Mutex::new(Vec::new()));
    let mut tasks = Vec::new();
    for w in 0..CLAIMERS {
        let (q, spurious, claimed) = (q.clone(), spurious.clone(), claimed.clone());
        tasks.push(tokio::spawn(async move {
            let fair = FairRoundRobin::default();
            let owner = format!("w{w}");
            loop {
                let jobs = fair.dequeue(&q, &[KIND], &owner, LIMIT, LEASE).await.unwrap();
                if jobs.is_empty() {
                    // Counted after the claim, so it can only be lower than at claim time.
                    let queued: i64 = sqlx::query_scalar(
                        "SELECT count(*) FROM jobs WHERE state = 'queued'",
                    )
                    .fetch_one(q.pool())
                    .await
                    .unwrap();
                    if queued == 0 {
                        break;
                    }
                    if queued > (CLAIMERS as i64 - 1) * LIMIT {
                        spurious.lock().unwrap().push(queued);
                    }
                    tokio::task::yield_now().await;
                    continue;
                }
                for j in jobs {
                    start_game(q.pool(), &j).await;
                    assert!(q.complete(&j).await.unwrap());
                    claimed.lock().unwrap().push(j.id);
                }
            }
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    let claimed = claimed.lock().unwrap().clone();
    assert_eq!(claimed.len(), total);
    assert_eq!(claimed.iter().collect::<HashSet<_>>().len(), total);
    let spurious = spurious.lock().unwrap().clone();
    assert!(
        spurious.is_empty(),
        "empty claims with this many jobs still queued: {spurious:?}"
    );
}

/// Deterministic contention: another transaction holds the first candidate window (the best
/// 4 × limit jobs) locked. The first attempt claims nothing although it had candidates, so the
/// claim is re-run once with the wider window and takes the next job.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn short_claim_retries_past_rows_locked_by_others(pool: PgPool) {
    let q = Queue::new(pool.clone());
    let a = account(&pool, "a", 1).await;
    let g = games(&pool, a, 6, None).await;
    let ids = enqueue(&q, Some(a), &g, p0()).await;
    let mut other = pool.begin().await.unwrap();
    let held: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM jobs WHERE id = ANY($1) ORDER BY id LIMIT $2 FOR UPDATE",
    )
    .bind(&ids)
    .bind(first_candidates(1))
    .fetch_all(&mut *other)
    .await
    .unwrap();
    assert_eq!(held, ids[..4]);
    let got = FairRoundRobin::default()
        .dequeue(&q, &[KIND], "w", 1, LEASE)
        .await
        .unwrap();
    assert_eq!(got.iter().map(|j| j.id).collect::<Vec<_>>(), vec![ids[4]]);
    other.rollback().await.unwrap();
}

/// Scale guard at 2 000 accounts with work (release builds only: it is a wall-clock bound).
/// ADR-R2-15: the claim is linear in accounts with work, about 12 µs each; 2 000 accounts must
/// stay well under 3 × 2 000 × 12 µs ≈ 72 ms, and the rows read from `jobs` stay bounded.
#[cfg_attr(debug_assertions, ignore)]
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn fair_claim_at_2000_accounts_stays_linear(pool: PgPool) {
    sqlx::raw_sql(
        "INSERT INTO linked_accounts (platform, username, display_name, created_at) \
           SELECT 'lichess', 'p' || a, 'p' || a, now() - a * interval '1 minute' \
           FROM generate_series(1, 2000) a; \
         INSERT INTO jobs (kind, payload, user_key, priority, run_at, created_at) \
           SELECT 'analyse-game', '{}', a.id, 4, t.at, t.at \
           FROM linked_accounts a, generate_series(1, 10) g, \
                LATERAL (SELECT now() - (g + random()) * interval '1 minute' AS at) t",
    )
    .execute(&pool)
    .await
    .unwrap();
    for table in ["jobs", "linked_accounts"] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("VACUUM ANALYZE {table}")))
            .execute(&pool)
            .await
            .unwrap();
    }
    let q = Queue::new(pool.clone());
    let fair = FairRoundRobin::default();
    fair.dequeue(&q, &[KIND], "warm", 1, LEASE).await.unwrap();
    let mut best = Duration::MAX;
    for _ in 0..5 {
        let t = Instant::now();
        let jobs = fair.dequeue(&q, &[KIND], "w", 4, LEASE).await.unwrap();
        best = best.min(t.elapsed());
        assert_eq!(jobs.len(), 4);
    }
    eprintln!("fair claim at 2000 accounts: best of 5 = {best:?}");
    assert!(best < Duration::from_millis(72), "claim took {best:?}");
}
