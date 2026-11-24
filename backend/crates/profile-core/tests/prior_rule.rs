//! The prior rules the lab compares (EXP-0006, `stats::variant`): the champion is the production
//! estimate unchanged, and each option of a challenger does what its doc says.

use profile_core::constants::n0;
use profile_core::stats::posterior::posterior;
use profile_core::stats::priority::{
    EstimateInput, estimate_weakness, estimate_weakness_against_model_with, estimate_weakness_with,
};
use profile_core::stats::variant::{MAX_DISPERSION, PriorRule};
use profile_core::stats::weights::{HALF_LIFE_DAYS, recency_weight};
use profile_core::types::{Band, BandReferenceStat, MetricId, WeaknessKey};

fn reference(key: WeaknessKey, rate: f64) -> BandReferenceStat {
    BandReferenceStat {
        metric: MetricId::Weakness(key),
        median: rate,
        p25: rate * 0.6,
        p40: rate * 0.85,
        p75: rate * 1.5,
        pooled_rate: Some(rate),
        epl_per_event: Some(0.3),
        opps_per_game: None,
        opp_slope_per_100: None,
        n_players: 40,
        n_games: 1200,
        low_sample: false,
    }
}

/// 60 games, one a day, `events` of `chances` each.
fn series(events: &[f64], chances: f64) -> Vec<(f64, f64, f64, f64)> {
    (0..60)
        .map(|g| {
            (
                events[g % events.len()],
                chances,
                0.0,
                f64::from(59 - g as u32),
            )
        })
        .collect()
}

fn input<'a>(
    key: WeaknessKey,
    points: &'a [(f64, f64, f64, f64)],
    r: &'a BandReferenceStat,
) -> EstimateInput<'a> {
    EstimateInput {
        key,
        points,
        games_in_window: 60,
        band: Band::B2,
        reference: Some(r),
        fatigue: 0.0,
    }
}

const DEV: fn(Option<f64>, bool, bool) -> PriorRule = |g, ess, disp| PriorRule {
    prior_games: g,
    effective_counts: ess,
    dispersion: disp,
};

#[test]
fn the_champion_is_production() {
    let r = reference(WeaknessKey::W4Missed, 0.3);
    let pts = series(&[1.0, 0.0, 1.0], 1.0);
    let i = input(WeaknessKey::W4Missed, &pts, &r);
    assert_eq!(
        estimate_weakness(&i),
        estimate_weakness_with(&i, PriorRule::CHAMPION)
    );
}

#[test]
fn prior_games_caps_the_prior_at_that_many_games() {
    let r = reference(WeaknessKey::W4Missed, 0.3);
    let pts = series(&[1.0, 0.0, 1.0], 1.0);
    let i = input(WeaknessKey::W4Missed, &pts, &r);
    let champ = estimate_weakness_with(&i, PriorRule::CHAMPION);
    let two = estimate_weakness_with(&i, DEV(Some(2.0), false, false));
    // Two games' worth of the weighted chances, below n0 = 40.
    let n0_eff = 2.0 * champ.n / 60.0;
    assert!(n0_eff < n0(WeaknessKey::W4Missed));
    assert_eq!(two.posterior, posterior(champ.k, champ.n, 0.3, n0_eff));
    // The player's 2-in-3 rate pulls the mean further from the peers' 0.3 than n0 = 40 allows.
    assert!(two.posterior.mean > champ.posterior.mean);
    // A cap above n0 changes nothing.
    let many = estimate_weakness_with(&i, DEV(Some(1e6), false, false));
    assert_eq!(many.posterior, champ.posterior);
}

#[test]
fn effective_counts_are_kish() {
    let r = reference(WeaknessKey::W1, 0.004);
    let pts = series(&[1.0, 0.0, 0.0, 2.0], 50.0);
    let i = input(WeaknessKey::W1, &pts, &r);
    let e = estimate_weakness_with(&i, DEV(None, true, false));
    let w: Vec<f64> = pts
        .iter()
        .map(|p| recency_weight(p.3, HALF_LIFE_DAYS))
        .collect();
    let s1: f64 = w.iter().map(|w| w * 50.0).sum();
    let s2: f64 = w.iter().map(|w| w * w * 50.0).sum();
    let k: f64 = pts.iter().zip(&w).map(|(p, w)| w * p.0).sum();
    assert!((e.n - s1 * s1 / s2).abs() < 1e-9);
    assert!((e.k / e.n - k / s1).abs() < 1e-12);
    // More effective chances than the weighted sum: the weights are below 1.
    let champ = estimate_weakness_with(&i, PriorRule::CHAMPION);
    assert!(e.n > champ.n);
    // Every game today: the effective counts are the raw ones.
    let today: Vec<_> = pts.iter().map(|p| (p.0, p.1, p.2, 0.0)).collect();
    let t = estimate_weakness_with(&input(WeaknessKey::W1, &today, &r), DEV(None, true, false));
    assert!((t.n - 3000.0).abs() < 1e-9 && (t.k - t.k_unweighted).abs() < 1e-9);
}

#[test]
fn dispersion_shrinks_the_counts_of_a_lumpy_series_only() {
    let r = reference(WeaknessKey::W1, 0.004);
    // Even: one event in every game. Lumpy: four events in one game of four.
    let even = series(&[1.0], 50.0);
    let lumpy = series(&[4.0, 0.0, 0.0, 0.0], 50.0);
    let ess = DEV(None, true, false);
    let disp = DEV(None, true, true);
    let e_even = estimate_weakness_with(&input(WeaknessKey::W1, &even, &r), disp);
    let e_even_ess = estimate_weakness_with(&input(WeaknessKey::W1, &even, &r), ess);
    assert_eq!(
        e_even.n, e_even_ess.n,
        "an under-dispersed series is clamped to 1"
    );
    let e_lumpy = estimate_weakness_with(&input(WeaknessKey::W1, &lumpy, &r), disp);
    let e_lumpy_ess = estimate_weakness_with(&input(WeaknessKey::W1, &lumpy, &r), ess);
    let phi = e_lumpy_ess.n / e_lumpy.n;
    assert!(phi > 2.0 && phi <= MAX_DISPERSION, "phi {phi}");
    assert!((e_lumpy.k / e_lumpy.n - e_lumpy_ess.k / e_lumpy_ess.n).abs() < 1e-12);
}

#[test]
fn the_model_baseline_keeps_its_flat_prior() {
    let r = reference(WeaknessKey::W5, 0.15);
    let pts = series(&[1.0, 0.0], 1.0);
    let i = input(WeaknessKey::W5, &pts, &r);
    let e = estimate_weakness_against_model_with(&i, DEV(Some(1.0), false, false));
    let champ = estimate_weakness_against_model_with(&i, PriorRule::CHAMPION);
    assert_eq!(
        e.posterior, champ.posterior,
        "prior_games leaves the flat prior alone"
    );
    assert!(e.model_baseline);
}
