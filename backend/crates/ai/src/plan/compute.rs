//! The plan context's arithmetic, ported from the web so the coach quotes what the cards show:
//! the focus progress of `apps/web/src/server/focus/goal.ts` (ADR 0091) and the "Is it working?"
//! rules of `apps/web/src/server/train/model.ts` (ADR 0090). Pure, tested apart from the database.

use std::collections::{BTreeMap, BTreeSet};

use time::OffsetDateTime;

use super::types::{EfficacyVerdict, FocusOutcome, FocusStatus, PlanRate};

/// The two-sided 80% normal quantile (web `Z[0.8]`).
pub const Z80: f64 = 1.281_551_565_544_600_4;
/// Chances before a focus gets a verdict (web `MIN_VERDICT_CHANCES`).
pub const MIN_VERDICT_CHANCES: u32 = 10;
/// Chances on each side before "Is it working?" claims anything (web `MIN_CHANCES`).
pub const MIN_EFFICACY_CHANCES: u32 = 30;

/// The trainer's sources (web `SOURCES`), most specific first.
pub const SOURCES: [&str; 5] = ["W4.missed", "W3.shallow", "W3.deep", "W7.traps", "blunder"];

/// The 80% Wilson score interval of `k` successes in `n` (web `wilson`); `None` when n is 0.
pub fn wilson80(k: f64, n: f64) -> Option<(f64, f64)> {
    if !n.is_finite() || !k.is_finite() || n <= 0.0 {
        return None;
    }
    let s = k.clamp(0.0, n);
    let z2 = Z80 * Z80;
    let p = s / n;
    let denom = 1.0 + z2 / n;
    let centre = (p + z2 / (2.0 * n)) / denom;
    let half = (Z80 * ((p * (1.0 - p)) / n + z2 / (4.0 * n * n)).sqrt()) / denom;
    let low = if s == 0.0 {
        0.0
    } else {
        (centre - half).max(0.0)
    };
    let high = if s == n {
        1.0
    } else {
        (centre + half).min(1.0)
    };
    Some((low, high))
}

/// A rate per chance with its 80% range (web `rateOf`).
pub fn rate_of(events: u32, chances: u32) -> PlanRate {
    let ci = wilson80(f64::from(events), f64::from(chances));
    PlanRate {
        events,
        chances,
        rate: (chances > 0).then(|| f64::from(events) / f64::from(chances)),
        low: ci.map(|c| c.0),
        high: ci.map(|c| c.1),
    }
}

/// A claim only when both sides have [`MIN_EFFICACY_CHANCES`] and the ranges separate (web
/// `verdictOf`).
pub fn verdict_of(before: &PlanRate, after: &PlanRate) -> EfficacyVerdict {
    if before.chances < MIN_EFFICACY_CHANCES || after.chances < MIN_EFFICACY_CHANCES {
        return EfficacyVerdict::TooEarly;
    }
    match (before.low, before.high, after.low, after.high) {
        (Some(bl), Some(_), Some(_), Some(ah)) if ah < bl => EfficacyVerdict::Lower,
        (Some(_), Some(bh), Some(al), Some(_)) if al > bh => EfficacyVerdict::Higher,
        (Some(_), Some(_), Some(_), Some(_)) => EfficacyVerdict::NoClearChange,
        _ => EfficacyVerdict::TooEarly,
    }
}

/// One game's events and chances of a weakness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameCount {
    pub played_at: OffsetDateTime,
    pub events: u32,
    pub chances: u32,
}

/// The rate in games before `first_drill` and from it on (web `beforeAfter`). Without a drill
/// every game is "before".
pub fn before_after(
    games: &[GameCount],
    first_drill: Option<OffsetDateTime>,
) -> (PlanRate, PlanRate, EfficacyVerdict) {
    let (mut be, mut bn, mut ae, mut an) = (0u32, 0u32, 0u32, 0u32);
    for g in games {
        if first_drill.is_some_and(|f| g.played_at >= f) {
            ae = ae.saturating_add(g.events);
            an = an.saturating_add(g.chances);
        } else {
            be = be.saturating_add(g.events);
            bn = bn.saturating_add(g.chances);
        }
    }
    let before = rate_of(be, bn);
    let after = rate_of(ae, an);
    let verdict = verdict_of(&before, &after);
    (before, after, verdict)
}

/// One game's chances and misses per motif (`game_metrics.metrics.missedByMotif`).
#[derive(Clone, Debug, PartialEq)]
pub struct MotifGame {
    pub played_at: OffsetDateTime,
    /// tag → (opportunities, misses)
    pub by_motif: BTreeMap<String, (f64, f64)>,
}

/// How often the player found any of `tags` in games from `since` on (web `patternFound`):
/// `events` counts the chances found.
pub fn pattern_found(
    games: &[MotifGame],
    tags: &BTreeSet<String>,
    since: OffsetDateTime,
) -> PlanRate {
    let (mut found, mut chances) = (0f64, 0f64);
    for g in games.iter().filter(|g| g.played_at >= since) {
        for tag in tags {
            let Some((opp, miss)) = g.by_motif.get(tag) else {
                continue;
            };
            if !opp.is_finite() || !miss.is_finite() {
                continue;
            }
            let n = opp.max(0.0);
            chances += n;
            found += (n - n.min(miss.max(0.0))).max(0.0);
        }
    }
    let as_u32 = |x: f64| x.round().clamp(0.0, f64::from(u32::MAX)) as u32;
    rate_of(as_u32(found), as_u32(chances))
}

/// A focus's progress against its goal (web `focusProgress`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Progress {
    pub rate: PlanRate,
    pub games_played: u32,
    pub status: FocusStatus,
    pub more_chances: Option<u32>,
    pub outcome: Option<FocusOutcome>,
}

