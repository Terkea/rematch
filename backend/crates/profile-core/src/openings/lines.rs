//! Opening lines: classification with the ownership rule, band resolution and line summaries
//! (R2 Task 18, TS M2 Task 13 `lines.ts`).
//!
//! A line is the [`line_family`] of the deepest named position a game reached, keyed by
//! `(line_key, kind)`: the same family can be chosen in some games and faced in others, and the
//! two are never merged (T16 ruling).

use std::collections::{BTreeMap, BTreeSet};

use chess_core::board::Position;
use chess_core::fen::fen_key;
use chess_core::{Color, Ply, TimeClass};
use coaching_input::{LineSide, Verdict};
use serde::{Deserialize, Serialize};

use super::classify::{classify_opening, line_family, line_key};
use super::dataset::openings;
use super::repertoire::{
    ExplorerIndex, MAX_REPERTOIRE_PLIES, RepertoireGame, RepertoireLine, book_exit, game_stats,
    has_theory_evidence, mean_known,
};
use super::verdicts::{
    ALTERNATIVE_MIN_TOTAL, MAX_ALTERNATIVES, STRUGGLE_THRESHOLDS, StruggleReason, decide_verdict,
    detect_struggles,
};
use crate::bands::{band_of, to_lichess_equivalent};
use crate::stats::intervals::{Interval, Z80};
use crate::types::{Band, PlatformId};

/// The explorer band may fall back at most this many bands from the player's (as the band
/// reference, ADR-R2-23).
pub const EXPLORER_MAX_FALLBACK_BANDS: usize = 2;

/// Words that make Black the owner of a family (T16 ruling).
const BLACK_WORDS: [&str; 5] = [
    "Defense",
    "Defence",
    "Declined",
    "Accepted",
    "Countergambit",
];

/// The side that owns a line family (T16 ruling, ADR-R2-8):
/// a family with the word "Reversed" is White's; one with "Defense", "Defence", "Declined",
/// "Accepted" or "Countergambit" is Black's; otherwise the mover of the line's entry ply.
/// Whole words only. "Reversed" is checked first ("Zukertort Opening: Reversed Mexican
/// Defense" is White's).
pub fn line_owner(family: &str, entry_mover: Color) -> Color {
    let words: Vec<&str> = family
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    if words.contains(&"Reversed") {
        Color::White
    } else if words.iter().any(|w| BLACK_WORDS.contains(w)) {
        Color::Black
    } else {
        entry_mover
    }
}

/// `Chosen` when the player owns the family, else `Faced`.
pub fn line_side(family: &str, entry_mover: Color, player: Color) -> LineSide {
    if line_owner(family, entry_mover) == player {
        LineSide::Chosen
    } else {
        LineSide::Faced
    }
}

/// The line of a game from its first [`MAX_REPERTOIRE_PLIES`] plies, for the player's colour;
/// `None` when no position is named.
pub fn classify_line(plies: &[Ply], player: Color) -> Option<RepertoireLine> {
    let fens: Vec<(String, Color)> = plies
        .iter()
        .take(MAX_REPERTOIRE_PLIES)
        .map(|p| (p.fen_after.clone(), p.color))
        .collect();
    let c = classify_opening(&fens)?;
    let family = line_family(&c.name).to_owned();
    Some(RepertoireLine {
        key: line_key(player, &c.name),
        kind: line_side(&family, c.entry_mover, player),
        name: family,
        eco: c.eco,
        entry_ply: c.entry_ply,
    })
}

/// Where the player's band came from (R-4, ruling Q2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BandSource {
    /// The player's latest rating in the time class.
    Rating,
    /// The mean rating of the opponents of the last 20 games.
    OpponentMean,
    /// Nothing known: B1 (ruling Q2).
    Default,
}

