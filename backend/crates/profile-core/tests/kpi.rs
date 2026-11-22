//! R2 Task 15: KPI catalogue, snapshots and area scores.
//!
//! The first block ports TS M2 Task 19's assertions. The second block holds seeded property
//! tests for the honesty rules: no NaN or infinity in any output, intervals that bracket the
//! value, no claim from zero evidence, invalid input skipped and counted, and output that does
//! not depend on the input order (so change detection never sees a spurious change).

use chess_core::{Color, TimeClass};
use coaching_input::MetricUnit;
use profile_core::kpi::area_scores::{area_components, compute_area_scores};
use profile_core::kpi::catalogue::{
    CATALOGUE_LEN, KpiGame, KpiKind, is_count_mean, kpi_catalogue, kpi_definition,
};
use profile_core::kpi::iso_week::iso_week_key;
use profile_core::kpi::snapshots::{
    Comparison, KpiValue, Window, WindowKind, clamp_to_unit, compare_kpi, compute_kpis,
    compute_kpis_report, garwood_rate_interval, should_append,
};
use profile_core::stats::intervals::{Z80, wilson};
use profile_core::stats::rng::Rng;
use profile_core::types::{
    AREAS, Area, BandReference, BandReferenceStat, BaseMetric, EndgameOutcome, GameContext,
    MetricId, MissCounts, MotifTag, PLAYER_GAME_METRICS_VERSION, PlatformId, PlayerGameMetrics,
    SessionInfo, WeaknessKey,
};
use time::macros::datetime;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

// ---------------------------------------------------------------- fixtures

#[derive(Clone)]
struct Owned {
    ctx: GameContext,
    metrics: PlayerGameMetrics,
    session: Option<SessionInfo>,
}

fn owned(id: u128, played_at: OffsetDateTime, time_class: TimeClass) -> Owned {
    let ctx = GameContext {
        game_id: Uuid::from_u128(id),
        platform_game_id: format!("g{id}"),
        platform: PlatformId::Lichess,
        played_at,
        time_class,
        base_seconds: (time_class != TimeClass::Daily).then_some(180),
        increment_seconds: (time_class != TimeClass::Daily).then_some(2),
        rated: true,
        player_color: Color::White,
        player_rating: Some(1500),
        opponent_rating: Some(1500),
        result: "1-0".into(),
        score: Some(1.0),
        termination: None,
        accuracy: None,
        protocol: "test".into(),
    };
    let metrics = PlayerGameMetrics {
        version: PLAYER_GAME_METRICS_VERSION,
        moves: 30,
        player_rating: Some(1500),
        opponent_rating: Some(1500),
        time_trouble: (time_class != TimeClass::Daily).then_some(false),
        ..Default::default()
    };
    Owned {
        ctx,
        metrics,
        session: None,
    }
}

fn kpi_games(v: &[Owned]) -> Vec<KpiGame<'_>> {
    v.iter()
        .map(|o| KpiGame {
            ctx: &o.ctx,
            metrics: &o.metrics,
            session: o.session.as_ref(),
        })
        .collect()
}

fn base(m: BaseMetric) -> MetricId {
    MetricId::Base(m)
}

fn row(values: &[KpiValue], metric: MetricId) -> &KpiValue {
    values
        .iter()
        .find(|v| v.metric == metric)
        .unwrap_or_else(|| panic!("no row for {metric}"))
}

fn stat(metric: MetricId, p25: f64, p40: f64, median: f64, p75: f64) -> BandReferenceStat {
    BandReferenceStat {
        metric,
        median,
        p25,
        p40,
        p75,
        pooled_rate: None,
        epl_per_event: None,
        opps_per_game: None,
        opp_slope_per_100: Some(0.0),
        n_players: 100,
        n_games: 5000,
        low_sample: false,
    }
}

fn value_row(metric: MetricId, value: f64, n: u32) -> KpiValue {
    KpiValue {
        metric,
        window_kind: WindowKind::LastN,
        window_key: "last50".into(),
        value: Some(value),
        value_adjusted: Some(value),
        value_adjusted_raw: Some(value),
        n,
        ci_low: Some(value),
        ci_high: Some(value),
    }
}

fn rate_row(metric: MetricId, k: u32, n: u32) -> KpiValue {
    let ci = wilson(f64::from(k), f64::from(n), Z80).unwrap();
    KpiValue {
        metric,
        window_kind: WindowKind::Week,
        window_key: "2026-W47".into(),
        value: Some(f64::from(k) / f64::from(n)),
        value_adjusted: None,
        value_adjusted_raw: None,
        n,
        ci_low: Some(ci.low),
        ci_high: Some(ci.high),
    }
}

const NOW: OffsetDateTime = datetime!(2026-11-21 12:00 UTC);
const LAST50: Window = Window::LastN(50);

// ---------------------------------------------------------------- TS M2 Task 19

#[test]
fn iso_week_keys() {
    assert_eq!(iso_week_key(datetime!(2026-11-21 12:00 UTC)), "2026-W47");
    assert_eq!(iso_week_key(datetime!(2027-01-01 00:00 UTC)), "2026-W53");
    assert_eq!(iso_week_key(datetime!(2026-01-01 00:00 UTC)), "2026-W01");
    // UTC, whatever the offset: Monday 00:30 at +02:00 is still Sunday in UTC.
    assert_eq!(iso_week_key(datetime!(2026-11-23 00:30 +02:00)), "2026-W47");
    assert_eq!(iso_week_key(datetime!(2026-11-23 00:30 UTC)), "2026-W48");
}

#[test]
fn blunders_mean_over_three_games() {
    let games: Vec<Owned> = [0, 2, 1]
        .iter()
        .enumerate()
        .map(|(i, &b)| {
            let mut o = owned(i as u128, NOW - Duration::hours(i as i64), TimeClass::Blitz);
            o.metrics.blunders = b;
            o
        })
        .collect();
    let out = compute_kpis(&kpi_games(&games), &LAST50, None);
    let r = row(&out, base(BaseMetric::BlundersPerGame));
    assert_eq!(r.value, Some(1.0));
    assert_eq!(r.n, 3);
    assert_eq!(r.window_kind, WindowKind::LastN);
    assert_eq!(r.window_key, "last50");
    let (lo, hi) = (r.ci_low.unwrap(), r.ci_high.unwrap());
    assert!(lo <= 1.0 && 1.0 <= hi, "[{lo}, {hi}]");
    assert!(lo < hi);
    // No reference: nothing to adjust against.
    assert_eq!(r.value_adjusted, None);
}

