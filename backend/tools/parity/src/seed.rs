//! The parity fixture set (Task 24 step 1): every recorded Lichess and chess.com fixture game,
//! plus PGN fixtures as Lichess-style rows, all pending since an hour ago.

use std::path::Path;

use analysis::jobs::analyse::enqueue_analyse;
use anyhow::Context;
use db::games::{NewGame, upsert_game};
use platforms::{NormalizedGame, chesscom::normalize_chesscom, lichess::normalize_lichess};
use queue::queue::Queue;
use serde_json::Value;
use sqlx::PgPool;
use time::OffsetDateTime;
use time::macros::datetime;
use uuid::Uuid;

/// The account that owns the Lichess fixture games and the PGN rows.
pub const LICHESS_USER: &str = "DrNykterstein";
/// The account that owns the chess.com fixture games.
pub const CHESSCOM_USER: &str = "Hikaru";

/// The `RUY` line of the worker tests.
pub const RUY: &str = "1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 5. O-O Be7 *";

/// Where a PGN row's text comes from.
pub enum PgnSource {
    /// A file under the fixtures directory.
    File(&'static str),
    /// Movetext without headers.
    Inline(&'static str),
}

/// The PGN rows: `platform_game_id` and source. Stored as Lichess blitz games (180+0).
pub const PGN_ROWS: &[(&str, PgnSource)] = &[
    (
        "pgn-scholars-mate",
        PgnSource::File("pgn/scholars-mate.pgn"),
    ),
    ("pgn-no-clocks", PgnSource::File("pgn/no-clocks.pgn")),
    ("pgn-chess960", PgnSource::File("pgn/chess960.pgn")),
    (
        "pgn-stalemate",
        PgnSource::File("pgn/terminal/stalemate.pgn"),
    ),
    (
        "pgn-fools-mate",
        PgnSource::File("pgn/terminal/fools-mate.pgn"),
    ),
    (
        "pgn-threefold",
        PgnSource::File("pgn/terminal/threefold.pgn"),
    ),
    ("pgn-ruy", PgnSource::Inline(RUY)),
];

async fn account(pool: &PgPool, platform: &str, username: &str) -> anyhow::Result<Uuid> {
    Ok(sqlx::query_scalar(
        "INSERT INTO linked_accounts (platform, username, display_name) \
         VALUES ($1::platform, $2, $3) \
         ON CONFLICT (platform, username) DO UPDATE SET display_name = EXCLUDED.display_name \
         RETURNING id",
    )
    .bind(platform)
    .bind(username.to_lowercase())
    .bind(username)
    .fetch_one(pool)
    .await?)
}

/// The importer's row for a normalised game (`import.rs` `new_game`); the colour from `username`.
fn new_game(account_id: Uuid, username: &str, g: &NormalizedGame) -> NewGame {
    let white = g.white.name.eq_ignore_ascii_case(username);
    NewGame {
        account_id,
        platform: g.platform.as_str().to_owned(),
        platform_game_id: g.platform_game_id.clone(),
        pgn: g.pgn.clone(),
        played_at: g.played_at,
        time_class: g.time_class.as_str().to_owned(),
        base_seconds: g.base_seconds,
        increment_seconds: g.increment_seconds,
        rated: g.rated,
        player_color: if white { "white" } else { "black" }.to_owned(),
        white_name: g.white.name.clone(),
        black_name: g.black.name.clone(),
        white_rating: g.white.rating,
        black_rating: g.black.rating,
        result: g.result.clone(),
        termination: g.termination.clone(),
        eco: g.eco.clone(),
        opening_name: g.opening_name.clone(),
        lichess_evals: g
            .lichess_evals
            .as_ref()
            .and_then(|e| serde_json::to_value(e).ok()),
    }
}

/// A PGN header value, e.g. `header(pgn, "White")`.
fn header(pgn: &str, name: &str) -> Option<String> {
    let prefix = format!("[{name} \"");
    pgn.lines()
        .find_map(|l| l.trim().strip_prefix(&prefix)?.strip_suffix("\"]"))
        .map(str::to_owned)
}

fn pgn_row(account_id: Uuid, id: &str, pgn: String, played_at: OffsetDateTime) -> NewGame {
    let white = header(&pgn, "White").unwrap_or_else(|| "A".to_owned());
    let black = header(&pgn, "Black").unwrap_or_else(|| "B".to_owned());
    let result = header(&pgn, "Result").unwrap_or_else(|| "*".to_owned());
    NewGame {
        account_id,
        platform: "lichess".to_owned(),
        platform_game_id: id.to_owned(),
        pgn,
        played_at,
        time_class: "blitz".to_owned(),
        base_seconds: Some(180),
        increment_seconds: Some(0),
        rated: false,
        player_color: "white".to_owned(),
        white_name: white,
        black_name: black,
        white_rating: None,
        black_rating: None,
        result,
        termination: None,
        eco: None,
        opening_name: None,
        lichess_evals: None,
    }
}

fn read(fixtures: &Path, rel: &str) -> anyhow::Result<String> {
    let p = fixtures.join(rel);
    std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))
}

