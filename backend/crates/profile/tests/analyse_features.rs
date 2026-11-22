//! The analyse-game hook (R2 Task 31, TS M2 Task 24): features, metrics, openings and weakness
//! series written at analysis time, the requeue of unenriched games, the pending-Maia sweep's
//! recompute, and the loaders.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use analysis::eval_cache::DbEvalCache;
use analysis::hook::AnalyseHook;
use analysis::jobs::analyse::{
    AnalyseGameDeps, AnalyseOutcome, handle_analyse_game, request_reanalysis,
};
use chess_core::pgn::parse_pgn;
use chess_core::{Color, EngineScore};
use engine::{AnalyseOptions, Analyser, AnalysisLine, AnalysisResult, EngineError};
use enrich::difficulty::{DifficultyOutcome, DifficultySource, PendingReason};
use enrich::enrich_game::EnrichDeps;
use enrich::reenrich::SweepOptions;
use enrich::store::MemoryEnrichmentStore;
use enrich::types::DifficultyInput;
use futures::future::BoxFuture;
use profile::analyse_features::opening_name;
use profile::hook_impl::{BUILD_PROFILE_DELAY, ENRICH_GAME_KIND, ProfileHook};
use profile::load::{load_analysed_games, load_exemplar_moves, load_series_points};
use profile::requeue::{
    DifficultySweepDeps, REQUEUE_PAGE_SQL, requeue_unenriched, run_difficulty_sweep,
};
use profile_core::openings::lines::classify_line;
use profile_core::types::{
    Difficulty, PLAYER_GAME_METRICS_VERSION, PlayerGameMetrics, SeriesKey, WeaknessKey,
};
use queue::queue::Queue;
use queue::{AnalyseGamePayload, Job, KIND_ANALYSE, KIND_BUILD_PROFILE, profile_key};
use serde_json::{Value, json};
use sqlx::PgPool;
use testkit::{GameSeed, insert_account, insert_game};
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const TEST_TIMEOUT: Duration = Duration::from_secs(60);

async fn within<T>(what: &str, fut: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(TEST_TIMEOUT, fut)
        .await
        .unwrap_or_else(|_| panic!("timed out after {TEST_TIMEOUT:?} waiting for {what}"))
}

// ---------------------------------------------------------------------------------------------
// Fakes

/// The position after 1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 (White mates in 1 with Qxf7#).
const MATE_IN_ONE: &str = "r1bqkb1r/pppp1ppp/2n2n2/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR w";

/// Scores every position cp 30 for the side to move with the game's next move as its PV,
/// except the Scholar's mate position (mate in 1, PV `h5f7`). MultiPV calls fail while
/// `fail_multipv` is set.
struct ScriptedAnalyser {
    fail_multipv: AtomicBool,
    multipv_calls: AtomicUsize,
    /// Position (first four FEN fields) to the move the test games play there.
    book: HashMap<String, String>,
}

fn position_key(fen: &str) -> String {
    fen.split(' ').take(4).collect::<Vec<_>>().join(" ")
}

impl Default for ScriptedAnalyser {
    fn default() -> Self {
        let mut book = HashMap::new();
        for pgn in [SCHOLARS, RUY_LOPEZ] {
            for p in parse_pgn(pgn).unwrap().plies {
                book.insert(position_key(&p.fen_before), p.uci);
            }
        }
        ScriptedAnalyser {
            fail_multipv: AtomicBool::new(false),
            multipv_calls: AtomicUsize::new(0),
            book,
        }
    }
}

