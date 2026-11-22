//! R2 Task 25: re-enriching `pending_maia` difficulty once Maia is healthy (owner override).
//!
//! - Maia down: the sweep checks `/healthz` only, changes nothing and counts no attempt.
//! - Maia back: pending positions get Maia's estimate; opening plies (<= 10) are never sent.
//! - Deterministic invalid answers are retried at most `MAX_DIFFICULTY_ATTEMPTS` times in all,
//!   then the position is final: `not_applicable/model_error`.

mod common;

use std::sync::Arc;

use chess_core::Color;
use common::*;
use enrich::difficulty::select::DifficultyEstimator;
use enrich::difficulty::{DifficultyOutcome, DifficultySource};
use enrich::enrich_game::enrich_game;
use enrich::persist::persist_move_features;
use enrich::reenrich::{
    MAX_CALL_FAILURES, MAX_DIFFICULTY_ATTEMPTS, SweepOptions, count_parked,
    reenrich_pending_difficulty, unpark_call_failures,
};
use enrich::types::DifficultyInput;
use futures::future::BoxFuture;
use serde_json::json;
use sqlx::PgPool;
use testkit::fake_engine::FakeAnalyser;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PV: &[&str] = &["a2a3", "a7a6"];

/// A White game whose eligible plies > 10 (11, 13, ..., 23: seven) are pending, Maia down.
async fn seed_pending(pool: &PgPool) -> (Uuid, Game) {
    let game_id = seed_game(pool).await;
    let g = Game::new(game(RUY), PV, &[(13, 6.0), (15, 12.0)], &[Color::White]);
    insert_moves(pool, game_id, &g.records).await;
    let down = FakeMaia::new(false, MaiaMode::Ok);
    let a = Arc::new(FakeAnalyser::new());
    let out = enrich_game(&deps(a, memory_store(), None, Some(down)), &g.input())
        .await
        .unwrap();
    let mut conn = pool.acquire().await.unwrap();
    persist_move_features(
        &mut conn,
        game_id,
        &g.records,
        &out.raw,
        None,
        &out.difficulty,
    )
    .await
    .unwrap();
    (game_id, g)
}

async fn status_counts(pool: &PgPool) -> (i64, i64, i64) {
    (
        count(
            pool,
            "SELECT count(*) FROM move_features WHERE difficulty_status = 'pending_maia'",
        )
        .await,
        count(
            pool,
            "SELECT count(*) FROM move_features WHERE difficulty_status = 'ok'",
        )
        .await,
        count(
            pool,
            "SELECT count(*) FROM move_features WHERE difficulty_status = 'not_applicable'",
        )
        .await,
    )
}

