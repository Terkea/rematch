//! R2 Task 13: change detectors (BOCPD, CUSUM, EWMA).
//!
//! The first block ports TS M2 Task 17's assertions verbatim. The second block checks the
//! detectors against independent computations:
//! - BOCPD run-length posteriors against brute-force enumeration of every change-point
//!   configuration, with segment likelihoods from exact rising factorials (no `lgamma`);
//! - the zero-evidence posterior against its closed form `1 - (1 - H)^L`;
//! - CUSUM against the max-suffix-sum characterisation `S_t = max(0, max_j sum_{i=j..t} llr_i)`;
//! - EWMA against the closed form `z_t = (1 - lambda)^t z_0 + sum lambda (1 - lambda)^(t-i) x_i`.
//!
//! Seeded property tests cover invariants (finite, normalised, bounded) and the honesty rules:
//! invalid inputs are skipped and counted, invalid options are typed errors, NaN and inf never
//! reach output, and no detector claims a change from zero evidence.

use profile_core::stats::rng::Rng;
use profile_core::timeline::bocpd::{
    BocpdOpts, ChangeSummary, DEFAULT_HAZARD, DEFAULT_LEVEL, DEFAULT_MAX_RUN,
    DEFAULT_MIN_PRIOR_GAMES, DEFAULT_WINDOW_GAMES, DetectorError, change_summary, run_bocpd,
    run_bocpd_counted,
};
use profile_core::timeline::cusum::{CUSUM_DEFAULT_H, cusum, cusum_counted};
use profile_core::timeline::ewma::{
    EWMA_DEFAULT_K, EWMA_DEFAULT_LAMBDA, ewma_drift, ewma_drift_counted,
};

fn ts_opts() -> BocpdOpts {
    BocpdOpts {
        hazard: 1.0 / 200.0,
        prior_a: 1.5,
        prior_b: 48.5,
        max_run: 500,
    }
}

fn repeat(n: usize, obs: (u32, u32)) -> Vec<(u32, u32)> {
    vec![obs; n]
}

fn up_series() -> Vec<(u32, u32)> {
    let mut v = repeat(60, (1, 50));
    v.extend(repeat(40, (3, 50)));
    v
}

fn down_series() -> Vec<(u32, u32)> {
    let mut v = repeat(60, (3, 50));
    v.extend(repeat(40, (1, 50)));
    v
}

fn flat_series() -> Vec<(u32, u32)> {
    (0..100)
        .map(|i| if i % 2 == 0 { (1, 50) } else { (2, 50) })
        .collect()
}

fn summary_at_end(obs: &[(u32, u32)], o: &BocpdOpts) -> ChangeSummary {
    let post = run_bocpd(obs, o);
    assert_eq!(post.len(), obs.len());
    let last = post.last().expect("non-empty series");
    change_summary(last, obs.len(), 50, 10, 0.8)
}

// ---------------------------------------------------------------- TS M2 Task 17 ports

#[test]
fn ts_defaults() {
    assert_eq!(DEFAULT_HAZARD, 1.0 / 200.0);
    assert_eq!(DEFAULT_MAX_RUN, 500);
    assert_eq!(DEFAULT_WINDOW_GAMES, 50);
    assert_eq!(DEFAULT_MIN_PRIOR_GAMES, 10);
    assert_eq!(DEFAULT_LEVEL, 0.8);
    assert_eq!(CUSUM_DEFAULT_H, 4.0);
    assert_eq!(EWMA_DEFAULT_LAMBDA, 0.1);
    assert_eq!(EWMA_DEFAULT_K, 2.0);
}

#[test]
fn ts_bocpd_up() {
    let s = summary_at_end(&up_series(), &ts_opts());
    assert!(s.p_change >= 0.99, "{s:?}");
    assert_eq!(s.onset_index, 61, "{s:?}");
    assert!(s.onset_low <= 61 && 61 <= s.onset_high, "{s:?}");
    assert!(s.onset_high - s.onset_low <= 6, "{s:?}");
}

