//! The colour split of the CoachingInput (v2 `byColor`, ADR 0085) and its `benchmark`.
//!
//! Per colour, over the player's games with that colour:
//! - `games`, W-D-L and `score` over the colour's most recent analysed games, at most
//!   [`LAST_N`] (the KPI `last50` window): points per game with its 80% Wilson interval;
//! - `accuracy` and `blundersPerGame` from the colour's stored `last50` snapshot rows (ADR 0082),
//!   with their stored intervals;
//! - `weaknesses`: the colour's estimates (its own series points, its own games in the 6-month
//!   window, its own benchmark rows, never the both ones), claimable against its own peers
//!   (priority > 0) first by priority, then uncompared ones by rate; never below the minimum
//!   evidence, `MIN_GAMES` (30) games of the colour or a single event.
//!
//! Minimum evidence is kept per colour: below [`MIN_COLOR_GAMES`] games the colour, and below a
//! metric's own minimum the metric, is `sufficient: false` with null numbers (never 0).

use std::cmp::Ordering;

use coaching_input::{
    Benchmark, BenchmarkSource, ColorMetric, ColorSummary, ColorWeakness, LIMITS, MIN_COLOR_GAMES,
    MetricUnit,
};
use profile_core::constants::{RANKABLE, weakness_area, weakness_label};
use profile_core::kpi::catalogue::kpi_catalogue;
use profile_core::kpi::snapshots::{KpiValue, clamp_to_unit};
use profile_core::stats::intervals::{Z80, wilson};
use profile_core::stats::priority::{EstimateReason, WeaknessEstimate};
use profile_core::types::{BandReference, BaseMetric, GameContext, MetricId, SeriesKey};

use crate::snapshots::LAST_N;

fn finite(x: f64) -> Option<f64> {
    x.is_finite().then_some(x)
}

fn in_unit(x: f64) -> Option<f64> {
    finite(x).filter(|v| (0.0..=1.0).contains(v))
}

fn count(x: f64) -> Option<u32> {
    finite(x)
        .filter(|v| *v >= 0.0 && *v <= f64::from(u32::MAX))
        .map(|v| v.round() as u32)
}

/// An insufficient metric: no number, and how many more observations it needs (at least 1).
pub fn insufficient(unit: MetricUnit, n: u32, min: u32) -> ColorMetric {
    ColorMetric {
        value: None,
        unit,
        n,
        low: None,
        high: None,
        sufficient: false,
        need_more: Some(min.saturating_sub(n).max(1)),
    }
}

/// W-D-L and points of the colour's most recent games (at most [`LAST_N`]); games without a
/// result are not counted.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Results {
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
}

impl Results {
    pub fn games(&self) -> u32 {
        self.wins + self.draws + self.losses
    }

    pub fn points(&self) -> f64 {
        f64::from(self.wins) + 0.5 * f64::from(self.draws)
    }
}

/// The colour's results: `contexts` are the colour's games, in any order.
pub fn results<'a>(contexts: impl IntoIterator<Item = &'a GameContext>) -> Results {
    let mut scored: Vec<(&GameContext, f64)> = contexts
        .into_iter()
        .filter_map(|c| Some((c, c.score.filter(|s| s.is_finite())?)))
        .collect();
    scored.sort_by(|a, b| {
        b.0.played_at
            .cmp(&a.0.played_at)
            .then(a.0.game_id.cmp(&b.0.game_id))
    });
    let mut r = Results::default();
    for (_, s) in scored.into_iter().take(LAST_N) {
        if s >= 1.0 {
            r.wins += 1;
        } else if s <= 0.0 {
            r.losses += 1;
        } else {
            r.draws += 1;
        }
    }
    r
}

/// Points per game with its 80% Wilson interval (which always holds the observed share);
/// insufficient below [`MIN_COLOR_GAMES`] games.
pub fn score_metric(r: &Results) -> ColorMetric {
    let n = r.games();
    if n < MIN_COLOR_GAMES {
        return insufficient(MetricUnit::Rate, n, MIN_COLOR_GAMES);
    }
    let value = r.points() / f64::from(n);
    let Ok(i) = wilson(r.points(), f64::from(n), Z80) else {
        return insufficient(MetricUnit::Rate, n, MIN_COLOR_GAMES);
    };
    ColorMetric {
        value: Some(value),
        unit: MetricUnit::Rate,
        n,
        low: Some(i.low.clamp(0.0, value)),
        high: Some(i.high.clamp(value, 1.0)),
        sufficient: true,
        need_more: None,
    }
}

