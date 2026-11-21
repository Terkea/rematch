//! Tier selection rules (R2 Task 26). Pure: values exactly TS M2 Task 23.
//!
//! Candidates are games with `status == Pending` and no tier yet (the `Imported` status
//! arrives with Task 3; Task 29 maps it onto `tier.is_none()`), and `ply_count >= 10` or unknown.

use std::collections::{HashMap, HashSet};

use chess_core::TimeClass;
use db::games::AnalysisStatus;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::priority::job_priority;

pub const P2_PER_MONTH: usize = 15;
/// Games more than this many calendar months old (by month difference) are never tier 2.
pub const P2_MAX_MONTHS: i32 = 36;
pub const P1_MIN_RECENT: usize = 20;
pub const RECENT_DAYS: i64 = 90;
pub const MIN_PLIES: i32 = 10;
pub const BATCH_SIZE: usize = 25;
pub const PREVIEW_GAMES: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plan {
    Free,
    Pro,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TierLimits {
    pub import_max: usize,
    pub p0: usize,
    pub p1: usize,
    pub p2: bool,
    pub p3_per_week: usize,
}

pub fn tier_limits(plan: Plan) -> TierLimits {
    match plan {
        Plan::Free => TierLimits {
            import_max: 1000,
            p0: 50,
            p1: 0,
            p2: false,
            p3_per_week: 20,
        },
        Plan::Pro => TierLimits {
            import_max: 10_000,
            p0: 200,
            p1: 100,
            p2: true,
            p3_per_week: 300,
        },
    }
}

#[derive(Clone, Debug)]
pub struct CandidateGame {
    pub id: Uuid,
    pub time_class: TimeClass,
    pub played_at: OffsetDateTime,
    pub created_at: OffsetDateTime,
    pub rated: bool,
    pub ply_count: Option<i32>,
    pub has_lichess_evals: bool,
    pub status: AnalysisStatus,
    pub tier: Option<i16>,
}

pub struct SelectInput<'a> {
    pub games: &'a [CandidateGame],
    pub plan: Plan,
    pub now: OffsetDateTime,
    pub first_scheduled_at: OffsetDateTime,
    pub p3_selected_last_7d: u32,
    pub stopped: &'a HashSet<TimeClass>,
    pub in_flight_backfill: &'a HashMap<TimeClass, u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selected {
    pub game_id: Uuid,
    pub tier: u8,
    pub priority: i16,
}

/// Longer time classes rank higher (tie-break for the primary).
fn length_rank(tc: TimeClass) -> u8 {
    match tc {
        TimeClass::Bullet => 0,
        TimeClass::Blitz => 1,
        TimeClass::Rapid => 2,
        TimeClass::Classical => 3,
        TimeClass::Daily => 4,
    }
}

fn counts_since(
    games: &[CandidateGame],
    now: OffsetDateTime,
    cutoff: Option<OffsetDateTime>,
) -> HashMap<TimeClass, usize> {
    let mut m = HashMap::new();
    for g in games
        .iter()
        .filter(|g| g.played_at <= now && cutoff.is_none_or(|c| g.played_at >= c))
    {
        *m.entry(g.time_class).or_insert(0) += 1;
    }
    m
}

/// Counts over the first non-empty window: 90 days, else 365 days, else all time.
/// The flag says whether the 90-day window had any game (an active account).
fn window_counts(
    games: &[CandidateGame],
    now: OffsetDateTime,
) -> (HashMap<TimeClass, usize>, bool) {
    for (days, active) in [(RECENT_DAYS, true), (365, false)] {
        let m = counts_since(games, now, Some(now - Duration::days(days)));
        if !m.is_empty() {
            return (m, active);
        }
    }
    (counts_since(games, now, None), false)
}

/// The time class with the most games in the last 90 days; ties go to the longer one.
/// A dormant account (nothing in 90 days) falls back to 365 days, then to all time.
/// Future-dated games are ignored.
pub fn primary_time_class(games: &[CandidateGame], now: OffsetDateTime) -> Option<TimeClass> {
    window_counts(games, now)
        .0
        .into_iter()
        .max_by_key(|(tc, n)| (*n, length_rank(*tc)))
        .map(|(tc, _)| tc)
}

/// The primary plus every time class with at least `P1_MIN_RECENT` recent games, ordered by
/// recent count then length (descending). A dormant account has only its primary.
pub fn active_time_classes(games: &[CandidateGame], now: OffsetDateTime) -> Vec<TimeClass> {
    let primary = primary_time_class(games, now);
    let (counts, recent) = window_counts(games, now);
    let mut v: Vec<(TimeClass, usize)> = counts
        .into_iter()
        .filter(|(tc, n)| Some(*tc) == primary || (recent && *n >= P1_MIN_RECENT))
        .collect();
    v.sort_by_key(|(tc, n)| std::cmp::Reverse((*n, length_rank(*tc))));
    v.into_iter().map(|(tc, _)| tc).collect()
}

fn month_index(t: OffsetDateTime) -> i32 {
    let t = t.to_offset(time::UtcOffset::UTC);
    t.year() * 12 + i32::from(u8::from(t.month())) - 1
}

/// Rated before casual, then most recent first.
fn order(a: &&CandidateGame, b: &&CandidateGame) -> std::cmp::Ordering {
    (!a.rated, std::cmp::Reverse(a.played_at), a.id).cmp(&(
        !b.rated,
        std::cmp::Reverse(b.played_at),
        b.id,
    ))
}

