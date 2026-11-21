//! Row seeding for DB tests (mirrors apps/worker/test/jobs/helpers.ts).

use std::time::Duration;

use sqlx::PgPool;
use uuid::Uuid;

/// Scholar's mate, the default game PGN.
pub const SCHOLARS: &str = "1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 4. Qxf7# 1-0";

/// Inserts a linked account. The username is stored lower-cased, display_name as given.
pub async fn insert_account(pool: &PgPool, platform: &str, username: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO linked_accounts (platform, username, display_name) \
         VALUES ($1::platform, $2, $3) RETURNING id",
    )
    .bind(platform)
    .bind(username.to_lowercase())
    .bind(username)
    .fetch_one(pool)
    .await
    .expect("insert linked_accounts")
}

#[derive(Clone, Debug)]
pub struct GameSeed {
    pub platform_game_id: String,
    pub pgn: String,
    /// "white" | "black"
    pub player_color: &'static str,
    pub white: String,
    pub black: String,
    pub base_seconds: Option<i32>,
    pub increment_seconds: Option<i32>,
    /// An analysis_status value.
    pub status: &'static str,
    /// Sets pending_since to now() minus this; None keeps the column default (now()).
    pub pending_since_ago: Option<Duration>,
    pub lichess_evals: Option<serde_json::Value>,
}

impl Default for GameSeed {
    fn default() -> Self {
        Self {
            platform_game_id: Uuid::new_v4().simple().to_string()[..8].to_owned(),
            pgn: SCHOLARS.to_owned(),
            player_color: "white",
            white: "Hero".to_owned(),
            black: "Villain".to_owned(),
            base_seconds: Some(180),
            increment_seconds: Some(0),
            status: "pending",
            pending_since_ago: None,
            lichess_evals: None,
        }
    }
}

/// Time class from the base and increment (est = base + 40 * increment); no clock is daily.
fn time_class(base: Option<i32>, increment: Option<i32>) -> &'static str {
    let Some(base) = base else {
        return "daily";
    };
    let est = i64::from(base) + 40 * i64::from(increment.unwrap_or(0));
    if est < 180 {
        "bullet"
    } else if est < 480 {
        "blitz"
    } else if est < 1500 {
        "rapid"
    } else {
        "classical"
    }
}

/// Inserts a game for `account_id` (platform taken from the account), played at
/// 2026-01-01T00:00:00Z, rated, result "1-0", ratings 1500. Returns the game id.
pub async fn insert_game(pool: &PgPool, account_id: Uuid, seed: GameSeed) -> Uuid {
    let pending_since_ago_secs = seed.pending_since_ago.map(|d| d.as_secs_f64());
    sqlx::query_scalar(
        "INSERT INTO games (account_id, platform, platform_game_id, pgn, played_at, time_class, \
           base_seconds, increment_seconds, rated, player_color, white_name, black_name, \
           white_rating, black_rating, result, lichess_evals, analysis_status, pending_since) \
         SELECT a.id, a.platform, $2, $3, '2026-01-01T00:00:00Z'::timestamptz, $4::time_class, \
           $5, $6, true, $7::color, $8, $9, 1500, 1500, '1-0'::result, $10, \
           $11::analysis_status, \
           CASE WHEN $12::float8 IS NULL THEN now() \
                ELSE now() - make_interval(secs => $12::float8) END \
         FROM linked_accounts a WHERE a.id = $1 \
         RETURNING id",
    )
    .bind(account_id)
    .bind(&seed.platform_game_id)
    .bind(&seed.pgn)
    .bind(time_class(seed.base_seconds, seed.increment_seconds))
    .bind(seed.base_seconds)
    .bind(seed.increment_seconds)
    .bind(seed.player_color)
    .bind(&seed.white)
    .bind(&seed.black)
    .bind(&seed.lichess_evals)
    .bind(seed.status)
    .bind(pending_since_ago_secs)
    .fetch_one(pool)
    .await
    .expect("insert games (does the account exist?)")
}
