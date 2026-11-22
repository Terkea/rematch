//! Repertoire tree and book exit (R2 Task 18, TS M2 Task 13 `repertoire.ts`).
//!
//! The tree is a DAG keyed by position (`fen_key`), not by move order, so transpositions merge
//! into one node with several parents. One tree per account × time class × colour. A game
//! counts once per node and once per edge, even when it repeats a position.
//!
//! Theory comes from our own rating-band opening explorer (`refdata::explorer`, Task 17). This
//! crate has no I/O, so [`ExplorerMove`] mirrors the stored row and [`in_theory`] mirrors
//! `refdata::explorer::in_theory` (a refdata test pins that they agree).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chess_core::fen::fen_key;
use chess_core::{Color, Ply};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Plies of a game that enter the repertoire (the opening is capped at ply 30).
pub const MAX_REPERTOIRE_PLIES: usize = 30;
/// A position is theory only with at least this many explorer games …
pub const THEORY_MIN_TOTAL: i64 = 50;
/// … and a move only with at least this share of them, in percent.
pub const THEORY_MIN_SHARE_PCT: i64 = 5;

/// One explorer move of a position: results after it, `total = white + draws + black`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplorerMove {
    pub uci: String,
    pub white: i64,
    pub draws: i64,
    pub black: i64,
    pub total: i64,
}

/// Explorer moves by `fen_key` for one band × time class.
pub type ExplorerIndex = HashMap<String, Vec<ExplorerMove>>;

/// Games in a position (0 when it is unknown).
pub fn position_total(index: &ExplorerIndex, fen_key: &str) -> i64 {
    index
        .get(fen_key)
        .map_or(0, |moves| moves.iter().map(|m| m.total).sum())
}

/// True when the position has at least [`THEORY_MIN_TOTAL`] games and `uci` has at least
/// [`THEORY_MIN_SHARE_PCT`] percent of them (exact integer comparison).
pub fn in_theory(index: &ExplorerIndex, fen_key: &str, uci: &str) -> bool {
    let Some(moves) = index.get(fen_key) else {
        return false;
    };
    let total: i64 = moves.iter().map(|m| m.total).sum();
    if total < THEORY_MIN_TOTAL {
        return false;
    }
    moves
        .iter()
        .find(|m| m.uci == uci)
        .is_some_and(|m| m.total * 100 >= total * THEORY_MIN_SHARE_PCT)
}

/// One opening ply of a game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepertoirePly {
    pub uci: String,
    pub san: String,
    pub fen_key_before: String,
    pub fen_key_after: String,
    pub mover: Color,
}

/// The first [`MAX_REPERTOIRE_PLIES`] plies of a parsed game, keyed by position.
pub fn opening_plies(plies: &[Ply]) -> Vec<RepertoirePly> {
    plies
        .iter()
        .take(MAX_REPERTOIRE_PLIES)
        .map(|p| RepertoirePly {
            uci: p.uci.clone(),
            san: p.san.clone(),
            fen_key_before: fen_key(&p.fen_before),
            fen_key_after: fen_key(&p.fen_after),
            mover: p.color,
        })
        .collect()
}

/// The classified line of a game (see `lines::classify_line`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepertoireLine {
    /// R-7 line key, `"{color}:{slug}"`.
    pub key: String,
    /// `line_family` of the deepest named position.
    pub name: String,
    pub eco: String,
    /// By the ownership rule (`lines::line_owner`), not by the entry mover alone.
    pub kind: coaching_input::LineSide,
    /// 1-based ply of the first named position of the family.
    pub entry_ply: u32,
}

/// Engine data of an analysed game's opening. `delta_w` and `best_uci` are indexed like
/// [`RepertoireGame::plies`] (`delta_w` is the mover's win% loss).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysedOpening {
    /// Player-POV win% after the last opening ply.
    pub win_at_opening_end: Option<f64>,
    pub opening_accuracy: Option<f64>,
    pub time_in_opening: Option<f64>,
    /// White-POV centipawns after the last opening ply (mate = ±1000, R-8).
    pub eval_at_opening_end_white_cp: Option<i32>,
    pub delta_w: Vec<Option<f64>>,
    pub best_uci: Vec<Option<String>>,
}

