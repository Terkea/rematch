use std::collections::{HashMap, HashSet};

use chess_core::TimeClass;
use db::games::AnalysisStatus;
use profile_core::types::WeaknessKey;
use scheduler::precision::{PrecisionInput, is_precise};
use scheduler::priority::{fair_tier, job_priority};
use scheduler::select::*;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

fn now() -> OffsetDateTime {
    time::macros::datetime!(2026-11-21 12:00 UTC)
}

fn game(tc: TimeClass, days_ago: i64) -> CandidateGame {
    CandidateGame {
        id: Uuid::new_v4(),
        time_class: tc,
        played_at: now() - Duration::days(days_ago),
        created_at: now() - Duration::days(days_ago),
        rated: true,
        ply_count: Some(60),
        has_lichess_evals: false,
        status: AnalysisStatus::Pending,
        tier: None,
    }
}

fn done(mut g: CandidateGame, tier: i16) -> CandidateGame {
    g.status = AnalysisStatus::Done;
    g.tier = Some(tier);
    g
}

struct Env {
    stopped: HashSet<TimeClass>,
    in_flight: HashMap<TimeClass, u32>,
}

fn env() -> Env {
    Env {
        stopped: HashSet::new(),
        in_flight: HashMap::new(),
    }
}

fn run(
    games: &[CandidateGame],
    plan: Plan,
    first: OffsetDateTime,
    p3: u32,
    e: &Env,
) -> Vec<Selected> {
    select_for_analysis(&SelectInput {
        games,
        plan,
        now: now(),
        first_scheduled_at: first,
        p3_selected_last_7d: p3,
        stopped: &e.stopped,
        in_flight_backfill: &e.in_flight,
    })
}

fn by_id(games: &[CandidateGame], id: Uuid) -> &CandidateGame {
    games.iter().find(|g| g.id == id).unwrap()
}

fn tier_count(sel: &[Selected], t: u8) -> usize {
    sel.iter().filter(|s| s.tier == t).count()
}

#[test]
fn constants_and_limits() {
    assert_eq!(P2_PER_MONTH, 15);
    assert_eq!(P2_MAX_MONTHS, 36);
    assert_eq!(P1_MIN_RECENT, 20);
    assert_eq!(RECENT_DAYS, 90);
    assert_eq!(MIN_PLIES, 10);
    assert_eq!(BATCH_SIZE, 25);
    assert_eq!(PREVIEW_GAMES, 20);
    let f = tier_limits(Plan::Free);
    assert_eq!(
        (f.import_max, f.p0, f.p1, f.p2, f.p3_per_week),
        (1000, 50, 0, false, 20)
    );
    let p = tier_limits(Plan::Pro);
    assert_eq!(
        (p.import_max, p.p0, p.p1, p.p2, p.p3_per_week),
        (10_000, 200, 100, true, 300)
    );
}

#[test]
fn free_selects_50_tier0_blitz_most_recent_first() {
    let mut games: Vec<_> = (0..70).map(|i| game(TimeClass::Blitz, i)).collect();
    games.extend((0..30).map(|i| game(TimeClass::Rapid, i)));
    let sel = run(&games, Plan::Free, now(), 0, &env());
    assert_eq!(sel.len(), 50);
    assert!(sel.iter().all(|s| s.tier == 0));
    assert!(
        sel.iter()
            .all(|s| by_id(&games, s.game_id).time_class == TimeClass::Blitz)
    );
    let ages: Vec<_> = sel
        .iter()
        .map(|s| by_id(&games, s.game_id).played_at)
        .collect();
    assert!(ages.windows(2).all(|w| w[0] >= w[1]));
    assert_eq!(ages[0], now());
    assert_eq!(sel[0].priority, 5);
}

#[test]
fn pro_p0_p1_and_bullet_skipped() {
    let mut games: Vec<_> = (0..250).map(|i| game(TimeClass::Blitz, i / 3)).collect();
    games.extend((0..30).map(|i| game(TimeClass::Rapid, i)));
    games.extend((0..40).map(|i| game(TimeClass::Bullet, i)));
    let sel = run(&games, Plan::Pro, now(), 0, &env());
    assert_eq!(tier_count(&sel, 0), 200);
    let t1: Vec<_> = sel.iter().filter(|s| s.tier == 1).collect();
    assert_eq!(t1.len(), 25);
    assert!(
        t1.iter()
            .all(|s| by_id(&games, s.game_id).time_class == TimeClass::Rapid)
    );
    assert!(
        sel.iter()
            .all(|s| by_id(&games, s.game_id).time_class != TimeClass::Bullet)
    );
    assert!(t1.iter().all(|s| s.priority == 11));
}

