//! Bayesian online change-point detection (R2 Task 13).
//!
//! Adams & MacKay (2007) with a Beta-binomial predictive, computed in log space. Each
//! observation is a game's `(events, opportunities)` pair. After observation `t` the
//! run-length posterior `post[r]` is the probability that the last `r` games form the current
//! segment, so the segment started at game `t - r + 1` (1-based); `r = 0` means a change right
//! after game `t`.
//!
//! Per observation (TS M2 Task 17):
//! ```text
//! pred_r = lchoose(n, k) + lbeta(a_r + k, b_r + n - k) - lbeta(a_r, b_r)
//! grow_r = logp_r + pred_r + ln(1 - H);   cp = logsumexp_r(logp_r + pred_r + ln H)
//! runs   = [{cp, prior}] ++ [{grow_r, a_r + k, b_r + n - k}]   // truncate to max_run
//! normalise
//! ```
//!
//! Truncation keeps run lengths `0..=max_run`. The run that would grow past `max_run` is
//! merged into the `max_run` bucket (which then means "at least `max_run` games") instead of
//! being dropped: dropping it would move the mass of a long stable series onto short runs and
//! claim a change from nothing. The merged bucket keeps the parameters of the heavier of the
//! two runs.
//!
//! Honesty rules: an observation with `k > n` is skipped as evidence (treated as a game with
//! no opportunities, so game indices stay aligned) and counted; invalid options are a typed
//! [`DetectorError`]; no NaN or infinity reaches a posterior or a [`ChangeSummary`].

use crate::stats::special::{lgamma, ln_beta};

/// Prior probability of a change point after any game.
pub const DEFAULT_HAZARD: f64 = 1.0 / 200.0;
/// Longest tracked run length (longer runs share the last bucket).
pub const DEFAULT_MAX_RUN: usize = 500;
/// Runs shorter than this many games count as "changed recently".
pub const DEFAULT_WINDOW_GAMES: usize = 50;
/// Games that must precede a change before it can be claimed.
pub const DEFAULT_MIN_PRIOR_GAMES: usize = 10;
/// Mass of the highest-mass run-length set behind the onset bounds.
pub const DEFAULT_LEVEL: f64 = 0.8;

/// Invalid options of a change detector (BOCPD, CUSUM, EWMA).
#[derive(Clone, Copy, Debug, PartialEq, thiserror::Error)]
pub enum DetectorError {
    /// The hazard must be finite and strictly inside `(0, 1)`.
    #[error("hazard must be in (0, 1) (hazard = {hazard})")]
    InvalidHazard { hazard: f64 },
    /// Beta prior shapes must be finite and positive.
    #[error("prior shapes must be finite and positive (a = {a}, b = {b})")]
    InvalidPrior { a: f64, b: f64 },
    /// `max_run` must be at least 1.
    #[error("max_run must be at least 1")]
    InvalidMaxRun,
    /// CUSUM rates must be finite, inside `(0, 1)` and different.
    #[error("CUSUM rates must be in (0, 1) and differ (p0 = {p0}, p1 = {p1})")]
    InvalidRates { p0: f64, p1: f64 },
    /// The CUSUM threshold must be finite and positive.
    #[error("CUSUM threshold must be finite and positive (h = {h})")]
    InvalidThreshold { h: f64 },
    /// The EWMA weight must be in `(0, 1]`.
    #[error("EWMA lambda must be in (0, 1] (lambda = {lambda})")]
    InvalidLambda { lambda: f64 },
    /// EWMA level, sigma and k must be finite, sigma and k non-negative.
    #[error(
        "EWMA level, sigma and k must be finite, sigma and k >= 0 (level = {level}, sigma = {sigma}, k = {k})"
    )]
    InvalidEwma { level: f64, sigma: f64, k: f64 },
}

/// BOCPD options (TS: `{ hazard, prior: {a, b}, maxRun }`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BocpdOpts {
    pub hazard: f64,
    pub prior_a: f64,
    pub prior_b: f64,
    pub max_run: usize,
}

/// Posteriors plus the number of observations skipped as invalid (`k > n`).
#[derive(Clone, Debug, PartialEq)]
pub struct BocpdRun {
    /// `posteriors[t - 1][r]`: run-length posterior after game `t`, `r` in `0..=min(t, max_run)`.
    pub posteriors: Vec<Vec<f64>>,
    pub skipped: usize,
}

#[derive(Clone, Copy)]
struct Run {
    logp: f64,
    a: f64,
    b: f64,
}

fn logsumexp(a: f64, b: f64) -> f64 {
    if a == f64::NEG_INFINITY {
        return b;
    }
    if b == f64::NEG_INFINITY {
        return a;
    }
    let m = a.max(b);
    m + ((a - m).exp() + (b - m).exp()).ln()
}

fn validate(o: &BocpdOpts) -> Result<(), DetectorError> {
    if !(o.hazard > 0.0 && o.hazard < 1.0) {
        return Err(DetectorError::InvalidHazard { hazard: o.hazard });
    }
    if !(o.prior_a > 0.0 && o.prior_b > 0.0 && o.prior_a.is_finite() && o.prior_b.is_finite()) {
        return Err(DetectorError::InvalidPrior {
            a: o.prior_a,
            b: o.prior_b,
        });
    }
    if o.max_run == 0 {
        return Err(DetectorError::InvalidMaxRun);
    }
    Ok(())
}