/// One game as the repertoire sees it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepertoireGame {
    pub game_id: Uuid,
    /// The player's colour.
    pub color: Color,
    /// At most [`MAX_REPERTOIRE_PLIES`].
    pub plies: Vec<RepertoirePly>,
    /// Player POV: 1, 0.5 or 0.
    pub score: f64,
    pub expected_score: Option<f64>,
    pub line: Option<RepertoireLine>,
    /// `None` when not analysed (or the metrics row is stale).
    pub analysed: Option<AnalysedOpening>,
}

/// A move out of a node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepertoireEdge {
    pub uci: String,
    pub san: String,
    pub to_fen_key: String,
    pub games: u32,
    pub score: f64,
}

/// One position of the tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepertoireNode {
    pub fen_key: String,
    pub games: u32,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
    pub score: f64,
    /// Mean of `score - expected` over the games with an expected score; `None` without any.
    pub perf_vs_expected: Option<f64>,
    /// Games with an expected score.
    pub perf_games: u32,
    pub analysed_games: u32,
    pub avg_win_at_opening_end: Option<f64>,
    pub avg_opening_accuracy: Option<f64>,
    pub avg_time_in_opening: Option<f64>,
    /// By games desc, then uci.
    pub children: Vec<RepertoireEdge>,
    /// Distinct parent positions, sorted.
    pub parents: Vec<String>,
}

/// Mean of the known values, `None` without any.
pub(crate) fn mean_known(values: impl IntoIterator<Item = Option<f64>>) -> (Option<f64>, u32) {
    let (mut sum, mut n) = (0.0, 0u32);
    for v in values.into_iter().flatten() {
        if v.is_finite() {
            sum += v;
            n += 1;
        }
    }
    ((n > 0).then(|| sum / f64::from(n)), n)
}

/// Counts and means of a set of games (indices into `games`).
pub(crate) struct GameStats {
    pub games: u32,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
    pub score: f64,
    pub perf: Option<f64>,
    pub perf_games: u32,
}

pub(crate) fn game_stats<'a>(games: impl IntoIterator<Item = &'a RepertoireGame>) -> GameStats {
    let mut s = GameStats {
        games: 0,
        wins: 0,
        draws: 0,
        losses: 0,
        score: 0.0,
        perf: None,
        perf_games: 0,
    };
    let (mut total, mut resid) = (0.0, 0.0);
    for g in games {
        s.games += 1;
        total += g.score;
        if g.score >= 1.0 {
            s.wins += 1;
        } else if g.score > 0.0 {
            s.draws += 1;
        } else {
            s.losses += 1;
        }
        if let Some(e) = g.expected_score {
            resid += g.score - e;
            s.perf_games += 1;
        }
    }
    if s.games > 0 {
        s.score = total / f64::from(s.games);
    }
    if s.perf_games > 0 {
        s.perf = Some(resid / f64::from(s.perf_games));
    }
    s
}