#[test]
fn ts_bocpd_down() {
    let s = summary_at_end(&down_series(), &ts_opts());
    assert_eq!(s.onset_index, 61, "{s:?}");
    assert!(s.p_change >= 0.99, "{s:?}");
    assert!(s.onset_low <= 61 && 61 <= s.onset_high, "{s:?}");
}

#[test]
fn ts_bocpd_flat() {
    let s = summary_at_end(&flat_series(), &ts_opts());
    assert!(s.p_change < 0.2, "{s:?}");
}

#[test]
fn ts_bocpd_empty_games() {
    let s = summary_at_end(&repeat(60, (0, 0)), &ts_opts());
    assert!(s.p_change < 0.3, "{s:?}");
}

#[test]
fn ts_cusum() {
    assert_eq!(cusum(&up_series(), 0.02, 0.03, 4.0).0, Some(66));
    assert_eq!(cusum(&down_series(), 0.06, 0.04, 4.0).0, Some(67));
    // The flat series runs at 0.03, so it is monitored with p0 = the segment rate and
    // p1 = 1.5 * p0, the way Task 14 configures CUSUM (TS M2 Task 18).
    let (alarm, stat) = cusum(&flat_series(), 0.03, 0.045, 4.0);
    assert_eq!(alarm, None);
    assert!(stat.is_finite() && stat >= 0.0);
}

#[test]
fn ts_ewma() {
    let (alarm, z) = ewma_drift(&[0.02, 0.02, 0.06, 0.06], 0.1, 0.02, 0.01, 2.0);
    assert_eq!(alarm, Some(4));
    // z_4 = 0.1 * 0.06 + 0.9 * (0.1 * 0.06 + 0.9 * 0.02) = 0.0276.
    assert!((z - 0.0276).abs() < 1e-15, "{z}");
}

// ---------------------------------------------------------------- BOCPD vs brute force

/// `ln [B(a + K, b + M) / B(a, b)]` from rising factorials: no gamma function involved.
fn ln_segment_ratio(a: f64, b: f64, k_sum: u32, m_sum: u32) -> f64 {
    let mut acc = 0.0;
    for j in 0..k_sum {
        acc += (a + f64::from(j)).ln();
    }
    for j in 0..m_sum {
        acc += (b + f64::from(j)).ln();
    }
    for j in 0..(k_sum + m_sum) {
        acc -= (a + b + f64::from(j)).ln();
    }
    acc
}

/// Run-length posterior after `obs` by enumerating all `2^t` change indicators
/// (`c_i = 1`: a change point right after observation `i`). The binomial coefficients are a
/// common factor of every configuration and cancel.
fn brute_force_posterior(obs: &[(u32, u32)], h: f64, a: f64, b: f64) -> Vec<f64> {
    let t = obs.len();
    let mut ln_w = vec![f64::NEG_INFINITY; t + 1];
    for mask in 0u32..(1u32 << t) {
        let mut lw = 0.0;
        let mut seg_k = 0;
        let mut seg_m = 0;
        let mut last_change = 0; // number of observations before the current segment
        for (i, &(k, n)) in obs.iter().enumerate() {
            seg_k += k;
            seg_m += n - k;
            if mask >> i & 1 == 1 {
                lw += h.ln() + ln_segment_ratio(a, b, seg_k, seg_m);
                seg_k = 0;
                seg_m = 0;
                last_change = i + 1;
            } else {
                lw += (1.0 - h).ln();
            }
        }
        lw += ln_segment_ratio(a, b, seg_k, seg_m);
        let r = t - last_change;
        let cur = ln_w[r];
        ln_w[r] = if cur == f64::NEG_INFINITY {
            lw
        } else {
            let m = cur.max(lw);
            m + ((cur - m).exp() + (lw - m).exp()).ln()
        };
    }
    let m = ln_w.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let w: Vec<f64> = ln_w.iter().map(|x| (x - m).exp()).collect();
    let s: f64 = w.iter().sum();
    w.into_iter().map(|x| x / s).collect()
}