impl Analyser for ScriptedAnalyser {
    fn analyse<'a>(
        &'a self,
        fen: &'a str,
        opts: AnalyseOptions,
    ) -> BoxFuture<'a, Result<AnalysisResult, EngineError>> {
        Box::pin(async move {
            if opts.multipv > 1 {
                self.multipv_calls.fetch_add(1, Ordering::SeqCst);
                if self.fail_multipv.load(Ordering::SeqCst) {
                    return Err(EngineError::Crashed);
                }
            }
            let (score, pv) = if fen.starts_with(MATE_IN_ONE) {
                (EngineScore::Mate(1), vec!["h5f7".to_owned()])
            } else {
                let pv = self.book.get(&position_key(fen)).cloned();
                (EngineScore::Cp(30), pv.into_iter().collect())
            };
            Ok(AnalysisResult {
                lines: vec![AnalysisLine {
                    multipv: 1,
                    score,
                    pv,
                    depth: 10,
                    nodes: 1,
                }],
                wall: Duration::from_millis(1),
                cpu: Some(Duration::from_millis(1)),
            })
        })
    }
}

const MAIA_MODEL: &str = "maia2-rapid-0.11.0";

/// Maia that answers every input with one fixed difficulty while `up`.
struct FakeMaia {
    up: AtomicBool,
}

impl DifficultySource for FakeMaia {
    fn estimate<'a>(
        &'a self,
        inputs: &'a [DifficultyInput],
    ) -> BoxFuture<'a, Vec<DifficultyOutcome>> {
        Box::pin(async move {
            inputs
                .iter()
                .map(|_| {
                    if self.up.load(Ordering::SeqCst) {
                        DifficultyOutcome::Ok {
                            difficulty: Difficulty {
                                p_best: 0.25,
                                p_ok: 0.75,
                                model: MAIA_MODEL.to_owned(),
                            },
                        }
                    } else {
                        DifficultyOutcome::PendingMaia {
                            reason: PendingReason::MaiaUnavailable,
                        }
                    }
                })
                .collect()
        })
    }

    fn healthy(&self) -> BoxFuture<'_, bool> {
        Box::pin(async move { self.up.load(Ordering::SeqCst) })
    }
}

// ---------------------------------------------------------------------------------------------
// Setup

struct World {
    queue: Queue,
    analyser: Arc<ScriptedAnalyser>,
    deps: AnalyseGameDeps,
}

fn world(pool: &PgPool) -> World {
    let analyser = Arc::new(ScriptedAnalyser::default());
    let queue = Queue::new(pool.clone());
    let hook = ProfileHook {
        pool: pool.clone(),
        enrich: EnrichDeps {
            analyser: analyser.clone(),
            store: Arc::new(MemoryEnrichmentStore::default()),
            tb: None,
            // Maia is not configured: difficulty stays pending, never estimated.
            difficulty: None,
            cancel: CancellationToken::new(),
        },
        queue: queue.clone(),
    };
    let mut deps = AnalyseGameDeps::new(
        pool.clone(),
        analyser.clone(),
        Arc::new(DbEvalCache::new(pool.clone())),
    );
    deps.hook = Some(Arc::new(hook) as Arc<dyn AnalyseHook>);
    World {
        queue,
        analyser,
        deps,
    }
}

const SCHOLARS: &str = "1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 4. Qxf7# 1-0";
/// 16 plies; Black's plies 12, 14 and 16 are past the opening-ply cut (≤ 10).
const RUY_LOPEZ: &str =
    "1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 5. O-O Be7 6. Re1 b5 7. Bb3 d6 8. c3 O-O 1/2-1/2";

async fn account(pool: &PgPool, name: &str) -> Uuid {
    insert_account(pool, "chesscom", name).await
}

/// A pending blitz game in which `account` (named `name`) plays black.
async fn black_game(pool: &PgPool, account: Uuid, name: &str, pgn: &str) -> Uuid {
    insert_game(
        pool,
        account,
        GameSeed {
            pgn: pgn.to_owned(),
            player_color: "black",
            white: "Villain".to_owned(),
            black: name.to_owned(),
            ..GameSeed::default()
        },
    )
    .await
}

fn job(game_id: Uuid) -> Job {
    let now = OffsetDateTime::now_utc();
    Job {
        id: 1,
        kind: KIND_ANALYSE.to_owned(),
        payload: json!({ "gameId": game_id }),
        dedupe_key: None,
        user_key: None,
        priority: 100,
        attempts: 1,
        max_attempts: 3,
        lease_token: Uuid::new_v4(),
        run_at: now,
        created_at: now,
        started_at: now,
    }
}

