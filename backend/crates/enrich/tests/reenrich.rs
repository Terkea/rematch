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
use enrich::enrich_game::enrich_game;
use enrich::persist::persist_move_features;
use enrich::reenrich::{MAX_DIFFICULTY_ATTEMPTS, SweepOptions, reenrich_pending_difficulty};
use sqlx::PgPool;
use testkit::fake_engine::FakeAnalyser;
use uuid::Uuid;

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
    assert_eq!(report.checked, 3, "stops after the first failed page");
    assert_eq!(status_counts(&pool).await, (7, 0, 5));
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