#[test]
fn tactics_missed_unknown_is_not_zero_and_not_in_n() {
    // Games 0-2 have W3 computed (2, 0, 1 missed); games 3-4 had W3 dropped (unknown).
    let missed = [Some(2), Some(0), Some(1), None, None];
    let games: Vec<Owned> = missed
        .iter()
        .enumerate()
        .map(|(i, &m)| {
            let mut o = owned(i as u128, NOW - Duration::hours(i as i64), TimeClass::Blitz);
            o.metrics.tactics.missed = m;
            o
        })
        .collect();
    let out = compute_kpis(&kpi_games(&games), &LAST50, None);
    let r = row(&out, base(BaseMetric::TacticsMissedPerGame));
    // Unknown games counted as 0 would give 3 / 5 = 0.6 with n 5.
    assert_eq!(r.value, Some(1.0));
    assert_eq!(r.n, 2 + 1);

    // Every game unknown: no value and n 0, never "0 missed".
    let games: Vec<Owned> = (0..5)
        .map(|i| {
            let mut o = owned(i, NOW - Duration::hours(i as i64), TimeClass::Blitz);
            o.metrics.tactics.missed = None;
            o
        })
        .collect();
    let out = compute_kpis(&kpi_games(&games), &LAST50, None);
    let r = row(&out, base(BaseMetric::TacticsMissedPerGame));
    assert_eq!((r.value, r.n, r.ci_low), (None, 0, None));
}

#[test]
fn time_trouble_rate_counts_only_clocked_games() {
    let mut games = Vec::new();
    for i in 0..10u128 {
        let mut o = owned(i, NOW - Duration::hours(i as i64), TimeClass::Blitz);
        o.metrics.time_trouble = Some(i < 2);
        games.push(o);
    }
    for i in 10..15u128 {
        let mut o = owned(i, NOW - Duration::hours(i as i64), TimeClass::Daily);
        o.metrics.time_trouble = None;
        o.metrics.lost_on_time = true; // ignored: daily games have no clock
        games.push(o);
    }
    let out = compute_kpis(&kpi_games(&games), &LAST50, None);
    let r = row(&out, base(BaseMetric::TimeTroubleRate));
    assert_eq!(r.n, 10);
    assert_eq!(r.value, Some(0.2));
    assert!(
        (r.ci_high.unwrap() - 0.398404).abs() < 5e-7,
        "{:?}",
        r.ci_high
    );
    let lot = row(&out, base(BaseMetric::LossesOnTimeRate));
    assert_eq!(lot.n, 10);
    assert_eq!(lot.value, Some(0.0));
    assert_eq!(lot.ci_low, Some(0.0));
}

#[test]
fn last_n_uses_the_fifty_most_recent_games() {
    let mut games: Vec<Owned> = (0..60u32)
        .map(|i| {
            // Game i is played i hours after the first; blunders = i.
            let at = NOW - Duration::hours(100) + Duration::hours(i64::from(i));
            let mut o = owned(u128::from(i), at, TimeClass::Blitz);
            o.metrics.blunders = i;
            o
        })
        .collect();
    games.reverse();
    let out = compute_kpis(&kpi_games(&games), &LAST50, None);
    let r = row(&out, base(BaseMetric::BlundersPerGame));
    assert_eq!(r.n, 50);
    // Games 10..=59: mean 34.5.
    assert_eq!(r.value, Some(34.5));
}

#[test]
fn week_window_holds_only_that_week() {
    let mut games = Vec::new();
    // Week 47 of 2026 runs Mon 16 Nov to Sun 22 Nov.
    for (i, at) in [
        datetime!(2026-11-15 23:59 UTC), // W46
        datetime!(2026-11-16 00:00 UTC), // W47
        datetime!(2026-11-19 10:00 UTC), // W47
        datetime!(2026-11-22 23:59 UTC), // W47
        datetime!(2026-11-23 00:00 UTC), // W48
    ]
    .into_iter()
    .enumerate()
    {
        let mut o = owned(i as u128, at, TimeClass::Blitz);
        o.metrics.blunders = if iso_week_key(at) == "2026-W47" { 2 } else { 9 };
        games.push(o);
    }
    let w = Window::Week("2026-W47".into());
    let out = compute_kpis(&kpi_games(&games), &w, None);
    let r = row(&out, base(BaseMetric::BlundersPerGame));
    assert_eq!(r.n, 3);
    assert_eq!(r.value, Some(2.0));
    assert_eq!(r.window_kind, WindowKind::Week);
    assert_eq!(r.window_key, "2026-W47");
}

#[test]
fn opponent_adjustment_shifts_value_and_interval() {
    let games: Vec<Owned> = [1, 3, 2, 4]
        .iter()
        .enumerate()
        .map(|(i, &b)| {
            let mut o = owned(i as u128, NOW - Duration::hours(i as i64), TimeClass::Blitz);
            o.metrics.blunders = b;
            o.metrics.player_rating = Some(1500);
            // Mean (opp - own) / 100 = (0 + 200 + 100 + 100) / 4 / 100 = 1.
            o.metrics.opponent_rating = Some([1500, 1700, 1600, 1600][i]);
            o
        })
        .collect();
    let metric = base(BaseMetric::BlundersPerGame);
    let mut s = stat(metric, 1.0, 1.5, 2.0, 3.0);
    s.opp_slope_per_100 = Some(2.0);
    let reference: BandReference = [(metric, s)].into_iter().collect();
    let out = compute_kpis(&kpi_games(&games), &LAST50, Some(&reference));
    let r = row(&out, metric);
    assert_eq!(r.value, Some(2.5));
    assert_eq!(r.value_adjusted, Some(0.5));
    // The stored interval brackets the raw value; the adjusted one is shifted by the same amount.
    let (lo, hi) = (r.ci_low.unwrap(), r.ci_high.unwrap());
    assert!(lo <= 2.5 && 2.5 <= hi);
    assert_eq!(r.value_adjusted_raw, Some(0.5));
    // Shifted by -2, then clamped at 0 like the value (per_game >= 0).
    assert_eq!(
        r.adjusted_interval(),
        Some(((lo - 2.0).max(0.0), (hi - 2.0).max(0.0)))
    );
    // A metric without a reference stat is not adjusted.
    assert_eq!(
        row(&out, base(BaseMetric::OneMoveBlundersPerGame)).value_adjusted,
        None
    );
    // An unknown slope (R2 T32: NULL, never NaN) means no adjustment, never a guessed one.
    let mut unknown = reference.clone();
    unknown.get_mut(&metric).unwrap().opp_slope_per_100 = None;
    let out_unknown = compute_kpis(&kpi_games(&games), &LAST50, Some(&unknown));
    let ru = row(&out_unknown, metric);
    assert_eq!(ru.value, Some(2.5));
    assert_eq!((ru.value_adjusted, ru.value_adjusted_raw), (None, None));
    // `last` rows are never adjusted.
    let mut rref = reference.clone();
    let rating = base(BaseMetric::Rating);
    let mut rs = stat(rating, 1400.0, 1450.0, 1500.0, 1600.0);
    rs.opp_slope_per_100 = Some(2.0);
    rref.insert(rating, rs);
    let out = compute_kpis(&kpi_games(&games), &LAST50, Some(&rref));
    let r = row(&out, rating);
    assert_eq!(r.value, Some(1500.0));
    assert_eq!((r.value_adjusted, r.ci_low, r.ci_high), (None, None, None));
}

