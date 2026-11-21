# ADR-R2-5. Posterior and priority conventions

Date: 2026-11-21
Status: Accepted (statistics layer, R2 Task 11 fix round 1; posteriors, P_imp and priority, R2 Task 12 in section 7). Placeholder number; the controller renumbers R2 ADRs before merging.

## Context

R2 Task 11 ports the TS M2 statistics numerics (`profile-core::stats`): mulberry32, `ln Gamma`, the incomplete beta function, Beta quantiles and sampling, Wilson and bootstrap intervals, type-7 quantiles, band percentiles, the Beta-binomial fit, Spearman and recency weights. These numbers feed the weakness posteriors, the change-point timeline and the KPI confidence bands.

The first review found that the numerics were accurate on the TS test values, but:
- invalid shapes and degenerate fits produced NaN, which would flow silently into posteriors, intervals and contract fields;
- the quantile bisection used an absolute tolerance, so rare-event lower tails were artefacts: Beta(0.03, 1) gave about 3e-11 instead of 4.6e-34;
- band percentiles depended on float noise in tied quartiles. A zero rate against a band with many zero-rate players got four different answers depending on how the quartiles came out, and flat end segments saturated to plus or minus infinity.

## Decision

### 1. Error policy

- **Typed errors at the distribution layer.** `stats::error::StatsError` has four variants: `InvalidShape { a, b }`, `InvalidProbability { p }`, `InvalidCounts { k, n }` and `InvalidZ { z }`. These return a `Result`:
  - `BetaDist::{new, point_mass, from_shapes, quantile}`;
  - `beta_quantile` and `sample_beta`;
  - `wilson`;
  - `fit_beta_binomial`, which also validates its input.
- **The special functions keep the libm convention.** Only NaN input gives NaN. `lgamma` is `ln |Gamma|` (`+inf` at poles and at `+-inf`). `ln_beta` expects positive shapes; at negative shapes it gives `ln |B|` or `+inf` (a pole), not an error. `reg_inc_beta` gives NaN for NaN `x` or invalid shapes, and `BetaDist::cdf` gives NaN at a NaN `x`.
- **Sample-statistics primitives keep their documented empty cases.** `quantile` of an empty slice is NaN. `bootstrap_mean_interval` returns `None` for fewer than two values. `spearman` returns `None` for constant or mismatched input. `percentile_from_quartiles` is NaN only if an input is NaN.
- **A condition that valid data cannot produce is a `debug_assert!`** with documented release behaviour:
  - Wilson `k > n` is clamped to `n`;
  - a continued fraction that hits its 20 000-term cap returns the partial value (realistic shapes need about √max(a, b) terms);
  - a non-positive or NaN half-life: release builds give weight 0 to every observation, never NaN.

### 2. Degenerate fits and priors are point masses

- **The fit.** `fit_beta_binomial` returns `BetaBinomialFit::PointMass { at: 0 }` when no observation has an event, or when there are no observations at all (zero opportunities). It returns `PointMass { at: 1 }` when every opportunity was an event. Otherwise it returns `Beta { mu, phi }`. `fit.rates()` gives the matching `BetaDist`, and a point mass's quantiles are exactly its location.
- **Valid input only.** Pairs with `k = n = 0` (players without opportunities, expected in thin band cells) carry no information and are skipped, as in TS. Every other observation needs a finite `n > 0` and `0 ≤ k ≤ n`; NaN, infinite or negative values, `k > n` and `k > 0 = n` are `InvalidCounts`.
- **Prior limits.** `BetaDist::from_shapes(0, b)` is the point mass at 0, the limit of Beta(a, b) as a → 0. Likewise `(a, 0)` is the point mass at 1. This is how a prior `Beta(n0·pooled, n0·(1 − pooled))` with `pooled = 0` is represented (Task 12). `(0, 0)` stays `InvalidShape`.

### 3. Quantiles are relative in the lower tail

- **Bisection.** `BetaDist::quantile` bisects on `ln x` over `[ln f64::MIN_POSITIVE, 0]` until the bracket is at most 1e-10 wide. The relative error is then ≤ 5e-11, and the absolute error is ≤ 5e-11, which still meets the brief's "bisection to 1e-10".
- **Underflow.** A quantile below the smallest normal float, that is when `CDF(MIN_POSITIVE) ≥ p`, is exactly 0.
- **`ln B` is computed once** per distribution, outside the bisection loop.
- **Large shapes.** For shapes ≥ 10, `ln B` uses Stirling differences written with `ln_1p`, not `lgamma(a) + lgamma(b) − lgamma(a + b)`, so large shapes such as Beta(2000, 98000) or `I_x(1, 1e5)` keep about 1e-12 relative accuracy.

