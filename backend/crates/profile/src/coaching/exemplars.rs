//! Exemplar positions (R2 Task 34, TS M2 Task 26 `exemplars.ts`).
//!
//! An [`ExemplarRow`] is one stored move with its enrichment, its game and the White-POV evals
//! of the positions before and after it ([`load_exemplar_rows`]). [`to_exemplar_position`] turns
//! it into the contract's `ExemplarPosition`:
//! - `gameId` is the platform game id (R-7); scores are White POV (R-8);
//! - SAN comes from replaying the UCI moves on the FEN (`Position::push_uci`), so `played.san`
//!   is exactly what replaying `played.uci` gives;
//! - `best` is `None` when the played move is the best move;
//! - `bestPv` and `refutationPv` keep their legal prefix (from `fen`, and from the position after
//!   `played`), cut to 16 plies; a `bestPv` that does not start with `best` is dropped (it would
//!   contradict `best`);
//! - tags are the `contract_tag()`s of the best and refutation lines' player-visible tags
//!   (EXP-0007, `MotifTag::is_player_visible`), deduplicated, at most 12; a position whose only
//!   tags are hidden has none, and is still shown by its move, eval swing and best move.
//!
//! A row that cannot give a valid position (an unknown eval, an illegal move, a FEN whose side to
//! move disagrees with the ply, an over-long id) gives `None`: unknown is never invented.

use std::collections::{HashMap, HashSet};

