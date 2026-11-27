//! EXP-0030's candidate rules (`RatingRule`): the memory model's closed forms against brute
//! force, the windows each rule compares, and the production rule unchanged by them.

use coaching_input::{TrajectoryKind, WindowBasis};
use profile::coaching::trajectory::{
    Computed, Estimate, Input, Memory, RATING_SCALE, RatedGame, RatingRule, Verdict, compute,
    compute_with, memory_compare,
};
use time::{Duration, OffsetDateTime};

fn brute_block(phi: f64, m: usize) -> f64 {
    let mut s = 0.0;
    for i in 0..m {
        for j in 0..m {
            s += phi.powi((i as i32 - j as i32).abs());
        }
    }
    s
}

fn brute_diff_var(mem: &Memory, a: std::ops::Range<usize>, b: std::ops::Range<usize>) -> f64 {
    // Var(mean b - mean a) = sigma2 * sum_ij w_i w_j phi^|i-j|, with w = -1/ma on a, 1/mb on b.
    let n = b.end;
    let w: Vec<f64> = (0..n)
        .map(|i| {
            if a.contains(&i) {
                -1.0 / a.len() as f64
            } else if b.contains(&i) {
                1.0 / b.len() as f64
            } else {
                0.0
            }
        })
        .collect();
    let mut s = 0.0;
    for i in 0..n {
        for j in 0..n {
            s += w[i] * w[j] * mem.phi.powi((i as i32 - j as i32).abs());
        }
    }
    mem.sigma2 * s
}

#[test]
fn the_closed_forms_are_the_sums() {
    for phi in [0.0, 0.3, 0.9, 0.984, 0.999] {
        let mem = Memory { sigma2: 950.0, phi };
        for m in [1, 5, 30, 90] {
            let (got, want) = (mem.block(m), brute_block(phi, m));
            assert!(
                (got - want).abs() < 1e-6 * want.max(1.0),
                "S({m}) at {phi}: {got} vs {want}"
            );
        }
        for (a, b) in [
            (0..30, 30..60),
            (0..30, 90..120),
            (0..5, 35..40),
            (0..220, 220..250),
        ] {
            let (got, want) = (
                mem.diff_var(a.clone(), b.clone()),
                brute_diff_var(&mem, a, b),
            );
            assert!(
                (got - want).abs() < 1e-6 * want.max(1.0),
                "{got} vs {want} at {phi}"
            );
        }
    }
}

#[test]
fn the_memory_comes_from_the_steps() {
    // Steps of +6 and -6: RMS 6, a = q 6 / 2, sigma2 = 36 / (a (2 - a)).
    let r: Vec<f64> = (0..40)
        .map(|i| if i % 2 == 0 { 1500.0 } else { 1506.0 })
        .collect();
    let mem = Memory::of(&r).unwrap();
    let a = RATING_SCALE * 6.0 / 2.0;
    assert!((mem.phi - (1.0 - a)).abs() < 1e-12);
    assert!((mem.sigma2 - 36.0 / (a * (2.0 - a))).abs() < 1e-9);
    // About 31 points at lila's floor deviation (EXP-0015's world): RMS step about 5.6.
    let lila = Memory::of(&[0.0, 5.6]).unwrap();
    assert!(
        (lila.sigma2.sqrt() - 31.2).abs() < 0.5,
        "{}",
        lila.sigma2.sqrt()
    );
    assert!(Memory::of(&[1500.0; 30]).is_none());
    assert!(Memory::of(&[1500.0]).is_none());
}

#[test]
fn a_series_that_never_moved_gets_no_verdict() {
    let r = vec![1500.0; 60];
    let (c, (a, b)) = memory_compare(&r, 0..30, 30..60);
    assert_eq!(c.verdict, Verdict::NoClearChange);
    assert!(matches!(a, Estimate::Estimate { interval: None, .. }));
    assert!(matches!(b, Estimate::Estimate { interval: None, .. }));
}

