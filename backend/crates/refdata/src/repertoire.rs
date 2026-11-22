//! Repertoire rows (R2 Task 18, TS M2 Task 13 `rebuildRepertoire`).
//!
//! [`rebuild_repertoire`] works on one account × time class:
//! 1. [`fill_game_openings`] classifies every game of any status (`imported` included) that has
//!    no `game_openings` row yet, from its PGN (first 30 plies). A PGN that does not parse gets
//!    a row with empty arrays and no line, so it is not parsed again and adds no evidence.
//! 2. [`player_band`] resolves the band (R-4, ruling Q2) and [`load_explorer_with_fallback`]
//!    loads the explorer of that band, or of the nearest band with data within
//!    [`EXPLORER_MAX_FALLBACK_BANDS`] (recorded in every line's `comparison`).
//! 3. Analysed data comes from `game_metrics` only when its version is
//!    [`PLAYER_GAME_METRICS_VERSION`]; a stale row is no evidence, never zero evidence.
//! 4. The whole rebuild runs in one transaction that first takes an advisory lock per
//!    account × time class, so the reads and writes of two rebuilds never interleave. It
//!    refreshes the book exits in `game_openings` and replaces `repertoire_nodes` and
//!    `opening_lines`.

use std::collections::HashMap;

use chess_core::pgn::parse_pgn;
use chess_core::{Color, TimeClass};
use profile_core::openings::lines::{
    BandComparison, BandSource, EXPLORER_MAX_FALLBACK_BANDS, LineSummary, classify_line,
    resolve_band, summarise_lines,
};
use profile_core::openings::repertoire::{
    AnalysedOpening, ExplorerIndex, MAX_REPERTOIRE_PLIES, RepertoireGame, RepertoireLine,
    RepertoirePly, book_exit, build_repertoire_tree, opening_plies,
};
use profile_core::types::{Band, PLAYER_GAME_METRICS_VERSION, PlatformId, PlayerGameMetrics};
use profile_core::weakness::sessions::{
    PreGameInput, expected_score_from_ratings, pre_game_ratings,
};
use sqlx::{FromRow, PgConnection, PgPool};
use uuid::Uuid;

use crate::builds::{RefdataError, Result};
use crate::explorer::load_explorer_index;

/// Rows per `repertoire_nodes` insert statement.
pub const NODE_CHUNK: usize = 5_000;
/// Games whose opponent ratings estimate the band when the player has no rating (R-4).
pub const OPPONENT_BAND_GAMES: i64 = 20;

#[derive(FromRow)]
struct MissingGame {
    id: Uuid,
    pgn: String,
    player_color: String,
}

fn color_of(s: &str) -> Result<Color> {
    match s {
        "white" => Ok(Color::White),
        "black" => Ok(Color::Black),
        _ => Err(RefdataError::Decode(format!("color {s:?}"))),
    }
}

/// Classifies the account × time class games without a `game_openings` row; returns how many
/// rows it wrote.
pub async fn fill_game_openings(pool: &PgPool, account_id: Uuid, tc: TimeClass) -> Result<u64> {
    let mut tx = pool.begin().await?;
    let written = fill_game_openings_in(&mut tx, account_id, tc).await?;
    tx.commit().await?;
    Ok(written)
}