#[test]
fn compare_kpi_on_rates() {
    let metric = base(BaseMetric::MissedPunishRate);
    let def = kpi_definition(metric).unwrap();
    assert_eq!(def.higher_is_better, Some(false));
    assert_eq!(
        compare_kpi(&rate_row(metric, 30, 100), &rate_row(metric, 10, 100), def),
        Comparison::Improved
    );
    assert_eq!(
        compare_kpi(&rate_row(metric, 10, 100), &rate_row(metric, 30, 100), def),
        Comparison::Worse
    );
    assert_eq!(
        compare_kpi(&rate_row(metric, 12, 100), &rate_row(metric, 10, 100), def),
        Comparison::NoClearChange
    );
}

#[test]
fn compare_kpi_needs_values_and_a_direction() {
    let acc = base(BaseMetric::Accuracy);
    let def = kpi_definition(acc).unwrap();
    let mut a = value_row(acc, 70.0, 40);
    let mut b = value_row(acc, 80.0, 40);
    (a.ci_low, a.ci_high, b.ci_low, b.ci_high) = (Some(68.0), Some(72.0), Some(78.0), Some(82.0));
    assert_eq!(compare_kpi(&a, &b, def), Comparison::Improved);
    assert_eq!(compare_kpi(&b, &a, def), Comparison::Worse);
    b.ci_low = Some(71.0);
    assert_eq!(compare_kpi(&a, &b, def), Comparison::NoClearChange);
    b.value = None;
    assert_eq!(compare_kpi(&a, &b, def), Comparison::NoClearChange);
    // No direction: a change is never "improved" or "worse".
    let tpm = base(BaseMetric::TimePerMoveOpening);
    let tdef = kpi_definition(tpm).unwrap();
    assert_eq!(tdef.higher_is_better, None);
    let mut x = value_row(tpm, 5.0, 40);
    let mut y = value_row(tpm, 15.0, 40);
    (x.ci_low, x.ci_high, y.ci_low, y.ci_high) = (Some(4.0), Some(6.0), Some(14.0), Some(16.0));
    assert_eq!(compare_kpi(&x, &y, tdef), Comparison::NoClearChange);
}

#[test]
fn area_with_too_little_data_reports_have_and_need() {
    let metric = base(BaseMetric::AccuracyEndgame);
    let kpis = vec![value_row(metric, 60.0, 12)];
    let reference: BandReference = [(metric, stat(metric, 50.0, 55.0, 58.0, 65.0))]
        .into_iter()
        .collect();
    let scores = compute_area_scores(&kpis, Some(&reference));
    let e = scores.iter().find(|s| s.area == Area::Endgame).unwrap();
    assert_eq!(e.score, None);
    assert!(!e.sufficient);
    assert_eq!(e.need_more, Some(8));
    assert_eq!(e.n, 12);
}

#[test]
fn area_components_at_the_band_median_score_50() {
    let comps = area_components(Area::Middlegame);
    assert_eq!(
        comps,
        &[
            base(BaseMetric::AccuracyMiddlegame),
            MetricId::Weakness(WeaknessKey::W10),
            MetricId::Weakness(WeaknessKey::W5),
            MetricId::Weakness(WeaknessKey::W6),
        ]
    );
    let mut kpis = Vec::new();
    let mut reference = BandReference::new();
    for (i, &m) in comps.iter().enumerate() {
        let median = 0.1 * (i + 1) as f64;
        reference.insert(m, stat(m, median * 0.5, median * 0.8, median, median * 1.5));
        kpis.push(value_row(m, median, kpi_definition(m).unwrap().min_n + 5));
    }
    let scores = compute_area_scores(&kpis, Some(&reference));
    let m = scores.iter().find(|s| s.area == Area::Middlegame).unwrap();
    assert_eq!(m.score, Some(50.0));
    assert!(m.sufficient);
    assert_eq!(m.need_more, None);
}

#[test]
fn mindset_has_no_benchmark_and_areas_come_in_contract_order() {
    let mut kpis = Vec::new();
    let mut reference = BandReference::new();
    for a in AREAS {
        for &m in area_components(a) {
            reference.insert(m, stat(m, 0.1, 0.2, 0.3, 0.4));
            kpis.push(value_row(m, 0.3, 5000));
        }
    }
    let scores = compute_area_scores(&kpis, Some(&reference));
    assert_eq!(scores.iter().map(|s| s.area).collect::<Vec<_>>(), AREAS);
    for s in &scores {
        if s.area == Area::Mindset {
            assert_eq!((s.score, s.sufficient, s.need_more), (None, false, None));
        } else {
            assert_eq!(s.score, Some(50.0), "{:?}", s.area);
        }
    }
}