### 4. Band percentiles

`percentile_from_quartiles` keeps TS M2's rule for distinct quartiles: piecewise linear through (p25, 25), (p40, 40), (median, 50), (p75, 75), extended by the end slopes, clamped to [1, 99], and `100 − raw` for lower-is-better. For ties:

- **Merging.** Knots within `TIE_REL_TOL·scale` of each other merge into one point at their mean x, carrying the mid-rank of their percentiles. `TIE_REL_TOL = 1e-9` and `scale = max |knot|`. For example, `p25 = p40 = 0` becomes one point (0, 32.5).
- **Snapping.** A value within the same tolerance of a point takes that point's percentile exactly.
- **Interpolation.** Otherwise the polyline runs through the merged points. Beyond the ends it extends the nearest non-flat segment, so there are never infinite slopes.
- **Fully flat reference.** When all four knots merge, the point gets 50, values below it 25 and values above it 75 (the knot bounds).

The result is monotone, bounded in [1, 99], and continuous except for steps of `slope·tolerance` at the snap edges. These are negligible when merged points are far apart compared with the tolerance, but can reach the full percentile gap between two points that sit between `tol` and `2·tol` apart, which real Beta-fit or per-player-rate quartiles never do. The one exception is a reference whose quartiles are all exactly 0, which has no scale and steps at 0. Float noise in the quartiles (exact zeros, 1e-15 residues, unsorted knots) no longer changes the answer. A zero rate against a band whose p25, p40 and median are 0 always gets raw 37.5, so 62.5 for a lower-is-better rate.

### 5. Edge conventions

