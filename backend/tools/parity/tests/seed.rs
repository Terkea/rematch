//! `parity seed`, `parity enqueue-pending` and the snapshot loader against a real database.

use std::path::PathBuf;

use parity::compare::{diff, load};
use parity::seed::{PGN_ROWS, enqueue_pending, phase_count, seed, seed_phase};
use sqlx::PgPool;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// Lichess: 3 of 4 (the chess960 one is skipped by the importer). chess.com: 5 of 6 (chess960).
const LICHESS_FIXTURE_GAMES: i64 = 3;
const CHESSCOM_FIXTURE_GAMES: i64 = 5;

async fn count(pool: &PgPool, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn seed_inserts_every_fixture_game_pending_for_an_hour(pool: PgPool) {
    let n = seed(&pool, &fixtures(), false).await.unwrap();
    let expected = LICHESS_FIXTURE_GAMES + CHESSCOM_FIXTURE_GAMES + PGN_ROWS.len() as i64;
    assert_eq!(n as i64, expected);
    assert_eq!(count(&pool, "SELECT count(*) FROM games").await, expected);
    assert_eq!(
        count(&pool, "SELECT count(*) FROM linked_accounts").await,
        2
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM games WHERE analysis_status = 'pending' \
             AND pending_since BETWEEN now() - interval '61 minutes' AND now() - interval '59 minutes'"
        )
        .await,
        expected
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM games WHERE platform = 'chesscom'"
        )
        .await,
        CHESSCOM_FIXTURE_GAMES
    );
    // Every Lichess fixture game carries its evals; the PGN rows carry none.
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM games WHERE lichess_evals IS NOT NULL"
        )
        .await,
        LICHESS_FIXTURE_GAMES
    );
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM games WHERE platform_game_id LIKE 'pgn-%' \
             AND platform = 'lichess' AND base_seconds = 180 AND increment_seconds = 0 \
             AND time_class = 'blitz' AND lichess_evals IS NULL"
        )
        .await,
        PGN_ROWS.len() as i64
    );
    assert_eq!(count(&pool, "SELECT count(*) FROM jobs").await, 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn seed_includes_the_loyd_stalemate_and_the_ruy_line(pool: PgPool) {
    seed(&pool, &fixtures(), false).await.unwrap();
    let pgns: Vec<String> = sqlx::query_scalar(
        "SELECT pgn FROM games WHERE platform_game_id IN ('pgn-stalemate', 'pgn-ruy') \
         ORDER BY platform_game_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(pgns.len(), 2);
    assert!(pgns[0].contains("5. O-O Be7"));
    assert!(pgns[1].contains("10. Qe6"));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn seed_with_enqueue_creates_one_analyse_job_per_game(pool: PgPool) {
    let n = seed(&pool, &fixtures(), true).await.unwrap();
    assert!(n > 0);
    assert_eq!(
        count(
            &pool,
            "SELECT count(*) FROM jobs j JOIN games g \
             ON j.dedupe_key = 'analyse:' || g.id AND j.payload->>'gameId' = g.id::text \
             AND j.user_key = g.account_id WHERE j.kind = 'analyse-game' AND j.max_attempts = 3"
        )
        .await,
        n as i64
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn enqueue_pending_enqueues_only_pending_games_once(pool: PgPool) {
    let n = seed(&pool, &fixtures(), false).await.unwrap();
    sqlx::query("UPDATE games SET analysis_status = 'done' WHERE platform_game_id = 'pgn-ruy'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(enqueue_pending(&pool).await.unwrap(), n - 1);
    // A live job already holds each game's dedupe key.
    assert_eq!(enqueue_pending(&pool).await.unwrap(), 0);
    assert_eq!(
        count(&pool, "SELECT count(*) FROM jobs").await,
        (n - 1) as i64
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn load_reads_games_moves_and_evals_keyed_for_compare(pool: PgPool) {
    seed(&pool, &fixtures(), false).await.unwrap();
    let id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM games WHERE platform_game_id = 'pgn-ruy'")
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO moves (game_id, ply, color, san, uci, fen_before, best_uci, win_before, \
         win_after, delta_w, accuracy, judgement, phase, clock_before, time_spent, decided) \
         VALUES ($1, 1, 'white', 'e4', 'e2e4', 'f0', 'e2e4', 50, 51, 1, 99.5, NULL, 'opening', \
         180, 0, false), ($1, 2, 'black', 'e5', 'e7e5', 'f1', NULL, 51, 50, -1, 98, 'blunder', \
         'opening', NULL, NULL, true)",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO position_evals (fen_key, protocol, score_cp, score_mate, best_uci, pv, source) \
         VALUES ('k', 'p', 12, NULL, 'e2e4', '{e2e4,e7e5}', 'stockfish')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE games SET white_accuracy = 81.5 WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

    let s = load(&pool).await.unwrap();
    let r = diff(&s, &s);
    assert!(r.mismatches.is_empty());
    assert_eq!((r.games, r.moves, r.evals), (s.games.len(), 2, 1));
    let (g, moves) = &s.games[&("lichess".to_owned(), "pgn-ruy".to_owned())];
    assert_eq!(g.analysis_status, "pending");
    assert_eq!(g.white_accuracy, Some(81.5));
    assert_eq!(moves.len(), 2);
    assert_eq!(moves[1].judgement.as_deref(), Some("blunder"));
    assert_eq!(moves[0].clock_before, Some(180.0));
    let e = &s.evals[&("k".to_owned(), "p".to_owned())];
    assert_eq!(e.pv, vec!["e2e4".to_owned(), "e7e5".to_owned()]);
    assert_eq!(e.source, "stockfish");
}

/// The Lichess-eval games first, one per phase (the shared eval cache keeps the first write for a
/// position, so their order decides which eval other games read), then every other game.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn phases_put_each_lichess_eval_game_alone_before_the_rest(pool: PgPool) {
    let phases = phase_count(&fixtures()).unwrap();
    assert_eq!(phases, LICHESS_FIXTURE_GAMES as usize + 1);
    for k in 0..phases - 1 {
        assert_eq!(seed_phase(&pool, &fixtures(), false, k).await.unwrap(), 1);
        assert_eq!(
            count(
                &pool,
                "SELECT count(*) FROM games WHERE lichess_evals IS NULL"
            )
            .await,
            0
        );
    }
    let rest = seed_phase(&pool, &fixtures(), false, phases - 1)
        .await
        .unwrap();
    assert_eq!(rest as i64, CHESSCOM_FIXTURE_GAMES + PGN_ROWS.len() as i64);
    assert_eq!(
        count(&pool, "SELECT count(*) FROM games").await,
        LICHESS_FIXTURE_GAMES + CHESSCOM_FIXTURE_GAMES + PGN_ROWS.len() as i64
    );
    assert!(seed_phase(&pool, &fixtures(), false, phases).await.is_err());
}
