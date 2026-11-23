//! The ranking floor and the evaluation baseline (ADR 0101, issues #10 and #12).
//!
//! A weakness ranks only when its 80% interval lies above the benchmark median and its excess
//! clears [`RANK_MIN_EXCESS_PER_GAME`]. W5 has no peer row by design, so it is ranked against
//! what the evaluation itself predicts from a winning episode ([`model_baseline`]).
//!
//! The counts are the Tier 1 smoke suite's (`docs/benchmark.md`, first run, 80 games).

use profile_core::stats::priority::{
    EstimateInput, EstimateReason, MODEL_LOSS_PER_WINNING_EPISODE, RANK_MIN_EXCESS_PER_GAME,
    estimate_weakness, model_baseline,
};
use profile_core::types::{Band, BandReferenceStat, MetricId, WeaknessKey};

fn peer(key: WeaknessKey, median: f64, epl: f64) -> BandReferenceStat {
    BandReferenceStat {
        metric: MetricId::Weakness(key),
        median,
        p25: median * 0.8,
        p40: median * 0.95,
        p75: median * 1.3,
        pooled_rate: Some(median),
        epl_per_event: Some(epl),
        opps_per_game: None,
        opp_slope_per_100: None,
        n_players: 16,
        n_games: 80,
        low_sample: false,
    }
}

/// `games` points, two games a day (the smoke suite's pace, newest last), spreading `chances`
/// over the games and `events` over the first chances.
fn points(events: u32, chances: u32, games: u32) -> Vec<(f64, f64, f64, f64)> {
    let mut left = events;
    (0..games)
        .map(|g| {
            let n = chances * (g + 1) / games - chances * g / games;
            let k = left.min(n);
            left -= k;
            let age = f64::from(games - 1 - g) / 2.0;
            (f64::from(k), f64::from(n), 0.0, age)
        })
        .collect()
}

fn estimate(
    key: WeaknessKey,
    pts: &[(f64, f64, f64, f64)],
    r: &BandReferenceStat,
) -> profile_core::stats::priority::WeaknessEstimate {
    estimate_weakness(&EstimateInput {
        key,
        points: pts,
        games_in_window: 80,
        band: Band::B3,
        reference: Some(r),
        fatigue: 0.0,
    })
}

#[test]
fn an_interval_that_overlaps_the_median_is_not_ranked() {
    // bench-clean's W9b: 11 of 846 against a peer median of 0.008 (confidence 0.87).
    let r = peer(WeaknessKey::W9b, 0.008, 0.3);
    let e = estimate(WeaknessKey::W9b, &points(11, 846, 80), &r);
    assert_eq!(e.reason, EstimateReason::Ok);
    assert!(e.posterior.interval.low <= 0.008, "{:?}", e.posterior);
    assert!(e.confidence > 0.5 && e.confidence < 0.9, "{}", e.confidence);
    assert!(e.excess > 0.0);
    assert!(!e.credible);
    assert_eq!(e.priority, 0.0);
}

#[test]
fn a_clear_but_cheap_excess_is_not_ranked() {
    // bench-clean's W4.allowed: 3 of 4274 against 0.0001. The interval clears the median, but
    // the excess is about 0.01 of a point per game.
    let r = peer(WeaknessKey::W4Allowed, 0.0001, 0.08);
    let e = estimate(WeaknessKey::W4Allowed, &points(3, 4274, 80), &r);
    assert!(e.posterior.interval.low > 0.0001, "{:?}", e.posterior);
    assert!(
        e.excess > 0.0 && e.excess < RANK_MIN_EXCESS_PER_GAME,
        "{}",
        e.excess
    );
    assert!(!e.credible);
    assert_eq!(e.priority, 0.0);
}

