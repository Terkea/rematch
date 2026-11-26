//! coaching-input v3 `trajectory` (ADR 0089): the rating over time of one account × time class,
//! its peak and current rating, and the "then" window (the games around the peak, or the first
//! games) against the last games, for rating, accuracy, blunders per game, score and the top
//! weaknesses.
//!
//! The rule is the web's (`apps/web/src/server/trajectory/model.ts`), line for line, so the coach
//! and the Progress page say the same thing: the same windows, the same seeds, the same
//! mulberry32 stream and bootstrap (`compareMeans`, 2000 resamples), the same Beta posteriors and
//! P(better) draws (`compareRates`). `tests/trajectory.rs` replays the web's fixture
//! (`backend/fixtures/trajectory/cases.json`) and checks every number. A difference is called
//! only on evidence (ADR 0083 §6): both windows hold `KPI_MIN_GAMES` games and the 80% interval
//! of the difference excludes 0 (a mean), or P(better) reaches 0.8 with a 15% relative change
//! (a rate).
//!
//! [`contract`] is the rule's whole output, which `cli lab trajectory` returns. The builder
//! passes it through `coaching_input::trajectory::player_visible`, which withholds `kind` and the
//! rating's verdict from the coach since EXP-0015 (ADR 0116); the web does the same in its view.

use std::collections::{BTreeMap, HashMap};

use coaching_input::{
    ChangeVerdict, ColorMetric, LIMITS, MetricUnit, MonthRating, RatingAt, ThenNowMetric,
    ThenNowWeakness, ThenVsNow, Trajectory as ContractTrajectory, TrajectoryKind, TrajectoryWindow,
    WindowBasis,
};
use profile_core::constants::{weakness_area, weakness_label};
use profile_core::stats::intervals::{Z80, wilson};
use profile_core::stats::posterior::{
    BetaPosterior, P_IMP_DRAWS, p_improvement, p_regression, posterior,
};
use profile_core::stats::rng::Rng;
use profile_core::types::WeaknessKey;
use sqlx::PgPool;
use time::{Date, OffsetDateTime, UtcOffset};
use uuid::Uuid;

/// Games per window (the web's `TRAJECTORY_WINDOW`).
pub const TRAJECTORY_WINDOW: usize = 30;
/// Games a side needs before anything about it is said (the web's `KPI_MIN_GAMES`).
pub const MIN_GAMES: usize = 10;
/// Rated games before a trajectory is stated: two windows of [`MIN_GAMES`].
pub const TRAJECTORY_MIN_GAMES: usize = 2 * MIN_GAMES;
/// Top weaknesses compared (the web's `TRAJECTORY_WEAKNESSES`).
pub const TRAJECTORY_WEAKNESSES: usize = 3;
/// The web's `BOOTSTRAP_RESAMPLES` (its `compareMeans` and `estimateMean`).
pub const RESAMPLES: usize = 2000;
const CONFIDENCE: f64 = 0.8;
/// `CONFIRM_P_CHANGE` and `CONFIRM_REL_CHANGE` (ADR 0083 §6).
const CONFIRM_P: f64 = 0.8;
const CONFIRM_REL: f64 = 0.15;
/// The engine's prior without a benchmark (`estimate_weakness`).
const NO_BENCHMARK_PRIOR: Prior = Prior { mu: 0.5, n0: 2.0 };

// ---- the web's estimators -------------------------------------------------------------------