/// Every fixture game, in fixture order: Lichess, chess.com, then the PGN rows.
fn fixture_rows(fixtures: &Path, lichess: Uuid, chesscom: Uuid) -> anyhow::Result<Vec<NewGame>> {
    let mut rows: Vec<NewGame> = Vec::new();
    for line in read(fixtures, "lichess-games.ndjson")?.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let raw: Value = serde_json::from_str(line)?;
        if let Some(g) = normalize_lichess(&raw) {
            rows.push(new_game(lichess, LICHESS_USER, &g));
        }
    }
    let month: Value = serde_json::from_str(&read(fixtures, "chesscom-month.json")?)?;
    for raw in month["games"].as_array().into_iter().flatten() {
        if let Some(g) = normalize_chesscom(raw) {
            rows.push(new_game(chesscom, CHESSCOM_USER, &g));
        }
    }
    let base = datetime!(2026-01-01 12:00 UTC);
    for (i, (id, src)) in PGN_ROWS.iter().enumerate() {
        let pgn = match src {
            PgnSource::File(rel) => read(fixtures, rel)?,
            PgnSource::Inline(movetext) => {
                format!("[White \"A\"]\n[Black \"B\"]\n[Result \"*\"]\n\n{movetext}\n")
            }
        };
        let at = base + time::Duration::minutes(i as i64);
        rows.push(pgn_row(lichess, id, pgn, at));
    }
    Ok(rows)
}

/// The analysis phases: each game with Lichess evals alone, in fixture order, then all others.
///
/// The eval cache is shared and keeps the first write per position, and a game that misses the
/// cache uses its own Lichess or engine eval. So when a Lichess-eval game and another game reach
/// a shared position at the same time, which eval each one records depends on scheduling. Run
/// phase by phase, every game reads the same cache state on both workers.
fn phases(rows: Vec<NewGame>) -> Vec<Vec<NewGame>> {
    let (with, rest): (Vec<_>, Vec<_>) = rows.into_iter().partition(|g| g.lichess_evals.is_some());
    let mut out: Vec<Vec<NewGame>> = with.into_iter().map(|g| vec![g]).collect();
    out.push(rest);
    out
}

/// The number of phases of [`seed_phase`].
pub fn phase_count(fixtures: &Path) -> anyhow::Result<usize> {
    Ok(phases(fixture_rows(fixtures, Uuid::nil(), Uuid::nil())?).len())
}

async fn insert(pool: &PgPool, rows: &[NewGame], enqueue: bool) -> anyhow::Result<usize> {
    let queue = Queue::new(pool.clone());
    for g in rows {
        let res = upsert_game(pool, g).await?;
        sqlx::query(
            "UPDATE games SET analysis_status = 'pending', \
             pending_since = now() - interval '1 hour' WHERE id = $1",
        )
        .bind(res.id)
        .execute(pool)
        .await?;
        if enqueue {
            enqueue_analyse(&queue, res.id, g.account_id).await?;
        }
    }
    Ok(rows.len())
}

/// Inserts the whole fixture set and returns the number of games. Every game is `pending` with
/// `pending_since` an hour ago. With `enqueue`, each also gets an analyse job.
pub async fn seed(pool: &PgPool, fixtures: &Path, enqueue: bool) -> anyhow::Result<usize> {
    let lichess = account(pool, "lichess", LICHESS_USER).await?;
    let chesscom = account(pool, "chesscom", CHESSCOM_USER).await?;
    insert(pool, &fixture_rows(fixtures, lichess, chesscom)?, enqueue).await
}

/// [`seed`] restricted to phase `phase` (0-based, below [`phase_count`]).
pub async fn seed_phase(
    pool: &PgPool,
    fixtures: &Path,
    enqueue: bool,
    phase: usize,
) -> anyhow::Result<usize> {
    let lichess = account(pool, "lichess", LICHESS_USER).await?;
    let chesscom = account(pool, "chesscom", CHESSCOM_USER).await?;
    let mut all = phases(fixture_rows(fixtures, lichess, chesscom)?);
    let n = all.len();
    anyhow::ensure!(phase < n, "phase {phase} out of range (0..{n})");
    insert(pool, &all.swap_remove(phase), enqueue).await
}

/// Enqueues an analyse job for every pending game; returns how many were new.
pub async fn enqueue_pending(pool: &PgPool) -> anyhow::Result<usize> {
    let pending: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT id, account_id FROM games WHERE analysis_status = 'pending' ORDER BY id",
    )
    .fetch_all(pool)
    .await?;
    let queue = Queue::new(pool.clone());
    let mut n = 0;
    for (game, account) in pending {
        if enqueue_analyse(&queue, game, account).await? {
            n += 1;
        }
    }
    Ok(n)
}