/// `k` evenly spaced picks (chronologically) from a newest-first slice; all when `k >= len`.
fn spaced<'a>(pool: &[&'a CandidateGame], k: usize) -> Vec<&'a CandidateGame> {
    let n = pool.len();
    if n <= k {
        return pool.to_vec();
    }
    let asc: Vec<&CandidateGame> = pool.iter().rev().copied().collect();
    (0..k).map(|j| asc[j * n / k]).collect()
}

fn taken_contains(out: &[Selected], id: Uuid) -> bool {
    out.iter().any(|s| s.game_id == id)
}

fn push(out: &mut Vec<Selected>, plan: Plan, g: &CandidateGame, tier: u8) {
    out.push(Selected {
        game_id: g.id,
        tier,
        priority: job_priority(tier, plan, g.has_lichess_evals),
    });
}

pub fn select_for_analysis(i: &SelectInput) -> Vec<Selected> {
    let limits = tier_limits(i.plan);
    let Some(primary) = primary_time_class(i.games, i.now) else {
        return Vec::new();
    };
    let active = active_time_classes(i.games, i.now);
    // Bullet is only analysed when it is the primary time class.
    let allowed = |tc: TimeClass| tc != TimeClass::Bullet || tc == primary;

    let mut cands: Vec<&CandidateGame> = i
        .games
        .iter()
        .filter(|g| {
            g.status == AnalysisStatus::Pending
                && g.tier.is_none()
                && g.ply_count.is_none_or(|p| p >= MIN_PLIES)
                && g.played_at <= i.now
                && allowed(g.time_class)
        })
        .collect();
    let future = i
        .games
        .iter()
        .filter(|g| g.played_at > i.now && g.status == AnalysisStatus::Pending && g.tier.is_none())
        .count();
    if future > 0 {
        tracing::warn!(future, "skipping future-dated games in tier selection");
    }
    cands.sort_by(order);

    let mut out: Vec<Selected> = Vec::new();
    let plan = i.plan;
    let existing = |tc: TimeClass, tier: i16| {
        i.games
            .iter()
            .filter(|g| g.time_class == tc && g.tier == Some(tier))
            .count()
    };

    // P0: most recent primary games up to the plan limit, counting earlier selections.
    let room = limits.p0.saturating_sub(existing(primary, 0));
    let p0: Vec<&CandidateGame> = cands
        .iter()
        .filter(|g| g.time_class == primary)
        .take(room)
        .copied()
        .collect();
    for g in p0 {
        push(&mut out, plan, g, 0);
    }

    // P3: games created after the first scheduling run, within the weekly budget.
    let budget = limits
        .p3_per_week
        .saturating_sub(i.p3_selected_last_7d as usize);
    let p3: Vec<&CandidateGame> = cands
        .iter()
        .filter(|g| {
            !taken_contains(&out, g.id)
                && g.rated
                && g.created_at > i.first_scheduled_at
                && active.contains(&g.time_class)
        })
        .take(budget)
        .copied()
        .collect();
    for g in p3 {
        push(&mut out, plan, g, 3);
    }

    // P1 and P2 backfill, at most BATCH_SIZE per time class per run.
    let now_month = month_index(i.now);
    for &tc in &active {
        if !allowed(tc)
            || i.stopped.contains(&tc)
            || i.in_flight_backfill.get(&tc).copied().unwrap_or(0) > 0
        {
            continue;
        }
        let mut batch = BATCH_SIZE;
        if tc != primary && limits.p1 > 0 {
            let room = limits.p1.saturating_sub(existing(tc, 1)).min(batch);
            let picks: Vec<&CandidateGame> = cands
                .iter()
                .filter(|g| g.time_class == tc && !taken_contains(&out, g.id))
                .take(room)
                .copied()
                .collect();
            batch -= picks.len();
            for g in picks {
                push(&mut out, plan, g, 1);
            }
        }
        if !limits.p2 || batch == 0 {
            continue;
        }
        // Calendar months, newest first.
        let mut months: Vec<i32> = cands
            .iter()
            .filter(|g| g.time_class == tc && !taken_contains(&out, g.id))
            .map(|g| month_index(g.played_at))
            .filter(|m| now_month - m <= P2_MAX_MONTHS)
            .collect();
        months.sort_unstable_by_key(|m| std::cmp::Reverse(*m));
        months.dedup();
        for m in months {
            if batch == 0 {
                break;
            }
            // Every analysed game of the month counts, whatever its tier, plus this run's picks.
            let have = i
                .games
                .iter()
                .filter(|g| {
                    g.time_class == tc
                        && month_index(g.played_at) == m
                        && (g.tier.is_some() || taken_contains(&out, g.id))
                })
                .count();
            let k = P2_PER_MONTH.saturating_sub(have).min(batch);
            if k == 0 {
                continue;
            }
            let pool: Vec<&CandidateGame> = cands
                .iter()
                .filter(|g| {
                    g.time_class == tc
                        && !taken_contains(&out, g.id)
                        && month_index(g.played_at) == m
                })
                .copied()
                .collect();
            // Rated first; casual only fills what rated cannot. Pool is newest first, so
            // reverse for chronological spacing, then emit newest first again.
            let (rated, casual): (Vec<_>, Vec<_>) = pool.into_iter().partition(|g| g.rated);
            let mut picks = spaced(&rated, k);
            if picks.len() < k {
                picks.extend(spaced(&casual, k - picks.len()));
            }
            picks.sort_by_key(|g| (std::cmp::Reverse(g.played_at), g.id));
            batch -= picks.len();
            for g in picks {
                push(&mut out, plan, g, 2);
            }
        }
    }
    out
}