/// Runs BOCPD and counts skipped observations. See the module docs.
pub fn run_bocpd_counted(obs: &[(u32, u32)], o: &BocpdOpts) -> Result<BocpdRun, DetectorError> {
    validate(o)?;
    let ln_h = o.hazard.ln();
    let ln_1mh = (-o.hazard).ln_1p();
    let mut runs = vec![Run {
        logp: 0.0,
        a: o.prior_a,
        b: o.prior_b,
    }];
    let mut posteriors = Vec::with_capacity(obs.len());
    let mut skipped = 0;
    for &(k, n) in obs {
        let (k, n) = if k > n {
            skipped += 1;
            (0, 0)
        } else {
            (k, n)
        };
        let (kf, mf) = (f64::from(k), f64::from(n - k));
        let lchoose = lgamma(f64::from(n) + 1.0) - lgamma(kf + 1.0) - lgamma(mf + 1.0);
        let mut cp = f64::NEG_INFINITY;
        let mut next = Vec::with_capacity((runs.len() + 1).min(o.max_run + 1));
        next.push(Run {
            logp: 0.0,
            a: o.prior_a,
            b: o.prior_b,
        });
        for run in &runs {
            let pred = if n == 0 {
                0.0
            } else {
                lchoose + ln_beta(run.a + kf, run.b + mf) - ln_beta(run.a, run.b)
            };
            let joint = run.logp + pred;
            cp = logsumexp(cp, joint + ln_h);
            let grown = Run {
                logp: joint + ln_1mh,
                a: run.a + kf,
                b: run.b + mf,
            };
            if next.len() <= o.max_run {
                next.push(grown);
            } else {
                // Merge into the "at least max_run" bucket.
                let last = next.last_mut().expect("bucket exists");
                let logp = logsumexp(last.logp, grown.logp);
                if grown.logp > last.logp {
                    *last = grown;
                }
                last.logp = logp;
            }
        }
        next[0].logp = cp;
        let total = next
            .iter()
            .fold(f64::NEG_INFINITY, |acc, r| logsumexp(acc, r.logp));
        let mut post: Vec<f64> = if total.is_finite() {
            next.iter_mut()
                .map(|r| {
                    r.logp -= total;
                    r.logp.exp()
                })
                .collect()
        } else {
            // Unreachable for valid options (every predictive is finite). Fall back to "no
            // change" (all mass on the longest run) rather than emit NaN.
            let last = next.len() - 1;
            for (r, run) in next.iter_mut().enumerate() {
                run.logp = if r == last { 0.0 } else { f64::NEG_INFINITY };
            }
            next.iter().map(|r| r.logp.exp()).collect()
        };
        for p in &mut post {
            if !p.is_finite() {
                *p = 0.0;
            }
        }
        posteriors.push(post);
        runs = next;
    }
    Ok(BocpdRun {
        posteriors,
        skipped,
    })
}

/// Run-length posterior after each observation. Invalid options give an empty vector (no
/// posterior, so no change can be claimed); use [`run_bocpd_counted`] for the error and the
/// skipped-observation count.
pub fn run_bocpd(obs: &[(u32, u32)], o: &BocpdOpts) -> Vec<Vec<f64>> {
    run_bocpd_counted(obs, o)
        .map(|r| r.posteriors)
        .unwrap_or_default()
}

/// Summary of a run-length posterior after game `t`. Game indices are 1-based.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChangeSummary {
    /// Mass on runs `r < min(window_games, t - min_prior_games)`.
    pub p_change: f64,
    /// Most probable run length (ties prefer the longer run, i.e. no change).
    pub map_run: usize,
    /// `t - map_run + 1`: first game of the current segment.
    pub onset_index: usize,
    /// Earliest onset in the smallest highest-mass run-length set covering `level`.
    pub onset_low: usize,
    /// Latest onset in that set.
    pub onset_high: usize,
}

/// Summarises `post` (the posterior after game `t`).
///
/// Entries that are NaN, infinite or negative, and entries for impossible runs `r > t`, are
/// skipped; the remaining mass is renormalised. Without any valid mass the summary claims
/// nothing: `p_change = 0`, `map_run = t`, onset 1 and bounds `[1, t + 1]`. A NaN or
/// above-one `level` uses the full set; a level at or below zero uses the MAP run alone.
pub fn change_summary(
    post: &[f64],
    t: usize,
    window_games: usize,
    min_prior_games: usize,
    level: f64,
) -> ChangeSummary {
    let mass: Vec<(usize, f64)> = post
        .iter()
        .take(t + 1)
        .enumerate()
        .filter(|(_, p)| p.is_finite() && **p > 0.0)
        .map(|(r, p)| (r, *p))
        .collect();
    let total: f64 = mass.iter().map(|(_, p)| p).sum();
    if mass.is_empty() || !(total.is_finite() && total > 0.0) {
        return ChangeSummary {
            p_change: 0.0,
            map_run: t,
            onset_index: 1,
            onset_low: 1,
            onset_high: t + 1,
        };
    }
    let limit = window_games.min(t.saturating_sub(min_prior_games));
    let near: f64 = mass
        .iter()
        .filter(|(r, _)| *r < limit)
        .map(|(_, p)| p)
        .sum();
    let p_change = (near / total).clamp(0.0, 1.0);

    // Highest mass first; ties prefer the longer run.
    let mut order = mass.clone();
    order.sort_by(|x, y| y.1.total_cmp(&x.1).then(y.0.cmp(&x.0)));
    let map_run = order[0].0;
    let level = if level.is_nan() {
        1.0
    } else {
        level.clamp(0.0, 1.0)
    };
    let target = level * total * (1.0 - 1e-12);
    let (mut r_min, mut r_max) = (map_run, map_run);
    let mut cum = 0.0;
    for &(r, p) in &order {
        r_min = r_min.min(r);
        r_max = r_max.max(r);
        cum += p;
        if cum >= target {
            break;
        }
    }
    ChangeSummary {
        p_change,
        map_run,
        onset_index: t - map_run + 1,
        onset_low: t - r_max + 1,
        onset_high: t - r_min + 1,
    }
}