#[test]
fn bullet_is_analysed_when_primary() {
    let mut games: Vec<_> = (0..30).map(|i| game(TimeClass::Bullet, i)).collect();
    games.extend((0..5).map(|i| game(TimeClass::Blitz, i)));
    assert_eq!(primary_time_class(&games, now()), Some(TimeClass::Bullet));
    let sel = run(&games, Plan::Free, now(), 0, &env());
    assert_eq!(sel.len(), 30);
    assert!(
        sel.iter()
            .all(|s| by_id(&games, s.game_id).time_class == TimeClass::Bullet)
    );
}

#[test]
fn ply_count_boundary_table() {
    for (plies, expect) in [
        (Some(8), 0),
        (Some(9), 0),
        (Some(10), 1),
        (Some(11), 1),
        (None, 1),
    ] {
        let mut g = game(TimeClass::Blitz, 1);
        g.ply_count = plies;
        let sel = run(&[g], Plan::Free, now(), 0, &env());
        assert_eq!(sel.len(), expect, "plies {plies:?}");
    }
}

#[test]
fn already_selected_games_are_not_candidates() {
    let mut a = game(TimeClass::Blitz, 1);
    a.status = AnalysisStatus::Pending;
    a.tier = Some(0);
    let mut b = game(TimeClass::Blitz, 2);
    b.status = AnalysisStatus::Skipped;
    let c = game(TimeClass::Blitz, 3);
    let cid = c.id;
    let sel = run(&[a, b, c], Plan::Free, now(), 0, &env());
    assert_eq!(sel.len(), 1);
    assert_eq!(sel[0].game_id, cid);
}

#[test]
fn casual_only_after_rated_run_out() {
    let mut games: Vec<_> = (0..10).map(|i| game(TimeClass::Blitz, i + 5)).collect();
    let mut casual: Vec<_> = (0..45).map(|i| game(TimeClass::Blitz, i)).collect();
    casual.iter_mut().for_each(|g| g.rated = false);
    games.extend(casual);
    let sel = run(&games, Plan::Free, now(), 0, &env());
    assert_eq!(sel.len(), 50);
    assert!(sel[..10].iter().all(|s| by_id(&games, s.game_id).rated));
    assert!(sel[10..].iter().all(|s| !by_id(&games, s.game_id).rated));
}

#[test]
fn p0_counts_already_selected_games() {
    let mut games: Vec<_> = (0..48)
        .map(|i| done(game(TimeClass::Blitz, i + 10), 0))
        .collect();
    games.extend((0..30).map(|i| game(TimeClass::Blitz, i)));
    let sel = run(&games, Plan::Free, now(), 0, &env());
    assert_eq!(sel.len(), 2);
    assert!(sel.iter().all(|s| s.tier == 0));
}

fn mid_month_game(months_back: i64, hour: i64) -> CandidateGame {
    let mut g = game(TimeClass::Blitz, 0);
    g.played_at = time::macros::datetime!(2026-11-15 12:00 UTC)
        - Duration::days(30 * months_back)
        - Duration::hours(hour);
    g
}