/// The player's band: from the latest rating, else from the mean opponent rating (the caller
/// passes the last 20 games), else B1. Ratings are mapped to the Lichess-equivalent scale.
pub fn resolve_band(
    platform: PlatformId,
    tc: TimeClass,
    latest_rating: Option<i32>,
    opponent_ratings: &[i32],
) -> (Band, BandSource) {
    if let Some(r) = to_lichess_equivalent(platform, tc, latest_rating) {
        return (band_of(f64::from(r)), BandSource::Rating);
    }
    let eq: Vec<f64> = opponent_ratings
        .iter()
        .filter_map(|r| to_lichess_equivalent(platform, tc, Some(*r)))
        .map(f64::from)
        .collect();
    if eq.is_empty() {
        return (Band::B1, BandSource::Default);
    }
    let mean = eq.iter().sum::<f64>() / eq.len() as f64;
    (band_of(mean), BandSource::OpponentMean)
}

/// Which band the explorer comparison used. Every band claim (theory, alternatives) is made
/// against `explorer_band`, and must say so when it is a fallback.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BandComparison {
    pub player_band: Band,
    pub band_source: BandSource,
    /// The band whose explorer data was used; `None` when no band within
    /// [`EXPLORER_MAX_FALLBACK_BANDS`] had any.
    pub explorer_band: Option<Band>,
    /// True when `explorer_band` is not `player_band`.
    pub explorer_fallback: bool,
}

/// How a reason names the comparison group.
pub fn comparison_label(c: &BandComparison) -> String {
    let estimated = match c.band_source {
        BandSource::Rating => "",
        BandSource::OpponentMean => ", band estimated from opponents' ratings",
        BandSource::Default => ", band unknown (default B1)",
    };
    match c.explorer_band {
        None => format!(
            "no explorer data within {EXPLORER_MAX_FALLBACK_BANDS} bands of {}{estimated}",
            c.player_band
        ),
        Some(b) if c.explorer_fallback => format!(
            "{b} players (fallback band: no explorer data for {}{estimated})",
            c.player_band
        ),
        Some(b) => {
            if estimated.is_empty() {
                format!("{b} players")
            } else {
                format!("{b} players ({})", estimated.trim_start_matches(", "))
            }
        }
    }
}

/// An explorer move the player could play instead at the line's choice node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlternativeMove {
    pub uci: String,
    pub san: String,
    /// Dataset name of the position after the move.
    pub name: Option<String>,
    /// Explorer score for the side to move, 0..1 (in [`BandComparison::explorer_band`]).
    pub band_score: f64,
    pub games: u32,
}

/// The player's move that repeatedly lost ΔW ≥ 10 in this line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixSpot {
    pub fen_key: String,
    pub uci: String,
    pub san: String,
    pub games: u32,
}

/// One line of the player, with every `coaching_input::OpeningVerdict` input and the evidence
/// behind its verdict.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineSummary {
    pub line_key: String,
    pub name: String,
    pub eco: Option<String>,
    pub color: Color,
    pub kind: LineSide,
    pub time_class: String,
    /// The most common move order up to the line's entry ply.
    pub moves_san: Vec<String>,
    pub games: u32,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
    pub score: f64,
    /// Mean `score - expected` over the `perf_games` games with an expected score.
    pub perf_vs_expected: Option<f64>,
    pub perf_games: u32,
    /// 80% normal interval of `perf_vs_expected` from the sample variance alone; `None` when it
    /// cannot be estimated (fewer than 2 games, or no spread). Never widened.
    pub perf_interval: Option<Interval>,
    /// Analysed games with a win% at the opening end.
    pub analysed_games: u32,
    pub avg_win_at_opening_end: Option<f64>,
    pub avg_opening_accuracy: Option<f64>,
    pub avg_time_in_opening: Option<f64>,
    /// R-8: rounded mean White-POV cp at the opening end over `eval_games` analysed games.
    pub avg_eval_white_cp: Option<i32>,
    pub eval_games: u32,
    /// Typical (lower-median) ply where the player left theory; `None` unless the median is a
    /// real deviation.
    pub deviation_ply: Option<u32>,
    pub deviation_move_san: Option<String>,
    /// Median player deviation ply, with games where the player never left theory counted as
    /// later than any ply; `None` when that median is not a deviation or the evidence is under
    /// 8 games.
    pub median_deviation_ply: Option<f64>,
    /// Games with explorer theory evidence for a deviation claim.
    pub deviation_evidence_games: u32,
    /// Where the player chose the line (chosen lines only).
    pub choice_fen_key: Option<String>,
    pub choice_uci: Option<String>,
    pub alternatives: Vec<AlternativeMove>,
    pub comparison: BandComparison,
    /// `games` over the player's games with this colour.
    pub share: f64,
    /// The player's most repeated ΔW ≥ 10 move, whatever its count.
    pub error_spot: Option<FixSpot>,
    pub struggles: Vec<StruggleReason>,
    /// `error_spot` when it makes a repeated deviation error.
    pub fix_spot: Option<FixSpot>,
    pub verdict: Option<Verdict>,
    pub reason: String,
}

