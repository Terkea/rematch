//! Struggle detection and opening verdicts (R2 Task 18, TS M2 Task 13 `verdicts.ts`).
//!
//! Thresholds and rules are TS M2's. Every rule needs evidence: no struggle and no verdict
//! under [`StruggleThresholds::min_games`] games, a performance claim only from games with an
//! expected score and an 80% interval below zero, a deviation claim only with explorer theory.

use coaching_input::{LineSide, Verdict};
use serde::{Deserialize, Serialize};

use super::lines::{AlternativeMove, EXPLORER_MAX_FALLBACK_BANDS, LineSummary, comparison_label};

/// Why a line is a struggle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StruggleReason {
    /// perf ≤ −0.10 and the upper end of its 80% interval < 0 (no estimable interval: no claim).
    ScoreBelowExpected,
    /// ≥ 5 analysed games and a mean player win% at the opening end ≤ 40.
    WorseAfterOpening,
    /// One `(fen_key, uci)` of the player with ΔW ≥ 10 in ≥ 3 games and ≥ 30% of the line.
    RepeatedDeviationError,
}

impl StruggleReason {
    pub fn as_str(self) -> &'static str {
        match self {
            StruggleReason::ScoreBelowExpected => "score_below_expected",
            StruggleReason::WorseAfterOpening => "worse_after_opening",
            StruggleReason::RepeatedDeviationError => "repeated_deviation_error",
        }
    }
}

/// TS M2 `STRUGGLE_THRESHOLDS`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StruggleThresholds {
    pub min_games: u32,
    pub perf_drop: f64,
    pub worse_win: f64,
    pub min_analysed: u32,
    pub deviation_min_games: u32,
    pub deviation_share: f64,
    pub deviation_delta_w: f64,
}

pub const STRUGGLE_THRESHOLDS: StruggleThresholds = StruggleThresholds {
    min_games: 8,
    perf_drop: -0.10,
    worse_win: 40.0,
    min_analysed: 5,
    deviation_min_games: 3,
    deviation_share: 0.3,
    deviation_delta_w: 10.0,
};

/// Rule 3: `replace` needs this many games …
pub const REPLACE_MIN_GAMES: u32 = 15;
/// … and a performance at or below this.
pub const REPLACE_MAX_PERF: f64 = -0.15;
/// Rule 4: a median player deviation at or before this ply …
pub const LEARN_MAX_DEVIATION_PLY: f64 = 10.0;
/// … in a line with at least this share of the player's games with the colour.
pub const LEARN_MIN_SHARE: f64 = 0.10;
/// Alternatives: explorer moves with at least this many games …
pub const ALTERNATIVE_MIN_TOTAL: i64 = 100;
/// … at most this many.
pub const MAX_ALTERNATIVES: usize = 3;

/// Tolerance for comparing means and shares with their thresholds (float sums of exact
/// inputs such as 15 × −0.15 are off by a few ulps).
pub(crate) const EPS: f64 = 1e-9;

/// The struggles of a line (TS `detectStruggles`), in [`StruggleReason`] order. Empty under
/// [`StruggleThresholds::min_games`] games.
pub fn detect_struggles(line: &LineSummary) -> Vec<StruggleReason> {
    let t = STRUGGLE_THRESHOLDS;
    let mut out = Vec::new();
    if line.games < t.min_games {
        return out;
    }
    let below = line.perf_games >= t.min_games
        && line
            .perf_vs_expected
            .is_some_and(|p| p <= t.perf_drop + EPS)
        && line.perf_interval.is_some_and(|iv| iv.high < 0.0);
    if below {
        out.push(StruggleReason::ScoreBelowExpected);
    }
    let worse = line.analysed_games >= t.min_analysed
        && line
            .avg_win_at_opening_end
            .is_some_and(|w| w <= t.worse_win + EPS);
    if worse {
        out.push(StruggleReason::WorseAfterOpening);
    }
    let repeated = line.error_spot.as_ref().is_some_and(|s| {
        s.games >= t.deviation_min_games
            && f64::from(s.games) / f64::from(line.games) >= t.deviation_share - EPS
    });
    if repeated {
        out.push(StruggleReason::RepeatedDeviationError);
    }
    out
}