#[test]
fn bocpd_matches_brute_force_enumeration() {
    let mut rng = Rng::new(13);
    for case in 0..12 {
        let len = 1 + (rng.next_u32() % 12) as usize;
        let obs: Vec<(u32, u32)> = (0..len)
            .map(|_| {
                let n = rng.next_u32() % 11;
                let k = if n == 0 { 0 } else { rng.next_u32() % (n + 1) };
                (k, n)
            })
            .collect();
        let h = [0.005, 0.1, 0.3][case % 3];
        let (a, b) = [(1.5, 48.5), (0.7, 2.3), (5.0, 5.0)][case % 3];
        let o = BocpdOpts {
            hazard: h,
            prior_a: a,
            prior_b: b,
            max_run: 500,
        };
        let post = run_bocpd(&obs, &o);
        assert_eq!(post.len(), obs.len());
        for t in 1..=obs.len() {
            let want = brute_force_posterior(&obs[..t], h, a, b);
            let got = &post[t - 1];
            assert_eq!(got.len(), t + 1, "case {case} t {t}");
            for (r, (g, w)) in got.iter().zip(&want).enumerate() {
                assert!((g - w).abs() < 1e-12, "case {case} t {t} r {r}: {g} vs {w}");
            }
        }
    }
}

#[test]
fn bocpd_zero_evidence_is_the_prior_run_length_distribution() {
    // With no information every predictive is 1, so P(r = j) = H (1 - H)^j for j < t and
    // P(r = t) = (1 - H)^t; the mass below L is 1 - (1 - H)^L.
    let o = ts_opts();
    let h = o.hazard;
    for t in [1usize, 5, 40, 60, 120] {
        let post = run_bocpd(&repeat(t, (0, 0)), &o);
        let last = post.last().unwrap();
        for (j, p) in last.iter().enumerate() {
            let want = if j < t {
                h * (1.0 - h).powi(j as i32)
            } else {
                (1.0 - h).powi(t as i32)
            };
            assert!((p - want).abs() < 1e-14, "t {t} j {j}");
        }
        let s = change_summary(last, t, 50, 10, 0.8);
        let limit = 50.min(t.saturating_sub(10));
        let want = 1.0 - (1.0 - h).powi(limit as i32);
        assert!(
            (s.p_change - want).abs() < 1e-12,
            "t {t}: {} vs {want}",
            s.p_change
        );
        // The confirmation threshold of Task 14 (0.8) is out of reach without evidence.
        assert!(s.p_change < 0.8);
    }
}

// ---------------------------------------------------------------- BOCPD properties

fn binomial(rng: &mut Rng, n: u32, p: f64) -> u32 {
    (0..n).filter(|_| rng.next_f64() < p).count() as u32
}

#[test]
fn bocpd_seeded_invariants_and_invalid_observations() {
    for seed in 0..40u32 {
        let mut rng = Rng::new(1000 + seed);
        let len = 1 + (rng.next_u32() % 300) as usize;
        let max_run = 1 + (rng.next_u32() % 120) as usize;
        let mut invalid = 0;
        let obs: Vec<(u32, u32)> = (0..len)
            .map(|_| {
                let n = rng.next_u32() % 60;
                if rng.next_f64() < 0.05 {
                    invalid += 1;
                    (n + 1 + rng.next_u32() % 5, n) // k > n
                } else {
                    (binomial(&mut rng, n, 0.04), n)
                }
            })
            .collect();
        let o = BocpdOpts {
            hazard: 1.0 / 200.0,
            prior_a: 1.5,
            prior_b: 48.5,
            max_run,
        };
        let run = run_bocpd_counted(&obs, &o).expect("valid opts");
        assert_eq!(run.skipped, invalid, "seed {seed}");
        assert_eq!(run.posteriors.len(), len);
        for (i, post) in run.posteriors.iter().enumerate() {
            let t = i + 1;
            assert_eq!(post.len(), t.min(max_run) + 1, "seed {seed} t {t}");
            assert!(
                post.iter().all(|p| p.is_finite() && *p >= 0.0),
                "seed {seed} t {t}"
            );
            let s: f64 = post.iter().sum();
            assert!((s - 1.0).abs() < 1e-9, "seed {seed} t {t}: sum {s}");
            let cs = change_summary(post, t, 50, 10, 0.8);
            assert!((0.0..=1.0).contains(&cs.p_change), "{cs:?}");
            assert!(cs.p_change.is_finite());
            assert!(cs.map_run <= t);
            assert_eq!(cs.onset_index, t - cs.map_run + 1);
            assert!(
                1 <= cs.onset_low && cs.onset_low <= cs.onset_index,
                "{cs:?}"
            );
            assert!(
                cs.onset_index <= cs.onset_high && cs.onset_high <= t + 1,
                "{cs:?}"
            );
            if t <= 10 {
                assert_eq!(
                    cs.p_change, 0.0,
                    "no change is claimed inside the prior games"
                );
            }
        }
        // An invalid observation is skipped as evidence: identical to a no-information game.
        let cleaned: Vec<(u32, u32)> = obs
            .iter()
            .map(|&(k, n)| if k > n { (0, 0) } else { (k, n) })
            .collect();
        assert_eq!(run_bocpd(&cleaned, &o), run.posteriors, "seed {seed}");
    }
}