#[test]
fn catalogue_has_36_base_rows_and_contract_units() {
    let cat = kpi_catalogue();
    assert_eq!(cat.len(), 69);
    assert_eq!(CATALOGUE_LEN, 69);
    assert_eq!(
        cat.iter()
            .filter(|d| matches!(d.metric, MetricId::MotifMissRate(_)))
            .count(),
        16
    );
    assert_eq!(
        cat.iter()
            .filter(|d| matches!(d.metric, MetricId::Weakness(_)))
            .count(),
        17
    );
    assert_eq!(
        "score_after_non_loss".parse::<MetricId>().unwrap(),
        base(BaseMetric::ScoreAfterNonLoss)
    );
    assert!("score_after_win".parse::<MetricId>().is_err());
    for (d, b) in cat.iter().zip(BaseMetric::ALL) {
        assert_eq!(d.metric, MetricId::Base(b));
    }
    let mut seen = std::collections::BTreeSet::new();
    for d in cat {
        assert!(seen.insert(d.metric), "duplicate {}", d.metric);
        assert!(!d.label.is_empty() && d.label.len() <= 80);
        assert!(d.min_n >= 1);
        assert_eq!(kpi_definition(d.metric).map(|x| x.metric), Some(d.metric));
        if d.contract_unit.is_none() {
            let MetricId::Base(b) = d.metric else {
                panic!("{} has no unit", d.metric)
            };
            assert!(
                [
                    BaseMetric::PerfVsExpected,
                    BaseMetric::EvalTrend1535,
                    BaseMetric::CriticalTimeRatio,
                    BaseMetric::BookExitPly,
                    BaseMetric::SessionAccuracyDecline,
                ]
                .contains(&b),
                "{b} has no unit"
            );
        }
        if d.kind == KpiKind::Rate {
            assert_eq!(d.contract_unit, Some(MetricUnit::Rate), "{}", d.metric);
        }
    }
    let get = |m: BaseMetric| kpi_definition(MetricId::Base(m)).unwrap();
    let r = get(BaseMetric::Rating);
    assert_eq!(
        (r.kind, r.area, r.higher_is_better, r.min_n, r.contract_unit),
        (KpiKind::Last, None, Some(true), 1, Some(MetricUnit::Elo))
    );
    let b = get(BaseMetric::BlunderRateLowClock);
    assert_eq!(
        (b.kind, b.area, b.higher_is_better, b.min_n),
        (KpiKind::Rate, Some(Area::Time), Some(false), 40)
    );
    let c = get(BaseMetric::ConversionRate);
    assert_eq!((c.area, c.min_n), (Some(Area::Middlegame), 15));
    let s = get(BaseMetric::SaveableResignationRate);
    assert_eq!((s.area, s.min_n), (Some(Area::Mindset), 10));
    assert_eq!(get(BaseMetric::Accuracy).area, None);
    assert_eq!(
        get(BaseMetric::TimePerMoveEndgame).contract_unit,
        Some(MetricUnit::Seconds)
    );
    assert_eq!(
        get(BaseMetric::TbErrorsPerEndgame).contract_unit,
        Some(MetricUnit::PerGame)
    );
    assert_eq!(
        get(BaseMetric::ScoreAfterLoss).contract_unit,
        Some(MetricUnit::Rate)
    );
    let m = kpi_definition(MetricId::MotifMissRate(MotifTag::Fork)).unwrap();
    assert_eq!(
        (m.kind, m.area, m.min_n, m.higher_is_better),
        (KpiKind::Rate, Some(Area::Tactics), 20, Some(false))
    );
    let w = kpi_definition(MetricId::Weakness(WeaknessKey::W8Basics)).unwrap();
    assert_eq!(
        (w.kind, w.area, w.min_n),
        (KpiKind::Rate, Some(Area::Endgame), 80)
    );
    assert!(kpi_definition(MetricId::Weakness(WeaknessKey::W12)).is_none());
    // Every area component has a catalogue row with a direction.
    for a in AREAS {
        for &c in area_components(a) {
            assert!(kpi_definition(c).unwrap().higher_is_better.is_some(), "{c}");
        }
    }
}

// ---------------------------------------------------------------- further rules

#[test]
fn rows_from_counts_and_sessions() {
    let mut games = Vec::new();
    for i in 0..8u32 {
        let mut o = owned(
            u128::from(i),
            NOW - Duration::hours(i64::from(i)),
            TimeClass::Rapid,
        );
        o.metrics.missed_by_motif.insert(
            MotifTag::Fork,
            MissCounts {
                opportunities: 2,
                misses: u32::from(i % 2 == 0),
            },
        );
        o.metrics.endgame.outcome = Some(if i < 3 {
            EndgameOutcome::Converted
        } else if i < 4 {
            EndgameOutcome::FailedConversion
        } else {
            EndgameOutcome::Held
        });
        o.metrics.accuracy = Some(80.0 - f64::from(i));
        o.metrics.score = Some(if i % 2 == 0 { 1.0 } else { 0.0 });
        o.session = Some(SessionInfo {
            game_id: o.ctx.game_id,
            session_id: 1,
            index_in_session: i + 1,
            after_loss: i % 2 == 0 && i > 0,
            residual: None,
        });
        games.push(o);
    }
    let out = compute_kpis(&kpi_games(&games), &LAST50, None);
    let fork = row(&out, MetricId::MotifMissRate(MotifTag::Fork));
    assert_eq!((fork.value, fork.n), (Some(0.25), 16));
    let conv = row(&out, base(BaseMetric::EndgameConversionRate));
    assert_eq!((conv.value, conv.n), (Some(0.75), 4));
    let hold = row(&out, base(BaseMetric::EndgameHoldRate));
    assert_eq!((hold.value, hold.n), (Some(1.0), 4));
    assert_eq!(hold.ci_high, Some(1.0));
    let first = row(&out, base(BaseMetric::FirstGameAccuracy));
    assert_eq!((first.value, first.n), (Some(80.0), 1));
    assert_eq!((first.ci_low, first.ci_high), (None, None));
    // Indices 4..=8 have accuracy 77..=73 (mean 75); index 1 has 80.
    let decline = row(&out, base(BaseMetric::SessionAccuracyDecline));
    assert_eq!((decline.value, decline.n), (Some(-5.0), 6));
    // One first game: no interval for the difference.
    assert_eq!(decline.ci_low, None);
    // after_loss at i = 2, 4, 6: scores 1.
    let sal = row(&out, base(BaseMetric::ScoreAfterLoss));
    assert_eq!((sal.value, sal.n), (Some(1.0), 3));
}