1. **Percentile ties** use the mid-rank as in section 4 (this replaces the first version's exact-tie midpoint and infinite-slope saturation).
2. **Wilson.** `n = 0` gives {0, 1}. A NaN or negative `k` or `n` is `InvalidCounts`, and a NaN, infinite or negative `z` is `InvalidZ`. `k > n` is a `debug_assert!`; release builds clamp it. The bounds are exactly 0 at k = 0 and exactly 1 at k = n.
3. **Recency weights.** A negative age (clock skew) counts as 0 days, so no point weighs more than 1. A NaN age is excluded with weight 0.
4. **Beta-binomial.** Degenerate fits are point masses (section 2). The φ grid is `10^(5i/59)` with exact end points, the first maximum wins, and the binomial coefficient is dropped because it does not depend on φ.
5. **Beta shapes.** Invalid Beta shapes are `InvalidShape`, never NaN. Sampling is a Marsaglia–Tsang gamma ratio formed in log space, so tiny shapes never give 0/0. A point mass samples its location and consumes no RNG draws.

### 6. Determinism scope

- **Bit-identical everywhere.** mulberry32 uses 32-bit integer arithmetic and is bit-identical to the canonical JavaScript on every platform. `bootstrap_mean_interval`, `wilson`, `quantile`, `percentile_from_quartiles` and `spearman` use only IEEE-exact operations (`+ − × ÷`, `sqrt`, `floor`). Given the same inputs and seed, they are bit-identical across platforms.
- **Same image only.** `lgamma`, `ln_beta`, `reg_inc_beta`, `beta_quantile`, `sample_beta`, `fit_beta_binomial` and `recency_weight`/`weighted_counts` call libm (`ln`, `exp`, `ln_1p`, `powf`, `sin`, `cos`). Libm implementations may differ in the last ulp. One ulp can flip a Marsaglia–Tsang acceptance test or a bisection branch, which desyncs a `sample_beta` stream and therefore P_imp. Results are reproducible within one image (same toolchain and libc, which the `rust-dev`/`rust-worker` images pin), not across platforms. Stored snapshots must not assume cross-platform bit equality of these values.

### 7. Posteriors, P_imp and priority (R2 Task 12)

`stats::posterior` and `stats::priority` port TS M2 Task 16.

- **80% everywhere.** Posterior intervals are the equal-tailed 10% and 90% Beta quantiles (`INTERVAL_LEVEL = 0.8`), like the Wilson and bootstrap intervals of section 1 and the KPI bands.
- **Window and weights.** Only points with `age_days ≤ 183` (six months, inclusive) count. Inside the window each point is recency-weighted `0.5^(age/60)` (section 5.3 rules for negative and NaN ages; a NaN age is also outside the window). `k`/`n` are the weighted counts and feed the posterior; `k_unweighted`/`n_unweighted` are the raw counts and feed the contract `events`/`opportunities`, `min_evidence_met` and the excess.
- **Prior.** `posterior(k, n, mu, n0)` is `Beta(n0·mu + k, n0·(1 − mu) + n − k)` with `mu = pooled_rate` and `n0 = N0[key]` (constants). Without a usable benchmark (no reference, or one whose `pooled_rate` or `epl_per_event` is missing or out of range) the prior is `Beta(1, 1)`.
- **Shape floor.** Both posterior shapes are floored at `SHAPE_FLOOR = 0.1`. This replaces the point-mass route of section 2 for posteriors: a zero pooled rate with no events would otherwise be a point mass at 0, claiming certainty from no data, and equal-tailed intervals of shapes below about 0.04 exclude the mean (Beta(0.02, 40) has q90 = 7.5e-5 < mean 5e-4), which violates the contract rule `intervalLow ≤ posteriorRate ≤ intervalHigh`. At 0.1 the worst case is the Gamma limit, `P(Gamma(0.1) ≤ 0.1) = 0.83 < 0.9`, so the mean always lies inside the interval; the floor adds at most a tenth of a pseudo-event. As a belt-and-braces guard against bisection tolerance the interval is also widened to the mean. Invalid counts or priors fail a `debug_assert!`; release builds clamp them (section 1 policy), so `posterior` is total and never NaN.
- **Confidence** `= P(theta > band median)` (`prob_above`, 0 without a benchmark).
- **Excess** `= max(0, mean − median) × (n_unweighted / games_in_window) × epl_per_event`: the player's own opportunities per game times the band's EPL per event, so excess is in expected points lost per game above the band median.
- **Priority** `= excess × TRAINABILITY[key][band] × confidence × (1 − fatigue)`. Fatigue is 0 until plans exist (M6/M7); the input is clamped to [0, 1] and NaN counts as 0.
- **No ranking without a benchmark or 30 games.** Fewer than `MIN_GAMES = 30` games in the window is `insufficient_data`; otherwise a missing benchmark is `no_benchmark`. Both force priority 0. `insufficient_data` wins when both apply, because it is what the player can fix. The contract (R-3) only carries `ok` estimates.
- **Precise** when the interval width is at most `0.25 × median`; **min evidence** when `n_unweighted ≥ MIN_EVIDENCE[key]`. The band percentile is of the posterior mean, lower rate is better.
- **Ranking.** `rank_weaknesses` keeps `RANKABLE` keys (drops the continuous W9d and W12) and sorts by priority descending, then by the TS key string (`"W10" < "W2"`).
- **P_imp.** `p_improvement = P(theta_c < theta_b (1 − 0.15))` and `p_regression = P(theta_c > theta_b × 1.15)`, estimated from 10 000 seeded Monte Carlo pairs (seed 1). One mulberry32 stream; each pair draws current first, then baseline. Zero draws or invalid shapes give 0 (no evidence). Per section 6, P_imp is reproducible per seed within one image, not bit-identical to TS or across platforms.
- **mulberry32** is the only generator (section 6), so every stochastic value in a profile is a function of its inputs and seed.


- **Callers handle `Result`** from the distribution layer.
- **Task 12** floors posterior shapes at 0.1 (section 7) instead of using the point-mass limit, so a zero pooled rate gives a proper, contract-valid posterior. Its intervals come from `BetaDist::quantile`, and its P_imp from `BetaDist::sample`.
- **A rare-event band (median 0) gives a small positive excess even with zero events**, because any proper posterior has a mean above 0. This is TS M2's rule (it already happens for any small pooled rate); the resulting priorities are tiny and rank last.
- **Tasks 15 and 34** never receive NaN or ±inf percentiles from finite references. For distinct quartiles the values equal TS M2's; the TS test values are pinned in `profile-core/tests/stats.rs`. Degenerate references now have defined values, where TS's division by zero gave NaN.
- **The test suite pins:**
  - the JS mulberry32 goldens;
  - exact 60-digit references for the incomplete beta and quantiles, including the relative lower tail and Beta(2000, 98000);
  - the point-mass fit-to-quantile chain;
  - the percentile tie cases;
  - continuity under 1e-12 perturbations;
  - monotonicity and bounds.