use chess_core::board::Position;
use chess_core::fen::fen_key;
use chess_core::{Color, EngineScore};
use coaching_input::limits::{FEN_MAX_CHARS, MAX_PLY};
use coaching_input::{ExemplarPosition, LIMITS, MoveRef};
use profile_core::types::{LineAnalysis, MotifTag};
use sqlx::types::Json;
use sqlx::{FromRow, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use super::mapping::contract_score;
use crate::analyse_features::parse_color;

/// One exemplar move (see the module docs).
#[derive(Clone, Debug, PartialEq)]
pub struct ExemplarRow {
    pub game_id: Uuid,
    /// The platform's game id (contract `gameId`, R-7).
    pub platform_game_id: String,
    pub played_at: OffsetDateTime,
    pub player_color: Color,
    /// 1-based half-move number.
    pub ply: u32,
    pub color: Color,
    pub san: String,
    pub uci: String,
    pub fen_before: String,
    pub best_uci: Option<String>,
    /// The mover's win% before and after the move.
    pub win_before: f64,
    pub win_after: f64,
    pub delta_w: f64,
    pub phase: String,
    /// Empty without enrichment.
    pub best_pv: Vec<String>,
    pub refutation_pv: Vec<String>,
    pub best_tags: Vec<MotifTag>,
    pub refutation_tags: Vec<MotifTag>,
    /// White POV; `None` when no eval is stored.
    pub eval_before: Option<EngineScore>,
    pub eval_after: Option<EngineScore>,
}

/// The legal prefix of `pv` from `start`, in chess.js UCI, at most 16 plies.
fn legal_prefix(start: &Position, pv: &[String]) -> Vec<String> {
    let mut p = start.clone();
    let mut out = Vec::new();
    for uci in pv.iter().take(LIMITS.pv_plies) {
        match p.push_uci(uci) {
            Ok(m) => out.push(m.uci),
            Err(_) => break,
        }
    }
    out
}

fn tags(row: &ExemplarRow) -> Vec<String> {
    let mut seen = HashSet::new();
    row.best_tags
        .iter()
        .chain(&row.refutation_tags)
        .filter(|t| t.is_player_visible())
        .map(|t| t.contract_tag())
        .filter(|t| seen.insert(*t))
        .take(LIMITS.tags)
        .map(str::to_owned)
        .collect()
}

fn win(x: f64) -> Option<f64> {
    x.is_finite().then(|| x.clamp(0.0, 100.0))
}

/// The contract position of a row, or `None` when it cannot be stated (module docs).
pub fn to_exemplar_position(row: &ExemplarRow) -> Option<ExemplarPosition> {
    let id_len = row.platform_game_id.chars().count();
    if !(1..=LIMITS.id_chars).contains(&id_len)
        || !(1..=MAX_PLY).contains(&row.ply)
        || row.fen_before.chars().count() > FEN_MAX_CHARS
    {
        return None;
    }
    let start = Position::from_fen(&row.fen_before).ok()?;
    let white_to_move = start.side_to_move() == Color::White;
    if white_to_move != (row.ply % 2 == 1) {
        return None;
    }
    let mut after = start.clone();
    let played = after.push_uci(&row.uci).ok()?;
    let best = match &row.best_uci {
        None => None,
        Some(b) => {
            let mut p = start.clone();
            let m = p.push_uci(b).ok()?;
            (m.uci != played.uci).then_some(MoveRef {
                san: m.san,
                uci: m.uci,
            })
        }
    };
    let mut best_pv = legal_prefix(&start, &row.best_pv);
    if let Some(b) = &best
        && best_pv.first() != Some(&b.uci)
    {
        best_pv.clear();
    }
    let refutation_pv = legal_prefix(&after, &row.refutation_pv);
    Some(ExemplarPosition {
        game_id: row.platform_game_id.clone(),
        ply: row.ply,
        played_at: row.played_at,
        fen: row.fen_before.clone(),
        played: MoveRef {
            san: played.san,
            uci: played.uci,
        },
        best,
        best_pv,
        refutation_pv,
        tags: tags(row),
        eval_before: contract_score(row.eval_before?),
        eval_after: contract_score(row.eval_after?),
        win_before: win(row.win_before)?,
        win_after: win(row.win_after)?,
    })
}

#[derive(FromRow)]
struct DbRow {
    game_id: Uuid,
    platform_game_id: String,
    played_at: OffsetDateTime,
    player_color: String,
    ply: i32,
    color: String,
    san: String,
    uci: String,
    fen_before: String,
    best_uci: Option<String>,
    win_before: f64,
    win_after: f64,
    delta_w: f64,
    phase: String,
    best_pv: Option<Vec<String>>,
    refutation_pv: Option<Vec<String>>,
    best_line: Option<Json<LineAnalysis>>,
    refutation_line: Option<Json<LineAnalysis>>,
}

/// White-POV score of a position the analysis does not cache: a final checkmate (the side that
/// just moved won, ±1000 as `evaluate_game`'s terminal eval) or stalemate (0).
fn terminal_score(p: &Position) -> Option<EngineScore> {
    if p.is_checkmate() {
        Some(EngineScore::Cp(match p.side_to_move() {
            Color::White => -1000,
            Color::Black => 1000,
        }))
    } else if p.is_stalemate() {
        Some(EngineScore::Cp(0))
    } else {
        None
    }
}

/// The moves `refs` names, keyed by `(game id, ply)`, with the evals of their game's analysis
/// protocol (`protocols`, by game). Missing moves are absent; an undecodable row is skipped and
/// counted in `skipped`.
pub async fn load_exemplar_rows(
    pool: &PgPool,
    refs: &[(Uuid, u32)],
    protocols: &HashMap<Uuid, String>,
    skipped: &Skipped,
) -> Result<HashMap<(Uuid, u32), ExemplarRow>, sqlx::Error> {
    if refs.is_empty() {
        return Ok(HashMap::new());
    }
    let games: Vec<Uuid> = refs.iter().map(|r| r.0).collect();
    let plies: Vec<i32> = refs
        .iter()
        .map(|r| i32::try_from(r.1).unwrap_or(i32::MAX))
        .collect();
    let rows: Vec<DbRow> = sqlx::query_as(
        "SELECT DISTINCT ON (m.game_id, m.ply) m.game_id, g.platform_game_id, g.played_at, \
           g.player_color::text AS player_color, m.ply, m.color::text AS color, m.san, m.uci, \
           m.fen_before, m.best_uci, m.win_before, m.win_after, m.delta_w, \
           m.phase::text AS phase, mf.best_pv, mf.refutation_pv, mf.best_line, \
           mf.refutation_line \
         FROM UNNEST($1::uuid[], $2::int4[]) AS r(game_id, ply) \
         JOIN moves m ON m.game_id = r.game_id AND m.ply = r.ply \
         JOIN games g ON g.id = m.game_id \
         LEFT JOIN move_features mf ON mf.game_id = m.game_id AND mf.ply = m.ply \
         ORDER BY m.game_id, m.ply",
    )
    .bind(&games)
    .bind(&plies)
    .fetch_all(pool)
    .await?;

    // Positions before and after each move, and the evals they need.
    let mut positions: Vec<(String, Option<Position>)> = Vec::with_capacity(rows.len());
    let mut keys: Vec<String> = Vec::new();
    for r in &rows {
        let after = Position::from_fen(&r.fen_before)
            .ok()
            .and_then(|p| p.play_uci(&r.uci).ok());
        keys.push(fen_key(&r.fen_before));
        if let Some(a) = &after {
            keys.push(fen_key(&a.fen()));
        }
        positions.push((fen_key(&r.fen_before), after));
    }
    keys.sort_unstable();
    keys.dedup();
    let evals: Vec<(String, String, Option<i32>, Option<i32>)> = sqlx::query_as(
        "SELECT fen_key, protocol, score_cp, score_mate FROM position_evals \
         WHERE fen_key = ANY($1)",
    )
    .bind(&keys)
    .fetch_all(pool)
    .await?;
    let evals: HashMap<(String, String), EngineScore> = evals
        .into_iter()
        .filter_map(|(k, p, cp, mate)| {
            let s = match (mate, cp) {
                (Some(m), _) => EngineScore::Mate(m),
                (None, Some(c)) => EngineScore::Cp(c),
                (None, None) => return None,
            };
            Some(((k, p), s))
        })
        .collect();

    let mut out = HashMap::with_capacity(rows.len());
    for (r, (before_key, after)) in rows.into_iter().zip(positions) {
        let (Ok(ply), Some(player_color), Some(color)) = (
            u32::try_from(r.ply),
            parse_color(&r.player_color),
            parse_color(&r.color),
        ) else {
            skipped.add(r.game_id, u32::try_from(r.ply).unwrap_or(u32::MAX));
            continue;
        };
        let protocol = protocols.get(&r.game_id);
        let eval_of = |key: String| protocol.and_then(|p| evals.get(&(key, p.clone())).copied());
        let eval_before = eval_of(before_key);
        let eval_after = after
            .as_ref()
            .and_then(|a| terminal_score(a).or_else(|| eval_of(fen_key(&a.fen()))));
        let row = ExemplarRow {
            game_id: r.game_id,
            platform_game_id: r.platform_game_id,
            played_at: r.played_at,
            player_color,
            ply,
            color,
            san: r.san,
            uci: r.uci,
            fen_before: r.fen_before,
            best_uci: r.best_uci,
            win_before: r.win_before,
            win_after: r.win_after,
            delta_w: r.delta_w,
            phase: r.phase,
            best_pv: r.best_pv.unwrap_or_default(),
            refutation_pv: r.refutation_pv.unwrap_or_default(),
            best_tags: r.best_line.map(|j| j.0.tags).unwrap_or_default(),
            refutation_tags: r.refutation_line.map(|j| j.0.tags).unwrap_or_default(),
            eval_before,
            eval_after,
        };
        out.insert((row.game_id, ply), row);
    }
    Ok(out)
}

/// The stored rows that could not become a position (skipped, never fabricated), each counted
/// once however many lists tried it (T34 review m6).
#[derive(Debug, Default)]
pub struct Skipped(std::cell::RefCell<HashSet<(Uuid, u32)>>);

impl Skipped {
    pub fn add(&self, game_id: Uuid, ply: u32) {
        self.0.borrow_mut().insert((game_id, ply));
    }

    /// Distinct rows skipped.
    pub fn get(&self) -> u32 {
        u32::try_from(self.0.borrow().len()).unwrap_or(u32::MAX)
    }
}

/// Picks at most `n` positions from `rows` in `order`, one per game. A row that gives no
/// position ([`to_exemplar_position`] is `None`) is skipped and counted in `skipped`.
pub fn pick_distinct<'a>(
    mut rows: Vec<&'a ExemplarRow>,
    n: usize,
    order: impl FnMut(&&'a ExemplarRow, &&'a ExemplarRow) -> std::cmp::Ordering,
    skipped: &Skipped,
) -> Vec<ExemplarPosition> {
    rows.sort_by(order);
    let mut games = HashSet::new();
    let mut out = Vec::new();
    for r in rows {
        if out.len() >= n {
            break;
        }
        if games.contains(&r.game_id) {
            continue;
        }
        match to_exemplar_position(r) {
            Some(p) => {
                games.insert(r.game_id);
                out.push(p);
            }
            None => skipped.add(r.game_id, r.ply),
        }
    }
    out
}

/// ΔW descending, then most recent first, then ply (a total order).
pub fn by_delta_then_recency(a: &&ExemplarRow, b: &&ExemplarRow) -> std::cmp::Ordering {
    b.delta_w
        .total_cmp(&a.delta_w)
        .then(b.played_at.cmp(&a.played_at))
        .then(a.game_id.cmp(&b.game_id))
        .then(a.ply.cmp(&b.ply))
}

/// Most recent first, then ply.
pub fn by_recency(a: &&ExemplarRow, b: &&ExemplarRow) -> std::cmp::Ordering {
    b.played_at
        .cmp(&a.played_at)
        .then(a.game_id.cmp(&b.game_id))
        .then(a.ply.cmp(&b.ply))
}