#[test]
fn bocpd_truncation_never_invents_a_change_on_a_long_stable_series() {
    // 1 200 stationary games at 3 %: the run of length >= max_run keeps the mass. Dropping
    // that tail would push all mass onto short runs and claim a change from nothing.
    let mut rng = Rng::new(7);
    let obs: Vec<(u32, u32)> = (0..1200)
        .map(|_| (binomial(&mut rng, 40, 0.03), 40))
        .collect();
    let o = ts_opts();
    let post = run_bocpd(&obs, &o);
    for (i, p) in post.iter().enumerate().skip(100) {
        let s = change_summary(p, i + 1, 50, 10, 0.8);
        assert!(s.p_change < 0.5, "t {}: {s:?}", i + 1);
    }
    let last = post.last().unwrap();
    assert_eq!(last.len(), 501);
    assert!(last[500] > 0.5, "capped bucket mass {}", last[500]);
}

#[test]
fn bocpd_detects_a_change_after_the_cap() {
    // A change late in a series longer than max_run is still found.
    let mut obs = repeat(700, (1, 50));
    obs.extend(repeat(40, (4, 50)));
    let s = summary_at_end(&obs, &ts_opts());
    assert!(s.p_change >= 0.99, "{s:?}");
    assert_eq!(s.onset_index, 701, "{s:?}");
}

#[test]
fn bocpd_huge_counts_stay_finite() {
    let obs = vec![
        (u32::MAX / 2, u32::MAX),
        (0, u32::MAX),
        (u32::MAX, u32::MAX),
        (7, 9),
    ];
    let post = run_bocpd(&obs, &ts_opts());
    for p in &post {
        assert!(p.iter().all(|x| x.is_finite() && *x >= 0.0));
        assert!((p.iter().sum::<f64>() - 1.0).abs() < 1e-9);
    }
}

#[test]
fn bocpd_invalid_options_are_typed_errors() {
    let obs = up_series();
    let base = ts_opts();
    let bad = [
        BocpdOpts {
            hazard: 0.0,
            ..base
        },
        BocpdOpts {
            hazard: 1.0,
            ..base
        },
        BocpdOpts {
            hazard: -0.1,
            ..base
        },
        BocpdOpts {
            hazard: f64::NAN,
            ..base
        },
        BocpdOpts {
            prior_a: 0.0,
            ..base
        },
        BocpdOpts {
            prior_b: -1.0,
            ..base
        },
        BocpdOpts {
            prior_a: f64::INFINITY,
            ..base
        },
        BocpdOpts {
            prior_b: f64::NAN,
            ..base
        },
        BocpdOpts { max_run: 0, ..base },
    ];
    for o in bad {
        assert!(run_bocpd_counted(&obs, &o).is_err(), "{o:?}");
        assert!(run_bocpd(&obs, &o).is_empty(), "{o:?}");
    }
    assert!(matches!(
        run_bocpd_counted(
            &obs,
            &BocpdOpts {
                hazard: 2.0,
                ..base
            }
        ),
        Err(DetectorError::InvalidHazard { .. })
    ));
    assert!(matches!(
        run_bocpd_counted(
            &obs,
            &BocpdOpts {
                prior_a: -2.0,
                ..base
            }
        ),
        Err(DetectorError::InvalidPrior { .. })
    ));
    assert!(matches!(
        run_bocpd_counted(&obs, &BocpdOpts { max_run: 0, ..base }),
        Err(DetectorError::InvalidMaxRun)
    ));
    assert!(run_bocpd(&[], &base).is_empty());
}

