//! Candidate cause hints for a confirmed change (R2 Task 14, TS M2 Task 18).
//!
//! [`cause_hints`] compares the games before a change point with the games after it and
//! reports what else changed at the same time. These are correlates, never proven causes: the
//! descriptions state what changed ("Most games moved from 300+0 to 180+0") and never claim
//! that it caused anything. Kinds and triggers, in output order:
//! - `time_control_mix`: the dominant time control changes and the new one's share rises by at
//!   least 30 percentage points;
//! - `new_opening`: a line under 10% of games before and at least 30% after (the most played
//!   such line; ties by key);
//! - `session_length`: games per active (UTC) day up by at least 50%;
//! - `rating_level`: mean opponent rating (rated games only) shifts by at least 75;
//! - `protocol_version`: the sets of analysis protocols differ.
//!
//! Each side needs at least [`MIN_CAUSE_GAMES`] games (rated games for `rating_level`),
//! otherwise nothing is claimed. Shares are compared in exact integer arithmetic, the output
//! does not depend on input order, and every text fits the contract limits (description 600
//! chars, values 120 chars).

use std::collections::{BTreeMap, BTreeSet};

use coaching_input::{CandidateCause, CauseKind, CauseValue, LIMITS};
use time::{OffsetDateTime, UtcOffset};

/// Games each side needs before any hint is given.
pub const MIN_CAUSE_GAMES: usize = 5;
/// Opponent-rating shift that triggers `rating_level`.
pub const RATING_SHIFT: f64 = 75.0;
/// Longest name quoted inside a description.
const QUOTE_CHARS: usize = 200;

/// One game on either side of a change point.
#[derive(Clone, Debug, PartialEq)]
pub struct CauseGame {
    /// Time control, such as `"180+0"`.
    pub tc: String,
    pub opponent_rating: Option<i32>,
    pub line_key: Option<String>,
    pub played_at: OffsetDateTime,
    pub protocol: String,
}

fn cut(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn text(s: &str) -> CauseValue {
    CauseValue::Text(cut(s, LIMITS.label_chars))
}

fn hint(
    kind: CauseKind,
    description: String,
    before: CauseValue,
    after: CauseValue,
) -> CandidateCause {
    CandidateCause {
        kind,
        description: cut(&description, LIMITS.description_chars),
        before,
        after,
    }
}

fn round_to(x: f64, places: i32) -> f64 {
    let f = 10f64.powi(places);
    (x * f).round() / f
}

fn counts<'a>(games: impl Iterator<Item = &'a str>) -> BTreeMap<&'a str, u64> {
    let mut m = BTreeMap::new();
    for g in games {
        *m.entry(g).or_insert(0) += 1;
    }
    m
}

/// Most frequent key; ties go to the smallest key (BTreeMap order).
fn dominant<'a>(m: &BTreeMap<&'a str, u64>) -> Option<(&'a str, u64)> {
    m.iter()
        .fold(None, |best: Option<(&str, u64)>, (&k, &c)| match best {
            Some((_, bc)) if bc >= c => best,
            _ => Some((k, c)),
        })
}

fn time_control_mix(before: &[CauseGame], after: &[CauseGame]) -> Option<CandidateCause> {
    let cb = counts(before.iter().map(|g| g.tc.as_str()));
    let ca = counts(after.iter().map(|g| g.tc.as_str()));
    let (db, _) = dominant(&cb)?;
    let (da, a_count) = dominant(&ca)?;
    if db == da {
        return None;
    }
    let (nb, na) = (before.len() as u128, after.len() as u128);
    let b_count = u128::from(cb.get(da).copied().unwrap_or(0));
    // share_after - share_before >= 0.30, exactly: 10 (ca nb - cb na) >= 3 na nb.
    let lhs = 10 * u128::from(a_count) * nb;
    let rhs = 3 * na * nb + 10 * b_count * na;
    // "Most games" only for a majority (2 a > na); otherwise it is a plurality.
    let lead = if 2 * u128::from(a_count) > na {
        "Most games moved"
    } else {
        "The most played time control moved"
    };
    (lhs >= rhs).then(|| {
        hint(
            CauseKind::TimeControlMix,
            format!(
                "{lead} from {} to {}",
                cut(db, QUOTE_CHARS),
                cut(da, QUOTE_CHARS)
            ),
            text(db),
            text(da),
        )
    })
}