/// Most frequent value; ties go to the smallest.
fn mode<T: Ord + Clone>(values: impl IntoIterator<Item = T>) -> Option<T> {
    let mut counts: BTreeMap<T, u32> = BTreeMap::new();
    for v in values {
        *counts.entry(v).or_default() += 1;
    }
    let mut best: Option<(T, u32)> = None;
    for (v, c) in counts {
        if best.as_ref().is_none_or(|(_, b)| c > *b) {
            best = Some((v, c));
        }
    }
    best.map(|(v, _)| v)
}

/// Half-widths at or below this, relative to `max(1, |mean|)`, count as zero spread. Every
/// threshold is compared with a tolerance of `EPS` = 1e-9 (verdicts), so a narrower interval
/// cannot be told apart from its centre at that resolution; it is far above the summation noise
/// of identical residuals (about 1e-17) and far below any real spread of game results.
pub const ZERO_SPREAD_REL_TOL: f64 = 1e-9;

/// 80% normal interval (`Z80`) of the mean residual `score - expected`, from the sample
/// variance only (binding ruling: an interval is never widened). `None` when it cannot be
/// estimated: fewer than 2 pairs, a non-finite value, or zero spread within
/// [`ZERO_SPREAD_REL_TOL`]. Nothing that depends on the interval is then claimed.
pub fn perf_interval(pairs: &[(f64, f64)]) -> Option<Interval> {
    if pairs.len() < 2 {
        return None;
    }
    let n = pairs.len() as f64;
    let resid: Vec<f64> = pairs.iter().map(|(s, e)| s - e).collect();
    let mean = resid.iter().sum::<f64>() / n;
    let sample = resid.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
    let half = Z80 * (sample / n).sqrt();
    if !mean.is_finite() || !half.is_finite() || half <= ZERO_SPREAD_REL_TOL * mean.abs().max(1.0) {
        return None;
    }
    let (low, high) = (mean - half, mean + half);
    (low < high).then_some(Interval { low, high })
}

/// Up to [`MAX_ALTERNATIVES`] explorer moves at `choice` with ≥ [`ALTERNATIVE_MIN_TOTAL`]
/// games, not in `exclude`, by score for the side to move (then games, then uci).
pub fn select_alternatives(
    index: &ExplorerIndex,
    choice: &str,
    exclude: &BTreeSet<String>,
) -> Vec<AlternativeMove> {
    let (Some(moves), Ok(pos)) = (
        index.get(choice),
        Position::from_fen(&format!("{choice} 0 1")),
    ) else {
        return Vec::new();
    };
    let stm = pos.side_to_move();
    let mut out: Vec<(AlternativeMove, i64)> = moves
        .iter()
        .filter(|m| m.total >= ALTERNATIVE_MIN_TOTAL && !exclude.contains(&m.uci))
        .filter_map(|m| {
            let mut after = pos.clone();
            let played = after.push_uci(&m.uci).ok()?;
            let wins = match stm {
                Color::White => m.white,
                Color::Black => m.black,
            };
            let band_score = (wins as f64 + 0.5 * m.draws as f64) / m.total as f64;
            let name = openings()
                .get(&fen_key(&after.fen()))
                .map(|o| o.name.clone());
            Some((
                AlternativeMove {
                    uci: played.uci,
                    san: played.san,
                    name,
                    band_score,
                    games: u32::try_from(m.total).unwrap_or(u32::MAX),
                },
                m.total,
            ))
        })
        .collect();
    out.sort_by(|(a, at), (b, bt)| {
        b.band_score
            .total_cmp(&a.band_score)
            .then_with(|| bt.cmp(at))
            .then_with(|| a.uci.cmp(&b.uci))
    });
    out.into_iter()
        .take(MAX_ALTERNATIVES)
        .map(|(a, _)| a)
        .collect()
}