/// The verdict and its reason (TS `decideVerdict`), rules in order:
/// 1. games < 8 → `None`;
/// 2. a repeated deviation error → `Fix`;
/// 3. score below expected, ≥ 15 games, perf ≤ −0.15 and a chosen line → `Replace`
///    (a faced line is never replaced);
/// 4. a chosen line with a median player deviation ply ≤ 10 and ≥ 10% of the colour's games
///    → `Learn`;
/// 5. any other struggle → `Learn`;
/// 6. otherwise `Keep`.
pub fn decide_verdict(
    line: &LineSummary,
    struggles: &[StruggleReason],
    color_games: u32,
    alternatives: &[AlternativeMove],
) -> (Option<Verdict>, String) {
    let t = STRUGGLE_THRESHOLDS;
    let n = line.games;
    if n < t.min_games {
        return (
            None,
            format!(
                "{n} games in this line; a verdict needs at least {}",
                t.min_games
            ),
        );
    }
    if struggles.contains(&StruggleReason::RepeatedDeviationError) {
        let spot = line.error_spot.as_ref();
        let reason = match spot {
            Some(s) => format!(
                "loses at least {} win% with {} in {} of {n} games",
                t.deviation_delta_w, s.san, s.games
            ),
            None => "repeated error at one spot".to_owned(),
        };
        return (Some(Verdict::Fix), reason);
    }
    let perf = line.perf_vs_expected;
    if struggles.contains(&StruggleReason::ScoreBelowExpected)
        && n >= REPLACE_MIN_GAMES
        && perf.is_some_and(|p| p <= REPLACE_MAX_PERF + EPS)
        && line.kind == LineSide::Chosen
    {
        let label = comparison_label(&line.comparison);
        let alts = if line.comparison.explorer_band.is_none() {
            format!(
                "no explorer data within {EXPLORER_MAX_FALLBACK_BANDS} bands of {} to suggest alternatives",
                line.comparison.player_band
            )
        } else if alternatives.is_empty() {
            format!("no alternative with {ALTERNATIVE_MIN_TOTAL}+ games among {label}")
        } else {
            let list: Vec<String> = alternatives
                .iter()
                .map(|a| format!("{} ({:.2} over {} games)", a.san, a.band_score, a.games))
                .collect();
            format!("alternatives among {label}: {}", list.join(", "))
        };
        return (
            Some(Verdict::Replace),
            format!(
                "scores {:+.2} against expectation over {} rated games; {alts}",
                perf.unwrap_or(0.0),
                line.perf_games
            ),
        );
    }
    let share = if color_games == 0 {
        0.0
    } else {
        f64::from(n) / f64::from(color_games)
    };
    if line.kind == LineSide::Chosen
        && line
            .median_deviation_ply
            .is_some_and(|m| m <= LEARN_MAX_DEVIATION_PLY)
        && share >= LEARN_MIN_SHARE - EPS
    {
        return (
            Some(Verdict::Learn),
            format!(
                "usually leaves the theory of {} by ply {} in a line with {:.0}% of these games",
                comparison_label(&line.comparison),
                line.median_deviation_ply.unwrap_or(0.0),
                share * 100.0
            ),
        );
    }
    if !struggles.is_empty() {
        let list: Vec<&str> = struggles.iter().map(|s| s.as_str()).collect();
        return (
            Some(Verdict::Learn),
            format!("struggles: {}", list.join(", ")),
        );
    }
    (
        Some(Verdict::Keep),
        format!("no struggle over {n} games (score {:.2})", line.score),
    )
}