/// A colour's `last50` snapshot row of `metric` as a [`ColorMetric`]: sufficient when the colour
/// is and the row's `n` meets the metric's own minimum; the value and interval clamped to the
/// unit (the interval only when both ends are finite and ordered around the value).
pub fn snapshot_metric(
    metric: BaseMetric,
    unit: MetricUnit,
    row: Option<&KpiValue>,
    colour_sufficient: bool,
) -> ColorMetric {
    let min = kpi_catalogue()
        .iter()
        .find(|d| d.metric == MetricId::Base(metric))
        .map_or(MIN_COLOR_GAMES, |d| d.min_n);
    let n = row.map_or(0, |r| r.n);
    let value = row.and_then(|r| r.value).and_then(finite);
    let (Some(value), true, true) = (value, colour_sufficient, n >= min) else {
        return insufficient(unit, n, min);
    };
    let clamp = |x: f64| clamp_to_unit(x, Some(unit)).clamp(0.0, unit.max());
    let value = clamp(value);
    let (low, high) = match row.and_then(|r| Some((finite(r.ci_low?)?, finite(r.ci_high?)?))) {
        Some((lo, hi)) if clamp(lo) <= value && value <= clamp(hi) => {
            (Some(clamp(lo)), Some(clamp(hi)))
        }
        _ => (None, None),
    };
    ColorMetric {
        value: Some(value),
        unit,
        n,
        low,
        high,
        sufficient: true,
        need_more: None,
    }
}

/// A colour's estimate as a [`ColorWeakness`] when it may be stated (module docs); `None`
/// otherwise, or when a number is unknown or the interval does not hold the rate.
pub fn color_weakness(
    e: &WeaknessEstimate,
    reference: Option<&BandReference>,
) -> Option<ColorWeakness> {
    let compared = match e.reason {
        EstimateReason::Ok => e.priority > 0.0 && e.priority.is_finite(),
        EstimateReason::NoBenchmark => false,
        EstimateReason::InsufficientData | EstimateReason::NoEvents => return None,
    };
    if !e.min_evidence_met || e.k_unweighted <= 0.0 || (e.reason == EstimateReason::Ok && !compared)
    {
        return None;
    }
    let rate = in_unit(e.posterior.mean)?;
    let low = in_unit(e.posterior.interval.low)?;
    let high = in_unit(e.posterior.interval.high)?;
    if !(low <= rate && rate <= high) {
        return None;
    }
    let events = count(e.k_unweighted)?;
    let opportunities = count(e.n_unweighted)?;
    if events > opportunities {
        return None;
    }
    let (peer_rate, peer_percentile, peer_players) = if compared {
        let stat = reference.and_then(|r| r.get(&MetricId::Weakness(e.key)));
        (
            Some(in_unit(e.band_median?)?),
            e.percentile.and_then(finite).map(|p| p.clamp(0.0, 100.0)),
            stat.map(|s| s.n_players),
        )
    } else {
        (None, None, None)
    };
    let (id, variant) = e.key.contract_id();
    Some(ColorWeakness {
        series_key: SeriesKey::Weakness(e.key).to_string(),
        id: id.to_owned(),
        variant: variant.map(str::to_owned),
        label: weakness_label(e.key).to_owned(),
        area: weakness_area(e.key),
        rate,
        low,
        high,
        events,
        opportunities,
        peer_rate,
        peer_percentile,
        peer_players,
    })
}

/// The colour's top weaknesses: compared ones by priority, then uncompared ones by rate, at most
/// `LIMITS.color_weaknesses`.
pub fn color_weaknesses(
    estimates: &[WeaknessEstimate],
    reference: Option<&BandReference>,
) -> Vec<ColorWeakness> {
    let mut picked: Vec<(&WeaknessEstimate, ColorWeakness)> = estimates
        .iter()
        .filter(|e| RANKABLE.contains(&e.key))
        .filter_map(|e| Some((e, color_weakness(e, reference)?)))
        .collect();
    picked.sort_by(|(a, wa), (b, wb)| {
        let compared = |w: &ColorWeakness| w.peer_rate.is_some();
        compared(wb)
            .cmp(&compared(wa))
            .then_with(|| b.priority.total_cmp(&a.priority))
            .then_with(|| wb.rate.partial_cmp(&wa.rate).unwrap_or(Ordering::Equal))
            .then_with(|| wa.series_key.cmp(&wb.series_key))
    });
    picked
        .into_iter()
        .map(|(_, w)| w)
        .take(LIMITS.color_weaknesses)
        .collect()
}