// ---------------------------------------------------------------- change_summary

#[test]
fn change_summary_hand_computed() {
    // t = 20, window 50, min prior 10 -> runs r < 10 count as a change.
    let mut post = vec![0.0; 21];
    post[3] = 0.5; // onset 18
    post[4] = 0.2; // onset 17
    post[15] = 0.25; // onset 6
    post[20] = 0.05; // onset 1
    let s = change_summary(&post, 20, 50, 10, 0.8);
    assert!((s.p_change - 0.7).abs() < 1e-15, "{s:?}");
    assert_eq!(s.map_run, 3);
    assert_eq!(s.onset_index, 18);
    // Highest-mass set covering 0.8: {3, 15, 4} (0.5 + 0.25 + 0.2 = 0.95).
    assert_eq!((s.onset_low, s.onset_high), (6, 18));
    let s = change_summary(&post, 20, 50, 10, 0.7);
    // 0.5 + 0.25 = 0.75 >= 0.7: {3, 15}.
    assert_eq!((s.onset_low, s.onset_high), (6, 18));
    let s = change_summary(&post, 20, 50, 10, 0.5);
    assert_eq!((s.onset_low, s.onset_high), (18, 18));
    // The window caps the change runs: window 4 -> r < 4.
    let s = change_summary(&post, 20, 4, 10, 0.8);
    assert!((s.p_change - 0.5).abs() < 1e-15);
    // Fewer games than the prior minimum claim nothing.
    let s = change_summary(&post, 20, 50, 20, 0.8);
    assert_eq!(s.p_change, 0.0);
}

#[test]
fn change_summary_skips_invalid_mass_and_never_emits_nan() {
    let post = vec![f64::NAN, 0.1, f64::INFINITY, -0.5, 0.3, 0.6];
    let s = change_summary(&post, 5, 50, 1, 0.8);
    // Valid mass: r1 = 0.1, r4 = 0.3, r5 = 0.6 (total 1); r < 4 -> 0.1.
    assert!((s.p_change - 0.1).abs() < 1e-15, "{s:?}");
    assert_eq!(s.map_run, 5);
    assert_eq!(s.onset_index, 1);
    // No valid mass at all: no evidence, no change, the whole range as the onset interval.
    for post in [vec![], vec![f64::NAN, f64::NAN], vec![0.0; 4]] {
        let s = change_summary(&post, 3, 50, 0, 0.8);
        assert_eq!(
            s,
            ChangeSummary {
                p_change: 0.0,
                map_run: 3,
                onset_index: 1,
                onset_low: 1,
                onset_high: 4
            }
        );
    }
    // Entries beyond r = t are impossible and ignored.
    let s = change_summary(&[0.0, 0.0, 1.0, 5.0], 2, 50, 0, 0.8);
    assert_eq!(s.map_run, 2);
    assert_eq!(s.p_change, 0.0);
    // A non-finite or out-of-range level never yields NaN bounds.
    let post = vec![0.1, 0.2, 0.7];
    for level in [f64::NAN, -1.0, 0.0, 2.0, f64::INFINITY] {
        let s = change_summary(&post, 2, 50, 0, level);
        assert!(
            s.onset_low >= 1 && s.onset_high <= 3 && s.onset_low <= s.onset_high,
            "{s:?}"
        );
    }
    let s = change_summary(&post, 2, 50, 0, f64::NAN);
    assert_eq!(
        (s.onset_low, s.onset_high),
        (1, 3),
        "NaN level is the full set"
    );
}

