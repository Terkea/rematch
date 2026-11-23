//! A v3 trajectory for the contract tests (ADR 0088): a fall from a peak, with the peak window
//! against the last games, and one weakness compared.
#![allow(dead_code)]

use coaching_input::{
    AreaId, ChangeVerdict, ColorMetric, MetricUnit, MonthRating, RatingAt, ThenNowMetric,
    ThenNowWeakness, ThenVsNow, Trajectory, TrajectoryKind, TrajectoryWindow, WindowBasis,
};
use time::macros::date;

pub fn metric(v: (f64, f64, f64), unit: MetricUnit, n: u32) -> ColorMetric {
    ColorMetric {
        value: Some(v.0),
        unit,
        n,
        low: Some(v.1),
        high: Some(v.2),
        sufficient: true,
        need_more: None,
    }
}

fn called(
    then: ColorMetric,
    now: ColorMetric,
    verdict: ChangeVerdict,
    d: (f64, f64, f64),
) -> ThenNowMetric {
    ThenNowMetric {
        then,
        now,
        verdict,
        diff: Some(d.0),
        diff_low: Some(d.1),
        diff_high: Some(d.2),
    }
}

pub fn trajectory() -> Trajectory {
    Trajectory {
        rated_games: 140,
        sufficient: true,
        need_more: None,
        kind: Some(TrajectoryKind::Falling),
        peak: Some(RatingAt {
            rating: 1097,
            date: date!(2026 - 04 - 18),
        }),
        current: Some(RatingAt {
            rating: 816,
            date: date!(2026 - 11 - 20),
        }),
        change: Some(-281),
        months: vec![
            MonthRating {
                month: "2026-04".into(),
                games: 31,
                last: 1080,
                high: 1097,
            },
            MonthRating {
                month: "2026-11".into(),
                games: 22,
                last: 816,
                high: 870,
            },
        ],
        then_vs_now: Some(ThenVsNow {
            then: TrajectoryWindow {
                basis: WindowBasis::Peak,
                games: 30,
                from: date!(2026 - 04 - 02),
                to: date!(2026 - 05 - 01),
            },
            now: TrajectoryWindow {
                basis: WindowBasis::Last,
                games: 30,
                from: date!(2026 - 10 - 25),
                to: date!(2026 - 11 - 20),
            },
            rating: called(
                metric((1071.0, 1062.0, 1080.0), MetricUnit::Elo, 30),
                metric((842.0, 831.0, 853.0), MetricUnit::Elo, 30),
                ChangeVerdict::Worse,
                (-229.0, -243.0, -215.0),
            ),
            accuracy: called(
                metric((69.2, 66.8, 71.5), MetricUnit::Pct, 28),
                metric((66.1, 63.9, 68.4), MetricUnit::Pct, 29),
                ChangeVerdict::NoClearChange,
                (-3.1, -6.3, 0.2),
            ),
            blunders_per_game: called(
                metric((1.1, 0.9, 1.4), MetricUnit::PerGame, 28),
                metric((1.6, 1.3, 1.9), MetricUnit::PerGame, 29),
                ChangeVerdict::Worse,
                (0.5, 0.1, 0.9),
            ),
            score: called(
                metric((0.55, 0.44, 0.65), MetricUnit::Rate, 30),
                metric((0.42, 0.32, 0.53), MetricUnit::Rate, 30),
                ChangeVerdict::NoClearChange,
                (-0.13, -0.28, 0.02),
            ),
            weaknesses: vec![ThenNowWeakness {
                series_key: "W4.missed".into(),
                id: "W4".into(),
                variant: Some("missed".into()),
                label: "Missed mates".into(),
                area: AreaId::Tactics,
                then: metric((0.3, 0.26, 0.34), MetricUnit::Rate, 30),
                now: metric((0.43, 0.39, 0.47), MetricUnit::Rate, 30),
                verdict: ChangeVerdict::Worse,
                then_events: 25,
                then_opportunities: 84,
                now_events: 38,
                now_opportunities: 88,
            }],
        }),
    }
}

/// Below the minimum: only the count, the shortfall and the months.
pub fn short() -> Trajectory {
    Trajectory {
        rated_games: 12,
        sufficient: false,
        need_more: Some(8),
        kind: None,
        peak: None,
        current: None,
        change: None,
        months: vec![MonthRating {
            month: "2026-11".into(),
            games: 12,
            last: 905,
            high: 921,
        }],
        then_vs_now: None,
    }
}
