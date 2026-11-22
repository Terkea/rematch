//! The Task 33 seed with real moves (R2 Task 34), shared by the coaching-input and the
//! weekly-input tests: Scholar's-mate moves and evals in every game, and the W1 exemplar ply with
//! enrichment in three games.
#![allow(dead_code)]

use chess_core::pgn::parse_pgn;
use serde_json::{Value, json};
use sqlx::PgPool;
use testkit::SCHOLARS;
use uuid::Uuid;

use super::seed::{PROTOCOL, SeedOptions, seed};

/// The W1 exemplar ply of the seed (White's `3. Bc4` of Scholar's mate).
pub const EXEMPLAR_PLY: u32 = 5;
/// ΔW of the exemplar move in the three exemplar games (the order the builder must keep).
pub const EXEMPLAR_DELTAS: [f64; 3] = [30.0, 20.0, 10.0];
/// Seed game indices that carry a W1 exemplar ply (one early, two late: both sides of an onset).
pub const EXEMPLAR_GAMES: [usize; 3] = [5, 30, 35];
/// The engine's preferred move at the exemplar ply.
pub const BEST_UCI: &str = "g1f3";

pub struct Fixture {
    pub account_id: Uuid,
    pub games: Vec<Uuid>,
}

/// White-POV centipawns of every seeded position.
pub const SEED_CP: i32 = 35;

pub async fn insert_moves(pool: &PgPool, game_id: Uuid, exemplar_delta: Option<f64>) {
    let parsed = parse_pgn(SCHOLARS).expect("scholar's mate parses");
    for p in &parsed.plies {
        let delta = match (p.ply, exemplar_delta) {
            (EXEMPLAR_PLY, Some(d)) => d,
            _ => 0.0,
        };
        sqlx::query(
            "INSERT INTO moves (game_id, ply, color, san, uci, fen_before, best_uci, win_before, \
               win_after, delta_w, accuracy, judgement, phase, clock_before, time_spent, decided) \
             VALUES ($1, $2, $3::color, $4, $5, $6, $7, $8, $9, $10, 100, NULL, 'opening', \
               NULL, NULL, false)",
        )
        .bind(game_id)
        .bind(i32::try_from(p.ply).unwrap())
        .bind(p.color.as_str())
        .bind(&p.san)
        .bind(&p.uci)
        .bind(&p.fen_before)
        .bind((p.ply == EXEMPLAR_PLY).then_some(BEST_UCI))
        .bind(55.0)
        .bind(55.0 - delta)
        .bind(delta)
        .execute(pool)
        .await
        .expect("insert moves");
    }
}

pub async fn insert_evals(pool: &PgPool) {
    let parsed = parse_pgn(SCHOLARS).unwrap();
    let fens = std::iter::once(parsed.start_fen.clone())
        .chain(parsed.plies.iter().map(|p| p.fen_after.clone()));
    for fen in fens {
        sqlx::query(
            "INSERT INTO position_evals (fen_key, protocol, score_cp, score_mate, best_uci, pv, \
               source) VALUES ($1, $2, $3, NULL, NULL, '{}', 'stockfish') ON CONFLICT DO NOTHING",
        )
        .bind(chess_core::fen::fen_key(&fen))
        .bind(PROTOCOL)
        .bind(SEED_CP)
        .execute(pool)
        .await
        .expect("insert eval");
    }
}

pub async fn insert_features(
    pool: &PgPool,
    game_id: Uuid,
    ply: u32,
    best_pv: &[&str],
    refutation_pv: &[&str],
) {
    sqlx::query(
        "INSERT INTO move_features (game_id, ply, best_pv, refutation_pv, best_line, \
           refutation_line, enrichment_protocol) VALUES ($1, $2, $3, $4, $5, $6, 'enr-v1') \
         ON CONFLICT (game_id, ply) DO UPDATE SET best_pv = EXCLUDED.best_pv, \
           refutation_pv = EXCLUDED.refutation_pv",
    )
    .bind(game_id)
    .bind(i32::try_from(ply).unwrap())
    .bind(best_pv.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>())
    .bind(
        refutation_pv
            .iter()
            .map(|s| (*s).to_owned())
            .collect::<Vec<_>>(),
    )
    .bind(json!({"gain": 1.0, "depth": 2, "mateIn": null, "tags": ["fork"]}))
    .bind(json!({"gain": 99.0, "depth": 2, "mateIn": 1, "tags": ["mateIn1", "mate", "fork"]}))
    .execute(pool)
    .await
    .expect("insert move_features");
}

/// Sets a metrics field of a game (jsonb path, camelCase as stored).
pub async fn set_metrics_path(pool: &PgPool, game_id: Uuid, path: &[&str], value: Value) {
    let path: Vec<String> = path.iter().map(|s| (*s).to_owned()).collect();
    sqlx::query("UPDATE game_metrics SET metrics = jsonb_set(metrics, $2, $3) WHERE game_id = $1")
        .bind(game_id)
        .bind(&path)
        .bind(value)
        .execute(pool)
        .await
        .expect("set metrics path");
}

/// The Task 33 seed plus real moves: every game gets Scholar's-mate moves and evals; three games
/// carry the W1 exemplar ply with enrichment.
pub async fn seed_with_moves(pool: &PgPool, opts: SeedOptions) -> Fixture {
    let s = seed(pool, opts).await;
    insert_evals(pool).await;
    for (i, g) in s.games.iter().enumerate() {
        let delta = EXEMPLAR_GAMES
            .iter()
            .position(|x| *x == i)
            .map(|j| EXEMPLAR_DELTAS[j]);
        insert_moves(pool, *g, delta).await;
        if delta.is_some() {
            insert_features(
                pool,
                *g,
                EXEMPLAR_PLY,
                &[BEST_UCI, "g8f6"],
                // The third move is illegal after the mate: the builder keeps the legal prefix.
                &["g8f6", "h5f7", "e8e7"],
            )
            .await;
            set_metrics_path(
                pool,
                *g,
                &["weakness", "W1", "exemplarPlies"],
                json!([EXEMPLAR_PLY]),
            )
            .await;
        }
    }
    Fixture {
        account_id: s.account_id,
        games: s.games,
    }
}