fn new_opening(before: &[CauseGame], after: &[CauseGame]) -> Option<CandidateCause> {
    let cb = counts(before.iter().filter_map(|g| g.line_key.as_deref()));
    let ca = counts(after.iter().filter_map(|g| g.line_key.as_deref()));
    let (nb, na) = (before.len() as u128, after.len() as u128);
    let (line, a_count) = ca
        .iter()
        .filter(|&(line, &c)| {
            let b = u128::from(cb.get(line).copied().unwrap_or(0));
            // before < 10% and after >= 30%.
            10 * b < nb && 10 * u128::from(c) >= 3 * na
        })
        .fold(None, |best: Option<(&str, u64)>, (&k, &c)| match best {
            Some((_, bc)) if bc >= c => best,
            _ => Some((k, c)),
        })?;
    let share_b = cb.get(line).copied().unwrap_or(0) as f64 / before.len() as f64;
    let share_a = a_count as f64 / after.len() as f64;
    Some(hint(
        CauseKind::NewOpening,
        format!(
            // One decimal, so a 9.5% share never prints as "10%" next to the "< 10%" rule.
            "Opening line {} rose from {:.1}% to {:.1}% of games",
            cut(line, QUOTE_CHARS),
            share_b * 100.0,
            share_a * 100.0
        ),
        CauseValue::Number(round_to(share_b, 4)),
        CauseValue::Number(round_to(share_a, 4)),
    ))
}

fn active_days(games: &[CauseGame]) -> usize {
    games
        .iter()
        .map(|g| g.played_at.to_offset(UtcOffset::UTC).date())
        .collect::<BTreeSet<_>>()
        .len()
}

fn session_length(before: &[CauseGame], after: &[CauseGame]) -> Option<CandidateCause> {
    let (db, da) = (active_days(before) as u128, active_days(after) as u128);
    let (nb, na) = (before.len() as u128, after.len() as u128);
    // na/da >= 1.5 nb/db, exactly: 2 na db >= 3 nb da.
    (db > 0 && da > 0 && 2 * na * db >= 3 * nb * da).then(|| {
        let (gb, ga) = (nb as f64 / db as f64, na as f64 / da as f64);
        hint(
            CauseKind::SessionLength,
            format!("Games per playing day rose from {gb:.1} to {ga:.1}"),
            CauseValue::Number(round_to(gb, 2)),
            CauseValue::Number(round_to(ga, 2)),
        )
    })
}

fn mean_rating(games: &[CauseGame]) -> Option<f64> {
    let rated: Vec<f64> = games
        .iter()
        .filter_map(|g| g.opponent_rating.map(f64::from))
        .collect();
    (rated.len() >= MIN_CAUSE_GAMES).then(|| rated.iter().sum::<f64>() / rated.len() as f64)
}

fn rating_level(before: &[CauseGame], after: &[CauseGame]) -> Option<CandidateCause> {
    let (rb, ra) = (mean_rating(before)?, mean_rating(after)?);
    ((ra - rb).abs() >= RATING_SHIFT).then(|| {
        let (rb, ra) = (rb.round(), ra.round());
        hint(
            CauseKind::RatingLevel,
            format!("Average opponent rating moved from {rb:.0} to {ra:.0}"),
            CauseValue::Number(rb),
            CauseValue::Number(ra),
        )
    })
}

fn protocol_version(before: &[CauseGame], after: &[CauseGame]) -> Option<CandidateCause> {
    let pb: BTreeSet<&str> = before.iter().map(|g| g.protocol.as_str()).collect();
    let pa: BTreeSet<&str> = after.iter().map(|g| g.protocol.as_str()).collect();
    if pb == pa {
        return None;
    }
    let join = |s: &BTreeSet<&str>| s.iter().copied().collect::<Vec<_>>().join(", ");
    let (jb, ja) = (join(&pb), join(&pa));
    Some(hint(
        CauseKind::ProtocolVersion,
        format!(
            "Analysis protocol changed from {} to {}",
            cut(&jb, QUOTE_CHARS),
            cut(&ja, QUOTE_CHARS)
        ),
        text(&jb),
        text(&ja),
    ))
}

/// Correlated changes between the games before and after a change point; see the module docs.
pub fn cause_hints(before: &[CauseGame], after: &[CauseGame]) -> Vec<CandidateCause> {
    if before.len() < MIN_CAUSE_GAMES || after.len() < MIN_CAUSE_GAMES {
        return Vec::new();
    }
    [
        time_control_mix(before, after),
        new_opening(before, after),
        session_length(before, after),
        rating_level(before, after),
        protocol_version(before, after),
    ]
    .into_iter()
    .flatten()
    .take(LIMITS.candidate_causes)
    .collect()
}