#[test]
fn no_evidence_gives_no_value() {
    let out = compute_kpis(&[], &LAST50, None);
    assert_eq!(out.len(), kpi_catalogue().len());
    for r in &out {
        assert_eq!(r.n, 0, "{}", r.metric);
        assert_eq!(
            (r.value, r.value_adjusted, r.ci_low, r.ci_high),
            (None, None, None, None),
            "{}",
            r.metric
        );
    }
    let areas = compute_area_scores(&out, None);
    assert_eq!(areas.len(), 7);
    for a in &areas {
        assert!(!a.sufficient);
        assert_eq!(a.score, None);
        assert_eq!(a.n, 0);
    }
    // Zero opportunities in every game: a rate has no value, not 0 %.
    let games: Vec<Owned> = (0..30)
        .map(|i| owned(i, NOW - Duration::hours(i as i64), TimeClass::Blitz))
        .collect();
    let out = compute_kpis(&kpi_games(&games), &LAST50, None);
    let r = row(&out, base(BaseMetric::SharpErrorRate));
    assert_eq!((r.value, r.n, r.ci_low), (None, 0, None));
}

#[test]
fn invalid_inputs_are_skipped_and_counted() {
    let mut games = Vec::new();
    for i in 0..5u32 {
        let mut o = owned(
            u128::from(i),
            NOW - Duration::hours(i64::from(i)),
            TimeClass::Blitz,
        );
        o.metrics.accuracy = Some(70.0);
        o.metrics.blunders = 1;
        games.push(o);
    }
    // A stale-version row: every value of it would read as clean zero evidence.
    let mut stale = owned(100, NOW, TimeClass::Blitz);
    stale.metrics.version = PLAYER_GAME_METRICS_VERSION + 1;
    stale.metrics.blunders = 50;
    games.push(stale);
    // An identical copy of game 1 (merged), and a conflicting copy of game 0 (both refused).
    games.push(games[1].clone());
    let mut dup = games[0].clone();
    dup.metrics.blunders = 40;
    games.push(dup);
    // Non-finite values and impossible counts.
    let mut bad = owned(200, NOW - Duration::minutes(30), TimeClass::Blitz);
    bad.metrics.accuracy = Some(f64::NAN);
    bad.metrics.blunders = 1;
    bad.metrics.time_in_opening = Some(f64::INFINITY);
    bad.metrics.missed_by_motif.insert(
        MotifTag::Pin,
        MissCounts {
            opportunities: 1,
            misses: 3,
        },
    );
    games.push(bad);
    let report = compute_kpis_report(&kpi_games(&games), &LAST50, None);
    assert_eq!(report.skipped.stale_version, 1);
    assert_eq!(report.skipped.duplicate_game, 1);
    assert_eq!(report.skipped.conflicting_duplicates, 1);
    assert_eq!(report.skipped.invalid_value, 3);
    // Games 1..=4 (game 0 refused); the bad game's NaN accuracy is skipped.
    let acc = row(&report.values, base(BaseMetric::Accuracy));
    assert_eq!((acc.value, acc.n), (Some(70.0), 4));
    let bl = row(&report.values, base(BaseMetric::BlundersPerGame));
    assert_eq!((bl.value, bl.n), (Some(1.0), 5));
    let pin = row(&report.values, MetricId::MotifMissRate(MotifTag::Pin));
    assert_eq!((pin.value, pin.n), (None, 0));
    assert_eq!(
        compute_kpis(&kpi_games(&games), &LAST50, None),
        report.values
    );
}

#[test]
fn append_only_when_value_or_n_changed() {
    let m = base(BaseMetric::Accuracy);
    let a = value_row(m, 70.0, 30);
    assert!(should_append(None, &a));
    assert!(!should_append(Some(&a), &a.clone()));
    let mut b = a.clone();
    b.n = 31;
    assert!(should_append(Some(&a), &b));
    let mut c = a.clone();
    c.value = Some(70.000_000_000_1);
    assert!(should_append(Some(&a), &c));
    let mut d = a.clone();
    d.value = None;
    assert!(should_append(Some(&a), &d));
    // Only the interval moved (for example a new reference): not a new snapshot.
    let mut e = a.clone();
    e.ci_low = Some(60.0);
    e.value_adjusted = Some(68.0);
    assert!(!should_append(Some(&a), &e));
}

// ---------------------------------------------------------------- seeded properties

fn maybe(rng: &mut Rng, p_none: f64) -> bool {
    rng.next_f64() >= p_none
}

fn uniform(rng: &mut Rng, lo: f64, hi: f64) -> f64 {
    lo + (hi - lo) * rng.next_f64()
}

fn int(rng: &mut Rng, hi: u32) -> u32 {
    (rng.next_f64() * f64::from(hi + 1)) as u32
}

