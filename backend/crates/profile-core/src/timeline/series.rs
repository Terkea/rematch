//! Weakness series evaluation: states, confirmed changes and onset rows (R2 Task 14, TS M2
//! Task 18). The rules are recorded in ADR-R2-6.
//!
//! [`evaluate_series`] replays a series in game order (`played_at`, then `game_id`) and keeps a
//! *current segment*, the games since the last confirmed change. Per game it:
//! 1. steps a streaming [`Bocpd`] (hazard 1/200, prior `Beta(50 mu, 50 (1 - mu))`);
//! 2. looks for a change inside the current segment: run lengths `1 <= r < limit` with
//!    `limit = min(50, games in segment - 10)`. The onset is the MAP run restricted to that
//!    range, and its bounds come from the smallest highest-mass subset of it covering 0.8,
//!    holding at least two runs so the window is never zero-width;
//! 3. confirms that change only when `p_change >= 0.8`, the relative rate change is at least
//!    0.15 and the games since the onset hold at least `min_evidence` opportunities. A
//!    confirmation starts a new segment at the onset game (so CUSUM resets) and appends an
//!    onset row;
//! 4. computes the [`Signals`] on the current segment and applies [`next_state`].
//!
//! Honesty rules:
//! - `p_change`, `rate_before` and `rate_after` appear only on the rows of a confirmed change,
//!   i.e. after the evidence gate. BOCPD's raw `p_change` includes the prior hazard mass (about
//!   0.22 at window 50 with zero evidence), so it is never reported on its own.
//! - Below `min_evidence` opportunities in the segment, `confidence_above` is 0 and the P40
//!   streak does not count; a segment without events also has confidence 0.
//! - Points with `events > opportunities` are skipped and counted in `invalid_points`.
//! - No reference (or an invalid one: a value outside `[0, 1]` or non-finite) means
//!   `not_an_issue` only. A non-finite or negative `n0` is 0; a NaN `min_evidence` can never be
//!   met; a negative one is 0.
//! - Every reported float is finite; the output is a pure function of the input set.

use time::OffsetDateTime;
use uuid::Uuid;

use super::bocpd::{
    Bocpd, BocpdOpts, DEFAULT_HAZARD, DEFAULT_LEVEL, DEFAULT_MAX_RUN, change_summary,
};
use super::cusum::{CUSUM_DEFAULT_H, cusum_counted};
use super::ewma::{EWMA_DEFAULT_K, EWMA_DEFAULT_LAMBDA, ewma_drift_counted};
use super::states::{Signals, next_state};
use crate::stats::posterior::{
    BetaPosterior, P_IMP_DELTA_REL, P_IMP_DRAWS, P_IMP_SEED, SHAPE_FLOOR, p_improvement, posterior,
    prob_above,
};
use crate::types::WeaknessState;

/// BOCPD change window in games (must stay at or below 100: the prior hazard mass alone is 0.39
/// at 100 and 0.78 at 300).
pub const SERIES_WINDOW_GAMES: usize = 50;
/// Games of the current segment that must precede a change.
pub const SERIES_MIN_PRIOR_GAMES: usize = 10;
/// Strength of the BOCPD Beta prior around the pooled rate (`a = 50 mu`, `b = 50 (1 - mu)`).
pub const BOCPD_PRIOR_STRENGTH: f64 = 50.0;
/// `p_change` needed to confirm a change.
pub const CONFIRM_P_CHANGE: f64 = 0.8;
/// `|rate_after / rate_before - 1|` needed to confirm a change.
pub const CONFIRM_REL_CHANGE: f64 = 0.15;
/// CUSUM watches for `p1 = 1.5 p0`.
pub const CUSUM_FACTOR: f64 = 1.5;
/// Games in the current segment before CUSUM runs.
pub const CUSUM_MIN_GAMES: usize = 10;
/// Fewest games an onset window spans: the 0.8 highest-mass set is widened by the next most
/// likely run when one run alone covers it, so the window is never zero-width.
pub const MIN_ONSET_WINDOW_GAMES: usize = 2;

/// One game of a weakness series.
#[derive(Clone, Debug, PartialEq)]
pub struct SeriesPoint {
    pub game_id: Uuid,
    pub played_at: OffsetDateTime,
    pub events: u32,
    pub opportunities: u32,
    pub protocol: String,
}

/// `state_row_kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RowKind {
    /// The state changed (or the series' first row).
    Transition,
    /// A confirmed change, dated to its onset game.
    Onset,
}

impl RowKind {
    /// The `state_row_kind` text.
    pub fn as_str(self) -> &'static str {
        match self {
            RowKind::Transition => "transition",
            RowKind::Onset => "onset",
        }
    }
}