#[test]
fn a_clear_and_costly_excess_is_ranked() {
    // bench-lowclock's W9b: 40 of 430 against 0.0065.
    let r = peer(WeaknessKey::W9b, 0.0065, 0.3);
    let e = estimate(WeaknessKey::W9b, &points(40, 430, 80), &r);
    assert!(e.posterior.interval.low > 0.0065);
    assert!(e.excess >= RANK_MIN_EXCESS_PER_GAME, "{}", e.excess);
    assert!(e.credible);
    assert!(e.priority > 0.0);
}

#[test]
fn the_floor_is_a_fiftieth_of_a_point_per_game() {
    assert_eq!(RANK_MIN_EXCESS_PER_GAME, 0.02);
}

#[test]
fn the_w5_baseline_is_what_the_evaluation_predicts() {
    // A winning episode starts at W >= 85, so the evaluation expects at least 0.85 of a point.
    assert!((MODEL_LOSS_PER_WINNING_EPISODE - 0.15).abs() < 1e-12);
    // Every failure a loss: the baseline rate is 0.15. Every failure a draw (half a point each):
    // twice as many failures lose the same 0.15.
    let loss = model_baseline(WeaknessKey::W5, Some(1.0)).expect("W5 has one");
    assert!((loss.median - 0.15).abs() < 1e-12);
    assert_eq!(loss.pooled_rate, Some(loss.median));
    assert_eq!(loss.epl_per_event, Some(1.0));
    let draw = model_baseline(WeaknessKey::W5, Some(0.5)).expect("W5 has one");
    assert!((draw.median - 0.3).abs() < 1e-12);
    // Without a failure to measure the cost by, a failure is taken as a loss.
    let none = model_baseline(WeaknessKey::W5, None).expect("W5 has one");
    assert!((none.median - 0.15).abs() < 1e-12);
    // Only W5: every other key without a peer row stays not compared.
    for k in WeaknessKey::ALL
        .into_iter()
        .filter(|k| *k != WeaknessKey::W5)
    {
        assert!(model_baseline(k, Some(1.0)).is_none(), "{k:?}");
    }
}

#[test]
fn w5_ranks_a_player_who_throws_wins_away_and_not_a_clean_one() {
    // bench-noconvert: 11 of 21 winning episodes not won; bench-clean: 4 of 19.
    let base = model_baseline(WeaknessKey::W5, Some(0.9)).expect("W5 has one");
    let thrower = model_estimate(&points(11, 21, 80), &base);
    assert!(thrower.credible, "{thrower:?}");
    assert!(thrower.priority > 0.0);
    let clean = model_estimate(&points(4, 19, 80), &base);
    assert!(!clean.credible, "{clean:?}");
    assert_eq!(clean.priority, 0.0);
}

#[test]
fn the_evaluation_is_a_yardstick_not_a_prior() {
    // The evaluation's prediction is no population of players, so it does not shrink the rate:
    // the posterior keeps the flat Beta(1, 1) prior and the row only sets the comparison.
    use profile_core::stats::priority::estimate_weakness_against_model;
    let base = model_baseline(WeaknessKey::W5, Some(0.9)).expect("W5 has one");
    let pts = points(11, 21, 80);
    let e = estimate_weakness_against_model(&EstimateInput {
        key: WeaknessKey::W5,
        points: &pts,
        games_in_window: 80,
        band: Band::B3,
        reference: Some(&base),
        fatigue: 0.0,
    });
    assert_eq!(e.posterior.alpha, 1.0 + e.k);
    assert_eq!(e.posterior.beta, 1.0 + e.n - e.k);
    assert!(e.model_baseline);
    assert_eq!(e.band_median, Some(base.median));
    assert!(e.credible);
}

fn model_estimate(
    pts: &[(f64, f64, f64, f64)],
    r: &BandReferenceStat,
) -> profile_core::stats::priority::WeaknessEstimate {
    profile_core::stats::priority::estimate_weakness_against_model(&EstimateInput {
        key: WeaknessKey::W5,
        points: pts,
        games_in_window: 80,
        band: Band::B3,
        reference: Some(r),
        fatigue: 0.0,
    })
}