#[test]
fn the_interval_is_the_memory_models() {
    // A slow fall of 1 point a game with alternating steps.
    let r: Vec<f64> = (0..60)
        .map(|i| 1600.0 - i as f64 + if i % 2 == 0 { 0.0 } else { 6.0 })
        .collect();
    let (c, _) = memory_compare(&r, 0..30, 30..60);
    let mem = Memory::of(&r).unwrap();
    let h = 1.2815515655446004 * mem.diff_var(0..30, 30..60).sqrt();
    let (diff, i) = c.diff.unwrap();
    assert!((diff - (-30.0)).abs() < 1e-9);
    assert!((i.low - (diff - h)).abs() < 1e-9 && (i.high - (diff + h)).abs() < 1e-9);
    assert_eq!(
        c.verdict,
        if diff + h < 0.0 {
            Verdict::Worse
        } else {
            Verdict::NoClearChange
        }
    );
}

fn input(ratings: &[f64]) -> Input {
    let t0 = OffsetDateTime::from_unix_timestamp(1_767_600_000).unwrap();
    Input {
        games: ratings
            .iter()
            .enumerate()
            .map(|(i, &r)| RatedGame {
                id: format!("g{i:04}"),
                played_at: t0 + Duration::hours(12 * i as i64),
                rating: r,
            })
            .collect(),
        current: None,
        stats: Vec::new(),
        weaknesses: Vec::new(),
        seed: "trajectory:test:rapid".to_owned(),
    }
}

fn ready(c: Computed) -> Box<profile::coaching::trajectory::Ready> {
    match c {
        Computed::Ready(r) => r,
        Computed::NotEnoughGames { .. } => panic!("not enough games"),
    }
}

#[test]
fn each_rule_compares_its_windows() {
    // A peak in the middle of 120 games; the rest wanders by a few points.
    let r: Vec<f64> = (0..120)
        .map(|i| {
            1500.0
                + if (55..65).contains(&i) {
                    80.0
                } else {
                    ((i * 7) % 11) as f64
                }
        })
        .collect();
    let inp = input(&r);
    let champ = ready(compute_with(&inp, RatingRule::PeakBootstrap).1);
    assert_eq!(champ.then.basis, WindowBasis::Peak);
    assert_eq!(compute(&inp), compute_with(&inp, RatingRule::PRODUCTION));
    assert_eq!(RatingRule::PRODUCTION, RatingRule::PeakBootstrap);

    let halves = ready(compute_with(&inp, RatingRule::MemoryHalves).1);
    assert_eq!(halves.then.basis, WindowBasis::Start);
    assert_eq!((halves.then.games, halves.now.games), (30, 30));
    assert_eq!(halves.then.first_id, "g0000");
    assert_eq!(
        halves.change,
        (halves.now.mean_rating - halves.then.mean_rating).round()
    );

    let before = ready(compute_with(&inp, RatingRule::MemoryBefore).1);
    assert_eq!(before.then.basis, WindowBasis::Start);
    assert_eq!((before.then.games, before.now.games), (90, 30));
    assert_eq!(before.then.last_id, "g0089");
    // The peak is still a fact.
    assert_eq!(halves.peak.0, 1580.0);
    assert_eq!(before.peak.0, 1580.0);
}

#[test]
fn a_real_fall_is_called_and_a_wander_is_not() {
    // 250 points down over 120 games, with steps of +-6: worse under both candidates.
    let fall: Vec<f64> = (0..120)
        .map(|i| 1700.0 - 250.0 * i as f64 / 119.0 + if i % 2 == 0 { 0.0 } else { 6.0 })
        .collect();
    for rule in [RatingRule::MemoryHalves, RatingRule::MemoryBefore] {
        let t = ready(compute_with(&input(&fall), rule).1);
        assert_eq!(t.kind, TrajectoryKind::Falling, "{}", rule.name());
    }
    // The same steps with no trend: no verdict.
    let flat: Vec<f64> = (0..120)
        .map(|i| 1500.0 + if i % 2 == 0 { 0.0 } else { 6.0 })
        .collect();
    for rule in [RatingRule::MemoryHalves, RatingRule::MemoryBefore] {
        let t = ready(compute_with(&input(&flat), rule).1);
        assert_eq!(t.kind, TrajectoryKind::Flat, "{}", rule.name());
    }
}

#[test]
fn the_rules_have_stable_names() {
    for r in RatingRule::ALL {
        assert_eq!(RatingRule::from_name(r.name()), Some(r));
    }
    assert_eq!(
        RatingRule::from_name("memory-halves"),
        Some(RatingRule::MemoryHalves)
    );
    assert_eq!(RatingRule::from_name("nope"), None);
}