/// [`fill_game_openings`] on the caller's connection or transaction.
pub async fn fill_game_openings_in(
    conn: &mut PgConnection,
    account_id: Uuid,
    tc: TimeClass,
) -> Result<u64> {
    let missing: Vec<MissingGame> = sqlx::query_as(
        "SELECT g.id, g.pgn, g.player_color::text AS player_color FROM games g \
         WHERE g.account_id = $1 AND g.time_class = $2::time_class \
         AND NOT EXISTS (SELECT 1 FROM game_openings o WHERE o.game_id = g.id)",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_all(&mut *conn)
    .await?;
    let mut written = 0;
    for g in missing {
        let player = color_of(&g.player_color)?;
        let (plies, line) = match parse_pgn(&g.pgn) {
            Ok(parsed) => {
                let line = classify_line(&parsed.plies, player);
                (opening_plies(&parsed.plies), line)
            }
            Err(e) => {
                tracing::warn!(game = %g.id, error = %e, "unparsable PGN: no repertoire evidence");
                (Vec::new(), None)
            }
        };
        let mut fen_keys: Vec<&str> = plies
            .first()
            .map(|p| p.fen_key_before.as_str())
            .into_iter()
            .collect();
        fen_keys.extend(plies.iter().map(|p| p.fen_key_after.as_str()));
        let ucis: Vec<&str> = plies.iter().map(|p| p.uci.as_str()).collect();
        let sans: Vec<&str> = plies.iter().map(|p| p.san.as_str()).collect();
        let done = sqlx::query(
            "INSERT INTO game_openings (game_id, account_id, time_class, color, fen_keys, ucis, \
             sans, line_key, line_name, eco, line_kind, line_entry_ply) \
             VALUES ($1, $2, $3::time_class, $4::color, $5, $6, $7, $8, $9, $10, \
             $11::line_kind, $12) ON CONFLICT (game_id) DO NOTHING",
        )
        .bind(g.id)
        .bind(account_id)
        .bind(tc.as_str())
        .bind(player.as_str())
        .bind(&fen_keys)
        .bind(&ucis)
        .bind(&sans)
        .bind(line.as_ref().map(|l| l.key.as_str()))
        .bind(line.as_ref().map(|l| l.name.as_str()))
        .bind(line.as_ref().map(|l| l.eco.as_str()))
        .bind(line.as_ref().map(|l| l.kind.as_str()))
        .bind(line.as_ref().and_then(|l| i32::try_from(l.entry_ply).ok()))
        .execute(&mut *conn)
        .await?;
        written += done.rows_affected();
    }
    Ok(written)
}

/// The player's band for `tc` (R-4 with ruling Q2): the latest rating in the time class, else
/// the mean opponent rating of the last [`OPPONENT_BAND_GAMES`] games, else B1.
pub async fn player_band(
    pool: &PgPool,
    account_id: Uuid,
    tc: TimeClass,
) -> Result<(Band, BandSource)> {
    let mut conn = pool.acquire().await?;
    player_band_in(&mut conn, account_id, tc).await
}

/// [`player_band`] on the caller's connection or transaction.
pub async fn player_band_in(
    conn: &mut PgConnection,
    account_id: Uuid,
    tc: TimeClass,
) -> Result<(Band, BandSource)> {
    let platform: String =
        sqlx::query_scalar("SELECT platform::text FROM linked_accounts WHERE id = $1")
            .bind(account_id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or(RefdataError::NotFound(account_id))?;
    let platform: PlatformId = platform
        .parse()
        .map_err(|_| RefdataError::Decode(format!("platform {platform:?}")))?;
    let own: Option<i32> = sqlx::query_scalar(
        "SELECT CASE WHEN player_color = 'white' THEN white_rating ELSE black_rating END \
         FROM games WHERE account_id = $1 AND time_class = $2::time_class \
         AND (CASE WHEN player_color = 'white' THEN white_rating ELSE black_rating END) \
         IS NOT NULL ORDER BY played_at DESC, id LIMIT 1",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_optional(&mut *conn)
    .await?
    .flatten();
    let opponents: Vec<i32> = sqlx::query_scalar(
        "SELECT r FROM (SELECT CASE WHEN player_color = 'white' THEN black_rating \
         ELSE white_rating END AS r, played_at, id FROM games \
         WHERE account_id = $1 AND time_class = $2::time_class) s \
         WHERE r IS NOT NULL ORDER BY played_at DESC, id LIMIT $3",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .bind(OPPONENT_BAND_GAMES)
    .fetch_all(&mut *conn)
    .await?;
    Ok(resolve_band(platform, tc, own, &opponents))
}

fn band_pos(b: Band) -> usize {
    Band::ALL.iter().position(|x| *x == b).unwrap_or(0)
}

/// Bands to try for `band`: itself, then by distance up to [`EXPLORER_MAX_FALLBACK_BANDS`],
/// the lower band first on a tie.
pub fn explorer_band_order(band: Band) -> Vec<Band> {
    let at = band_pos(band);
    let mut bands: Vec<Band> = Band::ALL
        .into_iter()
        .filter(|b| band_pos(*b).abs_diff(at) <= EXPLORER_MAX_FALLBACK_BANDS)
        .collect();
    bands.sort_by_key(|b| (band_pos(*b).abs_diff(at), band_pos(*b)));
    bands
}

/// The explorer index of the first band in [`explorer_band_order`] that knows any of
/// `fen_keys`, and the comparison that records which band that was.
pub async fn load_explorer_with_fallback(
    pool: &PgPool,
    fen_keys: &[String],
    band: Band,
    source: BandSource,
    tc: TimeClass,
) -> Result<(ExplorerIndex, BandComparison)> {
    for b in explorer_band_order(band) {
        let index = load_explorer_index(pool, fen_keys, b, tc).await?;
        if !index.is_empty() {
            let comparison = BandComparison {
                player_band: band,
                band_source: source,
                explorer_band: Some(b),
                explorer_fallback: b != band,
            };
            return Ok((index, comparison));
        }
    }
    Ok((
        ExplorerIndex::new(),
        BandComparison {
            player_band: band,
            band_source: source,
            explorer_band: None,
            explorer_fallback: false,
        },
    ))
}

#[derive(FromRow)]
struct OpeningRow {
    game_id: Uuid,
    color: String,
    fen_keys: Vec<String>,
    ucis: Vec<String>,
    sans: Vec<String>,
    line_key: Option<String>,
    line_name: Option<String>,
    eco: Option<String>,
    line_kind: Option<String>,
    line_entry_ply: Option<i32>,
    result: String,
    white_rating: Option<i32>,
    black_rating: Option<i32>,
    platform: String,
    rated: bool,
    /// The player's rating in their previous rated game of the time class.
    previous_own_rating: Option<i32>,
    metrics: Option<serde_json::Value>,
}

#[derive(FromRow)]
struct MoveRow {
    game_id: Uuid,
    ply: i32,
    delta_w: f64,
    best_uci: Option<String>,
}

fn side_of(fen_key: &str) -> Option<Color> {
    match fen_key.split(' ').nth(1)? {
        "w" => Some(Color::White),
        "b" => Some(Color::Black),
        _ => None,
    }
}

/// Player-POV score of a result, `None` for an unfinished game (`*`).
fn score_of(result: &str, player: Color) -> Option<f64> {
    let white = match result {
        "1-0" => 1.0,
        "0-1" => 0.0,
        "1/2-1/2" => 0.5,
        _ => return None,
    };
    Some(match player {
        Color::White => white,
        Color::Black => 1.0 - white,
    })
}

/// A stored row as a [`RepertoireGame`] (without engine data); `None` without a result.
fn game_of(r: &OpeningRow) -> Result<Option<(RepertoireGame, Option<PlayerGameMetrics>)>> {
    let color = color_of(&r.color)?;
    let Some(score) = score_of(&r.result, color) else {
        return Ok(None);
    };
    let n = r.ucis.len().min(r.sans.len()).min(MAX_REPERTOIRE_PLIES);
    let mut plies = Vec::with_capacity(n);
    for i in 0..n {
        let (Some(before), Some(after)) = (r.fen_keys.get(i), r.fen_keys.get(i + 1)) else {
            break;
        };
        let mover =
            side_of(before).ok_or_else(|| RefdataError::Decode(format!("fen key {before:?}")))?;
        plies.push(RepertoirePly {
            uci: r.ucis[i].clone(),
            san: r.sans[i].clone(),
            fen_key_before: before.clone(),
            fen_key_after: after.clone(),
            mover,
        });
    }
    let line = match (
        &r.line_key,
        &r.line_name,
        &r.eco,
        &r.line_kind,
        r.line_entry_ply,
    ) {
        (Some(key), Some(name), Some(eco), Some(kind), Some(entry)) => Some(RepertoireLine {
            key: key.clone(),
            name: name.clone(),
            eco: eco.clone(),
            kind: kind
                .parse()
                .map_err(|_| RefdataError::Decode(format!("line kind {kind:?}")))?,
            entry_ply: u32::try_from(entry)
                .map_err(|_| RefdataError::Decode(format!("entry ply {entry}")))?,
        }),
        _ => None,
    };
    let (own, opp) = match color {
        Color::White => (r.white_rating, r.black_rating),
        Color::Black => (r.black_rating, r.white_rating),
    };
    // The rating-predicted score uses the pre-game ratings (ADR-FIX-C-1).
    let platform: PlatformId = r
        .platform
        .parse()
        .map_err(|_| RefdataError::Decode(format!("platform {:?}", r.platform)))?;
    let (own, opp) = pre_game_ratings(
        platform,
        PreGameInput {
            rated: r.rated,
            own_rating: own,
            opponent_rating: opp,
            previous_own_rating: r.previous_own_rating,
        },
    );
    // Version gate: only a current-version metrics row is engine evidence.
    let metrics = r
        .metrics
        .as_ref()
        .and_then(|v| serde_json::from_value::<PlayerGameMetrics>(v.clone()).ok())
        .filter(|m| m.version == PLAYER_GAME_METRICS_VERSION);
    let game = RepertoireGame {
        game_id: r.game_id,
        color,
        plies,
        score,
        expected_score: expected_score_from_ratings(own, opp),
        line,
        analysed: None,
    };
    Ok(Some((game, metrics)))
}

/// Loads the account × time class repertoire games with their engine data.
async fn load_games(
    conn: &mut PgConnection,
    account_id: Uuid,
    tc: TimeClass,
) -> Result<Vec<RepertoireGame>> {
    let rows: Vec<OpeningRow> = sqlx::query_as(
        "SELECT o.game_id, o.color::text AS color, o.fen_keys, o.ucis, o.sans, o.line_key, \
         o.line_name, o.eco, o.line_kind::text AS line_kind, o.line_entry_ply, \
         g.result::text AS result, g.white_rating, g.black_rating, \
         g.platform::text AS platform, g.rated, \
         (SELECT CASE p.player_color WHEN 'white' THEN p.white_rating ELSE p.black_rating END \
          FROM games p WHERE p.account_id = g.account_id AND p.time_class = g.time_class \
            AND p.rated AND (p.played_at, p.id) < (g.played_at, g.id) \
          ORDER BY p.played_at DESC, p.id DESC LIMIT 1) AS previous_own_rating, \
         gm.metrics \
         FROM game_openings o JOIN games g ON g.id = o.game_id \
         LEFT JOIN game_metrics gm ON gm.game_id = o.game_id \
         WHERE o.account_id = $1 AND o.time_class = $2::time_class \
         ORDER BY g.played_at, o.game_id",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_all(&mut *conn)
    .await?;
    let mut games = Vec::with_capacity(rows.len());
    let mut metrics: HashMap<Uuid, PlayerGameMetrics> = HashMap::new();
    let mut stale = 0u32;
    for r in &rows {
        if let Some((g, m)) = game_of(r)? {
            match m {
                Some(m) => {
                    metrics.insert(g.game_id, m);
                }
                None if r.metrics.is_some() => stale += 1,
                None => {}
            }
            games.push(g);
        }
    }
    if stale > 0 {
        tracing::info!(%account_id, stale, "stale game_metrics rows are not repertoire evidence");
    }
    let ids: Vec<Uuid> = metrics.keys().copied().collect();
    let moves: Vec<MoveRow> = if ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as(
            "SELECT game_id, ply, delta_w, best_uci FROM moves \
             WHERE game_id = ANY($1) AND ply <= $2",
        )
        .bind(&ids)
        .bind(i32::try_from(MAX_REPERTOIRE_PLIES).unwrap_or(i32::MAX))
        .fetch_all(&mut *conn)
        .await?
    };
    let mut by_game: HashMap<Uuid, Vec<MoveRow>> = HashMap::new();
    for m in moves {
        by_game.entry(m.game_id).or_default().push(m);
    }
    for g in &mut games {
        let Some(m) = metrics.remove(&g.game_id) else {
            continue;
        };
        let n = g.plies.len();
        let (mut delta_w, mut best_uci) = (vec![None; n], vec![None; n]);
        for mv in by_game.remove(&g.game_id).unwrap_or_default() {
            let Some(i) = usize::try_from(mv.ply).ok().and_then(|p| p.checked_sub(1)) else {
                continue;
            };
            if let (Some(d), Some(b)) = (delta_w.get_mut(i), best_uci.get_mut(i)) {
                *d = mv.delta_w.is_finite().then_some(mv.delta_w);
                *b = mv.best_uci;
            }
        }
        g.analysed = Some(AnalysedOpening {
            win_at_opening_end: m.opening.win_at_opening_end,
            opening_accuracy: m.accuracy_by_phase.opening,
            time_in_opening: m.time_in_opening,
            eval_at_opening_end_white_cp: m.opening.eval_at_opening_end_white_cp,
            delta_w,
            best_uci,
        });
    }
    Ok(games)
}

/// Rebuilds the repertoire of an account × time class (module doc); returns the number of
/// lines written to `opening_lines`.
///
/// The advisory lock is taken first and every account read runs in the same transaction, so a
/// rebuild that read older data cannot commit over a newer one (T18 review M3). The explorer is
/// read from the pool: it is reference data with its own one-snapshot read.
pub async fn rebuild_repertoire(pool: &PgPool, account_id: Uuid, tc: TimeClass) -> Result<usize> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('repertoire:' || $1::text || ':' || $2))")
        .bind(account_id)
        .bind(tc.as_str())
        .execute(&mut *tx)
        .await?;
    fill_game_openings_in(&mut tx, account_id, tc).await?;
    let (band, source) = player_band_in(&mut tx, account_id, tc).await?;
    let games = load_games(&mut tx, account_id, tc).await?;
    let mut keys: Vec<String> = games
        .iter()
        .flat_map(|g| g.plies.iter().map(|p| p.fen_key_before.clone()))
        .collect();
    keys.sort_unstable();
    keys.dedup();
    let (index, comparison) = load_explorer_with_fallback(pool, &keys, band, source, tc).await?;
    let lines: Vec<LineSummary> = summarise_lines(&games, &index, &comparison, tc);

    let mut ids = Vec::with_capacity(games.len());
    let mut exits: Vec<Option<i32>> = Vec::with_capacity(games.len());
    let mut devs: Vec<Option<i32>> = Vec::with_capacity(games.len());
    for g in &games {
        let (exit, dev) = book_exit(&g.plies, &index, g.color);
        ids.push(g.game_id);
        exits.push(exit.and_then(|v| i32::try_from(v).ok()));
        devs.push(dev.and_then(|v| i32::try_from(v).ok()));
    }

    let mut node_rows: Vec<(Color, String, i32, serde_json::Value)> = Vec::new();
    for color in [Color::White, Color::Black] {
        let mut tree: Vec<_> = build_repertoire_tree(&games, color).into_values().collect();
        tree.sort_by(|a, b| a.fen_key.cmp(&b.fen_key));
        for node in tree {
            let stats = serde_json::to_value(&node)
                .map_err(|e| RefdataError::Decode(format!("node stats: {e}")))?;
            let n = i32::try_from(node.games).unwrap_or(i32::MAX);
            node_rows.push((color, node.fen_key, n, stats));
        }
    }

    sqlx::query(
        "UPDATE game_openings o SET book_exit_ply = u.exit, player_deviation_ply = u.dev \
         FROM UNNEST($1::uuid[], $2::int4[], $3::int4[]) AS u(id, exit, dev) \
         WHERE o.game_id = u.id",
    )
    .bind(&ids)
    .bind(&exits)
    .bind(&devs)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM repertoire_nodes WHERE account_id = $1 AND time_class = $2::time_class",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM opening_lines WHERE account_id = $1 AND time_class = $2::time_class")
        .bind(account_id)
        .bind(tc.as_str())
        .execute(&mut *tx)
        .await?;
    for chunk in node_rows.chunks(NODE_CHUNK) {
        let colors: Vec<&str> = chunk.iter().map(|r| r.0.as_str()).collect();
        let fens: Vec<&str> = chunk.iter().map(|r| r.1.as_str()).collect();
        let counts: Vec<i32> = chunk.iter().map(|r| r.2).collect();
        let stats: Vec<&serde_json::Value> = chunk.iter().map(|r| &r.3).collect();
        sqlx::query(
            "INSERT INTO repertoire_nodes (account_id, time_class, color, fen_key, games, stats) \
             SELECT $1, $2::time_class, c::color, f, n, s \
             FROM UNNEST($3::text[], $4::text[], $5::int4[], $6::jsonb[]) AS u(c, f, n, s)",
        )
        .bind(account_id)
        .bind(tc.as_str())
        .bind(&colors)
        .bind(&fens)
        .bind(&counts)
        .bind(&stats)
        .execute(&mut *tx)
        .await?;
    }
    for l in &lines {
        let summary = serde_json::to_value(l)
            .map_err(|e| RefdataError::Decode(format!("line summary: {e}")))?;
        sqlx::query(
            "INSERT INTO opening_lines (account_id, time_class, color, line_key, kind, games, \
             verdict, summary) VALUES ($1, $2::time_class, $3::color, $4, $5::line_kind, $6, \
             $7::opening_verdict, $8)",
        )
        .bind(account_id)
        .bind(tc.as_str())
        .bind(l.color.as_str())
        .bind(&l.line_key)
        .bind(l.kind.as_str())
        .bind(i32::try_from(l.games).unwrap_or(i32::MAX))
        .bind(l.verdict.map(|v| v.as_str()))
        .bind(&summary)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(lines.len())
}