/// Below [`MIN_VERDICT_CHANCES`] it is too early to tell (and a closed focus inconclusive);
/// otherwise the rate is held against the target.
pub fn focus_progress(k: u32, n: u32, games: u32, games_n: u32, target: f64) -> Progress {
    let k = k.min(n);
    let rate = rate_of(k, n);
    let played = games.min(games_n);
    let done = played >= games_n;
    let enough = n >= MIN_VERDICT_CHANCES;
    let below = enough && rate.rate.is_some_and(|r| r <= target);
    let status = if !enough {
        FocusStatus::TooEarly
    } else if below {
        FocusStatus::OnTrack
    } else {
        FocusStatus::NotYet
    };
    let outcome = done.then_some(if !enough {
        FocusOutcome::Inconclusive
    } else if below {
        FocusOutcome::Met
    } else {
        FocusOutcome::NotMet
    });
    Progress {
        rate,
        games_played: played,
        status,
        more_chances: (!enough).then(|| MIN_VERDICT_CHANCES - n),
        outcome,
    }
}

/// The catalogue's plain names of the weakness series (web `catalogue.ts` `WEAKNESSES`).
const WEAKNESS_LABELS: [(&str, &str); 19] = [
    ("W1", "Leaving pieces hanging"),
    ("W2", "Missing your opponent's threat"),
    ("W3.shallow", "Missing a short tactic"),
    ("W3.deep", "Missing a longer tactic"),
    ("W4.missed", "Missing a mate"),
    ("W4.allowed", "Allowing a mate"),
    ("W5", "Not winning won games"),
    ("W6", "Defending lost positions"),
    ("W7.traps", "Early blunders"),
    ("W7.prep", "Leaving the opening worse"),
    ("W8.basics", "Basic endgames"),
    ("W8.advanced", "Harder endgames"),
    ("W9a", "Getting short of time"),
    ("W9b", "Blunders on a low clock"),
    ("W9c", "Rushing important moves"),
    ("W9d", "How you spend your time"),
    ("W10", "Drifting in quiet positions"),
    ("W11", "Mistakes on easy moves"),
    ("W12", "Tilt and tiredness"),
];

/// The plain name of a weakness series, when the catalogue has one.
pub fn weakness_label(series_key: &str) -> Option<&'static str> {
    WEAKNESS_LABELS
        .iter()
        .find(|(k, _)| *k == series_key)
        .map(|(_, l)| *l)
}

/// The trainer's name of a source (web `SOURCE_LABEL`).
pub fn source_label(source: &str) -> String {
    if source == "blunder" {
        return "Blunders".to_owned();
    }
    weakness_label(source).map_or_else(|| source.to_owned(), str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wilson_matches_the_web() {
        // Web `wilson(14, 100)`: { low: 0.10133…, high: 0.19031… }.
        let (lo, hi) = wilson80(14.0, 100.0).unwrap();
        assert!((lo - 0.101_33).abs() < 1e-4, "{lo}");
        assert!((hi - 0.190_31).abs() < 1e-4, "{hi}");
        assert_eq!(wilson80(0.0, 5.0).unwrap().0, 0.0);
        assert_eq!(wilson80(5.0, 5.0).unwrap().1, 1.0);
        assert!(wilson80(0.0, 0.0).is_none());
    }

    #[test]
    fn a_focus_is_too_early_below_ten_chances() {
        let p = focus_progress(1, 9, 6, 20, 0.11);
        assert_eq!(p.status, FocusStatus::TooEarly);
        assert_eq!(p.more_chances, Some(1));
        assert_eq!(p.outcome, None);
        let p = focus_progress(1, 10, 20, 20, 0.11);
        assert_eq!(p.status, FocusStatus::OnTrack);
        assert_eq!(p.outcome, Some(FocusOutcome::Met));
        let p = focus_progress(3, 10, 8, 20, 0.11);
        assert_eq!(p.status, FocusStatus::NotYet);
        let p = focus_progress(0, 4, 25, 20, 0.11);
        assert_eq!(p.games_played, 20);
        assert_eq!(p.outcome, Some(FocusOutcome::Inconclusive));
    }

    #[test]
    fn efficacy_claims_only_separated_ranges() {
        assert_eq!(
            verdict_of(&rate_of(10, 29), &rate_of(1, 100)),
            EfficacyVerdict::TooEarly
        );
        assert_eq!(
            verdict_of(&rate_of(40, 100), &rate_of(10, 100)),
            EfficacyVerdict::Lower
        );
        assert_eq!(
            verdict_of(&rate_of(14, 100), &rate_of(11, 100)),
            EfficacyVerdict::NoClearChange
        );
        assert_eq!(
            verdict_of(&rate_of(10, 100), &rate_of(40, 100)),
            EfficacyVerdict::Higher
        );
    }

    #[test]
    fn before_after_splits_at_the_first_drill() {
        let t = |d: i64| OffsetDateTime::UNIX_EPOCH + time::Duration::days(d);
        let games = [
            GameCount {
                played_at: t(1),
                events: 2,
                chances: 10,
            },
            GameCount {
                played_at: t(5),
                events: 1,
                chances: 10,
            },
        ];
        let (b, a, _) = before_after(&games, Some(t(5)));
        assert_eq!((b.events, b.chances, a.events, a.chances), (2, 10, 1, 10));
        let (b, a, _) = before_after(&games, None);
        assert_eq!((b.chances, a.chances), (20, 0));
        assert_eq!(a.rate, None);
    }

    #[test]
    fn labels_name_sources_plainly() {
        assert_eq!(source_label("W4.missed"), "Missing a mate");
        assert_eq!(source_label("blunder"), "Blunders");
        assert_eq!(weakness_label("motif.missed:fork"), None);
    }
}