/// TS `fnv1a` over the UTF-16 code units of `text`.
pub fn fnv1a(text: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for unit in text.encode_utf16() {
        h ^= u32::from(unit);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// TS `canonical`: a sorted copy, so row order cannot move a bootstrap.
fn canonical(values: &[f64]) -> Vec<f64> {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    v
}

/// TS `dataSeed`: `fnv1a(label)` mixed with each sample's length and the little-endian 32-bit
/// words of its sorted values (low word first), one FNV-1a step per word.
fn data_seed(label: &str, samples: &[&[f64]]) -> u32 {
    let mut h = fnv1a(label);
    let mut mix = |w: u32| h = (h ^ w).wrapping_mul(0x0100_0193);
    for sample in samples {
        mix(sample.len() as u32);
        for v in canonical(sample) {
            let bits = v.to_bits();
            mix(bits as u32);
            mix((bits >> 32) as u32);
        }
    }
    h
}

/// TS `Math.round`: halves round up.
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// The `floor(q * R)` order statistic of sorted resamples (the web's `at`).
fn at(sorted: &[f64], q: f64) -> f64 {
    let r = sorted.len();
    let i = (q * r as f64).floor();
    let i = if i < 0.0 { 0 } else { (i as usize).min(r - 1) };
    sorted[i]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interval {
    pub low: f64,
    pub high: f64,
}

/// TS `Estimate`: a value with its 80% interval (null without spread), or insufficient.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Estimate {
    Estimate {
        value: f64,
        interval: Option<Interval>,
        n: usize,
    },
    Insufficient {
        n: usize,
        needed: usize,
    },
}

#[derive(Clone, Copy, Debug)]
struct Domain {
    min: f64,
    max: f64,
}

const COUNTS: Domain = Domain {
    min: 0.0,
    max: f64::INFINITY,
};
const PCT: Domain = Domain {
    min: 0.0,
    max: 100.0,
};
const RATING: Domain = Domain {
    min: 0.0,
    max: 4000.0,
};
const UNBOUNDED: Domain = Domain {
    min: f64::NEG_INFINITY,
    max: f64::INFINITY,
};

fn insufficient(n: usize) -> Estimate {
    Estimate::Insufficient {
        n,
        needed: MIN_GAMES.saturating_sub(n),
    }
}

/// TS `bootstrapMean`: the mean and the seeded percentile interval.
fn bootstrap_mean(raw: &[f64], seed: &str) -> Option<(f64, Interval)> {
    let values = canonical(raw);
    let values = &values[..];
    let n = values.len();
    if n == 0 {
        return None;
    }
    let nf = n as f64;
    let mean = values.iter().fold(0.0, |a, b| a + b) / nf;
    let mut rng = Rng::new(data_seed(seed, &[values]));
    let mut means = vec![0.0; RESAMPLES];
    for m in means.iter_mut() {
        let mut sum = 0.0;
        for _ in 0..n {
            sum += values[(rng.next_f64() * nf).floor() as usize];
        }
        *m = sum / nf;
    }
    means.sort_by(f64::total_cmp);
    let tail = (1.0 - CONFIDENCE) / 2.0;
    Some((
        mean,
        Interval {
            low: at(&means, tail),
            high: at(&means, 1.0 - tail),
        },
    ))
}

/// TS `finishInterval`: clamped to the domain, none without spread.
fn finish(low: f64, high: f64, d: Domain) -> Option<Interval> {
    let lo = low.max(d.min);
    let hi = high.min(d.max);
    (lo.is_finite() && hi.is_finite() && hi - lo > 0.0).then_some(Interval { low: lo, high: hi })
}

/// TS `estimateMean`.
fn estimate_mean(values: &[f64], seed: &str, d: Domain) -> Estimate {
    let clean: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if clean.len() < MIN_GAMES {
        return insufficient(clean.len());
    }
    match bootstrap_mean(&clean, seed) {
        Some((value, i)) => Estimate::Estimate {
            value,
            interval: finish(i.low, i.high, d),
            n: clean.len(),
        },
        None => insufficient(clean.len()),
    }
}

/// TS `estimateRate` (a Wilson interval; `successes` may be fractional).
fn estimate_rate(successes: f64, n: usize) -> Estimate {
    if n < MIN_GAMES {
        return insufficient(n);
    }
    let nf = n as f64;
    let s = successes.clamp(0.0, nf);
    let interval = wilson(s, nf, Z80).ok().map(|i| Interval {
        low: i.low,
        high: i.high,
    });
    Estimate::Estimate {
        value: s / nf,
        interval,
        n,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Better,
    Worse,
    NoClearChange,
    NotEnoughGames,
}

impl Verdict {
    fn contract(self) -> ChangeVerdict {
        match self {
            Verdict::Better => ChangeVerdict::Better,
            Verdict::Worse => ChangeVerdict::Worse,
            Verdict::NoClearChange => ChangeVerdict::NoClearChange,
            Verdict::NotEnoughGames => ChangeVerdict::NotEnoughGames,
        }
    }
}

/// TS `MeanComparison`: the difference `b - a` and its interval when a verdict was reached.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeanComparison {
    pub verdict: Verdict,
    pub diff: Option<(f64, Interval)>,
}

/// TS `compareMeans`.
pub fn compare_means(a: &[f64], b: &[f64], higher_is_better: bool, seed: &str) -> MeanComparison {
    let ea = estimate_mean(a, &format!("{seed}:a"), UNBOUNDED);
    let eb = estimate_mean(b, &format!("{seed}:b"), UNBOUNDED);
    let (Estimate::Estimate { value: va, .. }, Estimate::Estimate { value: vb, .. }) = (ea, eb)
    else {
        return MeanComparison {
            verdict: Verdict::NotEnoughGames,
            diff: None,
        };
    };
    let clean = |xs: &[f64]| -> Vec<f64> { xs.iter().copied().filter(|v| v.is_finite()).collect() };
    let (ca, cb) = (canonical(&clean(a)), canonical(&clean(b)));
    let mut rng = Rng::new(data_seed(&format!("{seed}:diff"), &[&ca, &cb]));
    let mut resampled = |xs: &[f64]| {
        let n = xs.len() as f64;
        let mut sum = 0.0;
        for _ in 0..xs.len() {
            sum += xs[(rng.next_f64() * n).floor() as usize];
        }
        sum / n
    };
    let mut diffs = vec![0.0; RESAMPLES];
    for d in diffs.iter_mut() {
        // JS evaluates `resampledMean(b) - resampledMean(a)` left to right: b first.
        let mb = resampled(&cb);
        let ma = resampled(&ca);
        *d = mb - ma;
    }
    diffs.sort_by(f64::total_cmp);
    let tail = (1.0 - CONFIDENCE) / 2.0;
    let interval = Interval {
        low: at(&diffs, tail),
        high: at(&diffs, 1.0 - tail),
    };
    let up = interval.low > 0.0;
    let down = interval.high < 0.0;
    let verdict = if up || down {
        if up == higher_is_better {
            Verdict::Better
        } else {
            Verdict::Worse
        }
    } else {
        Verdict::NoClearChange
    };
    MeanComparison {
        verdict,
        diff: Some((vb - va, interval)),
    }
}

/// A Beta prior `{mu, n0}`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prior {
    pub mu: f64,
    pub n0: f64,
}

/// The prior behind an estimate's posterior (the web's `priorOf`): `n0 = alpha + beta - n` and
/// `mu = (alpha - k) / n0` with the weighted `k` and `n`; the no-benchmark prior otherwise.
pub fn prior_of(k: f64, n: f64, alpha: f64, beta: f64) -> Prior {
    let n0 = alpha + beta - n;
    let mu = if n0 > 0.0 { (alpha - k) / n0 } else { f64::NAN };
    if n0.is_nan() || n0 <= 1e-6 || !mu.is_finite() || mu <= 0.0 || mu >= 1.0 {
        return NO_BENCHMARK_PRIOR;
    }
    Prior { mu, n0 }
}

/// A rate's side: events, opportunities, and games with the chance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RateSide {
    pub events: u32,
    pub opportunities: u32,
    pub games: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RateComparison {
    pub verdict: Verdict,
    pub p_better: Option<f64>,
    pub p_worse: Option<f64>,
}

/// TS `compareRates` (`higherIsBetter` false: a weakness rate).
pub fn compare_rates(a: RateSide, b: RateSide, prior: Prior, seed: u32) -> RateComparison {
    if a.games < MIN_GAMES || b.games < MIN_GAMES {
        return RateComparison {
            verdict: Verdict::NotEnoughGames,
            p_better: None,
            p_worse: None,
        };
    }
    let pa = posterior(
        f64::from(a.events),
        f64::from(a.opportunities),
        prior.mu,
        prior.n0,
    );
    let pb = posterior(
        f64::from(b.events),
        f64::from(b.opportunities),
        prior.mu,
        prior.n0,
    );
    let p_lower = p_improvement(&pb, &pa, 0.0, P_IMP_DRAWS, seed);
    let p_higher = p_regression(&pb, &pa, 1.0, P_IMP_DRAWS, seed);
    let (p_better, p_worse) = (p_lower, p_higher);
    let rel = if pa.mean > 0.0 {
        (pb.mean / pa.mean - 1.0).abs()
    } else if pb.mean > 0.0 {
        f64::INFINITY
    } else {
        0.0
    };
    let big = rel >= CONFIRM_REL;
    let verdict = if big && p_better >= CONFIRM_P {
        Verdict::Better
    } else if big && p_worse >= CONFIRM_P {
        Verdict::Worse
    } else {
        Verdict::NoClearChange
    };
    RateComparison {
        verdict,
        p_better: Some(p_better),
        p_worse: Some(p_worse),
    }
}

// ---- the trajectory -------------------------------------------------------------------------

/// One rated game with the player's pre-game rating.
#[derive(Clone, Debug, PartialEq)]
pub struct RatedGame {
    pub id: String,
    pub played_at: OffsetDateTime,
    pub rating: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameResult {
    Win,
    Draw,
    Loss,
    Unfinished,
}

/// A game's stats from the player's side (the web's `GameStatRow`, the fields used here).
#[derive(Clone, Debug, PartialEq)]
pub struct StatRow {
    pub id: String,
    pub result: GameResult,
    pub analysed: bool,
    pub accuracy: Option<f64>,
    pub blunders: Option<f64>,
}

/// A top weakness: its prior and per-game `(events, opportunities)`.
#[derive(Clone, Debug, PartialEq)]
pub struct WeaknessInput {
    pub series_key: String,
    pub prior: Prior,
    pub points: HashMap<String, (u32, u32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    pub games: Vec<RatedGame>,
    pub current: Option<(f64, OffsetDateTime)>,
    pub stats: Vec<StatRow>,
    pub weaknesses: Vec<WeaknessInput>,
    pub seed: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub basis: WindowBasis,
    pub games: usize,
    pub from: OffsetDateTime,
    pub to: OffsetDateTime,
    pub first_id: String,
    pub last_id: String,
    pub mean_rating: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RowKey {
    Accuracy,
    Blunders,
    Score,
    Weakness(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    Mean {
        key: RowKey,
        a: Estimate,
        b: Estimate,
        result: MeanComparison,
    },
    Rate {
        key: RowKey,
        counts: (RateSide, RateSide),
        a: Option<BetaPosterior>,
        b: Option<BetaPosterior>,
        result: RateComparison,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ready {
    pub kind: TrajectoryKind,
    pub games: usize,
    pub peak: (f64, OffsetDateTime, Option<String>),
    pub current: (f64, OffsetDateTime),
    pub change: f64,
    pub then: Window,
    pub now: Window,
    pub rating: MeanComparison,
    pub rating_sides: (Estimate, Estimate),
    pub rows: Vec<Row>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Computed {
    NotEnoughGames { games: usize, needed: usize },
    Ready(Box<Ready>),
}

fn ms(t: OffsetDateTime) -> i128 {
    t.unix_timestamp_nanos().div_euclid(1_000_000)
}

/// The web's `computeTrajectory` (module docs). Returns the sorted series too.
pub fn compute(input: &Input) -> (Vec<RatedGame>, Computed) {
    let mut series: Vec<RatedGame> = input
        .games
        .iter()
        .filter(|g| g.rating.is_finite())
        .cloned()
        .collect();
    series.sort_by(|a, b| ms(a.played_at).cmp(&ms(b.played_at)).then(a.id.cmp(&b.id)));
    let n = series.len();
    if n < TRAJECTORY_MIN_GAMES {
        return (
            series,
            Computed::NotEnoughGames {
                games: n,
                needed: TRAJECTORY_MIN_GAMES - n,
            },
        );
    }
    let newest = &series[n - 1];
    let current = match input.current {
        Some((r, at)) if r.is_finite() => (r, at),
        _ => (newest.rating, newest.played_at),
    };
    let mut peak_idx = 0;
    for i in 1..n {
        if series[i].rating > series[peak_idx].rating {
            peak_idx = i;
        }
    }
    let peak_game = &series[peak_idx];
    let peak = if current.0 > peak_game.rating {
        (current.0, current.1, None)
    } else {
        (
            peak_game.rating,
            peak_game.played_at,
            Some(peak_game.id.clone()),
        )
    };
    let size = TRAJECTORY_WINDOW.min(n / 2);
    let now_start = n - size;
    let peak_earlier = peak.2.is_some() && peak_idx < now_start;
    let then_start = if peak_earlier {
        let centred = peak_idx as i64 - (size / 2) as i64;
        centred.min((now_start - size) as i64).max(0) as usize
    } else {
        0
    };
    let window = |start: usize, basis: WindowBasis| {
        let slice = &series[start..start + size];
        Window {
            basis,
            games: slice.len(),
            from: slice[0].played_at,
            to: slice[slice.len() - 1].played_at,
            first_id: slice[0].id.clone(),
            last_id: slice[slice.len() - 1].id.clone(),
            mean_rating: slice.iter().fold(0.0, |s, p| s + p.rating) / slice.len() as f64,
        }
    };
    let then = window(
        then_start,
        if peak_earlier {
            WindowBasis::Peak
        } else {
            WindowBasis::Start
        },
    );
    let now = window(now_start, WindowBasis::Last);
    let then_games = &series[then_start..then_start + size];
    let now_games = &series[now_start..];
    let then_ratings: Vec<f64> = then_games.iter().map(|g| g.rating).collect();
    let now_ratings: Vec<f64> = now_games.iter().map(|g| g.rating).collect();
    let seed = &input.seed;
    let rating = compare_means(&then_ratings, &now_ratings, true, &format!("{seed}:rating"));
    let rating_sides = (
        estimate_mean(&then_ratings, &format!("{seed}:rating:a"), RATING),
        estimate_mean(&now_ratings, &format!("{seed}:rating:b"), RATING),
    );
    let kind = match rating.verdict {
        Verdict::Worse => TrajectoryKind::Falling,
        Verdict::Better => TrajectoryKind::Rising,
        _ => TrajectoryKind::Flat,
    };

    let by_id: HashMap<&str, &StatRow> = input.stats.iter().map(|r| (r.id.as_str(), r)).collect();
    let rows_of = |games: &[RatedGame]| -> Vec<&StatRow> {
        games
            .iter()
            .filter_map(|g| by_id.get(g.id.as_str()).copied())
            .collect()
    };
    let (ra, rb) = (rows_of(then_games), rows_of(now_games));
    let mut rows = period_rows(&ra, &rb, &format!("{seed}:then-now"));
    for (i, w) in input.weaknesses.iter().enumerate() {
        let side = |games: &[RatedGame]| {
            let mut s = RateSide::default();
            for g in games {
                if let Some(&(e, o)) = w.points.get(&g.id)
                    && o > 0
                {
                    s.events += e;
                    s.opportunities += o;
                    s.games += 1;
                }
            }
            s
        };
        let (a, b) = (side(then_games), side(now_games));
        let seed = u32::try_from(i + 1).unwrap_or(u32::MAX);
        let post = |s: RateSide| {
            (s.games >= MIN_GAMES).then(|| {
                posterior(
                    f64::from(s.events),
                    f64::from(s.opportunities),
                    w.prior.mu,
                    w.prior.n0,
                )
            })
        };
        rows.push(Row::Rate {
            key: RowKey::Weakness(w.series_key.clone()),
            counts: (a, b),
            a: post(a),
            b: post(b),
            result: compare_rates(a, b, w.prior, seed),
        });
    }
    let reference = if then.basis == WindowBasis::Peak {
        peak.0
    } else {
        then.mean_rating
    };
    let ready = Ready {
        kind,
        games: n,
        peak,
        current,
        change: js_round(current.0 - reference),
        then,
        now,
        rating,
        rating_sides,
        rows,
    };
    (series, Computed::Ready(Box::new(ready)))
}

/// The web's `comparePeriodRows` over the two windows' stat rows (accuracy, blunders, score).
fn period_rows(a: &[&StatRow], b: &[&StatRow], seed: &str) -> Vec<Row> {
    let analysed = |rs: &[&StatRow], f: fn(&StatRow) -> Option<f64>| -> Vec<f64> {
        rs.iter()
            .filter(|r| r.analysed)
            .filter_map(|r| f(r))
            .filter(|v| v.is_finite())
            .collect()
    };
    let points = |rs: &[&StatRow]| -> Vec<f64> {
        rs.iter()
            .filter_map(|r| match r.result {
                GameResult::Win => Some(1.0),
                GameResult::Draw => Some(0.5),
                GameResult::Loss => Some(0.0),
                GameResult::Unfinished => None,
            })
            .collect()
    };
    let score = |rs: &[&StatRow]| {
        let wins = rs.iter().filter(|r| r.result == GameResult::Win).count();
        let draws = rs.iter().filter(|r| r.result == GameResult::Draw).count();
        let losses = rs.iter().filter(|r| r.result == GameResult::Loss).count();
        estimate_rate(wins as f64 + draws as f64 / 2.0, wins + draws + losses)
    };
    let acc = |r: &StatRow| r.accuracy;
    let bl = |r: &StatRow| r.blunders;
    let (sa, sb) = (format!("{seed}:a"), format!("{seed}:b"));
    vec![
        Row::Mean {
            key: RowKey::Accuracy,
            a: estimate_mean(&analysed(a, acc), &format!("{sa}:accuracy"), PCT),
            b: estimate_mean(&analysed(b, acc), &format!("{sb}:accuracy"), PCT),
            result: compare_means(
                &analysed(a, acc),
                &analysed(b, acc),
                true,
                &format!("{seed}:accuracy"),
            ),
        },
        Row::Mean {
            key: RowKey::Blunders,
            a: estimate_mean(&analysed(a, bl), &format!("{sa}:blunders"), COUNTS),
            b: estimate_mean(&analysed(b, bl), &format!("{sb}:blunders"), COUNTS),
            result: compare_means(
                &analysed(a, bl),
                &analysed(b, bl),
                false,
                &format!("{seed}:blunders"),
            ),
        },
        Row::Mean {
            key: RowKey::Score,
            a: score(a),
            b: score(b),
            result: compare_means(&points(a), &points(b), true, &format!("{seed}:score")),
        },
    ]
}

// ---- the contract ---------------------------------------------------------------------------

fn utc_date(t: OffsetDateTime) -> Date {
    t.to_offset(UtcOffset::UTC).date()
}

fn rating_u16(r: f64) -> u16 {
    js_round(r).clamp(0.0, f64::from(coaching_input::limits::MAX_RATING)) as u16
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// An estimate as a contract number in `unit`: below the minimum it is stated as insufficient.
fn metric_of(e: Estimate, unit: MetricUnit) -> ColorMetric {
    match e {
        Estimate::Estimate { value, interval, n } => ColorMetric {
            value: Some(value),
            unit,
            n: count(n),
            low: interval.map(|i| i.low.min(value)),
            high: interval.map(|i| i.high.max(value)),
            sufficient: true,
            need_more: None,
        },
        Estimate::Insufficient { n, needed } => ColorMetric {
            value: None,
            unit,
            n: count(n),
            low: None,
            high: None,
            sufficient: false,
            need_more: Some(count(needed.max(1))),
        },
    }
}

fn then_now(a: Estimate, b: Estimate, r: MeanComparison, unit: MetricUnit) -> ThenNowMetric {
    ThenNowMetric {
        then: metric_of(a, unit),
        now: metric_of(b, unit),
        verdict: r.verdict.contract(),
        diff: r.diff.map(|d| d.0),
        diff_low: r.diff.map(|d| d.1.low),
        diff_high: r.diff.map(|d| d.1.high),
    }
}

fn rate_metric(p: Option<BetaPosterior>, games: usize) -> ColorMetric {
    match p {
        Some(p) if games >= MIN_GAMES => ColorMetric {
            value: Some(p.mean),
            unit: MetricUnit::Rate,
            n: count(games),
            low: Some(p.interval.low.min(p.mean)),
            high: Some(p.interval.high.max(p.mean)),
            sufficient: true,
            need_more: None,
        },
        _ => metric_of(insufficient(games), MetricUnit::Rate),
    }
}

/// Each UTC month of the line: its games, last and highest pre-game rating; the newest
/// `LIMITS.trajectory_months`, oldest first.
pub fn months(series: &[RatedGame]) -> Vec<MonthRating> {
    let mut by: BTreeMap<(i32, u8), MonthRating> = BTreeMap::new();
    for g in series {
        let d = utc_date(g.played_at);
        let key = (d.year(), u8::from(d.month()));
        let r = rating_u16(g.rating);
        by.entry(key)
            .and_modify(|m| {
                m.games += 1;
                m.last = r;
                m.high = m.high.max(r);
            })
            .or_insert_with(|| MonthRating {
                month: format!("{:04}-{:02}", key.0, key.1),
                games: 1,
                last: r,
                high: r,
            });
    }
    let all: Vec<MonthRating> = by.into_values().collect();
    let skip = all.len().saturating_sub(LIMITS.trajectory_months);
    all.into_iter().skip(skip).collect()
}

/// The contract `trajectory` of a computed one.
pub fn contract(series: &[RatedGame], c: &Computed) -> ContractTrajectory {
    let months = months(series);
    let r = match c {
        Computed::NotEnoughGames { games, needed } => {
            return ContractTrajectory {
                rated_games: count(*games),
                sufficient: false,
                need_more: Some(count((*needed).max(1))),
                kind: None,
                peak: None,
                current: None,
                change: None,
                months,
                then_vs_now: None,
            };
        }
        Computed::Ready(r) => r,
    };
    let window = |w: &Window| TrajectoryWindow {
        basis: w.basis,
        games: count(w.games),
        from: utc_date(w.from),
        to: utc_date(w.to),
    };
    let mut metrics: Vec<ThenNowMetric> = Vec::new();
    let mut weaknesses = Vec::new();
    for row in &r.rows {
        match row {
            Row::Mean { key, a, b, result } => {
                let unit = match key {
                    RowKey::Accuracy => MetricUnit::Pct,
                    RowKey::Blunders => MetricUnit::PerGame,
                    _ => MetricUnit::Rate,
                };
                metrics.push(then_now(*a, *b, *result, unit));
            }
            Row::Rate {
                key: RowKey::Weakness(series_key),
                counts,
                a,
                b,
                result,
            } => {
                let Ok(k) = series_key.parse::<WeaknessKey>() else {
                    continue;
                };
                let (id, variant) = k.contract_id();
                weaknesses.push(ThenNowWeakness {
                    series_key: series_key.clone(),
                    id: id.to_owned(),
                    variant: variant.map(str::to_owned),
                    label: weakness_label(k).to_owned(),
                    area: weakness_area(k),
                    then: rate_metric(*a, counts.0.games),
                    now: rate_metric(*b, counts.1.games),
                    verdict: result.verdict.contract(),
                    then_events: counts.0.events,
                    then_opportunities: counts.0.opportunities,
                    now_events: counts.1.events,
                    now_opportunities: counts.1.opportunities,
                });
            }
            Row::Rate { .. } => {}
        }
    }
    let mut metrics = metrics.into_iter();
    let (Some(accuracy), Some(blunders_per_game), Some(score)) =
        (metrics.next(), metrics.next(), metrics.next())
    else {
        unreachable!("period_rows gives accuracy, blunders and score");
    };
    weaknesses.truncate(LIMITS.trajectory_weaknesses);
    ContractTrajectory {
        rated_games: count(r.games),
        sufficient: true,
        need_more: None,
        kind: Some(r.kind),
        peak: Some(RatingAt {
            rating: rating_u16(r.peak.0),
            date: utc_date(r.peak.1),
        }),
        current: Some(RatingAt {
            rating: rating_u16(r.current.0),
            date: utc_date(r.current.1),
        }),
        change: Some(r.change.clamp(-4000.0, 4000.0) as i32),
        months,
        then_vs_now: Some(ThenVsNow {
            then: window(&r.then),
            now: window(&r.now),
            rating: then_now(
                r.rating_sides.0,
                r.rating_sides.1,
                r.rating,
                MetricUnit::Elo,
            ),
            accuracy,
            blunders_per_game,
            score,
            weaknesses,
        }),
    }
}

// ---- the reads ------------------------------------------------------------------------------

/// Pre-game ratings (ADR-FIX-C-1, the web's `preGameRatings`): Lichess stores the rating at the
/// start; chess.com the rating after the game, so the pre-game rating is the one after the
/// previous rated game (the first game keeps its own).
pub fn pre_game_ratings(
    chesscom: bool,
    rows: &[(String, OffsetDateTime, Option<i32>)],
) -> Vec<RatedGame> {
    let mut out = Vec::with_capacity(rows.len());
    let mut previous: Option<i32> = None;
    for (id, at, own) in rows {
        let pre = if chesscom { previous.or(*own) } else { *own };
        if let Some(pre) = pre {
            out.push(RatedGame {
                id: id.clone(),
                played_at: *at,
                rating: f64::from(pre),
            });
        }
        previous = *own;
    }
    out
}

/// One rated game as [`load_trajectory`] reads it: the player's own stored rating (on chess.com
/// the rating after the game, on Lichess the one before it), the result from their side, and
/// the analysis.
#[derive(Clone, Debug, PartialEq)]
pub struct TrajectoryGame {
    pub id: String,
    pub played_at: OffsetDateTime,
    pub own_rating: Option<i32>,
    pub result: GameResult,
    /// `analysis_status` is `done`.
    pub analysed: bool,
    pub accuracy: Option<f64>,
    /// The player's own moves judged a blunder.
    pub blunders: i32,
}

/// The [`Input`] of an account's rated games in `(played_at, id)` order (pure; the loader's
/// rule and `cli lab trajectory`'s): pre-game ratings, the latest stored rating as the current
/// one, and each game's stats, accuracy and blunders only when analysed.
pub fn trajectory_input(
    games: &[TrajectoryGame],
    chesscom: bool,
    weaknesses: Vec<WeaknessInput>,
    seed: String,
) -> Input {
    let ratings: Vec<(String, OffsetDateTime, Option<i32>)> = games
        .iter()
        .map(|g| (g.id.clone(), g.played_at, g.own_rating))
        .collect();
    let current = games
        .iter()
        .rev()
        .find_map(|g| g.own_rating.map(|x| (f64::from(x), g.played_at)));
    let stats = games
        .iter()
        .map(|g| StatRow {
            id: g.id.clone(),
            result: g.result,
            analysed: g.analysed,
            accuracy: if g.analysed { g.accuracy } else { None },
            blunders: g.analysed.then_some(f64::from(g.blunders)),
        })
        .collect();
    Input {
        games: pre_game_ratings(chesscom, &ratings),
        current,
        stats,
        weaknesses,
        seed,
    }
}

type GameRow = (
    Uuid,
    OffsetDateTime,
    bool,
    Option<i32>,
    Option<i32>,
    String,
    String,
    Option<f64>,
    Option<f64>,
    i32,
);

/// Reads and computes the account × time class trajectory. `top` is the top claimable
/// weaknesses as `(series key, prior)`, in priority order.
pub async fn load_trajectory(
    pool: &PgPool,
    account_id: Uuid,
    tc: chess_core::TimeClass,
    chesscom: bool,
    top: &[(String, Prior)],
) -> sqlx::Result<ContractTrajectory> {
    // Accuracy goes through text, as the web's driver reads a `real`, so both see 70.3 (not
    // 70.30000305). Blunders count the player's own judged moves, as `gameStatRows` does.
    let rows: Vec<GameRow> = sqlx::query_as(
        "SELECT g.id, g.played_at, g.player_color = 'white', g.white_rating, g.black_rating, \
           g.result::text, g.analysis_status::text, g.white_accuracy::text::float8, \
           g.black_accuracy::text::float8, \
           (SELECT count(*) FROM moves m WHERE m.game_id = g.id AND m.color = g.player_color \
              AND m.judgement = 'blunder')::int \
         FROM games g WHERE g.account_id = $1 AND g.time_class = $2::time_class AND g.rated \
         ORDER BY g.played_at, g.id",
    )
    .bind(account_id)
    .bind(tc.as_str())
    .fetch_all(pool)
    .await?;
    let games: Vec<TrajectoryGame> = rows
        .iter()
        .map(|r| TrajectoryGame {
            id: r.0.to_string(),
            played_at: r.1,
            own_rating: if r.2 { r.3 } else { r.4 },
            result: match (r.5.as_str(), r.2) {
                ("1/2-1/2", _) => GameResult::Draw,
                ("1-0", true) | ("0-1", false) => GameResult::Win,
                ("1-0", false) | ("0-1", true) => GameResult::Loss,
                _ => GameResult::Unfinished,
            },
            analysed: r.6 == "done",
            accuracy: if r.2 { r.7 } else { r.8 },
            blunders: r.9,
        })
        .collect();
    let keys: Vec<String> = top.iter().map(|(k, _)| k.clone()).collect();
    let points: Vec<(String, Uuid, i32, i32)> = if keys.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as(
            "SELECT series_key, game_id, events, opportunities FROM weakness_series_point \
             WHERE account_id = $1 AND time_class = $2::time_class AND series_key = ANY($3)",
        )
        .bind(account_id)
        .bind(tc.as_str())
        .bind(&keys)
        .fetch_all(pool)
        .await?
    };
    let weaknesses = top
        .iter()
        .map(|(key, prior)| WeaknessInput {
            series_key: key.clone(),
            prior: *prior,
            points: points
                .iter()
                .filter(|p| &p.0 == key)
                .map(|p| {
                    (
                        p.1.to_string(),
                        (
                            u32::try_from(p.2).unwrap_or(0),
                            u32::try_from(p.3).unwrap_or(0),
                        ),
                    )
                })
                .collect(),
        })
        .collect();
    let input = trajectory_input(
        &games,
        chesscom,
        weaknesses,
        format!("trajectory:{account_id}:{}", tc.as_str()),
    );
    let (series, computed) = compute(&input);
    Ok(contract(&series, &computed))
}