/// One summary per `(line_key, kind)` of the games, by games desc then key and kind, with its
/// struggles and verdict. `index` is the explorer of `band.explorer_band`.
pub fn summarise_lines(
    games: &[RepertoireGame],
    index: &ExplorerIndex,
    band: &BandComparison,
    tc: TimeClass,
) -> Vec<LineSummary> {
    let usable = |g: &&RepertoireGame| !g.plies.is_empty() && g.score.is_finite();
    let mut color_games: BTreeMap<&str, u32> = BTreeMap::new();
    let mut groups: BTreeMap<(String, &str), Vec<&RepertoireGame>> = BTreeMap::new();
    for g in games.iter().filter(usable) {
        *color_games.entry(g.color.as_str()).or_default() += 1;
        if let Some(l) = &g.line {
            groups
                .entry((l.key.clone(), l.kind.as_str()))
                .or_default()
                .push(g);
        }
    }
    let mut out: Vec<LineSummary> = groups
        .into_values()
        .filter_map(|gs| {
            let first = *gs.first()?;
            let line = first.line.as_ref()?;
            let total_color = color_games.get(first.color.as_str()).copied().unwrap_or(0);
            Some(summarise_one(
                &gs,
                line,
                first.color,
                total_color,
                index,
                band,
                tc,
            ))
        })
        .collect();
    out.sort_by(|a, b| {
        b.games
            .cmp(&a.games)
            .then_with(|| a.line_key.cmp(&b.line_key))
            .then_with(|| a.kind.as_str().cmp(b.kind.as_str()))
    });
    out
}