fn opts() -> SweepOptions {
    SweepOptions {
        page_size: 100,
        max_pages: 10,
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn maia_down_leaves_pending_and_counts_no_attempt(pool: PgPool) {
    seed_pending(&pool).await;
    assert_eq!(status_counts(&pool).await, (7, 0, 5));
    let maia = FakeMaia::new(false, MaiaMode::Ok);
    let report = reenrich_pending_difficulty(&pool, maia.as_ref(), opts())
        .await
        .unwrap();
    assert!(!report.maia_healthy);
    assert_eq!(report.checked, 0);
    assert_eq!(
        *maia.health_checks.lock().unwrap(),
        1,
        "health is checked via /healthz"
    );
    assert_eq!(status_counts(&pool).await, (7, 0, 5));
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM move_features WHERE difficulty_attempts > 0"
        )
        .await,
        0
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn maia_back_resolves_pending_positions(pool: PgPool) {
    let (game_id, g) = seed_pending(&pool).await;
    let maia = FakeMaia::new(true, MaiaMode::Ok);
    let report = reenrich_pending_difficulty(&pool, maia.as_ref(), opts())
        .await
        .unwrap();
    assert!(report.maia_healthy);
    assert_eq!((report.checked, report.resolved), (7, 7));
    assert_eq!(report.games, vec![game_id]);
    assert_eq!(status_counts(&pool).await, (0, 7, 5));
    let (p_best, p_ok, model, reason): (Option<f32>, Option<f32>, Option<String>, Option<String>) =
        sqlx::query_as(
            "SELECT p_best, p_ok, difficulty_model, difficulty_reason FROM move_features WHERE ply = 13",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        (p_best, p_ok, model.as_deref(), reason),
        (Some(0.25), Some(0.75), Some(MODEL), None)
    );

    // The inputs are rebuilt from the stored rows: the same FEN, Elos, best move and lines.
    let sent = maia.sent();
    let fens: Vec<String> = sent.iter().map(|i| i.fen.clone()).collect();
    let expected: Vec<String> = [11, 13, 15, 17, 19, 21, 23]
        .iter()
        .map(|&p| g.parsed.plies[p - 1].fen_before.clone())
        .collect();
    assert_eq!(fens, expected);
    let i13 = &sent[1];
    assert_eq!((i13.elo_self, i13.elo_oppo), (Some(1500), Some(1500)));
    assert_eq!(i13.best_uci.as_deref(), Some("a2a3"));
    let lines = i13.lines.as_ref().unwrap();
    assert!(
        lines.iter().any(|(u, _)| u == "e2e4"),
        "stored multipv line: {lines:?}"
    );
    assert!(
        lines.iter().any(|(u, _)| *u == g.records[12].uci),
        "played move"
    );

    // Nothing is pending any more: a second sweep sends nothing.
    maia.clear();
    let again = reenrich_pending_difficulty(&pool, maia.as_ref(), opts())
        .await
        .unwrap();
    assert_eq!(again.checked, 0);
    assert!(maia.sent().is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn opening_plies_are_never_re_enriched(pool: PgPool) {
    seed_pending(&pool).await;
    // Even a pending row at ply <= 10 (written by an older build) is never sent.
    sqlx::query(
        "UPDATE move_features SET difficulty_status = 'pending_maia', \
           difficulty_reason = 'maia_unavailable' WHERE ply <= 10",
    )
    .execute(&pool)
    .await
    .unwrap();
    let maia = FakeMaia::new(true, MaiaMode::Ok);
    let report = reenrich_pending_difficulty(&pool, maia.as_ref(), opts())
        .await
        .unwrap();
    assert_eq!(report.checked, 7);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM move_features WHERE ply <= 10 AND p_best IS NOT NULL"
        )
        .await,
        0
    );
    assert_eq!(maia.sent().len(), 7);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn retry_cap_marks_model_error(pool: PgPool) {
    seed_pending(&pool).await;
    assert_eq!(MAX_DIFFICULTY_ATTEMPTS, 3);
    let maia = FakeMaia::new(true, MaiaMode::Invalid);
    for attempt in 1..MAX_DIFFICULTY_ATTEMPTS {
        let r = reenrich_pending_difficulty(&pool, maia.as_ref(), opts())
            .await
            .unwrap();
        assert_eq!(
            (r.checked, r.still_pending, r.capped),
            (7, 7, 0),
            "attempt {attempt}"
        );
        assert_eq!(
            count(
                &pool,
                &format!(
                    "SELECT count(*) FROM move_features WHERE difficulty_status = 'pending_maia' \
                     AND difficulty_reason = 'invalid_response' AND difficulty_attempts = {attempt}"
                )
            )
            .await,
            7
        );
    }
    let last = reenrich_pending_difficulty(&pool, maia.as_ref(), opts())
        .await
        .unwrap();
    assert_eq!((last.checked, last.capped), (7, 7));
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM move_features WHERE difficulty_status = 'not_applicable' \
             AND difficulty_reason = 'model_error' AND p_best IS NULL"
        )
        .await,
        7
    );
    // Final: never sent again, even when Maia answers properly now.
    maia.clear();
    maia.set_mode(MaiaMode::Ok);
    let after = reenrich_pending_difficulty(&pool, maia.as_ref(), opts())
        .await
        .unwrap();
    assert_eq!(after.checked, 0);
    assert!(maia.sent().is_empty());
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn maia_failing_mid_sweep_counts_no_attempt_and_stops(pool: PgPool) {
    seed_pending(&pool).await;
    let maia = FakeMaia::new(true, MaiaMode::FailsCalls);
    let report = reenrich_pending_difficulty(
        &pool,
        maia.as_ref(),
        SweepOptions {
            page_size: 3,
            max_pages: 10,
        },
    )
    .await
    .unwrap();
    assert!(report.stopped_early);
    assert_eq!(
        report.checked, 0,
        "no row was answered; stops after the first failed page"
    );
    assert_eq!(status_counts(&pool).await, (7, 0, 5));
    // Maia is still healthy, so the failed page's rows are charged one call failure (never an
    // attempt) and move to the back of the next sweep.
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM move_features WHERE difficulty_call_failures = 1"
        )
        .await,
        3
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM move_features WHERE difficulty_attempts > 0"
        )
        .await,
        0
    );
    // Back up: everything resolves.
    maia.set_mode(MaiaMode::Ok);
    let r = reenrich_pending_difficulty(&pool, maia.as_ref(), opts())
        .await
        .unwrap();
    assert_eq!(r.resolved, 7);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn sweep_is_paged_and_bounded(pool: PgPool) {
    seed_pending(&pool).await;
    let maia = FakeMaia::new(true, MaiaMode::Ok);
    let bounded = SweepOptions {
        page_size: 2,
        max_pages: 2,
    };
    let r = reenrich_pending_difficulty(&pool, maia.as_ref(), bounded)
        .await
        .unwrap();
    assert_eq!((r.checked, r.resolved), (4, 4));
    assert_eq!(status_counts(&pool).await, (3, 4, 5));
    let r = reenrich_pending_difficulty(&pool, maia.as_ref(), bounded)
        .await
        .unwrap();
    assert_eq!(r.checked, 3);
    assert_eq!(status_counts(&pool).await, (0, 7, 5));
}