// ---------------------------------------------------------------- CUSUM

fn llr(k: u32, n: u32, p0: f64, p1: f64) -> f64 {
    f64::from(k) * (p1 / p0).ln() + f64::from(n - k) * ((1.0 - p1) / (1.0 - p0)).ln()
}

/// `S_t = max(0, max_j sum_{i=j..t} llr_i)` by brute force over every suffix.
fn brute_cusum_path(obs: &[(u32, u32)], p0: f64, p1: f64) -> Vec<f64> {
    (1..=obs.len())
        .map(|t| {
            let mut best: f64 = 0.0;
            for j in 0..t {
                let s: f64 = obs[j..t].iter().map(|&(k, n)| llr(k, n, p0, p1)).sum();
                best = best.max(s);
            }
            best
        })
        .collect()
}

#[test]
fn cusum_matches_max_suffix_sum() {
    for seed in 0..30u32 {
        let mut rng = Rng::new(500 + seed);
        let len = 1 + (rng.next_u32() % 150) as usize;
        let rate = 0.01 + 0.05 * rng.next_f64();
        let obs: Vec<(u32, u32)> = (0..len)
            .map(|_| {
                let n = rng.next_u32() % 50;
                (binomial(&mut rng, n, rate), n)
            })
            .collect();
        let (p0, p1) = if seed % 2 == 0 {
            (0.02, 0.03)
        } else {
            (0.05, 0.035)
        };
        let path = brute_cusum_path(&obs, p0, p1);
        // No alarm with an unreachable threshold: the statistic is S at the end.
        let (alarm, stat) = cusum(&obs, p0, p1, 1e300);
        assert_eq!(alarm, None);
        assert!((stat - path[len - 1]).abs() < 1e-9, "seed {seed}");
        // With h = 2 the alarm is the first t with S_t > h, and the statistic is S there.
        let want = path.iter().position(|&s| s > 2.0);
        let (alarm, stat) = cusum(&obs, p0, p1, 2.0);
        assert_eq!(alarm, want.map(|i| i + 1), "seed {seed}");
        if let Some(i) = want {
            assert!((stat - path[i]).abs() < 1e-9);
        }
    }
}

#[test]
fn cusum_skips_and_counts_invalid_observations() {
    let mut obs = up_series();
    obs[10] = (5, 3);
    obs[70] = (9, 0);
    let r = cusum_counted(&obs, 0.02, 0.03, 4.0).unwrap();
    // The scan stops at the alarm (game 66): only the invalid game 11 was seen, not game 71.
    assert_eq!(r.skipped, 1);
    assert_eq!(r.alarm_game, Some(66));
    let r = cusum_counted(&obs, 0.02, 0.03, 1e9).unwrap();
    assert_eq!(r.skipped, 2);
    assert_eq!(r.alarm_game, None);
    assert!(r.statistic.is_finite());
    // Zero evidence never moves the statistic.
    let (alarm, stat) = cusum(&repeat(500, (0, 0)), 0.02, 0.03, 4.0);
    assert_eq!((alarm, stat), (None, 0.0));
}

#[test]
fn cusum_invalid_parameters_are_typed_errors() {
    let obs = up_series();
    for (p0, p1, h) in [
        (0.0, 0.03, 4.0),
        (0.02, 1.0, 4.0),
        (0.02, 0.02, 4.0),
        (f64::NAN, 0.03, 4.0),
        (0.02, f64::INFINITY, 4.0),
        (0.02, 0.03, 0.0),
        (0.02, 0.03, -1.0),
        (0.02, 0.03, f64::NAN),
        (0.02, 0.03, f64::INFINITY),
    ] {
        assert!(cusum_counted(&obs, p0, p1, h).is_err(), "{p0} {p1} {h}");
        let (alarm, stat) = cusum(&obs, p0, p1, h);
        assert_eq!((alarm, stat), (None, 0.0));
    }
    assert!(matches!(
        cusum_counted(&obs, 0.5, 0.5, 4.0),
        Err(DetectorError::InvalidRates { .. })
    ));
    assert!(matches!(
        cusum_counted(&obs, 0.2, 0.3, -4.0),
        Err(DetectorError::InvalidThreshold { .. })
    ));
}

