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
//!
//! [`Bocpd`] is the streaming form (memory `O(max_run)`); [`run_bocpd`] keeps every posterior.

use crate::stats::special::{lgamma, ln_beta};

pub use super::DetectorError;

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

#[derive(Clone, Copy, Debug)]
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

/// Streaming BOCPD: one [`Bocpd::step`] per game, memory `O(max_run)`.
#[derive(Clone, Debug)]
pub struct Bocpd {
    o: BocpdOpts,
    ln_h: f64,
    ln_1mh: f64,
    runs: Vec<Run>,
    post: Vec<f64>,
    t: usize,
    skipped: usize,
}

impl Bocpd {
    /// A detector before the first game. Invalid options are a typed error.
    pub fn new(o: &BocpdOpts) -> Result<Self, DetectorError> {
        validate(o)?;
        Ok(Self {
            o: *o,
            ln_h: o.hazard.ln(),
            ln_1mh: (-o.hazard).ln_1p(),
            runs: vec![Run {
                logp: 0.0,
                a: o.prior_a,
                b: o.prior_b,
            }],
            post: vec![1.0],
            t: 0,
            skipped: 0,
        })
    }

    /// Games stepped so far (including skipped ones).
    pub fn t(&self) -> usize {
        self.t
    }

    /// Observations skipped as invalid (`k > n`).
    pub fn skipped(&self) -> usize {
        self.skipped
    }

    /// The current run-length posterior (`[1.0]` before the first game).
    pub fn posterior(&self) -> &[f64] {
        &self.post
    }

    /// Adds game `(k, n)` and returns the run-length posterior after it. `k > n` is skipped as
    /// evidence (a no-information step) and counted.
    pub fn step(&mut self, k: u32, n: u32) -> &[f64] {
        let (k, n) = if k > n {
            self.skipped += 1;
            (0, 0)
        } else {
            (k, n)
        };
        self.t += 1;
        let max_run = self.o.max_run;
        let (kf, mf) = (f64::from(k), f64::from(n - k));
        let lchoose = lgamma(f64::from(n) + 1.0) - lgamma(kf + 1.0) - lgamma(mf + 1.0);
        let mut cp = f64::NEG_INFINITY;
        let mut next = Vec::with_capacity((self.runs.len() + 1).min(max_run + 1));
        next.push(Run {
            logp: 0.0,
            a: self.o.prior_a,
            b: self.o.prior_b,
        });
        for run in &self.runs {
            let pred = if n == 0 {
                0.0
            } else {
                lchoose + ln_beta(run.a + kf, run.b + mf) - ln_beta(run.a, run.b)
            };
            let joint = run.logp + pred;
            cp = logsumexp(cp, joint + self.ln_h);
            let grown = Run {
                logp: joint + self.ln_1mh,
                a: run.a + kf,
                b: run.b + mf,
            };
            if next.len() <= max_run {
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
        if total.is_finite() {
            for r in &mut next {
                r.logp -= total;
            }
        } else {
            // Unreachable for valid options (every predictive is finite). Fall back to "no
            // change" (all mass on the longest run) rather than emit NaN.
            let last = next.len() - 1;
            for (r, run) in next.iter_mut().enumerate() {
                run.logp = if r == last { 0.0 } else { f64::NEG_INFINITY };
            }
        }
        self.post.clear();
        self.post.extend(next.iter().map(|r| {
            let p = r.logp.exp();
            if p.is_finite() { p } else { 0.0 }
        }));
        self.runs = next;
        &self.post
    }
}

/// Runs BOCPD and counts skipped observations. See the module docs.
pub fn run_bocpd_counted(obs: &[(u32, u32)], o: &BocpdOpts) -> Result<BocpdRun, DetectorError> {
    let mut det = Bocpd::new(o)?;
    let posteriors = obs.iter().map(|&(k, n)| det.step(k, n).to_vec()).collect();
    Ok(BocpdRun {
        posteriors,
        skipped: det.skipped(),
    })
}

/// Run-length posterior after each observation. Invalid options give an empty vector (no
/// posterior, so no change can be claimed); use [`run_bocpd_counted`] for the error and the
/// skipped-observation count, or [`Bocpd`] to stream.
pub fn run_bocpd(obs: &[(u32, u32)], o: &BocpdOpts) -> Vec<Vec<f64>> {
    run_bocpd_counted(obs, o)
        .map(|r| r.posteriors)
        .unwrap_or_default()
}

/// Summary of a run-length posterior after game `t`. Game indices are 1-based.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChangeSummary {
    /// Mass on runs `r < min(window_games, t - min_prior_games)`, excluding the capped
    /// "at least max_run" bucket.
    ///
    /// It includes the prior hazard mass: with zero evidence it is `1 - (1 - H)^L` (about 0.22
    /// for `H = 1/200`, `L = 50`). Never show it, or act on it, without the evidence gate
    /// (opportunities since the onset `>= minEvidence`, Task 14).
    pub p_change: f64,
    /// Most probable run length (ties prefer the longer run, i.e. no change).
    pub map_run: usize,
    /// `t - map_run + 1`: first game of the current segment. When `map_run` is the capped
    /// bucket this is the latest possible onset.
    pub onset_index: usize,
    /// Earliest onset in the smallest highest-mass run-length set covering `level`. It is 1
    /// (an open lower bound) when the capped bucket is in that set.
    pub onset_low: usize,
    /// Latest onset in that set.
    pub onset_high: usize,
}

/// Summarises `post` (the posterior after game `t`).
///
/// When `post.len() <= t` its last entry is the capped bucket (run length "at least
/// `post.len() - 1`", see the module docs). That bucket never counts towards `p_change`, even
/// when `max_run < window_games`: it holds the no-change mass of a long stable series, so the
/// effective change window is `min(window_games, max_run)`. When the bucket is in the
/// highest-mass set, `onset_low` is 1: the segment may have started at any earlier game.
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
    let capped = (!post.is_empty() && post.len() <= t).then(|| post.len() - 1);
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
        .filter(|(r, _)| *r < limit && Some(*r) != capped)
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
    let onset_low = if capped == Some(r_max) {
        1
    } else {
        t - r_max + 1
    };
    ChangeSummary {
        p_change,
        map_run,
        onset_index: t - map_run + 1,
        onset_low,
        onset_high: t - r_min + 1,
    }
}