/// Reviewer I1: a call-level invalid answer (wrong item count) from a healthy Maia. One request
/// is sent for three inputs; none of them may gain an attempt, and the sweep stops.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn call_level_invalid_answer_counts_no_attempt_and_stops(pool: PgPool) {
    seed_pending(&pool).await;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/healthz"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "model": MODEL })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/predict"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
        .mount(&server)
        .await;
    // Batch size 1: the first request fails, the other two inputs are never sent.
    let est = DifficultyEstimator::new(Some(&server.uri()), std::time::Duration::from_secs(5), 1);
    let report = reenrich_pending_difficulty(
        &pool,
        &est,
        SweepOptions {
            page_size: 3,
            max_pages: 10,
        },
    )
    .await
    .unwrap();
    let predicts = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/v1/predict")
        .count();
    assert_eq!(predicts, 1);
    assert!(report.stopped_early);
    assert_eq!(
        (report.checked, report.capped, report.still_pending),
        (0, 0, 0)
    );
    assert_eq!(status_counts(&pool).await, (7, 0, 5));
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM move_features WHERE difficulty_attempts > 0"
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM move_features WHERE difficulty_status = 'pending_maia' \
             AND difficulty_reason = 'maia_unavailable'"
        )
        .await,
        7,
        "reasons unchanged"
    );
}

/// Maia goes down during the sweep (its `/healthz` fails on the re-probe): nothing is charged.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn maia_going_down_mid_sweep_charges_nothing(pool: PgPool) {
    seed_pending(&pool).await;
    let maia = FakeMaia::new(true, MaiaMode::GoesDown);
    let report = reenrich_pending_difficulty(&pool, maia.as_ref(), opts())
        .await
        .unwrap();
    assert!(report.stopped_early);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM move_features \
             WHERE difficulty_attempts > 0 OR difficulty_call_failures > 0"
        )
        .await,
        0
    );
}

/// Reviewer M2: a batch that always fails while Maia is healthy does not block the rows after
/// it, and its rows are parked (still `pending_maia`, never final) at `MAX_CALL_FAILURES`.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn always_failing_rows_move_back_and_park_at_the_cap(pool: PgPool) {
    let (_, g) = seed_pending(&pool).await;
    let maia = FakeMaia::new(true, MaiaMode::Ok);
    *maia.fail_fen.lock().unwrap() = Some(g.parsed.plies[10].fen_before.clone()); // ply 11
    let small = SweepOptions {
        page_size: 2,
        max_pages: 10,
    };
    let first = reenrich_pending_difficulty(&pool, maia.as_ref(), small)
        .await
        .unwrap();
    assert!(first.stopped_early);
    assert_eq!(first.resolved, 0, "ply 11 heads the first page");
    let second = reenrich_pending_difficulty(&pool, maia.as_ref(), small)
        .await
        .unwrap();
    // 15..21 resolve; the last page pairs 23 with 11 and fails.
    assert_eq!(second.resolved, 4, "the other rows go first and resolve");
    for _ in 2..MAX_CALL_FAILURES {
        reenrich_pending_difficulty(&pool, maia.as_ref(), small)
            .await
            .unwrap();
    }
    // Rows that only shared a page with ply 11 resolve later; ply 11 alone is parked at the
    // cap, still pending.
    assert_eq!(
        count(
            &pool,
            &format!(
                "SELECT count(*) FROM move_features WHERE difficulty_status = 'pending_maia' \
                 AND difficulty_call_failures = {MAX_CALL_FAILURES} AND difficulty_attempts = 0"
            )
        )
        .await,
        1
    );
    maia.clear();
    let parked = reenrich_pending_difficulty(&pool, maia.as_ref(), small)
        .await
        .unwrap();
    assert_eq!(parked.checked, 0);
    assert!(maia.sent().is_empty(), "parked rows are not sent again");
    assert_eq!(status_counts(&pool).await, (1, 6, 5));
}

/// Answers Maia-ok for every input, but first changes ply 13's row as a concurrent re-analysis
/// would (replacing it with a final status).
struct RacingMaia {
    pool: PgPool,
}