/// The player's tree for `color`: every position reached in the first
/// [`MAX_REPERTOIRE_PLIES`] plies of the games played with that colour.
pub fn build_repertoire_tree(
    games: &[RepertoireGame],
    color: Color,
) -> HashMap<String, RepertoireNode> {
    // Per node: game indices, parents, edges (uci → (san, to, game indices)).
    #[derive(Default)]
    struct Acc {
        games: BTreeSet<usize>,
        parents: BTreeSet<String>,
        edges: BTreeMap<String, (String, String, BTreeSet<usize>)>,
    }
    let mut acc: HashMap<String, Acc> = HashMap::new();
    for (gi, g) in games.iter().enumerate() {
        if g.color != color || !g.score.is_finite() {
            continue;
        }
        let Some(first) = g.plies.first() else {
            continue;
        };
        acc.entry(first.fen_key_before.clone())
            .or_default()
            .games
            .insert(gi);
        for p in g.plies.iter().take(MAX_REPERTOIRE_PLIES) {
            let from = acc.entry(p.fen_key_before.clone()).or_default();
            from.games.insert(gi);
            from.edges
                .entry(p.uci.clone())
                .or_insert_with(|| (p.san.clone(), p.fen_key_after.clone(), BTreeSet::new()))
                .2
                .insert(gi);
            let to = acc.entry(p.fen_key_after.clone()).or_default();
            to.games.insert(gi);
            to.parents.insert(p.fen_key_before.clone());
        }
    }
    acc.into_iter()
        .map(|(key, a)| {
            let s = game_stats(a.games.iter().filter_map(|i| games.get(*i)));
            let analysed: Vec<&AnalysedOpening> = a
                .games
                .iter()
                .filter_map(|i| games.get(*i)?.analysed.as_ref())
                .collect();
            let (avg_win, _) = mean_known(analysed.iter().map(|x| x.win_at_opening_end));
            let (avg_acc, _) = mean_known(analysed.iter().map(|x| x.opening_accuracy));
            let (avg_time, _) = mean_known(analysed.iter().map(|x| x.time_in_opening));
            let mut children: Vec<RepertoireEdge> = a
                .edges
                .into_iter()
                .map(|(uci, (san, to, gs))| {
                    let es = game_stats(gs.iter().filter_map(|i| games.get(*i)));
                    RepertoireEdge {
                        uci,
                        san,
                        to_fen_key: to,
                        games: es.games,
                        score: es.score,
                    }
                })
                .collect();
            children.sort_by(|x, y| y.games.cmp(&x.games).then_with(|| x.uci.cmp(&y.uci)));
            let node = RepertoireNode {
                fen_key: key.clone(),
                games: s.games,
                wins: s.wins,
                draws: s.draws,
                losses: s.losses,
                score: s.score,
                perf_vs_expected: s.perf,
                perf_games: s.perf_games,
                analysed_games: u32::try_from(analysed.len()).unwrap_or(u32::MAX),
                avg_win_at_opening_end: avg_win,
                avg_opening_accuracy: avg_acc,
                avg_time_in_opening: avg_time,
                children,
                parents: a.parents.into_iter().collect(),
            };
            (key, node)
        })
        .collect()
}

/// `(book_exit_ply, player_deviation_ply)`, both 1-based.
///
/// - The book exit is the first ply whose move is not [`in_theory`].
/// - It is the player's deviation only when the player made that move in a theory position
///   (≥ [`THEORY_MIN_TOTAL`] games). When the position is thin, theory ran out and nobody
///   deviated; when the opponent made the move, the player did not deviate.
/// - Without theory evidence (the first position is not a theory position, e.g. no explorer
///   data for the band) both are `None`: no claim from zero evidence. Every ply in theory gives
///   `(None, None)` too.
pub fn book_exit(
    plies: &[RepertoirePly],
    index: &ExplorerIndex,
    player: Color,
) -> (Option<u32>, Option<u32>) {
    let Some(first) = plies.first() else {
        return (None, None);
    };
    if position_total(index, &first.fen_key_before) < THEORY_MIN_TOTAL {
        return (None, None);
    }
    for (i, p) in plies.iter().enumerate() {
        if !in_theory(index, &p.fen_key_before, &p.uci) {
            let ply = u32::try_from(i + 1).unwrap_or(u32::MAX);
            let deviated =
                p.mover == player && position_total(index, &p.fen_key_before) >= THEORY_MIN_TOTAL;
            return (Some(ply), deviated.then_some(ply));
        }
    }
    (None, None)
}

/// True when the game has theory evidence for [`book_exit`] (its first position is a theory
/// position).
pub fn has_theory_evidence(plies: &[RepertoirePly], index: &ExplorerIndex) -> bool {
    plies
        .first()
        .is_some_and(|p| position_total(index, &p.fen_key_before) >= THEORY_MIN_TOTAL)
}