#[test]
fn pro_p2_from_month_11_newest_first_within_36_months() {
    // 40 months x 20 blitz games; month 0 is the current month.
    let mut games = Vec::new();
    for m in 0..40i64 {
        for h in 0..20 {
            games.push(mid_month_game(m, h));
        }
    }
    // P0 has already consumed the newest 200 (10 months).
    let mut order: Vec<usize> = (0..games.len()).collect();
    order.sort_by_key(|i| std::cmp::Reverse(games[*i].played_at));
    for i in order.iter().take(200) {
        games[*i].status = AnalysisStatus::Done;
        games[*i].tier = Some(0);
    }
    let sel = run(&games, Plan::Pro, now(), 0, &env());
    assert_eq!(sel.len(), 25);
    assert!(sel.iter().all(|s| s.tier == 2 && s.priority == 21));
    let ages: Vec<_> = sel
        .iter()
        .map(|s| by_id(&games, s.game_id).played_at)
        .collect();
    let months: Vec<_> = ages
        .iter()
        .map(|a| (a.year(), u8::from(a.month())))
        .collect();
    assert!(months.windows(2).all(|w| w[0] >= w[1]));
    // month 11 yields its 15, month 12 the remaining 10
    assert_eq!(months.iter().filter(|m| **m == months[0]).count(), 15);
    // month 11 back from 2026-11 is 2025-12 (30-day steps from mid-month)
    assert!(
        ages.iter()
            .all(|a| *a < time::macros::datetime!(2026-02-01 0:00 UTC))
    );
}

#[test]
fn p2_never_picks_games_older_than_36_months() {
    let mut games = Vec::new();
    for m in 38..44i64 {
        for h in 0..20 {
            games.push(mid_month_game(m, h));
        }
    }
    // Recent blitz keeps the time class active and P0 satisfied.
    games.extend((0..200).map(|i| done(game(TimeClass::Blitz, i / 4), 0)));
    let sel = run(&games, Plan::Pro, now(), 0, &env());
    assert!(sel.is_empty(), "{sel:?}");
}

#[test]
fn p2_per_month_quota_counts_existing_and_batch_caps() {
    let mut games = Vec::new();
    for h in 0..20 {
        let mut g = game(TimeClass::Blitz, 0);
        g.played_at = time::macros::datetime!(2026-05-20 12:00 UTC) - Duration::hours(h);
        games.push(g);
    }
    for h in 0..10 {
        let mut g = done(game(TimeClass::Blitz, 0), 2);
        g.played_at = time::macros::datetime!(2026-05-10 12:00 UTC) - Duration::hours(h);
        games.push(g);
    }
    games.extend((0..210).map(|i| done(game(TimeClass::Blitz, i / 4), 0)));
    let sel = run(&games, Plan::Pro, now(), 0, &env());
    assert_eq!(sel.len(), 5); // 15 per month minus 10 existing tier-2
}

#[test]
fn p3_weekly_budget_table() {
    for (used, expect) in [(0u32, 20usize), (15, 5), (20, 0), (25, 0)] {
        let first = now() - Duration::days(10);
        let mut games: Vec<_> = (0..50)
            .map(|i| done(game(TimeClass::Blitz, 40 + i), 0))
            .collect();
        games.extend((0..30).map(|i| {
            let mut g = game(TimeClass::Blitz, 0);
            g.created_at = now() - Duration::hours(i);
            g.played_at = now() - Duration::hours(i);
            g
        }));
        let sel = run(&games, Plan::Free, first, used, &env());
        assert_eq!(sel.len(), expect, "used {used}");
        assert!(sel.iter().all(|s| s.tier == 3 && s.priority == 9));
    }
}

#[test]
fn p3_ignores_games_created_before_first_scheduled() {
    let first = now() - Duration::days(1);
    let mut games: Vec<_> = (0..50)
        .map(|i| done(game(TimeClass::Blitz, 40 + i), 0))
        .collect();
    games.extend((0..10).map(|i| game(TimeClass::Blitz, 2 + i)));
    let sel = run(&games, Plan::Free, first, 0, &env());
    assert!(sel.is_empty());
}

#[test]
fn stopped_tc_and_in_flight_backfill_block_p1_p2() {
    let mut games: Vec<_> = (0..250).map(|i| game(TimeClass::Blitz, i / 3)).collect();
    games.extend((0..30).map(|i| game(TimeClass::Rapid, i)));
    let base = run(&games, Plan::Pro, now(), 0, &env());
    assert_eq!(tier_count(&base, 1), 25);

    let mut e = env();
    e.stopped.insert(TimeClass::Rapid);
    e.stopped.insert(TimeClass::Blitz);
    let sel = run(&games, Plan::Pro, now(), 0, &e);
    assert_eq!(tier_count(&sel, 0), 200);
    assert_eq!(tier_count(&sel, 1) + tier_count(&sel, 2), 0);

    let mut e = env();
    e.in_flight.insert(TimeClass::Rapid, 3);
    e.in_flight.insert(TimeClass::Blitz, 1);
    let sel = run(&games, Plan::Pro, now(), 0, &e);
    assert_eq!(tier_count(&sel, 1) + tier_count(&sel, 2), 0);

    let mut e = env();
    e.in_flight.insert(TimeClass::Rapid, 0);
    assert_eq!(tier_count(&run(&games, Plan::Pro, now(), 0, &e), 1), 25);
}