impl DifficultySource for RacingMaia {
    fn estimate<'a>(
        &'a self,
        inputs: &'a [DifficultyInput],
    ) -> BoxFuture<'a, Vec<DifficultyOutcome>> {
        Box::pin(async move {
            sqlx::query(
                "UPDATE move_features SET difficulty_status = 'not_applicable', \
                   difficulty_reason = 'no_legal_moves' WHERE ply = 13",
            )
            .execute(&self.pool)
            .await
            .unwrap();
            inputs
                .iter()
                .map(|_| DifficultyOutcome::Ok {
                    difficulty: FakeMaia::difficulty(),
                })
                .collect()
        })
    }

    fn healthy(&self) -> BoxFuture<'_, bool> {
        Box::pin(async { true })
    }
}

/// Reviewer M1: sweep updates apply only to rows still pending as read.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn sweep_never_overwrites_a_row_changed_meanwhile(pool: PgPool) {
    seed_pending(&pool).await;
    let report = reenrich_pending_difficulty(&pool, &RacingMaia { pool: pool.clone() }, opts())
        .await
        .unwrap();
    assert_eq!(report.resolved, 6);
    let (status, p_best): (Option<String>, Option<f32>) =
        sqlx::query_as("SELECT difficulty_status, p_best FROM move_features WHERE ply = 13")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((status.as_deref(), p_best), (Some("not_applicable"), None));
}

async fn set_call_failures(pool: &PgPool, ply: i32, n: i16) {
    sqlx::query("UPDATE move_features SET difficulty_call_failures = $2 WHERE ply = $1")
        .bind(ply)
        .bind(n)
        .execute(pool)
        .await
        .unwrap();
}

async fn call_failures(pool: &PgPool, ply: i32) -> i16 {
    sqlx::query_scalar("SELECT difficulty_call_failures FROM move_features WHERE ply = $1")
        .bind(ply)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Fix round 2 (N1): parked rows are reported, and unparking makes a sweep resolve them.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn parked_rows_are_reported_and_unpark_lets_a_sweep_resolve_them(pool: PgPool) {
    seed_pending(&pool).await;
    set_call_failures(&pool, 11, MAX_CALL_FAILURES).await;
    assert_eq!(count_parked(&pool).await.unwrap(), 1);
    let maia = FakeMaia::new(true, MaiaMode::Ok);
    let r = reenrich_pending_difficulty(&pool, maia.as_ref(), opts())
        .await
        .unwrap();
    assert_eq!(
        (r.resolved, r.parked),
        (6, 1),
        "the parked row is skipped but reported"
    );
    // Reported even when Maia is down.
    let down = FakeMaia::new(false, MaiaMode::Ok);
    let r = reenrich_pending_difficulty(&pool, down.as_ref(), opts())
        .await
        .unwrap();
    assert_eq!(r.parked, 1);

    assert_eq!(unpark_call_failures(&pool).await.unwrap(), 1);
    assert_eq!(count_parked(&pool).await.unwrap(), 0);
    let r = reenrich_pending_difficulty(&pool, maia.as_ref(), opts())
        .await
        .unwrap();
    assert_eq!((r.resolved, r.parked), (1, 0));
    assert_eq!(status_counts(&pool).await, (0, 7, 5));
}

/// Fix round 2 (N2): unparking resets only parked rows; rows below the cap keep their count
/// (and so their place at the back of the sweep).
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn unpark_resets_only_parked_rows(pool: PgPool) {
    seed_pending(&pool).await;
    set_call_failures(&pool, 11, MAX_CALL_FAILURES).await;
    set_call_failures(&pool, 13, MAX_CALL_FAILURES - 1).await;
    set_call_failures(&pool, 15, MAX_CALL_FAILURES + 1).await;
    assert_eq!(count_parked(&pool).await.unwrap(), 2);
    assert_eq!(unpark_call_failures(&pool).await.unwrap(), 2);
    assert_eq!(call_failures(&pool, 11).await, 0);
    assert_eq!(call_failures(&pool, 13).await, MAX_CALL_FAILURES - 1);
    assert_eq!(call_failures(&pool, 15).await, 0);
    // Only pending rows are touched.
    sqlx::query(
        "UPDATE move_features SET difficulty_status = 'not_applicable', \
           difficulty_reason = 'model_error', difficulty_call_failures = $1 WHERE ply = 17",
    )
    .bind(MAX_CALL_FAILURES)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(count_parked(&pool).await.unwrap(), 0);
    assert_eq!(unpark_call_failures(&pool).await.unwrap(), 0);
    assert_eq!(call_failures(&pool, 17).await, MAX_CALL_FAILURES);
}