// ---------------------------------------------------------------- EWMA

#[test]
fn ewma_matches_closed_form() {
    for seed in 0..30u32 {
        let mut rng = Rng::new(900 + seed);
        let len = 1 + (rng.next_u32() % 200) as usize;
        let x: Vec<f64> = (0..len).map(|_| 0.1 * rng.next_f64()).collect();
        let lambda = 0.05 + 0.9 * rng.next_f64();
        let level = 0.03;
        let sigma = 0.02;
        let closed = |t: usize| {
            let mut z = (1.0 - lambda).powi(t as i32) * level;
            for (i, xi) in x[..t].iter().enumerate() {
                z += lambda * (1.0 - lambda).powi((t - 1 - i) as i32) * xi;
            }
            z
        };
        let (alarm, z) = ewma_drift(&x, lambda, level, sigma, 1e300);
        assert_eq!(alarm, None);
        assert!((z - closed(len)).abs() < 1e-12, "seed {seed}");
        let k = 2.0;
        let limit = level + k * sigma * (lambda / (2.0 - lambda)).sqrt();
        let want = (1..=len).find(|&t| closed(t) > limit);
        let (alarm, z) = ewma_drift(&x, lambda, level, sigma, k);
        assert_eq!(alarm, want, "seed {seed}");
        if let Some(t) = want {
            assert!((z - closed(t)).abs() < 1e-12);
        }
    }
}

#[test]
fn ewma_skips_and_counts_non_finite_values() {
    let x = [
        0.02,
        f64::NAN,
        0.02,
        f64::INFINITY,
        0.06,
        f64::NEG_INFINITY,
        0.06,
    ];
    let r = ewma_drift_counted(&x, 0.1, 0.02, 0.01, 1e300).unwrap();
    assert_eq!(r.skipped, 3);
    let clean = ewma_drift_counted(&[0.02, 0.02, 0.06, 0.06], 0.1, 0.02, 0.01, 1e300).unwrap();
    assert_eq!(r.z, clean.z);
    // The alarm index is the position in the input (game 7), not in the cleaned list.
    let r = ewma_drift_counted(&x, 0.1, 0.02, 0.01, 2.0).unwrap();
    assert_eq!(r.alarm_game, Some(7));
    assert_eq!(r.skipped, 3);
    // Only non-finite values: z stays at the level and nothing is claimed.
    let (alarm, z) = ewma_drift(&[f64::NAN; 50], 0.1, 0.02, 0.0, 0.0);
    assert_eq!((alarm, z), (None, 0.02));
    assert_eq!(ewma_drift(&[], 0.1, 0.02, 0.01, 2.0), (None, 0.02));
}

#[test]
fn ewma_invalid_parameters_are_typed_errors() {
    let x = [0.02, 0.06];
    for (lambda, level, sigma, k) in [
        (0.0, 0.02, 0.01, 2.0),
        (1.5, 0.02, 0.01, 2.0),
        (f64::NAN, 0.02, 0.01, 2.0),
        (0.1, f64::NAN, 0.01, 2.0),
        (0.1, f64::INFINITY, 0.01, 2.0),
        (0.1, 0.02, -0.01, 2.0),
        (0.1, 0.02, f64::NAN, 2.0),
        (0.1, 0.02, 0.01, -2.0),
        (0.1, 0.02, 0.01, f64::NAN),
    ] {
        assert!(ewma_drift_counted(&x, lambda, level, sigma, k).is_err());
        let (alarm, z) = ewma_drift(&x, lambda, level, sigma, k);
        assert_eq!(alarm, None);
        assert!(z.is_finite());
    }
    // lambda = 1 is allowed: z is the last value.
    assert_eq!(ewma_drift(&x, 1.0, 0.02, 0.01, 1e300), (None, 0.06));
    assert!(matches!(
        ewma_drift_counted(&x, 0.0, 0.02, 0.01, 2.0),
        Err(DetectorError::InvalidLambda { .. })
    ));
}
