# EXP-0002: do the 80% ranges of the weakness rates hold the true rate 80% of the time?

- Status: pre-registered
- Issue: #92 (and #13)
- Author: the maintainer, with Claude (lane `exp/0001-0002`)
- Kind: measurement (one version)
- Follows: none
- Registered: 2026-11-23. The registration is the first commit that adds this file
  (`git log --diff-filter=A -- preregistration.md`), so it is not written here.

## 1. Question

When Rematch shows a player "you hang a piece in 1.6% of your chances (80% range 1.2–2.0%)", does that range hold the player's true rate about 80% of the time? The smoke suite says 60% (12 of 20 intervals, `docs/benchmark.md` score b), and W1's interval is pulled towards the opponents' rate ([#13](https://github.com/Terkea/rematch/issues/13)). That benchmark number mixes two causes: the detector under-reading the flaw, and the Beta posterior's prior pulling the estimate towards the peers. This experiment isolates the second. It gives `compute_estimates` (the production function, through `cli lab profile`) event counts drawn from rates we chose, so the detector is exact by construction, and measures how often its 80% interval holds the rate, per weakness, sample size and rating band. The end-to-end coverage with the real detectors is EXP-0001's secondary, measured on its pilot; the gap between the two is the detector's share.

Out of scope: the percentile-bootstrap intervals of the KPI snapshots (`profile_core::kpi::snapshots`, `bootstrap_mean_interval`) and the peer rows' bootstrap quantiles. Issue #92 names them, but lab-v1 has no step that returns them (`cli lab profile` returns the weakness estimates only), and adding one is a schema change (`lab-v2`). The follow-up issue says so.

## 2. Hypotheses

- H0: the coverage of the production 80% interval, over the registered weakness conditions (section 4), lies outside [0.75, 0.85].
- H1: it lies inside [0.75, 0.85].

The expectation written before the run, so it can be wrong: the Beta prior centred on the peers' rate gives more than 80% coverage for a player at the peers' rate (ρ = 1) and less than 80% for a player well above it (ρ = 4 or 8) at 30 to 60 games, most for W1 (prior strength 300 chances) and least for W5 (flat prior). Rival explanations of a miss, and what separates them:

- The prior pull (#13): the interval misses on the high side (the true rate above the interval) and the posterior mean sits below the true rate; it shrinks with more games. Reported per cell as the share missing below and above, and the mean ratio of the posterior mean to the true rate.
- Overdispersion: real games vary in difficulty, so chances within a game are not independent; the posterior assumes they are. Separated by the dispersion factor (none, or a design effect of 1.5 for W1 and W9b).
- Recency weighting (60-day half-life): it lowers the effective sample but keeps the rate when the rate is constant, as here; it widens intervals, so it cannot cause under-coverage by itself.
- Discreteness at small counts (W4.missed and W5 have about one chance in two games): misses at both ends, which the calibration curve shows.

## 3. Algorithm versions compared

| Role | algorithm_id | What differs |
|---|---|---|
| Measured | `algo-1.1.0+fa5e2186e1dd` | – |

From `cli algo show` on the cli built from this branch. The posterior is `profile_core::stats::posterior::posterior`, called by `estimate_weakness` with the prior `{pooled_rate, n0[key]}` (W1 300, W4.missed 40, W7.traps 40, W9b 40) or, for W5, the evaluation baseline's flat Beta(1, 1) (ADR 0101).

## 4. Primary metric and ground truth

- Primary metric (exactly one): the coverage of the production 80% interval (`posterior.interval`) over the players whose true rate is two or four times the peers' in odds (ρ in {2, 4}), every key, band, dispersion and checkpoint, with every cell weighted equally (equal players per cell, so it is the pooled share). These are the players Rematch tells about a weakness, so their ranges are the ones a player reads.
- Ground truth: the per-chance rate θ each synthetic player's events were drawn from.
- Secondary metrics, confirmatory (19, Holm): coverage over the whole grid by key (5), band (3), ρ (5), games (4) and dispersion (2).
- Exploratory: coverage of every cell (key × band × ρ × dispersion × games, 600 cells) with the share missing below and above; the calibration curve (the share of true rates inside the central q posterior interval, for q from 0.1 to 0.95, from the probability integral transform `F(θ; α, β)` of the production α and β) per ρ; the mean ratio of posterior mean to true rate; the median interval width over the true rate; the share of intervals with no event.

## 5. The minimum effect that matters

- Minimum effect: a coverage outside [0.75, 0.85].
- Why this size: an 80% range that holds the truth three times in four or seventeen times in twenty is still read correctly by a player ("usually inside"). Outside that, the words we put on it ("80% range") are wrong in a way a player would notice over a few weaknesses. The band is narrower than the benchmark's [0.65, 0.95] because the benchmark's threshold also absorbs detector error, which this design removes.

## 6. Power analysis

- Method: closed form, because the decision reads one interval of a share. The question is whether the 95% interval of the primary is narrow enough to land inside the band when the true coverage is near 0.80.
- Inputs: the primary pools 2 ρ × 3 bands × 2 dispersions × 200 players = 2,400 players, each giving 20 intervals (5 keys × 4 checkpoints). Keys are simulated independently; the 4 checkpoints of a key are nested and correlated. The standard error lies between sqrt(0.16 / 48,000) = 0.0018 (independent intervals) and sqrt(0.16 / 2,400) = 0.0082 (every interval of a player identical), so the 95% half-width is between 0.004 and 0.016.
- Result: 200 players per cell. Even at the worst-case half-width, a true coverage in [0.78, 0.82] gives "within the band" with probability at least 0.95, and a true coverage below 0.72 or above 0.88 gives "outside" with probability at least 0.95. Each confirmatory margin pools at least 2,400 players; at the Bonferroni level for 19 margins (z = 3.02) its worst-case half-width is 0.025. Each exploratory cell has 200 independent intervals (one per player), a Wilson 95% half-width of about 0.056 at 0.80.
- Sensitivity: 100 players per cell would double the worst-case variance (half-width 0.023), still enough for the primary but not for the cells; 200 costs about 24,000 profile computations, minutes in the lab image.

## 7. Design

- Unit of analysis: the simulated player. Every interval of a player (5 keys × 4 checkpoints) is resampled together.
- Factors: rating band B1, B2, B3 (the audience up to about 2000 Lichess-equivalent, `profile_core::bands`) × ρ in {0.5, 1, 2, 4, 8} × dispersion {none, de1.5}: 30 cells, 200 players each. Within each player: the five keys and four checkpoints (30, 60, 120 and 250 games; two games a day, so 250 games span 125 days, inside the 183-day window).
- The keys, their chances and the peers' rate at B2 (assumptions, written before the run; they set where the prior sits, not whether it is right):
  - W1: Poisson(53) eligible moves a game (the clean control's 4,274 in 80 games, `docs/benchmark.md`); peers 0.004 (the contract fixture's value);
  - W9b: a game reaches the low clock with probability 1/3 (`docs/benchmark.md`), then Poisson(31.8) chances (10.6 a game on average, 846 in 80 games); peers 0.02;
  - W4.missed: Poisson(1) mate chances a game; peers 0.30;
  - W7.traps: one chance per game; peers 0.10;
  - W5: a winning episode in a game with probability 0.4; no peer row by design, compared with the evaluation baseline 0.15 (an epl per event of 1) and a flat prior, as `compute_estimates` does.
- Bands: the peers' odds are multiplied by 1.25 at B1 and 0.8 at B3 relative to B2 (a rate about 20% higher or lower, the per-400-points gradient of the blunder rates in `docs/research/improvement-engine.md` section 2). W5's baseline is not a peer rate and does not change with the band. The band enters the interval only through the peers' rate; the band itself sets trainability, not the posterior.
- True rate: odds(θ) = ρ × odds(peers' rate), so θ stays below 1 at ρ = 8 (W4.missed at B1 and ρ = 8 is 0.94).
- Events: Binomial(chances, θ) per game; with de1.5 the game's rate is drawn from Beta(θκ, (1 − θ)κ) with κ = (m − 1)/0.5 − 1 for the mean chances m of a game that has any (W1 103, W9b 60.6), a design effect of 1.5 (the 1.3 to 2 of `docs/research/improvement-engine.md` section 5.2). The other keys have about one chance in a game, where within-game dispersion does not apply, and are simulated the same in both levels.
- Reference rows: the peers' rate as `pooledRate` and `median`, p25/p40/p75 at 0.6/0.85/1.5 times it, 40 players, not low-sample: an exact peer rate, so this measures the posterior, not the peer benchmark's own noise.
- Seeds: `players/<band>/<rho>/<disp>` for each of the 30 cells (one numpy generator per cell, drawn in key order), `bootstrap/primary`, `bootstrap/margins`. No other randomness.
- Development and confirmatory data: no candidate; the seeds are new and used once.
- Held fixed: the build's clock (one hour after the checkpoint's last game), the 2-a-day schedule, the reference rows, `games_in_window` equal to the checkpoint.
- Data: no file inputs (`inputs.sha256` lists none). No real player's games.
- Leakage controls: not a predictive question.

## 8. Analysis plan

- Model or test: no model is fitted. Each interval scores 1 when `low ≤ θ ≤ high`. A player's coverage is its share over the intervals in scope; a margin's or the primary's coverage is the mean over its cells of the mean over players.
- Effect size: the primary's coverage with its 80%, 95% and 97.5% percentile intervals from a stratified cluster bootstrap (2,000 resamples of players within each cell, seed `bootstrap/primary`). Each margin's coverage with its 95% percentile interval, a two-sided z test of coverage = 0.80 on the bootstrap standard error, and a Bonferroni interval (z = 3.02) for its classification. Each cell's coverage with a Wilson 95% interval (one interval per player in a cell).
- Multiple comparisons: the 19 confirmatory margins are one family, Holm-adjusted p values and Bonferroni-adjusted intervals. The 600 cells and the calibration curve are exploratory and uncorrected.
- Diagnostics: no Bayesian fit. The run stops if the `algorithm_id` changes between calls or a response is out of order.
- Exclusions and missing data: none. Every estimate has an interval, including a key with no event in the window (its interval is still stored and shown as a range); the share of such intervals is reported per cell.

## 9. Decision rule

Measurement (one version), on the primary's 95% interval:

- Within the band: wholly inside [0.75, 0.85]. The ranges are calibrated for the players they are shown to; margins outside the band are still reported as where they are not.
- Outside the band: wholly outside it. The 80% ranges are not calibrated for players with a weakness. The follow-up is a comparison experiment for a fix (for example a weaker prior for W1, or a prior centred between the peers and the player), drafted as a v0.3 issue linking #13; this experiment changes no algorithm component.
- Need more data: anything else, under the one-follow-up rule.

Whatever the primary shows, a confirmatory margin whose Bonferroni interval lies wholly outside the band is named in the decision and in the follow-up issue.

## 10. Analysis code

- Entry point: `lab/experiments/EXP-0002/analysis.py` (`python analysis.py --out DIR` writes `DIR/results.json`, with the `algorithm_id` it ran on, and `DIR/figures/`)
- Analysis code sha256: `38f763d39f22886221ab74771af2f210a414de8e6acf41d7cd5d2ab6330c5222`

## 11. Deviations

None.