/// Onset of a confirmed change: the first game of the new segment and its uncertainty window.
#[derive(Clone, Debug, PartialEq)]
pub struct Onset {
    pub game_id: Uuid,
    pub played_at: OffsetDateTime,
    pub low: OffsetDateTime,
    pub high: OffsetDateTime,
}

/// Counts behind a row (`weakness_state_history.evidence`).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Evidence {
    /// Events in the current segment (onset rows: since the onset).
    pub k: u64,
    /// Opportunities in the current segment (onset rows: since the onset).
    pub n: u64,
    /// `P(theta > band median)`; 0 below min evidence or without events.
    pub confidence: f64,
}

/// One `weakness_state_history` row.
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineRow {
    pub kind: RowKind,
    /// `None` only on the series' first row. On an onset row: the state before the onset game.
    pub from: Option<WeaknessState>,
    pub to: WeaknessState,
    pub at_game_id: Uuid,
    pub at_played_at: OffsetDateTime,
    pub onset: Option<Onset>,
    /// Only with a confirmed change (evidence gate met).
    pub p_change: Option<f64>,
    pub rate_before: Option<f64>,
    pub rate_after: Option<f64>,
    pub evidence: Evidence,
}

/// Band reference of the series' weakness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeriesRef {
    pub median: f64,
    pub p40: f64,
    pub pooled_rate: f64,
}

/// Result of [`evaluate_series`].
#[derive(Clone, Debug, PartialEq)]
pub struct SeriesOutcome {
    pub state: WeaknessState,
    pub rows: Vec<TimelineRow>,
    /// `P_imp` of the current segment against the one before the last confirmed change.
    pub p_imp: Option<f64>,
    /// Points skipped because `events > opportunities`.
    pub invalid_points: u32,
}

fn valid_ref(r: &SeriesRef) -> bool {
    [r.median, r.p40, r.pooled_rate]
        .iter()
        .all(|v| (0.0..=1.0).contains(v))
}

/// The last confirmed change: counts of the segment before it (fixed).
#[derive(Clone, Copy)]
struct Change {
    k_before: u64,
    n_before: u64,
}

/// A change confirmed at this game: candidate, `p_change`, `(k, n)` before and since the onset.
type Confirmed = (Candidate, f64, (u64, u64), (u64, u64));

/// A candidate change inside the current segment (game indices, 0-based).
struct Candidate {
    onset: usize,
    low: usize,
    high: usize,
}

/// Restricted MAP onset and bounds over runs `1 <= r < limit` (excluding the capped bucket).
fn candidate(post: &[f64], t: usize, limit: usize, level: f64) -> Option<Candidate> {
    let capped = (post.len() <= t).then(|| post.len() - 1);
    let mut runs: Vec<(usize, f64)> = (1..limit.min(post.len()))
        .filter(|r| Some(*r) != capped)
        .map(|r| (r, post[r]))
        .filter(|(_, p)| p.is_finite() && *p > 0.0)
        .collect();
    // The onset window must span at least two games (never zero-width), so a change needs at
    // least two candidate runs; until then nothing is confirmed.
    if runs.len() < MIN_ONSET_WINDOW_GAMES {
        return None;
    }
    let total: f64 = runs.iter().map(|(_, p)| p).sum();
    // Highest mass first; ties prefer the longer run (the earlier onset, fewer claimed games).
    runs.sort_by(|x, y| y.1.total_cmp(&x.1).then(y.0.cmp(&x.0)));
    let map = runs[0].0;
    let (mut r_min, mut r_max, mut cum) = (map, map, 0.0);
    for (taken, &(r, p)) in runs.iter().enumerate() {
        r_min = r_min.min(r);
        r_max = r_max.max(r);
        cum += p;
        // Adding the next most likely run only raises coverage above `level` (conservative).
        if taken + 1 >= MIN_ONSET_WINDOW_GAMES && cum >= level * total * (1.0 - 1e-12) {
            break;
        }
    }
    // Run r after game t: the segment began at game t - r + 1 (1-based), index t - r.
    Some(Candidate {
        onset: t - map,
        low: t - r_max,
        high: t - r_min,
    })
}

fn rate(k: u64, n: u64) -> Option<f64> {
    (n > 0).then(|| k as f64 / n as f64)
}