fn random_games(rng: &mut Rng, count: usize) -> Vec<Owned> {
    (0..count)
        .map(|i| {
            let tc = [
                TimeClass::Bullet,
                TimeClass::Blitz,
                TimeClass::Rapid,
                TimeClass::Daily,
            ][int(rng, 3) as usize];
            let at = NOW - Duration::minutes(i64::from(int(rng, 60 * 24 * 40)));
            let mut o = owned(i as u128 + 1, at, tc);
            let m = &mut o.metrics;
            let clocked = tc != TimeClass::Daily;
            m.moves = int(rng, 60);
            m.blunders = int(rng, 4);
            m.one_move_blunders = int(rng, 2);
            m.score = maybe(rng, 0.1).then(|| [0.0, 0.5, 1.0][int(rng, 2) as usize]);
            m.expected_score = maybe(rng, 0.2).then(|| uniform(rng, 0.0, 1.0));
            m.accuracy = maybe(rng, 0.1).then(|| uniform(rng, 30.0, 99.0));
            m.accuracy_by_phase.opening = maybe(rng, 0.2).then(|| uniform(rng, 30.0, 100.0));
            m.accuracy_by_phase.middlegame = maybe(rng, 0.3).then(|| uniform(rng, 30.0, 100.0));
            m.accuracy_by_phase.endgame = maybe(rng, 0.6).then(|| uniform(rng, 30.0, 100.0));
            m.player_rating = maybe(rng, 0.1).then(|| 1200 + int(rng, 800) as i32);
            m.opponent_rating = maybe(rng, 0.1).then(|| 1200 + int(rng, 800) as i32);
            m.low_clock.moves = if clocked { int(rng, 10) } else { 0 };
            m.low_clock.decisive = int(rng, m.low_clock.moves);
            m.missed_punish.opportunities = int(rng, 3);
            m.missed_punish.misses = int(rng, m.missed_punish.opportunities);
            m.tactics.missed = maybe(rng, 0.2).then(|| int(rng, 2));
            m.tactics.allowed = int(rng, 2);
            m.sharp.positions = int(rng, 5);
            m.sharp.errors = int(rng, m.sharp.positions);
            m.quiet.moves = int(rng, 20);
            m.quiet.loss = uniform(rng, 0.0, 50.0);
            m.eval_trend_15_to_35 = maybe(rng, 0.3).then(|| uniform(rng, -40.0, 40.0));
            m.time_trouble = clocked.then(|| int(rng, 3) == 0);
            m.lost_on_time = clocked && int(rng, 9) == 0;
            m.time_per_move.opening = clocked.then(|| uniform(rng, 0.5, 20.0));
            m.critical_time_ratio = (clocked && maybe(rng, 0.3)).then(|| uniform(rng, 0.2, 4.0));
            m.time_in_opening = clocked.then(|| uniform(rng, 5.0, 200.0));
            m.opening.win_at_opening_end = maybe(rng, 0.2).then(|| uniform(rng, 0.0, 100.0));
            m.opening.out_worse = maybe(rng, 0.2).then(|| int(rng, 2) == 0);
            m.opening.book_exit_ply = maybe(rng, 0.2).then(|| int(rng, 30));
            m.conversion.episode = int(rng, 3) == 0;
            m.conversion.converted = m.conversion.episode.then(|| int(rng, 1) == 0);
            m.endgame.tb_positions = int(rng, 4);
            m.endgame.tb_errors = int(rng, m.endgame.tb_positions);
            m.resigned_saveable = (int(rng, 4) == 0).then(|| int(rng, 1) == 0);
            for t in [MotifTag::Fork, MotifTag::Pin] {
                let opportunities = int(rng, 2);
                let misses = int(rng, opportunities);
                m.missed_by_motif.insert(
                    t,
                    MissCounts {
                        opportunities,
                        misses,
                    },
                );
            }
            for k in [WeaknessKey::W1, WeaknessKey::W10, WeaknessKey::W8Basics] {
                let opportunities = int(rng, 30);
                let events = int(rng, opportunities.min(3));
                m.weakness.insert(
                    k,
                    profile_core::types::BinomialObs {
                        events,
                        opportunities,
                        ..Default::default()
                    },
                );
            }
            o.session = maybe(rng, 0.2).then(|| SessionInfo {
                game_id: o.ctx.game_id,
                session_id: int(rng, 5),
                index_in_session: 1 + int(rng, 6),
                after_loss: int(rng, 2) == 0,
                residual: None,
            });
            o
        })
        .collect()
}

fn random_reference(rng: &mut Rng) -> BandReference {
    let mut reference = BandReference::new();
    for d in kpi_catalogue() {
        if maybe(rng, 0.3) {
            let a = uniform(rng, 0.0, 50.0);
            let mut s = stat(d.metric, a, a + 1.0, a + 2.0, a + 4.0);
            s.opp_slope_per_100 = Some(uniform(rng, -3.0, 3.0));
            reference.insert(d.metric, s);
        }
    }
    reference
}

fn shuffle<T>(rng: &mut Rng, v: &mut [T]) {
    for i in (1..v.len()).rev() {
        let j = (rng.next_f64() * (i + 1) as f64) as usize;
        v.swap(i, j.min(i));
    }
}

fn check_honest(values: &[KpiValue]) {
    for r in values {
        let def = kpi_definition(r.metric).unwrap();
        for x in [r.value, r.value_adjusted, r.ci_low, r.ci_high]
            .into_iter()
            .flatten()
        {
            assert!(x.is_finite(), "{} {:?}", r.metric, r);
            assert!(x != 0.0 || x.is_sign_positive(), "-0 in {}", r.metric);
        }
        match r.value {
            None => {
                assert_eq!((r.value_adjusted, r.ci_low, r.ci_high), (None, None, None));
                assert_eq!(r.value_adjusted_raw, None);
            }
            Some(v) => {
                assert!(r.n >= 1, "{} value from n 0", r.metric);
                if let (Some(lo), Some(hi)) = (r.ci_low, r.ci_high) {
                    assert!(lo <= v && v <= hi, "{}: {lo} {v} {hi}", r.metric);
                    // Never a zero-width interval: it would claim certainty.
                    assert!(lo < hi, "{}: zero width at {v}", r.metric);
                }
                assert_eq!(r.value_adjusted.is_some(), r.value_adjusted_raw.is_some());
                if let Some(va) = r.value_adjusted {
                    assert_eq!(clamp_to_unit(va, def.contract_unit), va, "{}", r.metric);
                    if let Some((lo, hi)) = r.adjusted_interval() {
                        assert!(lo <= va && va <= hi, "{}: {lo} {va} {hi}", r.metric);
                    }
                }
                assert_eq!(r.ci_low.is_some(), r.ci_high.is_some());
                if def.kind == KpiKind::Rate {
                    assert!((0.0..=1.0).contains(&v));
                    assert!(r.ci_low.is_some());
                }
                if def.kind == KpiKind::Last {
                    assert_eq!((r.value_adjusted, r.ci_low), (None, None));
                }
            }
        }
    }
}

#[test]
fn property_outputs_are_finite_bracketed_and_order_independent() {
    let mut rng = Rng::new(20261121);
    for case in 0..40 {
        let count = int(&mut rng, 120) as usize;
        let mut games = random_games(&mut rng, count);
        // Identical copies (merged) and conflicting copies (the id is refused).
        for j in 0..games.len().min(6) {
            let mut copy = games[j].clone();
            if j % 2 == 1 {
                copy.metrics.blunders += 1;
            }
            games.push(copy);
        }
        let reference = random_reference(&mut rng);
        let with_ref = (case % 3 != 0).then_some(&reference);
        let windows = [
            LAST50,
            Window::LastN(7),
            Window::Week(iso_week_key(NOW - Duration::days(int(&mut rng, 30).into()))),
        ];
        let before: Vec<Vec<KpiValue>> = windows
            .iter()
            .map(|w| compute_kpis(&kpi_games(&games), w, with_ref))
            .collect();
        for (w, out) in windows.iter().zip(&before) {
            check_honest(out);
            assert_eq!(out.len(), kpi_catalogue().len());
            if let Window::LastN(n) = w {
                let games_n = row(out, base(BaseMetric::Rating)).n as usize;
                assert!(games_n <= *n);
            }
            let areas = compute_area_scores(out, with_ref);
            assert_eq!(areas.iter().map(|a| a.area).collect::<Vec<_>>(), AREAS);
            for a in &areas {
                if a.sufficient {
                    let s = a.score.unwrap();
                    assert!((1.0..=99.0).contains(&s) && a.need_more.is_none());
                } else {
                    assert_eq!(a.score, None);
                }
                if with_ref.is_none() {
                    assert!(!a.sufficient);
                }
            }
        }
        shuffle(&mut rng, &mut games);
        for (w, out) in windows.iter().zip(&before) {
            assert_eq!(
                &compute_kpis(&kpi_games(&games), w, with_ref),
                out,
                "case {case}"
            );
        }
    }
}

