# EXP-0002: do the 80% ranges of the weakness rates hold the true rate 80% of the time?

- Status: pre-registered
- Issue: #92 (and #13)
- Author: the maintainer, with Claude (lane `exp/0001-0002`)
- Kind: measurement (one version)
- Follows: none
- Registered: 2026-11-23. The registration is the first commit that adds this file
  (`git log --diff-filter=A -- preregistration.md`), so it is not written here.

## 1. Question

When Rematch shows a player "you hang a piece in 1.6% of your chances (80% range 1.2–2.0%)", does that range hold the player's true rate about 80% of the time? The smoke suite says 60% (12 of 20 intervals, `docs/benchmark.md` score b), and W1's interval is pulled towards the opponents' rate ([#13](https://github.com/Terkea/rematch/issues/13)). That benchmark number mixes the detector under-reading the flaw with the Beta posterior's prior pulling the estimate towards the peers. This experiment measures the posterior on its own: it gives `compute_estimates` (the production function, through `cli lab profile`) event counts drawn from rates we chose, so the counts are exact by construction, and measures how often its 80% interval holds the rate, per weakness, sample size, peers' rate (rating band), dispersion and schedule. EXP-0001's pilot reports the end-to-end coverage with the real detectors; the two differ in their peer rates, rates and schedules as well as in the detector, so the comparison is descriptive, not a decomposition.

Out of scope: the percentile-bootstrap intervals of the KPI snapshots (`profile_core::kpi::snapshots`, `bootstrap_mean_interval`) and the peer rows' bootstrap quantiles. Issue #92 names them, but lab-v1 has no step that returns them (`cli lab profile` returns the weakness estimates only), and adding one is a schema change (`lab-v2`). The follow-up issue says so.

## 2. Hypotheses

- H0: at least one weakness × checkpoint coverage of the production 80% interval, for players with a weakness (section 4), lies outside [0.75, 0.85].
- H1: every one lies inside [0.75, 0.85].

The expectation written before the run, so it can be wrong: the Beta prior centred on the peers' rate gives more than 80% coverage for a player at the peers' rate (ρ = 1) and less than 80% for a player well above it (ρ = 4 or 8) at 30 to 60 games, most for W1 (prior strength 300 chances) and least for W5 (flat prior). Rival explanations of a miss, and what separates them:

- The prior pull (#13): the interval misses on the high side (the true rate above the interval) and the posterior mean sits below the true rate; it shrinks with more games. Reported per cell as the share missing below and above, and the mean ratio of the posterior mean to the true rate.
- Recency weighting (60-day half-life, `estimate_weakness`): it down-weights the data but not the prior strength n0, so it adds to the pull, and it treats weighted counts as real counts, which can widen the interval. Separated by the schedule factor: the same draws with every game inside the last day (weights above 0.988), against two games a day.
- Overdispersion: real games vary in difficulty, so chances within a game are not independent; the posterior assumes they are. Separated by the dispersion factor (none, or a design effect of 1.5 for W1 and W9b).
- Discreteness at small counts (W4.missed and W5 have about one chance in two games): misses at both ends, which the calibration curve shows.

## 3. Algorithm versions compared

| Role | algorithm_id | What differs |
|---|---|---|
| Measured | `algo-1.1.0+fa5e2186e1dd` | – |

From `cli algo show` on the cli built from this branch. The posterior is `profile_core::stats::posterior::posterior`, called by `estimate_weakness` with the prior `{pooled_rate, n0[key]}` (W1 300, W4.missed 40, W7.traps 40, W9b 40) or, for W5, the evaluation baseline's flat Beta(1, 1) (ADR 0101).

## 4. Primary metric and ground truth

- Primary metric (exactly one): the set of 20 weakness × checkpoint coverages (5 keys × 30, 60, 120 and 250 games) of the production 80% interval (`posterior.interval`), for players whose true rate is 2, 4 or 8 times the peers' in odds (ρ in {2, 4, 8}), on the two-games-a-day schedule, each pooled over band and dispersion with cells weighted equally. These are players with a weakness, and the decision reads the worst of the 20, so an average of over- and under-coverage cannot pass.
- Ground truth: the per-chance rate θ each synthetic player's events were drawn from.
- Secondary metrics, confirmatory (21, Holm): coverage at the primary ρ and schedule by key (5), band, which here only moves the peers' rate (3), checkpoint (4) and dispersion (2); coverage by ρ over all five (5), two games a day; and by schedule (2) at the primary ρ.
- Secondary, descriptive: coverage among the estimates Rematch would show (`credible`: the 80% interval above the peers' median and an excess of at least 0.02 points a game, ADR 0101), per key, every ρ, two games a day. This conditions on the interval itself (a range that landed high is more often shown), so it is reported, not decided on. And the #13 cell: W1 against peers at 0.0008 with a true rate of 0.023 (`docs/benchmark.md`), both dispersions, every checkpoint.
- Exploratory: coverage of every cell (key × band × ρ × dispersion × schedule × checkpoint, 1,200 cells) with the share missing below and above; the calibration curve of the central Beta(α, β) posterior interval (the share of true rates inside the central q interval for q from 0.1 to 0.95, from `F(θ; α, β)` with the production α and β; the production interval is the same Beta quantiles widened to include the mean, which differs only for tiny shapes) per ρ; the mean ratio of posterior mean to true rate; the median interval width over the true rate; the share of intervals with no event.

## 5. The minimum effect that matters

- Minimum effect: a coverage outside [0.75, 0.85].
- Why this size: an 80% range that holds the truth three times in four or seventeen times in twenty is still read correctly by a player ("usually inside"). Outside that, the words we put on it ("80% range") are wrong in a way a player would notice over a few weaknesses. The band is narrower than the benchmark's [0.65, 0.95] because the benchmark's threshold also absorbs detector error, which this design removes.

## 6. Power analysis

- Method: closed form, because the decision reads intervals of shares.
- Inputs: each of the 20 primary coverages pools 3 ρ × 3 bands × 2 dispersions × 200 players = 3,600 players, one interval each (a player gives one interval per key and checkpoint), so they are independent within a coverage: a standard error of sqrt(0.16 / 3,600) = 0.0067 at 0.80, and a Bonferroni half-width for 20 (z = 3.02) of 0.020.
- Result: 200 players per cell. A coverage whose true value is 0.80 lands "within the band" with probability above 0.99; one at 0.78 or 0.82 with probability about 0.93; one at 0.72 or 0.88 or further out lands "outside" with probability above 0.9. Each confirmatory margin pools at least 3,600 players (the schedule margins 7,200, the ρ margins 1,200 players with 20 intervals each); at the Bonferroni level for 21 (z = 3.03) the worst-case half-width is 0.035 for a ρ margin and at most 0.020 for the others. Each exploratory cell has 200 independent intervals, a Wilson 95% half-width of about 0.056 at 0.80.
- Sensitivity: 100 players per cell would widen the primary half-widths to 0.029, and a true 0.78 would then pass only about two times in three; 200 costs about 50,000 profile computations, under an hour in the lab image.

## 7. Design

- Unit of analysis: the simulated player. Every interval of a player (5 keys × 4 checkpoints, on one schedule) is resampled together.
- Factors: rating band B1, B2, B3 (the audience up to about 2000 Lichess-equivalent, `profile_core::bands`) × ρ in {0.5, 1, 2, 4, 8} × dispersion {none, de1.5}: 30 cells of 200 players, each drawn once and run on two schedules, two games a day ("spread", 250 games over 125 days, inside the 183-day window) and every game inside the 24 hours before the anchor ("compressed"), paired on the same draws. Within each player: the five keys and four checkpoints (30, 60, 120 and 250 games). Plus the #13 cell: 200 players per dispersion, W1 only, peers at 0.0008, θ = 0.023, band B2, two games a day.
- The keys, their chances and the peers' rate at B2. These are assumptions written before the run, not measurements: no per-band peer reference is committed to the repository, so they set where the prior sits in plausible places, and the #13 cell adds the one case we have measured.
  - W1: Poisson(53) eligible moves a game (the clean control's 4,274 in 80 games, `docs/benchmark.md`); peers 0.004 (the contract fixture's value);
  - W9b: a game reaches the low clock with probability 1/3 (`docs/benchmark.md`), then Poisson(31.8) chances (10.6 a game on average, 846 in 80 games); peers 0.02;
  - W4.missed: Poisson(1) mate chances a game; peers 0.30;
  - W7.traps: one chance per game; peers 0.10;
  - W5: a winning episode in a game with probability 0.4; no peer row by design, compared with the evaluation baseline 0.15 (an epl per event of 1) and a flat prior, as `compute_estimates` does.
- Bands: the peers' odds are multiplied by 1.25 at B1 and 0.8 at B3 relative to B2 (a rate about 20% higher or lower, the per-400-points gradient of the blunder rates in `docs/research/improvement-engine.md` section 2). W5's baseline is not a peer rate and does not change with the band. The band enters the interval only through the peers' rate.
- True rate: odds(θ) = ρ × odds(peers' rate), so θ stays below 1 (W4.missed at B1 and ρ = 8 is 0.811).
- Events: Binomial(chances, θ) per game; with de1.5 the game's rate is drawn from Beta(θκ, (1 − θ)κ) with κ = (m − 1)/0.5 − 1 for the mean chances m of a game that has any (W1 103, W9b 60.6), a design effect of 1.5 (the 1.3 to 2 of `docs/research/improvement-engine.md` section 5.2). The other keys have about one chance in a game, where within-game dispersion does not apply, and are simulated the same in both levels. The rate does not drift over time.
- Reference rows: the peers' rate as `pooledRate` and `median`, p25/p40/p75 at 0.6/0.85/1.5 times it, 40 players, not low-sample: an exact peer rate, so this measures the posterior, not the peer benchmark's own noise.
- Seeds: `players/<band>/<rho>/<disp>` for each of the 30 cells (one numpy generator per cell, drawn in key order, used for both schedules), `issue13/none` and `issue13/de1.5`, `bootstrap/primary`, `bootstrap/margins`. No other randomness.
- Development and confirmatory data: no candidate; the seeds are new and used once.
- Held fixed: the build's clock (one hour after the checkpoint's last game), the reference rows, `games_in_window` equal to the checkpoint.
- Data: no file inputs (`inputs.sha256` lists none). No real player's games.
- Leakage controls: not a predictive question.

## 8. Analysis plan

- Model or test: no model is fitted. Each interval scores 1 when `low ≤ θ ≤ high`. A player's coverage is its share over the intervals in scope; a coverage is the mean over its cells (band × ρ × dispersion × schedule) of the mean over players.
- Effect size: each primary coverage with its 95% percentile interval and its Bonferroni interval for 20 (the estimate ± 3.02 bootstrap standard errors), from a stratified cluster bootstrap (2,000 resamples of players within each cell, seed `bootstrap/primary`); the pooled primary coverage with its 95% interval, reported, not decided on. Each margin with its 95% percentile interval, a two-sided z test of coverage = 0.80 on the bootstrap standard error, and a Bonferroni interval (z = 3.03) for its classification; with this many players a z test rejects for any visible difference, so the classification, not the p value, is what the decision text uses. Each cell with a Wilson 95% interval (one interval per player in a cell). The credible-only and #13 coverages with Wilson 95% intervals over intervals, which are too narrow because a player gives up to four.
- Multiple comparisons: the 20 primary coverages, Bonferroni. The 21 confirmatory margins, Holm-adjusted p values and Bonferroni-adjusted intervals. The rest is exploratory and uncorrected. The intervals carry the simulation's sampling noise only, not the uncertainty in the assumed peer rates and chances.
- Diagnostics: no Bayesian fit. The run stops if the `algorithm_id` changes between calls or a response is out of order.
- Exclusions and missing data: none. Every estimate has an interval, including a key with no event in the window (its interval is still stored and shown as a range); the share of such intervals is reported per cell.

## 9. Decision rule

Measurement (one version), on the 20 primary coverages' Bonferroni intervals:

- Within the band: every one lies wholly inside [0.75, 0.85]. The ranges are calibrated for players with a weakness, on the registered grid.
- Outside the band: at least one lies wholly outside it. The 80% ranges are not calibrated for those players; the decision names each such key and checkpoint and the side it misses on. The follow-up is a comparison experiment for a fix (for example a weaker prior for W1, or a prior strength that decays with the recency weights as the data do), drafted as a v0.3 issue linking #13; this experiment changes no algorithm component.
- Need more data: anything else, under the one-follow-up rule.

Whatever the primary shows, a confirmatory margin whose Bonferroni interval lies wholly outside the band, and the #13 cell, are named in the decision and in the follow-up issue.

## 10. Analysis code

- Entry point: `lab/experiments/EXP-0002/analysis.py` (`python analysis.py --out DIR` writes `DIR/results.json`, with the `algorithm_id` it ran on, and `DIR/figures/`). The algorithm is an input: the cli of `$REMATCH_CLI` is the version that runs, `--algorithm ID` stops the run when it is another one, and `--seed-namespace NS` draws a fresh replication on new seeds (the default, the experiment id, is the registered run).
- Analysis code sha256: `bc0a9ec07c49f65f175008784d359c09491b3f29263d42779d19081e09f2f391`

## 11. Deviations

None.