/// Replays `points` and returns the timeline; see the module docs.
pub fn evaluate_series(
    points: &[SeriesPoint],
    reference: Option<&SeriesRef>,
    n0: f64,
    min_evidence: f64,
) -> SeriesOutcome {
    let mut pts: Vec<&SeriesPoint> = points
        .iter()
        .filter(|p| p.events <= p.opportunities)
        .collect();
    let invalid_points = u32::try_from(points.len() - pts.len()).unwrap_or(u32::MAX);
    pts.sort_by_key(|p| (p.played_at, p.game_id));
    let mut out = SeriesOutcome {
        state: WeaknessState::NotAnIssue,
        rows: Vec::new(),
        p_imp: None,
        invalid_points,
    };
    let Some(first) = pts.first() else {
        return out;
    };
    let Some(r) = reference.filter(|r| valid_ref(r)) else {
        out.rows.push(TimelineRow {
            kind: RowKind::Transition,
            from: None,
            to: WeaknessState::NotAnIssue,
            at_game_id: first.game_id,
            at_played_at: first.played_at,
            onset: None,
            p_change: None,
            rate_before: None,
            rate_after: None,
            evidence: Evidence {
                k: u64::from(first.events),
                n: u64::from(first.opportunities),
                confidence: 0.0,
            },
        });
        return out;
    };
    let n0 = if n0.is_finite() && n0 > 0.0 { n0 } else { 0.0 };
    let min_evidence = if min_evidence.is_nan() {
        f64::INFINITY
    } else {
        min_evidence.max(0.0)
    };
    let mu = r.pooled_rate;
    let opts = BocpdOpts {
        hazard: DEFAULT_HAZARD,
        prior_a: (BOCPD_PRIOR_STRENGTH * mu).max(SHAPE_FLOOR),
        prior_b: (BOCPD_PRIOR_STRENGTH * (1.0 - mu)).max(SHAPE_FLOOR),
        max_run: DEFAULT_MAX_RUN,
    };
    let Ok(mut det) = Bocpd::new(&opts) else {
        // Unreachable: the options are valid by construction. Claim nothing.
        return out;
    };

    let obs: Vec<(u32, u32)> = pts.iter().map(|p| (p.events, p.opportunities)).collect();
    // Prefix sums: events and opportunities of games [0, i).
    let mut cum_k = vec![0u64; obs.len() + 1];
    let mut cum_n = vec![0u64; obs.len() + 1];
    for (i, &(k, n)) in obs.iter().enumerate() {
        cum_k[i + 1] = cum_k[i] + u64::from(k);
        cum_n[i + 1] = cum_n[i] + u64::from(n);
    }
    let counts = |from: usize, to: usize| (cum_k[to] - cum_k[from], cum_n[to] - cum_n[from]);
    let post_of = |k: u64, n: u64| -> BetaPosterior { posterior(k as f64, n as f64, mu, n0) };

    let mut seg = 0usize; // index of the current segment's first game
    let mut last_change: Option<Change> = None;
    let mut states: Vec<WeaknessState> = Vec::with_capacity(obs.len());
    let mut streak = 0u32;
    // (index of the first game after entering `resolved`, level, sigma).
    let mut resolved_from: Option<(usize, f64, f64)> = None;
    let mut prev: Option<WeaknessState> = None;

    for t in 1..=obs.len() {
        let i = t - 1;
        let (k, n) = obs[i];
        let post = det.step(k, n);

        // 1-2. A change inside the current segment.
        let in_seg = t - seg;
        let limit = SERIES_WINDOW_GAMES.min(in_seg.saturating_sub(SERIES_MIN_PRIOR_GAMES));
        let mut confirmed: Option<Confirmed> = None;
        if limit >= 2 {
            let p_change = change_summary(
                post,
                t,
                SERIES_WINDOW_GAMES,
                seg + SERIES_MIN_PRIOR_GAMES,
                DEFAULT_LEVEL,
            )
            .p_change;
            if p_change >= CONFIRM_P_CHANGE
                && let Some(c) = candidate(post, t, limit, DEFAULT_LEVEL)
            {
                let before = counts(seg, c.onset);
                let after = counts(c.onset, t);
                if let (Some(rb), Some(ra)) = (rate(before.0, before.1), rate(after.0, after.1)) {
                    let rel = if rb > 0.0 {
                        (ra / rb - 1.0).abs()
                    } else if ra > 0.0 {
                        f64::INFINITY
                    } else {
                        0.0
                    };
                    if rel >= CONFIRM_REL_CHANGE && after.1 as f64 >= min_evidence {
                        confirmed = Some((c, p_change, before, after));
                    }
                }
            }
        }
        let (mut confirmed_up, mut confirmed_down) = (false, false);
        let mut onset_from: Option<WeaknessState> = None;
        if let Some((c, _, before, after)) = &confirmed {
            let (rb, ra) = (
                before.0 as f64 / before.1 as f64,
                after.0 as f64 / after.1 as f64,
            );
            confirmed_up = ra > rb;
            confirmed_down = ra < rb;
            // c.onset > seg >= 0, so the game before the onset exists.
            onset_from = states.get(c.onset - 1).copied();
            seg = c.onset;
            last_change = Some(Change {
                k_before: before.0,
                n_before: before.1,
            });
        }

        // 4. Signals on the current segment.
        let (ks, ns) = counts(seg, t);
        let sp = post_of(ks, ns);
        let evidence_met = ns as f64 >= min_evidence;
        let confidence = if evidence_met && ks > 0 {
            prob_above(&sp, r.median)
        } else {
            0.0
        };
        let at_or_below_median = ns == 0 || sp.mean <= r.median;
        streak = if evidence_met && sp.mean < r.p40 {
            streak.saturating_add(1)
        } else {
            0
        };
        let cusum_up = t - seg >= CUSUM_MIN_GAMES && {
            let p0 = sp.mean;
            let p1 = CUSUM_FACTOR * p0;
            p0 > 0.0
                && p1 < 1.0
                && cusum_counted(&obs[seg..t], p0, p1, CUSUM_DEFAULT_H)
                    .is_ok_and(|c| c.alarm_game.is_some())
        };
        let p_imp_now = |seg_post: &BetaPosterior| {
            last_change.map(|c| {
                let base = post_of(c.k_before, c.n_before);
                p_improvement(seg_post, &base, P_IMP_DELTA_REL, P_IMP_DRAWS, P_IMP_SEED)
            })
        };
        let tracked = matches!(
            prev,
            Some(WeaknessState::Active | WeaknessState::Emerging | WeaknessState::Relapsed)
        );
        // P_imp only matters to rule 3 (tracked states); skip the Monte Carlo otherwise.
        let p_imp = if tracked { p_imp_now(&sp) } else { None };
        let drift = match (prev, resolved_from) {
            (Some(WeaknessState::Resolved), Some((from, level, sigma))) => {
                let xs: Vec<f64> = obs[from..t]
                    .iter()
                    .map(|&(k, n)| {
                        if n > 0 {
                            f64::from(k) / f64::from(n)
                        } else {
                            f64::NAN
                        }
                    })
                    .collect();
                ewma_drift_counted(&xs, EWMA_DEFAULT_LAMBDA, level, sigma, EWMA_DEFAULT_K)
                    .is_ok_and(|e| e.alarm_game.is_some())
            }
            _ => false,
        };
        let signals = Signals {
            confirmed_up,
            confirmed_down,
            cusum_up,
            confidence_above: confidence,
            at_or_below_median,
            below_p40_streak: streak,
            p_imp,
            drift,
        };
        let next = next_state(prev, &signals);

        if next == WeaknessState::Resolved && prev != Some(WeaknessState::Resolved) {
            // EWMA from entering `resolved`: level = segment rate, sigma = per-game binomial SD.
            let games = (t - seg).max(1) as f64;
            let n_bar = (ns as f64 / games).max(1.0);
            let level = sp.mean;
            let sigma = (level * (1.0 - level) / n_bar).sqrt();
            resolved_from = Some((t, level, if sigma.is_finite() { sigma } else { 0.0 }));
        }

        let evidence = Evidence {
            k: ks,
            n: ns,
            confidence,
        };
        let change_fields = confirmed.as_ref().map(|(_, p, b, a)| {
            (
                p.clamp(0.0, 1.0),
                b.0 as f64 / b.1 as f64,
                a.0 as f64 / a.1 as f64,
            )
        });
        let point = pts[i];
        if prev.is_none() || prev != Some(next) {
            out.rows.push(TimelineRow {
                kind: RowKind::Transition,
                from: prev,
                to: next,
                at_game_id: point.game_id,
                at_played_at: point.played_at,
                onset: None,
                p_change: change_fields.map(|c| c.0),
                rate_before: change_fields.map(|c| c.1),
                rate_after: change_fields.map(|c| c.2),
                evidence,
            });
        }
        if let (Some((c, _, _, _)), Some((p, rb, ra))) = (&confirmed, change_fields) {
            out.rows.push(TimelineRow {
                kind: RowKind::Onset,
                from: onset_from,
                to: next,
                at_game_id: point.game_id,
                at_played_at: point.played_at,
                onset: Some(Onset {
                    game_id: pts[c.onset].game_id,
                    played_at: pts[c.onset].played_at,
                    low: pts[c.low].played_at,
                    high: pts[c.high].played_at,
                }),
                p_change: Some(p),
                rate_before: Some(rb),
                rate_after: Some(ra),
                evidence,
            });
        }
        states.push(next);
        prev = Some(next);
        if t == obs.len() {
            out.p_imp = p_imp_now(&sp);
        }
    }
    out.state = prev.unwrap_or(WeaknessState::NotAnIssue);
    out
}