#[test]
fn property_junk_rows_never_change_the_result() {
    let mut rng = Rng::new(7);
    for case in 0..25 {
        let count = 1 + int(&mut rng, 80) as usize;
        let games = random_games(&mut rng, count);
        let reference = random_reference(&mut rng);
        let clean = compute_kpis_report(&kpi_games(&games), &LAST50, Some(&reference));
        assert_eq!(clean.skipped.stale_version, 0);
        let mut noisy = random_games(&mut rng, 20);
        for (j, o) in noisy.iter_mut().enumerate() {
            o.ctx.game_id = Uuid::from_u128(10_000 + j as u128);
            o.ctx.played_at = NOW + Duration::hours(1);
            o.metrics.version = if j % 2 == 0 { 0 } else { 99 };
        }
        noisy.extend(random_games(&mut rng, 0));
        let mut all: Vec<Owned> = games;
        all.extend(noisy);
        shuffle(&mut rng, &mut all);
        let dirty = compute_kpis_report(&kpi_games(&all), &LAST50, Some(&reference));
        assert_eq!(dirty.skipped.stale_version, 20, "case {case}");
        assert_eq!(dirty.values, clean.values, "case {case}");
    }
}

#[test]
fn property_wilson_rows_match_pooled_counts() {
    let mut rng = Rng::new(99);
    for _ in 0..30 {
        let count = int(&mut rng, 60) as usize;
        let games = random_games(&mut rng, count);
        let out = compute_kpis(&kpi_games(&games), &Window::LastN(usize::MAX), None);
        let (mut k, mut n) = (0u32, 0u32);
        for o in &games {
            k += o.metrics.sharp.errors;
            n += o.metrics.sharp.positions;
        }
        let r = row(&out, base(BaseMetric::SharpErrorRate));
        assert_eq!(r.n, n);
        if n == 0 {
            assert_eq!(r.value, None);
        } else {
            let ci = wilson(f64::from(k), f64::from(n), Z80).unwrap();
            assert_eq!(r.value, Some(f64::from(k) / f64::from(n)));
            assert_eq!((r.ci_low, r.ci_high), (Some(ci.low), Some(ci.high)));
        }
    }
}

// ---------------------------------------------------------------- fix round 1

fn plain_games(count: u32) -> Vec<Owned> {
    (0..count)
        .map(|i| {
            owned(
                u128::from(i),
                NOW - Duration::hours(i64::from(i)),
                TimeClass::Blitz,
            )
        })
        .collect()
}

#[test]
fn adjusted_value_is_clamped_to_the_unit_domain() {
    // The review's case: a rate of 0 adjusted by -0.06.
    let mut games = plain_games(20);
    for o in &mut games {
        o.metrics.time_trouble = Some(false);
        o.metrics.opponent_rating = Some(1600);
        o.metrics.eval_trend_15_to_35 = Some(1.0);
    }
    let tt = base(BaseMetric::TimeTroubleRate);
    let trend = base(BaseMetric::EvalTrend1535);
    let mut reference = BandReference::new();
    let mut s = stat(tt, 0.05, 0.1, 0.15, 0.25);
    s.opp_slope_per_100 = Some(0.06);
    reference.insert(tt, s);
    let mut s = stat(trend, -5.0, -2.0, 0.0, 3.0);
    s.opp_slope_per_100 = Some(2.0);
    reference.insert(trend, s);
    let out = compute_kpis(&kpi_games(&games), &LAST50, Some(&reference));
    let r = row(&out, tt);
    assert_eq!(r.value, Some(0.0));
    assert_eq!(r.value_adjusted, Some(0.0));
    assert_eq!(r.value_adjusted_raw, Some(-0.06));
    let (lo, hi) = r.adjusted_interval().unwrap();
    assert_eq!(lo, 0.0);
    assert!((hi - (r.ci_high.unwrap() - 0.06)).abs() < 1e-15 && hi > 0.0);
    // A row without a unit stays signed.
    let r = row(&out, trend);
    assert_eq!((r.value, r.value_adjusted), (Some(1.0), Some(-1.0)));
    // The clamp per unit.
    assert_eq!(clamp_to_unit(1.2, Some(MetricUnit::Rate)), 1.0);
    assert_eq!(clamp_to_unit(101.0, Some(MetricUnit::Pct)), 100.0);
    assert_eq!(clamp_to_unit(-3.0, Some(MetricUnit::Seconds)), 0.0);
    assert_eq!(clamp_to_unit(-3.0, Some(MetricUnit::Elo)), 0.0);
    assert_eq!(clamp_to_unit(5000.0, Some(MetricUnit::PerGame)), 5000.0);
    assert_eq!(clamp_to_unit(-3.0, None), -3.0);
}

#[test]
fn no_rating_pair_means_no_adjustment() {
    let mut games = plain_games(5);
    for (i, o) in games.iter_mut().enumerate() {
        o.metrics.blunders = i as u32;
        o.metrics.opponent_rating = None;
        o.ctx.opponent_rating = None;
    }
    let metric = base(BaseMetric::BlundersPerGame);
    let mut s = stat(metric, 1.0, 1.5, 2.0, 3.0);
    s.opp_slope_per_100 = Some(2.0);
    let reference: BandReference = [(metric, s)].into_iter().collect();
    let out = compute_kpis(&kpi_games(&games), &LAST50, Some(&reference));
    let r = row(&out, metric);
    assert_eq!(r.value, Some(2.0));
    assert_eq!((r.value_adjusted, r.value_adjusted_raw), (None, None));
    assert_eq!(r.adjusted_interval(), None);
}