async fn analyse(w: &World, id: Uuid) -> AnalyseOutcome {
    within(
        "handle_analyse_game",
        handle_analyse_game(
            &w.deps,
            &job(id),
            AnalyseGamePayload { game_id: id },
            CancellationToken::new(),
        ),
    )
    .await
    .expect("analysis succeeds")
}

async fn count(pool: &PgPool, sql: &'static str, id: Uuid) -> i64 {
    sqlx::query_scalar(sql)
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn metrics_of(pool: &PgPool, id: Uuid) -> PlayerGameMetrics {
    let v: Value = sqlx::query_scalar("SELECT metrics FROM game_metrics WHERE game_id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
    serde_json::from_value(v).unwrap()
}

async fn series_row(pool: &PgPool, id: Uuid, key: &str) -> Option<(i32, i32)> {
    sqlx::query_as(
        "SELECT events, opportunities FROM weakness_series_point \
         WHERE game_id = $1 AND series_key = $2",
    )
    .bind(id)
    .bind(key)
    .fetch_optional(pool)
    .await
    .unwrap()
}

async fn row_counts(pool: &PgPool, id: Uuid) -> [i64; 4] {
    [
        count(
            pool,
            "SELECT count(*) FROM move_features WHERE game_id = $1",
            id,
        )
        .await,
        count(
            pool,
            "SELECT count(*) FROM game_metrics WHERE game_id = $1",
            id,
        )
        .await,
        count(
            pool,
            "SELECT count(*) FROM weakness_series_point WHERE game_id = $1",
            id,
        )
        .await,
        count(
            pool,
            "SELECT count(*) FROM game_openings WHERE game_id = $1",
            id,
        )
        .await,
    ]
}

async fn enrichment_protocol(pool: &PgPool, id: Uuid) -> Option<String> {
    sqlx::query_scalar("SELECT enrichment_protocol FROM games WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// `(count, max run_at - now() in seconds)` of the live build-profile jobs for the key.
async fn live_profile_jobs(pool: &PgPool, account: Uuid, tc: &str) -> (i64, Option<f64>) {
    sqlx::query_as(
        "SELECT count(*), max(extract(epoch FROM run_at - now()))::float8 FROM jobs \
         WHERE kind = $1 AND dedupe_key = $2 AND state IN ('queued', 'running', 'paused')",
    )
    .bind(KIND_BUILD_PROFILE)
    .bind(profile_key(account, tc))
    .fetch_one(pool)
    .await
    .unwrap()
}

// ---------------------------------------------------------------------------------------------
// The hook (TS M2 Task 24 cases first)

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn scholars_mate_for_black_writes_one_metrics_row(pool: PgPool) {
    let w = world(&pool);
    let acct = account(&pool, "blackhero").await;
    let id = black_game(&pool, acct, "blackhero", SCHOLARS).await;
    assert!(matches!(analyse(&w, id).await, AnalyseOutcome::Done { .. }));

    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM game_metrics WHERE game_id = $1",
            id
        )
        .await,
        1
    );
    let m = metrics_of(&pool, id).await;
    // TS M2 asserted version 1; R2 T9 bumped the version to 2.
    assert_eq!(m.version, PLAYER_GAME_METRICS_VERSION);
    assert_eq!(m.blunders, 1, "3...Nf6 allows mate in one");
    assert_eq!(m.score, Some(0.0), "Black lost");
    let (tc, enr, version): (String, Option<String>, i32) = sqlx::query_as(
        "SELECT time_class::text, enrichment_protocol, metrics_version FROM game_metrics \
         WHERE game_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(tc, "blitz");
    assert_eq!(enr.as_deref(), Some("enr-v1"));
    assert_eq!(version, PLAYER_GAME_METRICS_VERSION as i32);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn w1_series_opportunities_equal_blacks_eligible_moves(pool: PgPool) {
    let w = world(&pool);
    let acct = account(&pool, "blackhero").await;
    let id = black_game(&pool, acct, "blackhero", SCHOLARS).await;
    analyse(&w, id).await;
    let eligible = count(
        &pool,
        "SELECT count(*) FROM moves WHERE game_id = $1 AND color = 'black' \
         AND abs(win_before - 50) < 40",
        id,
    )
    .await;
    assert_eq!(eligible, 3);
    let (events, opportunities) = series_row(&pool, id, "W1").await.expect("a W1 row");
    assert_eq!(i64::from(opportunities), eligible);
    assert_eq!(events, 0, "a mating refutation is not hung material");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn rerun_replaces_the_derived_rows(pool: PgPool) {
    let w = world(&pool);
    let acct = account(&pool, "blackhero").await;
    let id = black_game(&pool, acct, "blackhero", SCHOLARS).await;
    analyse(&w, id).await;
    let first = row_counts(&pool, id).await;
    assert!(first.iter().all(|&n| n > 0), "{first:?}");
    assert!(
        within("reanalysis", request_reanalysis(&pool, &w.queue, id))
            .await
            .unwrap()
    );
    analyse(&w, id).await;
    assert_eq!(row_counts(&pool, id).await, first);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn full_pass_deletes_preview_row(pool: PgPool) {
    let w = world(&pool);
    let acct = account(&pool, "blackhero").await;
    let id = black_game(&pool, acct, "blackhero", SCHOLARS).await;
    sqlx::query(
        "INSERT INTO game_previews (game_id, protocol, moves) \
         VALUES ($1, 'sf19-n30k-preview-v1', '[]'::jsonb)",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    analyse(&w, id).await;
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM game_previews WHERE game_id = $1",
            id
        )
        .await,
        0
    );
    assert_eq!(
        enrichment_protocol(&pool, id).await.as_deref(),
        Some("enr-v1")
    );
    let analysed_at: Option<OffsetDateTime> =
        sqlx::query_scalar("SELECT analysed_at FROM games WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(analysed_at.is_some());
    let payloads: Vec<Value> = sqlx::query_scalar(
        "SELECT payload FROM user_events WHERE type = 'game.analysed' ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(payloads.len(), 1);
    assert!(payloads[0].get("preview").is_none(), "{}", payloads[0]);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn classified_opening_is_stored_in_game_openings(pool: PgPool) {
    let w = world(&pool);
    let acct = account(&pool, "blackhero").await;
    let id = black_game(&pool, acct, "blackhero", SCHOLARS).await;
    sqlx::query("UPDATE games SET eco = 'C20', opening_name = 'Platform Name' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    analyse(&w, id).await;
    let parsed = parse_pgn(SCHOLARS).unwrap();
    let expected = classify_line(&parsed.plies, Color::Black).expect("Scholar's is named");
    let (key, name, eco, kind, color): (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
    ) = sqlx::query_as(
        "SELECT line_key, line_name, eco, line_kind::text, color::text FROM game_openings \
         WHERE game_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(key.as_deref(), Some(expected.key.as_str()));
    assert_eq!(name.as_deref(), Some(expected.name.as_str()));
    assert_ne!(
        name.as_deref(),
        Some("Platform Name"),
        "classified beats platform"
    );
    assert_eq!(eco.as_deref(), Some(expected.eco.as_str()));
    assert_eq!(kind.as_deref(), Some(expected.kind.as_str()));
    assert_eq!(color, "black");
    // The opening series is keyed on the classified line.
    let key = format!("opening:{}", expected.key);
    assert!(series_row(&pool, id, &key).await.is_some(), "no {key} row");
}

#[test]
fn opening_name_falls_back_to_the_platform_when_unclassified() {
    let parsed = parse_pgn(SCHOLARS).unwrap();
    let line = classify_line(&parsed.plies, Color::Black);
    let (eco, name) = opening_name(line.as_ref(), Some("C20"), Some("Platform Name"));
    let line = line.unwrap();
    assert_eq!(
        (eco.as_deref(), name.as_deref()),
        (Some(line.eco.as_str()), Some(line.name.as_str()))
    );
    let (eco, name) = opening_name(None, Some("C20"), Some("Platform Name"));
    assert_eq!(
        (eco.as_deref(), name.as_deref()),
        (Some("C20"), Some("Platform Name"))
    );
    assert_eq!(opening_name(None, None, None), (None, None));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn build_profile_enqueued_once_per_account_and_tc(pool: PgPool) {
    let w = world(&pool);
    let acct = account(&pool, "blackhero").await;
    let a = black_game(&pool, acct, "blackhero", SCHOLARS).await;
    let b = black_game(&pool, acct, "blackhero", SCHOLARS).await;
    analyse(&w, a).await;
    analyse(&w, b).await;
    let (n, run_in) = live_profile_jobs(&pool, acct, "blitz").await;
    assert_eq!(n, 1, "two games, one live build-profile job");
    let run_in = run_in.unwrap();
    let delay = BUILD_PROFILE_DELAY.as_secs_f64();
    assert!(
        run_in > delay - 10.0 && run_in <= delay + 1.0,
        "run_at in {run_in} s"
    );
    let payload: Value = sqlx::query_scalar("SELECT payload FROM jobs WHERE kind = $1")
        .bind(KIND_BUILD_PROFILE)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(payload, json!({ "accountId": acct, "timeClass": "blitz" }));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn enrich_game_job_metrics_row_has_the_stages(pool: PgPool) {
    let w = world(&pool);
    let acct = account(&pool, "blackhero").await;
    let id = black_game(&pool, acct, "blackhero", SCHOLARS).await;
    analyse(&w, id).await;
    let (stages, total): (Value, f64) =
        sqlx::query_as("SELECT stages, total_ms FROM job_metrics WHERE kind = $1 AND game_id = $2")
            .bind(ENRICH_GAME_KIND)
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(total >= 0.0);
    for key in [
        "multipv_ms",
        "threat_ms",
        "tb_ms",
        "difficulty_ms",
        "features_ms",
        "skipped_moves_without_evals",
    ] {
        assert!(
            stages.get(key).is_some_and(Value::is_number),
            "{key} in {stages}"
        );
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn difficulty_is_never_estimated_without_maia(pool: PgPool) {
    let w = world(&pool);
    let acct = account(&pool, "blackhero").await;
    let id = black_game(&pool, acct, "blackhero", RUY_LOPEZ).await;
    analyse(&w, id).await;
    let (with_difficulty, pending): (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE p_best IS NOT NULL OR p_ok IS NOT NULL), \
         count(*) FILTER (WHERE difficulty_status = 'pending_maia') \
         FROM move_features WHERE game_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(with_difficulty, 0);
    assert_eq!(pending, 3, "Black's plies 12, 14 and 16 wait for Maia");
    let model: Option<String> =
        sqlx::query_scalar("SELECT difficulty_model FROM game_metrics WHERE game_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(model, None);
}

// ---------------------------------------------------------------------------------------------
// Failure and requeue

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn enrichment_failure_keeps_the_analysis_and_requeue_redoes_it(pool: PgPool) {
    let w = world(&pool);
    let acct = account(&pool, "blackhero").await;
    let id = black_game(&pool, acct, "blackhero", SCHOLARS).await;
    w.analyser.fail_multipv.store(true, Ordering::SeqCst);
    assert!(matches!(analyse(&w, id).await, AnalyseOutcome::Done { .. }));
    assert!(w.analyser.multipv_calls.load(Ordering::SeqCst) > 0);
    assert_eq!(
        count(&pool, "SELECT count(*) FROM moves WHERE game_id = $1", id).await,
        7
    );
    assert_eq!(row_counts(&pool, id).await, [0, 0, 0, 0]);
    assert_eq!(enrichment_protocol(&pool, id).await, None);
    assert_eq!(live_profile_jobs(&pool, acct, "blitz").await.0, 0);

    assert_eq!(
        within("requeue", requeue_unenriched(&pool, &w.queue))
            .await
            .unwrap(),
        1
    );
    let (status, tier): (String, Option<i16>) =
        sqlx::query_as("SELECT analysis_status::text, analysis_tier FROM games WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((status.as_str(), tier), ("pending", Some(2)));
    let jobs = count(
        &pool,
        "SELECT count(*) FROM jobs WHERE kind = 'analyse-game' AND payload->>'gameId' = $1::text",
        id,
    )
    .await;
    assert_eq!(jobs, 1);

    w.analyser.fail_multipv.store(false, Ordering::SeqCst);
    assert!(matches!(analyse(&w, id).await, AnalyseOutcome::Done { .. }));
    assert_eq!(
        enrichment_protocol(&pool, id).await.as_deref(),
        Some("enr-v1")
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM game_metrics WHERE game_id = $1",
            id
        )
        .await,
        1
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn requeue_unenriched_picks_null_and_leaves_enr_v1_alone(pool: PgPool) {
    let queue = Queue::new(pool.clone());
    let acct = account(&pool, "blackhero").await;
    let seed = |status: &'static str| GameSeed {
        status,
        player_color: "black",
        black: "blackhero".to_owned(),
        ..GameSeed::default()
    };
    let unenriched = insert_game(&pool, acct, seed("done")).await;
    let enriched = insert_game(&pool, acct, seed("done")).await;
    let other = insert_game(&pool, acct, seed("skipped")).await;
    sqlx::query("UPDATE games SET enrichment_protocol = 'enr-v1' WHERE id = $1")
        .bind(enriched)
        .execute(&pool)
        .await
        .unwrap();
    let old = insert_game(&pool, acct, seed("done")).await;
    sqlx::query("UPDATE games SET enrichment_protocol = 'enr-v0' WHERE id = $1")
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(requeue_unenriched(&pool, &queue).await.unwrap(), 2);
    let status = |id: Uuid| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, String>("SELECT analysis_status::text FROM games WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    assert_eq!(status(unenriched).await, "pending");
    assert_eq!(status(old).await, "pending");
    assert_eq!(status(enriched).await, "done");
    assert_eq!(status(other).await, "skipped");
    // A second call finds nothing (the requeued games are pending now).
    assert_eq!(requeue_unenriched(&pool, &queue).await.unwrap(), 0);
}

/// The keyset page is an ordered range scan of the primary key that stops after `LIMIT` rows:
/// no Seq Scan and no Sort, in the custom and the generic plan, from the first and a later page.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn requeue_page_is_an_index_ordered_range_scan(pool: PgPool) {
    sqlx::query(
        "WITH a AS (INSERT INTO linked_accounts (platform, username, display_name) \
           VALUES ('lichess', 'plan', 'plan') RETURNING id) \
         INSERT INTO games (account_id, platform, platform_game_id, pgn, played_at, \
           time_class, rated, player_color, white_name, black_name, result, \
           analysis_status, enrichment_protocol) \
         SELECT a.id, 'lichess', 'p-' || i, '1. e4 *', now(), 'blitz', true, 'white', 'w', \
           'b', '*', CASE WHEN i % 10 < 8 THEN 'done' ELSE 'pending' END::analysis_status, \
           CASE WHEN i % 10 < 7 THEN 'enr-v1' END \
         FROM a, generate_series(1, 100000) i",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("ANALYZE games").execute(&pool).await.unwrap();
    let mut conn = pool.acquire().await.unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "PREPARE requeue_page(uuid, int8) AS {REQUEUE_PAGE_SQL}"
    )))
    .execute(&mut *conn)
    .await
    .unwrap();
    for mode in ["force_custom_plan", "force_generic_plan"] {
        sqlx::query("SELECT set_config('plan_cache_mode', $1, false)")
            .bind(mode)
            .execute(&mut *conn)
            .await
            .unwrap();
        for after in ["NULL", "'7fffffff-ffff-ffff-ffff-ffffffffffff'"] {
            let plan: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "EXPLAIN EXECUTE requeue_page({after}, 500)"
            )))
            .fetch_all(&mut *conn)
            .await
            .unwrap();
            let plan = plan.join("\n");
            let at = format!("{mode} after={after}:\n{plan}");
            assert!(plan.contains("games_pkey"), "{at}");
            assert!(!plan.contains("Seq Scan"), "{at}");
            assert!(!plan.contains("Sort"), "{at}");
        }
    }
    sqlx::raw_sql("DEALLOCATE requeue_page; RESET plan_cache_mode")
        .execute(&mut *conn)
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------------------------
// The pending-Maia sweep

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn sweep_recomputes_metrics_for_the_games_it_changes(pool: PgPool) {
    let w = world(&pool);
    let acct = account(&pool, "blackhero").await;
    let id = black_game(&pool, acct, "blackhero", RUY_LOPEZ).await;
    let untouched = black_game(&pool, acct, "blackhero", SCHOLARS).await;
    analyse(&w, id).await;
    analyse(&w, untouched).await;
    let series_before = row_counts(&pool, id).await;
    let created = |gid: Uuid| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, OffsetDateTime>(
                "SELECT created_at FROM game_metrics WHERE game_id = $1",
            )
            .bind(gid)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    let untouched_created = created(untouched).await;
    // The build-profile job from the analysis ran already.
    sqlx::query("DELETE FROM jobs WHERE kind = $1")
        .bind(KIND_BUILD_PROFILE)
        .execute(&pool)
        .await
        .unwrap();

    let maia = Arc::new(FakeMaia {
        up: AtomicBool::new(true),
    });
    let deps = DifficultySweepDeps {
        pool: pool.clone(),
        queue: w.queue.clone(),
        analyser: w.analyser.clone(),
        cache: Arc::new(DbEvalCache::new(pool.clone())),
        source: maia.clone(),
        options: SweepOptions {
            page_size: 100,
            max_pages: 10,
        },
    };
    let report = within(
        "sweep",
        run_difficulty_sweep(&deps, &CancellationToken::new()),
    )
    .await
    .unwrap();
    assert_eq!(report.sweep.resolved, 3);
    assert_eq!(report.recomputed, vec![id]);
    let model: Option<String> =
        sqlx::query_scalar("SELECT difficulty_model FROM game_metrics WHERE game_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(model.as_deref(), Some(MAIA_MODEL));
    assert_eq!(
        row_counts(&pool, id).await,
        series_before,
        "replaced, not duplicated"
    );
    let ok: i64 = count(
        &pool,
        "SELECT count(*) FROM move_features WHERE game_id = $1 AND difficulty_status = 'ok'",
        id,
    )
    .await;
    assert_eq!(ok, 3, "the recompute keeps the sweep's difficulty");
    assert_eq!(
        created(untouched).await,
        untouched_created,
        "unchanged game left alone"
    );
    assert_eq!(live_profile_jobs(&pool, acct, "blitz").await.0, 1);

    // Maia down: nothing changes, nothing is recomputed.
    maia.up.store(false, Ordering::SeqCst);
    let report = run_difficulty_sweep(&deps, &CancellationToken::new())
        .await
        .unwrap();
    assert!(!report.sweep.maia_healthy);
    assert!(report.recomputed.is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn sweep_skips_a_game_that_is_being_reanalysed(pool: PgPool) {
    let w = world(&pool);
    let acct = account(&pool, "blackhero").await;
    let id = black_game(&pool, acct, "blackhero", RUY_LOPEZ).await;
    analyse(&w, id).await;
    assert!(request_reanalysis(&pool, &w.queue, id).await.unwrap());
    let before: Value = sqlx::query_scalar("SELECT metrics FROM game_metrics WHERE game_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let deps = DifficultySweepDeps {
        pool: pool.clone(),
        queue: w.queue.clone(),
        analyser: w.analyser.clone(),
        cache: Arc::new(DbEvalCache::new(pool.clone())),
        source: Arc::new(FakeMaia {
            up: AtomicBool::new(true),
        }),
        options: SweepOptions {
            page_size: 100,
            max_pages: 10,
        },
    };
    let report = run_difficulty_sweep(&deps, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(report.sweep.games, vec![id]);
    assert!(
        report.recomputed.is_empty(),
        "the re-analysis recomputes it"
    );
    let model: Option<String> =
        sqlx::query_scalar("SELECT difficulty_model FROM game_metrics WHERE game_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(model, None);
    let after: Value = sqlx::query_scalar("SELECT metrics FROM game_metrics WHERE game_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, before);
}

// ---------------------------------------------------------------------------------------------
// Loaders

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn load_analysed_games_returns_player_pov_scores(pool: PgPool) {
    let w = world(&pool);
    let acct = account(&pool, "blackhero").await;
    let lost = black_game(&pool, acct, "blackhero", SCHOLARS).await;
    let stale = black_game(&pool, acct, "blackhero", SCHOLARS).await;
    analyse(&w, lost).await;
    analyse(&w, stale).await;
    // A row from an older metrics version is stale: never read as evidence.
    sqlx::query(
        "UPDATE game_metrics SET metrics = jsonb_set(metrics, '{version}', '1') \
         WHERE game_id = $1",
    )
    .bind(stale)
    .execute(&pool)
    .await
    .unwrap();
    let games = load_analysed_games(&pool, acct, chess_core::TimeClass::Blitz)
        .await
        .unwrap();
    assert_eq!(games.len(), 1);
    let (ctx, m) = &games[0];
    assert_eq!(ctx.game_id, lost);
    assert_eq!(ctx.player_color, Color::Black);
    assert_eq!(ctx.result, "1-0");
    assert_eq!(ctx.score, Some(0.0), "1-0 is a loss for Black");
    assert_eq!(m.score, Some(0.0));
    assert_eq!(m.blunders, 1);
    assert_eq!(ctx.player_rating, Some(1500));
    assert!(
        load_analysed_games(&pool, acct, chess_core::TimeClass::Rapid)
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn load_series_points_groups_by_key(pool: PgPool) {
    let w = world(&pool);
    let acct = account(&pool, "blackhero").await;
    let a = black_game(&pool, acct, "blackhero", SCHOLARS).await;
    let b = black_game(&pool, acct, "blackhero", SCHOLARS).await;
    analyse(&w, a).await;
    analyse(&w, b).await;
    let series = load_series_points(&pool, acct, chess_core::TimeClass::Blitz)
        .await
        .unwrap();
    let w1 = series
        .get(&SeriesKey::Weakness(WeaknessKey::W1))
        .expect("W1 points");
    assert_eq!(w1.len(), 2);
    let mut ids: Vec<Uuid> = w1.iter().map(|p| p.game_id).collect();
    ids.sort();
    let mut want = vec![a, b];
    want.sort();
    assert_eq!(ids, want);
    assert!(
        w1.iter()
            .all(|p| p.opportunities == 3 && p.protocol == "sf19-n300k-v1")
    );
    let stored: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT series_key) FROM weakness_series_point WHERE account_id = $1",
    )
    .bind(acct)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(series.len() as i64, stored);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn load_exemplar_moves_returns_the_move_its_features_and_game(pool: PgPool) {
    let w = world(&pool);
    let acct = account(&pool, "blackhero").await;
    let id = black_game(&pool, acct, "blackhero", SCHOLARS).await;
    analyse(&w, id).await;
    let rows = load_exemplar_moves(&pool, &[(id, 6), (id, 99), (Uuid::new_v4(), 6)])
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let r: &profile::load::ExemplarRow = rows.get(&(id, 6)).expect("ply 6");
    assert_eq!(r.san, "Nf6");
    assert_eq!(r.color, Color::Black);
    assert_eq!(r.judgement.as_deref(), Some("blunder"));
    assert_eq!(r.player_color, Color::Black);
    assert!(!r.platform_game_id.is_empty());
    assert_eq!(r.refutation_pv, vec!["h5f7".to_owned()]);
    let refutation = r.refutation.as_ref().expect("a refutation line");
    assert_eq!(refutation.swing.mate_in, Some(1));
    let _: HashMap<(Uuid, u32), profile::load::ExemplarRow> = rows;
}