/// One colour's summary (module docs). `snapshots` are the colour's `last50` rows.
pub fn color_summary<'a>(
    contexts: impl IntoIterator<Item = &'a GameContext>,
    snapshots: &[KpiValue],
    estimates: &[WeaknessEstimate],
    reference: Option<&BandReference>,
) -> ColorSummary {
    let r = results(contexts);
    let games = r.games();
    let sufficient = games >= MIN_COLOR_GAMES;
    let row = |m: BaseMetric| snapshots.iter().find(|k| k.metric == MetricId::Base(m));
    ColorSummary {
        games,
        sufficient,
        need_more: (!sufficient).then(|| MIN_COLOR_GAMES - games),
        wins: r.wins,
        draws: r.draws,
        losses: r.losses,
        score: score_metric(&r),
        accuracy: snapshot_metric(
            BaseMetric::Accuracy,
            MetricUnit::Pct,
            row(BaseMetric::Accuracy),
            sufficient,
        ),
        blunders_per_game: snapshot_metric(
            BaseMetric::BlundersPerGame,
            MetricUnit::PerGame,
            row(BaseMetric::BlundersPerGame),
            sufficient,
        ),
        weaknesses: color_weaknesses(estimates, reference),
    }
}

/// The stored benchmark's band as the contract states it (`source = peers` needs all four
/// numbers, else there is none).
pub fn benchmark_info(
    source: &str,
    rating_low: Option<i32>,
    rating_high: Option<i32>,
    players: Option<i32>,
    games: Option<i32>,
) -> Option<Benchmark> {
    let rating = |v: Option<i32>| {
        v.and_then(|v| u16::try_from(v).ok())
            .filter(|v| *v <= coaching_input::limits::MAX_RATING)
    };
    let n = |v: Option<i32>| {
        v.and_then(|v| u32::try_from(v).ok())
            .filter(|v| *v <= coaching_input::limits::MAX_COUNT)
    };
    match source {
        "peers" => {
            let (low, high) = (rating(rating_low)?, rating(rating_high)?);
            (low <= high).then_some(Benchmark {
                source: BenchmarkSource::Peers,
                rating_low: Some(low),
                rating_high: Some(high),
                players: Some(n(players)?),
                games: Some(n(games)?),
            })
        }
        "lichess_bands" => Some(Benchmark {
            source: BenchmarkSource::LichessBands,
            rating_low: None,
            rating_high: None,
            players: None,
            games: None,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use profile_core::stats::posterior::posterior;
    use profile_core::types::{PlatformId, WeaknessKey};
    use time::macros::datetime;
    use uuid::Uuid;

    fn ctx(i: u32, score: Option<f64>) -> GameContext {
        GameContext {
            game_id: Uuid::from_u128(u128::from(i) + 1),
            platform_game_id: format!("g{i}"),
            platform: PlatformId::Chesscom,
            played_at: datetime!(2026-11-01 00:00 UTC) + time::Duration::hours(i.into()),
            time_class: chess_core::TimeClass::Rapid,
            base_seconds: Some(600),
            increment_seconds: Some(0),
            rated: true,
            player_color: chess_core::Color::White,
            player_rating: Some(800),
            opponent_rating: Some(820),
            result: "*".into(),
            score,
            termination: None,
            accuracy: None,
            protocol: "p".into(),
        }
    }

    fn kpi(metric: BaseMetric, value: f64, n: u32, ci: Option<(f64, f64)>) -> KpiValue {
        KpiValue {
            metric: MetricId::Base(metric),
            window_kind: profile_core::kpi::snapshots::WindowKind::LastN,
            window_key: "last50".into(),
            value: Some(value),
            value_adjusted: None,
            value_adjusted_raw: None,
            n,
            ci_low: ci.map(|c| c.0),
            ci_high: ci.map(|c| c.1),
        }
    }

    fn estimate(
        key: WeaknessKey,
        reason: EstimateReason,
        k: f64,
        n: f64,
        priority: f64,
    ) -> WeaknessEstimate {
        let post = posterior(k, n, 0.5, 2.0);
        let compared = reason == EstimateReason::Ok;
        WeaknessEstimate {
            key,
            k,
            n,
            n_unweighted: n,
            k_unweighted: k,
            posterior: post,
            band_median: compared.then_some(0.05),
            band_p40: compared.then_some(0.045),
            percentile: compared.then_some(30.0),
            confidence: if compared { 0.9 } else { 0.0 },
            excess: 0.1,
            trainability: 1.0,
            priority,
            precise: false,
            min_evidence_met: true,
            reason,
            invalid_points: 0,
        }
    }

    #[test]
    fn results_take_the_most_recent_games_with_a_result() {
        let games: Vec<GameContext> = (0..60)
            .map(|i| {
                ctx(
                    i,
                    Some(if i < 10 {
                        0.0
                    } else if i % 2 == 0 {
                        1.0
                    } else {
                        0.5
                    }),
                )
            })
            .chain([ctx(100, None)])
            .collect();
        let r = results(&games);
        // The 50 newest scored games are 10..60: 25 wins, 25 draws, no losses.
        assert_eq!(
            r,
            Results {
                wins: 25,
                draws: 25,
                losses: 0
            }
        );
        assert_eq!(r.games(), 50);
    }

    #[test]
    fn score_is_insufficient_below_ten_games_never_zero() {
        let r = Results {
            wins: 0,
            draws: 0,
            losses: 9,
        };
        let m = score_metric(&r);
        assert_eq!(m.value, None);
        assert_eq!((m.low, m.high), (None, None));
        assert_eq!((m.sufficient, m.need_more, m.n), (false, Some(1), 9));
        let none = score_metric(&Results::default());
        assert_eq!((none.value, none.need_more), (None, Some(10)));
    }

    #[test]
    fn score_has_an_interval_around_the_share() {
        let m = score_metric(&Results {
            wins: 6,
            draws: 2,
            losses: 4,
        });
        let v = m.value.unwrap();
        assert!((v - 7.0 / 12.0).abs() < 1e-12);
        let (lo, hi) = (m.low.unwrap(), m.high.unwrap());
        assert!(lo < v && v < hi && lo > 0.3 && hi < 0.8, "{lo} {hi}");
        // Every game won: the interval still holds the share.
        let all = score_metric(&Results {
            wins: 12,
            draws: 0,
            losses: 0,
        });
        assert_eq!(all.value, Some(1.0));
        assert_eq!(all.high, Some(1.0));
        assert!(all.low.unwrap() < 1.0);
    }

    #[test]
    fn snapshot_metric_keeps_the_metric_minimum_and_the_colour_minimum() {
        let row = kpi(BaseMetric::Accuracy, 78.0, 25, Some((76.0, 80.0)));
        let m = snapshot_metric(BaseMetric::Accuracy, MetricUnit::Pct, Some(&row), true);
        assert_eq!(
            (m.value, m.low, m.high, m.n),
            (Some(78.0), Some(76.0), Some(80.0), 25)
        );
        // Accuracy needs 20 games (the catalogue's min_n).
        let few = kpi(BaseMetric::Accuracy, 78.0, 15, Some((70.0, 85.0)));
        let m = snapshot_metric(BaseMetric::Accuracy, MetricUnit::Pct, Some(&few), true);
        assert_eq!((m.value, m.sufficient, m.need_more), (None, false, Some(5)));
        // An insufficient colour states nothing, whatever the row says.
        let m = snapshot_metric(BaseMetric::Accuracy, MetricUnit::Pct, Some(&row), false);
        assert_eq!((m.value, m.sufficient), (None, false));
        // No row: n 0, never a 0 value.
        let m = snapshot_metric(BaseMetric::BlundersPerGame, MetricUnit::PerGame, None, true);
        assert_eq!((m.value, m.n, m.need_more), (None, 0, Some(20)));
        // A broken interval is dropped, not widened.
        let bad = kpi(BaseMetric::BlundersPerGame, 1.0, 30, Some((1.2, 1.5)));
        let m = snapshot_metric(
            BaseMetric::BlundersPerGame,
            MetricUnit::PerGame,
            Some(&bad),
            true,
        );
        assert_eq!((m.value, m.low, m.high), (Some(1.0), None, None));
    }

    #[test]
    fn colour_weaknesses_are_compared_first_and_never_below_minimum_evidence() {
        let mut reference = BandReference::new();
        let mut stat = profile_core::types::BandReferenceStat {
            metric: MetricId::Weakness(WeaknessKey::W4Missed),
            median: 0.05,
            p25: 0.03,
            p40: 0.045,
            p75: 0.07,
            pooled_rate: Some(0.05),
            epl_per_event: Some(0.3),
            opps_per_game: Some(1.0),
            opp_slope_per_100: None,
            n_players: 14,
            n_games: 30,
            low_sample: false,
        };
        reference.insert(stat.metric, stat.clone());
        stat.metric = MetricId::Weakness(WeaknessKey::W2);
        reference.insert(stat.metric, stat);
        let estimates = vec![
            estimate(
                WeaknessKey::W1,
                EstimateReason::NoBenchmark,
                30.0,
                300.0,
                0.0,
            ),
            estimate(WeaknessKey::W4Missed, EstimateReason::Ok, 9.0, 61.0, 0.4),
            estimate(WeaknessKey::W2, EstimateReason::Ok, 20.0, 100.0, 0.9),
            // Better than its peers: not a weakness.
            estimate(WeaknessKey::W3Shallow, EstimateReason::Ok, 1.0, 100.0, 0.0),
            // Below 30 games of the colour, or no event: nothing is stated.
            estimate(
                WeaknessKey::W5,
                EstimateReason::InsufficientData,
                5.0,
                20.0,
                0.0,
            ),
            estimate(WeaknessKey::W6, EstimateReason::NoEvents, 0.0, 50.0, 0.0),
            // Not rankable.
            estimate(
                WeaknessKey::W12,
                EstimateReason::NoBenchmark,
                5.0,
                20.0,
                0.0,
            ),
        ];
        let got = color_weaknesses(&estimates, Some(&reference));
        let keys: Vec<&str> = got.iter().map(|w| w.series_key.as_str()).collect();
        assert_eq!(keys, vec!["W2", "W4.missed", "W1"]);
        let w4 = &got[1];
        assert_eq!(
            (w4.id.as_str(), w4.variant.as_deref()),
            ("W4", Some("missed"))
        );
        assert_eq!(
            (w4.peer_rate, w4.peer_percentile, w4.peer_players),
            (Some(0.05), Some(30.0), Some(14))
        );
        assert_eq!((w4.events, w4.opportunities), (9, 61));
        assert_eq!(got[2].peer_rate, None, "uncompared: no peer numbers");
        let mut thin = estimates[1].clone();
        thin.min_evidence_met = false;
        assert_eq!(color_weakness(&thin, Some(&reference)), None);
    }

    #[test]
    fn summary_of_a_colour_below_the_minimum_says_insufficient() {
        let games: Vec<GameContext> = (0..4).map(|i| ctx(i, Some(1.0))).collect();
        let rows = [kpi(BaseMetric::Accuracy, 80.0, 4, None)];
        let s = color_summary(&games, &rows, &[], None);
        assert_eq!(
            (s.games, s.wins, s.sufficient, s.need_more),
            (4, 4, false, Some(6))
        );
        assert_eq!(s.score.value, None);
        assert_eq!(s.accuracy.value, None);
        assert_eq!(s.blunders_per_game.value, None);
        assert!(s.weaknesses.is_empty());
    }

    #[test]
    fn benchmark_info_states_a_peer_band_or_nothing() {
        let b = benchmark_info("peers", Some(675), Some(975), Some(25), Some(61)).unwrap();
        assert_eq!(
            (b.rating_low, b.rating_high, b.players),
            (Some(675), Some(975), Some(25))
        );
        assert_eq!(
            benchmark_info("peers", Some(675), None, Some(25), Some(61)),
            None
        );
        assert_eq!(
            benchmark_info("peers", Some(900), Some(800), Some(1), Some(1)),
            None
        );
        let l = benchmark_info("lichess_bands", None, None, None, None).unwrap();
        assert_eq!(l.source, BenchmarkSource::LichessBands);
        assert_eq!(benchmark_info("other", None, None, None, None), None);
    }
}