#[test]
fn rate_rows_weight_the_rating_gap_by_opportunities() {
    let mut games = plain_games(2);
    // +1 per 100 over 9 opportunities, -1 per 100 over 1.
    games[0].metrics.sharp.positions = 9;
    games[0].metrics.sharp.errors = 3;
    games[0].metrics.opponent_rating = Some(1600);
    games[1].metrics.sharp.positions = 1;
    games[1].metrics.sharp.errors = 0;
    games[1].metrics.opponent_rating = Some(1400);
    let metric = base(BaseMetric::SharpErrorRate);
    let mut s = stat(metric, 0.1, 0.2, 0.3, 0.4);
    s.opp_slope_per_100 = Some(0.1);
    let reference: BandReference = [(metric, s)].into_iter().collect();
    let out = compute_kpis(&kpi_games(&games), &LAST50, Some(&reference));
    let r = row(&out, metric);
    assert_eq!((r.value, r.n), (Some(0.3), 10));
    // Weighted gap (9 - 1) / 10 = 0.8, so the shift is 0.08 (unweighted it would be 0).
    let va = r.value_adjusted.unwrap();
    assert!((va - 0.22).abs() < 1e-15, "{va}");
}

#[test]
fn zero_width_count_rows_use_the_garwood_interval() {
    // 20 games, no blunders: the bootstrap is [0, 0].
    let games = plain_games(20);
    let out = compute_kpis(&kpi_games(&games), &LAST50, None);
    let r = row(&out, base(BaseMetric::BlundersPerGame));
    assert!(is_count_mean(r.metric));
    assert_eq!((r.value, r.n, r.ci_low), (Some(0.0), 20, Some(0.0)));
    // P(X = 0) = 0.1 at mu = ln 10.
    let hi = r.ci_high.unwrap();
    assert!((hi - 10f64.ln() / 20.0).abs() < 1e-10, "{hi}");
    // Five games with one blunder each: chi2(0.1; 10)/2 = 2.432591, chi2(0.9; 12)/2 = 9.274674.
    let mut games = plain_games(5);
    for o in &mut games {
        o.metrics.blunders = 1;
    }
    let out = compute_kpis(&kpi_games(&games), &LAST50, None);
    let r = row(&out, base(BaseMetric::BlundersPerGame));
    assert_eq!(r.value, Some(1.0));
    let (lo, hi) = (r.ci_low.unwrap(), r.ci_high.unwrap());
    assert!((lo - 2.432591 / 5.0).abs() < 2e-6, "{lo}");
    assert!((hi - 9.274674 / 5.0).abs() < 2e-6, "{hi}");
    assert_eq!(garwood_rate_interval(5.0, 5.0), Some((lo, hi)));
    assert_eq!(garwood_rate_interval(1.5, 5.0), None);
    assert_eq!(garwood_rate_interval(1.0, 0.0), None);
}

#[test]
fn zero_width_other_rows_have_no_interval() {
    let mut games = plain_games(20);
    for o in &mut games {
        o.metrics.accuracy = Some(70.0);
    }
    let out = compute_kpis(&kpi_games(&games), &LAST50, None);
    let r = row(&out, base(BaseMetric::Accuracy));
    assert!(!is_count_mean(r.metric));
    assert_eq!((r.value, r.n), (Some(70.0), 20));
    assert_eq!((r.ci_low, r.ci_high), (None, None));
}

#[test]
fn need_more_does_not_underflow_when_n_exceeds_min_n() {
    // accuracy.endgame has n far above min_n but no value: it does not count, and it must not
    // produce `min_n - n` on a u32.
    let metric = base(BaseMetric::AccuracyEndgame);
    let mut r = value_row(metric, 60.0, 500);
    r.value = None;
    r.value_adjusted = None;
    let kpis = vec![r];
    let mut reference: BandReference = [(metric, stat(metric, 50.0, 55.0, 58.0, 65.0))]
        .into_iter()
        .collect();
    let endgame = |reference: &BandReference| {
        compute_area_scores(&kpis, Some(reference))
            .into_iter()
            .find(|s| s.area == Area::Endgame)
            .unwrap()
    };
    let e = endgame(&reference);
    assert_eq!(
        (e.score, e.sufficient, e.need_more, e.n),
        (None, false, None, 500)
    );
    // A benchmarked component with no data reports what it needs.
    let w8 = MetricId::Weakness(WeaknessKey::W8Basics);
    reference.insert(w8, stat(w8, 0.01, 0.02, 0.03, 0.05));
    let e = endgame(&reference);
    assert_eq!((e.need_more, e.n), (Some(80), 0));
}

#[test]
fn without_a_reference_no_area_asks_for_more_games() {
    let kpis = vec![value_row(base(BaseMetric::AccuracyEndgame), 60.0, 12)];
    for a in compute_area_scores(&kpis, None) {
        assert_eq!(
            (a.score, a.sufficient, a.need_more),
            (None, false, None),
            "{:?}",
            a.area
        );
    }
}

#[test]
fn duplicates_are_merged_or_refused_independent_of_order() {
    let mut games = plain_games(6);
    for (i, o) in games.iter_mut().enumerate() {
        o.metrics.blunders = i as u32;
    }
    let clean = compute_kpis(&kpi_games(&games), &LAST50, None);
    // Identical copies change nothing.
    let mut with_copies = games.clone();
    with_copies.push(games[2].clone());
    with_copies.push(games[2].clone());
    let report = compute_kpis_report(&kpi_games(&with_copies), &LAST50, None);
    assert_eq!(report.skipped.duplicate_game, 2);
    assert_eq!(report.values, clean);
    // A conflicting copy refuses the id, whichever copy comes first.
    let mut conflict = games[3].clone();
    conflict.metrics.blunders = 30;
    games.push(conflict);
    let first = compute_kpis_report(&kpi_games(&games), &LAST50, None);
    assert_eq!(first.skipped.conflicting_duplicates, 1);
    let bl = row(&first.values, base(BaseMetric::BlundersPerGame));
    // Games 0, 1, 2, 4, 5.
    assert_eq!((bl.value, bl.n), (Some(12.0 / 5.0), 5));
    let mut rng = Rng::new(3);
    for _ in 0..10 {
        shuffle(&mut rng, &mut games);
        assert_eq!(
            compute_kpis_report(&kpi_games(&games), &LAST50, None),
            first
        );
    }
}