#[test]
fn primary_time_class_ties_go_to_longer_and_old_games_ignored() {
    assert_eq!(primary_time_class(&[], now()), None);
    let old: Vec<_> = (0..10).map(|_| game(TimeClass::Blitz, 200)).collect();
    assert_eq!(primary_time_class(&old, now()), None);
    let tie = vec![game(TimeClass::Blitz, 1), game(TimeClass::Rapid, 2)];
    assert_eq!(primary_time_class(&tie, now()), Some(TimeClass::Rapid));
    let boundary = vec![game(TimeClass::Blitz, 90), game(TimeClass::Rapid, 91)];
    assert_eq!(primary_time_class(&boundary, now()), Some(TimeClass::Blitz));
}

#[test]
fn active_time_classes_need_20_recent_games_or_primary() {
    let mut games: Vec<_> = (0..40).map(|i| game(TimeClass::Blitz, i)).collect();
    games.extend((0..19).map(|i| game(TimeClass::Rapid, i)));
    games.extend((0..20).map(|i| game(TimeClass::Classical, i)));
    let a = active_time_classes(&games, now());
    assert_eq!(a, vec![TimeClass::Blitz, TimeClass::Classical]);
    assert!(active_time_classes(&[], now()).is_empty());
}

#[test]
fn job_priority_table() {
    use Plan::*;
    let cases = [
        (0, Pro, true, 2),
        (0, Pro, false, 3),
        (0, Free, true, 4),
        (0, Free, false, 5),
        (3, Pro, true, 6),
        (3, Free, false, 9),
        (1, Pro, true, 10),
        (1, Free, false, 11),
        (2, Pro, true, 20),
        (2, Pro, false, 21),
    ];
    for (tier, plan, evals, want) in cases {
        assert_eq!(
            job_priority(tier, plan, evals),
            want,
            "{tier} {plan:?} {evals}"
        );
    }
    assert_eq!(fair_tier(2), 1);
    assert_eq!(fair_tier(5), 2);
    assert_eq!(fair_tier(21), 10);
}

fn est(key: WeaknessKey, lo: f64, hi: f64, med: Option<f64>, n: f64) -> PrecisionInput {
    PrecisionInput {
        key,
        ci_low: lo,
        ci_high: hi,
        band_median: med,
        n_unweighted: n,
    }
}

#[test]
fn is_precise_table() {
    use WeaknessKey::*;
    let cases = [
        (vec![est(W1, 0.018, 0.022, Some(0.02), 10.0)], true),
        (vec![est(W1, 0.015, 0.025, Some(0.02), 500.0)], false),
        (vec![est(W1, 0.015, 0.025, Some(0.02), 1000.0)], true),
        (vec![est(W1, 0.015, 0.025, Some(0.02), 999.0)], false),
        (vec![est(W1, 0.0175, 0.0225, Some(0.02), 5.0)], true),
        (vec![est(W1, 0.015, 0.025, None, 10.0)], false),
        (vec![est(W1, 0.015, 0.025, None, 1000.0)], true),
        (vec![], false),
        (
            vec![
                est(W12, 0.0, 1.0, Some(0.1), 0.0),
                est(W1, 0.018, 0.022, Some(0.02), 1.0),
            ],
            true,
        ),
        (vec![est(W12, 0.0, 0.001, Some(0.1), 0.0)], false),
        (
            vec![
                est(W1, 0.018, 0.022, Some(0.02), 1.0),
                est(W2, 0.0, 0.5, Some(0.1), 5.0),
            ],
            false,
        ),
    ];
    for (i, (v, want)) in cases.iter().enumerate() {
        assert_eq!(is_precise(v), *want, "case {i}");
    }
}