fn summarise_one(
    gs: &[&RepertoireGame],
    line: &RepertoireLine,
    color: Color,
    color_games: u32,
    index: &ExplorerIndex,
    band: &BandComparison,
    tc: TimeClass,
) -> LineSummary {
    let t = STRUGGLE_THRESHOLDS;
    let stats = game_stats(gs.iter().copied());
    let lines = || gs.iter().filter_map(|g| g.line.as_ref());
    let name = mode(lines().map(|l| l.name.clone())).unwrap_or_else(|| line.name.clone());
    let eco = mode(lines().map(|l| l.eco.clone()));
    let moves_san = mode(gs.iter().map(|g| {
        let entry = g.line.as_ref().map_or(0, |l| l.entry_ply as usize);
        g.plies
            .iter()
            .take(entry)
            .map(|p| p.san.clone())
            .collect::<Vec<_>>()
    }))
    .unwrap_or_default();

    let pairs: Vec<(f64, f64)> = gs
        .iter()
        .filter_map(|g| Some((g.score, g.expected_score?)))
        .collect();

    let analysed: Vec<_> = gs.iter().filter_map(|g| g.analysed.as_ref()).collect();
    let (avg_win, analysed_games) = mean_known(analysed.iter().map(|a| a.win_at_opening_end));
    let (avg_acc, _) = mean_known(analysed.iter().map(|a| a.opening_accuracy));
    let (avg_time, _) = mean_known(analysed.iter().map(|a| a.time_in_opening));
    let (avg_eval, eval_games) = mean_known(
        analysed
            .iter()
            .map(|a| a.eval_at_opening_end_white_cp.map(f64::from)),
    );

    // The player's ΔW ≥ 10 moves, by (fen_key, uci), each game once.
    let mut errors: BTreeMap<(String, String), (String, BTreeSet<usize>)> = BTreeMap::new();
    for (gi, g) in gs.iter().enumerate() {
        let Some(a) = &g.analysed else { continue };
        for (p, dw) in g.plies.iter().zip(&a.delta_w) {
            if p.mover == g.color && dw.is_some_and(|d| d >= t.deviation_delta_w) {
                errors
                    .entry((p.fen_key_before.clone(), p.uci.clone()))
                    .or_insert_with(|| (p.san.clone(), BTreeSet::new()))
                    .1
                    .insert(gi);
            }
        }
    }
    let mut error_spot: Option<FixSpot> = None;
    for ((fen, uci), (san, set)) in errors {
        let n = u32::try_from(set.len()).unwrap_or(u32::MAX);
        if error_spot.as_ref().is_none_or(|s| n > s.games) {
            error_spot = Some(FixSpot {
                fen_key: fen,
                uci,
                san,
                games: n,
            });
        }
    }

    // Deviations: games with theory evidence; a game where the player never left theory is
    // later than any ply.
    let mut devs: Vec<(f64, Option<String>)> = gs
        .iter()
        .filter(|g| has_theory_evidence(&g.plies, index))
        .map(|g| match book_exit(&g.plies, index, g.color).1 {
            Some(ply) => {
                let san = g.plies.get(ply as usize - 1).map(|p| p.san.clone());
                (f64::from(ply), san)
            }
            None => (f64::INFINITY, None),
        })
        .collect();
    devs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let evidence = u32::try_from(devs.len()).unwrap_or(u32::MAX);
    let (mut median, mut deviation_ply, mut deviation_san) = (None, None, None);
    if evidence >= t.min_games {
        let n = devs.len();
        let m = if n % 2 == 1 {
            devs[n / 2].0
        } else {
            (devs[n / 2 - 1].0 + devs[n / 2].0) / 2.0
        };
        if m.is_finite() {
            median = Some(m);
            let lower = devs[(n - 1) / 2].0;
            deviation_ply = Some(lower as u32);
            deviation_san = mode(
                devs.iter()
                    .filter(|(p, _)| *p == lower)
                    .filter_map(|(_, s)| s.clone()),
            );
        }
    }

    // Choice node: the player's last move up to the entry ply (chosen lines only).
    let (mut choice_fen, mut choice_uci, mut alternatives) = (None, None, Vec::new());
    if line.kind == LineSide::Chosen {
        let choices: Vec<(String, String)> = gs
            .iter()
            .filter_map(|g| {
                let entry = g.line.as_ref()?.entry_ply as usize;
                g.plies
                    .iter()
                    .take(entry)
                    .rev()
                    .find(|p| p.mover == g.color)
                    .map(|p| (p.fen_key_before.clone(), p.uci.clone()))
            })
            .collect();
        if let Some(fen) = mode(choices.iter().map(|(f, _)| f.clone())) {
            let played: BTreeSet<String> = choices
                .iter()
                .filter(|(f, _)| *f == fen)
                .map(|(_, u)| u.clone())
                .collect();
            choice_uci = mode(
                choices
                    .iter()
                    .filter(|(f, _)| *f == fen)
                    .map(|(_, u)| u.clone()),
            );
            alternatives = select_alternatives(index, &fen, &played);
            choice_fen = Some(fen);
        }
    }

    let share = if color_games == 0 {
        0.0
    } else {
        f64::from(stats.games) / f64::from(color_games)
    };
    let mut summary = LineSummary {
        line_key: line.key.clone(),
        name,
        eco,
        color,
        kind: line.kind,
        time_class: tc.as_str().to_owned(),
        moves_san,
        games: stats.games,
        wins: stats.wins,
        draws: stats.draws,
        losses: stats.losses,
        score: stats.score,
        perf_vs_expected: stats.perf,
        perf_games: stats.perf_games,
        perf_interval: perf_interval(&pairs),
        analysed_games,
        avg_win_at_opening_end: avg_win,
        avg_opening_accuracy: avg_acc,
        avg_time_in_opening: avg_time,
        avg_eval_white_cp: avg_eval.map(|v| v.round() as i32),
        eval_games,
        deviation_ply,
        deviation_move_san: deviation_san,
        median_deviation_ply: median,
        deviation_evidence_games: evidence,
        choice_fen_key: choice_fen,
        choice_uci,
        alternatives,
        comparison: band.clone(),
        share,
        error_spot,
        struggles: Vec::new(),
        fix_spot: None,
        verdict: None,
        reason: String::new(),
    };
    summary.struggles = detect_struggles(&summary);
    if summary
        .struggles
        .contains(&StruggleReason::RepeatedDeviationError)
    {
        summary.fix_spot = summary.error_spot.clone();
    }
    let (verdict, reason) = decide_verdict(
        &summary,
        &summary.struggles,
        color_games,
        &summary.alternatives,
    );
    summary.verdict = verdict;
    summary.reason = reason;
    summary
}
