# EXP-0001: how many synthetic players and games the Tier 1 benchmark needs

- Status: decided (budget-bound: the suites resized to the budget)
- Issue: #91
- Author: the maintainer, with Claude (lane `exp/0001-0002`)
- Kind: measurement (one version): a design study that sizes the suites
- Follows: none
- Registered: 2026-11-23. The registration is the first commit that adds this file
  (`git log --diff-filter=A -- preregistration.md`), so it is not written here.

## 1. Question

How many synthetic players per flaw, and how many games per player, does the Tier 1 benchmark need so that a change of 0.1 in recall at 2 between two algorithm versions is detected at least 80% of the time, and what does that cost in games? The smoke suite has five flawed players, so one detection moves recall by 20 points, and the CI tolerances in `bench/baseline.json` (0.2 for recall, 0.15 for coverage) were not derived from anything ([#91](https://github.com/Terkea/rematch/issues/91), `docs/benchmark.md`). A chess example: a new W1 detector that finds the hanging-piece player's flaw for one more flawed player in ten is the smallest change we want the weekly suite to see; the CI suite should at least catch a change that loses that many.

## 2. Hypotheses

This is a sizing study, so the hypotheses are about the design, not the algorithm.

- H0: no full suite within the registered budget of 9,000 games reaches 80% power for a paired change of +0.1 in recall at 2 under the registered decision rule.
- H1: some full suite within the budget reaches it, and the smallest such suite is the one we adopt.

H0 and H1 name the two branches of section 9; they are not tested. The result is a calculation conditional on the registered churn and gain model, not evidence about the algorithm.

Rival explanations that would give the same numbers, and how each is handled:

- The pilot's players say little about the population of synthetic players because there are six per flaw. The baseline hit probability of each flaw is drawn from its Jeffreys posterior in every simulation, so this uncertainty is carried into the power, not ignored.
- The power depends on how the candidate differs from the reference, which no pilot can show (a candidate does not exist). The churn, the share of reference hits a candidate loses while it gains elsewhere, is a registered assumption with a sensitivity range (0, 0.05, 0.10). Nothing measures it here (the pilot runs one version), and it moves the required size by a factor of two to three, so the decision is taken at the conservative end, 0.10.
- A ceiling: flaws whose target is always found (or never, like W4.missed and W7.traps today, `docs/benchmark.md`) cannot move. The simulation applies the gains to misses only, so a suite whose misses are concentrated in a few flaws is modelled as it is.
- Search noise and the harsh clock model (`docs/benchmark.md`) are in the pilot's games, so their effect on hit rates is in the data. Their effect on a real player is not, and nothing here claims it.

## 3. Algorithm versions compared

| Role | algorithm_id | What differs |
|---|---|---|
| Measured | `algo-1.1.0+fa5e2186e1dd` | – |

From `cli algo show` on the cli built from this branch. The bench crate gains `cli bench run --suite-file` so a suite of the registered players can run; `backend/crates/bench` and `backend/crates/cli` are not in the manifest, and the id is unchanged.

## 4. Primary metric and ground truth

- Primary metric (exactly one): the simulated power of the section 9 ship rule for a paired change of +0.1 in recall at 2, as a function of players per flaw and games per player. Recall at 2 is the share of flawed players whose target is in the CoachingInput's top 2 at the checkpoint (`docs/benchmark.md`, score a).
- Ground truth: the injected flaw of each synthetic player (its target key) and the generator's tallies (`bench::flaws::Tally::truth`, with the clean controls' base rate), from one pilot run of the real pipeline.
- Secondary metrics:
  - confirmatory: the 95% half-width of interval coverage (score b) for one version, per suite size, with the player as the unit; and the power for a paired change of +0.05 in coverage;
  - confirmatory: at the smoke, full-80 and full-120 budget sizes, the minimum detectable change (MDE) of the ship rule, of ADR 0104's guard (an 80% interval of the drop wholly below 0) and of the rule CI runs (`bench::results::regressions`: recall below the baseline by more than the tolerance), and the false-alarm rate of both guards when the true change is 0;
  - exploratory: the pilot's observed recall at 2 per checkpoint and flaw (Wilson 95%), coverage with a cluster bootstrap, the within-player ICC of coverage, and clean controls with a rank-1 weakness.

Only the primary decides the full suite's size. Every metric named here is reported whatever it shows.

## 5. The minimum effect that matters

- Minimum effect: +0.1 absolute in recall at 2 (primary); 0.05 absolute in coverage (secondary).
- Why this size: 0.1 of recall at 2 is one flawed player in ten told the right focus instead of the wrong one, the size of change a detector or ranking fix is meant to make (ADR 0101 moved recall from 0.4 to 0.6). Five points of coverage is the half-width of the registered target band [0.75, 0.85] (EXP-0002), so a change smaller than that cannot move a suite out of the band.

## 6. Power analysis

- Method: simulation of the decision rule in section 9 (the method of `.claude/skills/science/statistical-power/references/simulation_based_power.md`, written out in `analysis.py` because the rule is a paired Newcombe interval on clustered data, not one of `power.py`'s tests). The experiment is itself the power analysis, so this section sizes the pilot, not the suites.
- Inputs: the pilot's hit probabilities and coverage vectors (section 7), the churn (0, 0.05, 0.10), two-sided intervals throughout.
- Pilot size: 6 players per flaw and 2 clean controls at 120 games, 3,840 games, about 35 minutes at the documented 0.544 seconds a game with 4 engines (`docs/benchmark.md`: 480 games in 4 minutes 21 seconds). It is set by compute, and the CI reruns it twice. With six players a flaw's hit probability has a wide posterior (6 of 6 gives a 95% Jeffreys interval of about [0.66, 1]); that width is drawn in every simulation instead of plugging in a point estimate.
- Monte Carlo error: 4,000 simulations per cell of the recall grid (a Monte Carlo standard error of at most 0.008 on a power) and 2,000 per cell of the coverage grid; each power is reported with its 95% Monte Carlo interval.
- Sensitivity: the full grid of players per flaw (1 to 100), games (30, 45, 60, 80, 120) and churn (0, 0.05, 0.10) is reported, not one number.

## 7. Design

- Unit of analysis: the player. A player's games and checkpoints are nested in it, and a player's intervals are resampled together. One seed is one player; the pipeline is deterministic, so rerunning a seed adds nothing.
- Pairing and blocking: the simulated comparison is paired (the same players under both versions, as CI compares the same seeds). Blocked by flaw: equal players per flaw. One rating band (the synthetic players are all rated 1500, `bench::suite::RATING`), so there is no band block.
- The pilot suite (`exp-0001-pilot`): for each flaw at the smoke suite's rate (`hang` 0.05, `miss_mate` 0.5, `opening_blunder` 0.3, `low_clock_blunder` 0.25, `no_convert` 0.5) six players, and two clean controls; 120 games each, checkpoints at 30, 45, 60, 80 and 120 games, 16 opponents, 12,000 nodes, no perturbation, 8 engines (the engine count changes the speed, never the facts).
- Seeds: `player/<flaw>/<j>` for each flaw and j in 0 to 5, `player/clean/0` and `player/clean/1` (the players' u32 seeds), then `power/recall`, `power/coverage`, `power/smoke` and `bootstrap/observed`, each the root of a numpy `SeedSequence` spawned in the grid order written in `analysis.py`. `analysis.py` refuses to run if a player seed repeats or equals a seed of the smoke (11 to 16) or full (101 to 115) suites.
- Development and confirmatory data: no candidate is developed; the pilot's seeds are new and used once.
- Held fixed: **Stockfish 19** at 12,000 nodes, the clean policy and injectors of `bench::policy` and `bench::flaws`, the clock model, the opponents' rating spread, the memo cache.
- Data: no file inputs; the pilot is generated by the cli (`inputs.sha256` lists none). No real player's games.
- Leakage controls: not a predictive question.

## 8. Analysis plan

- Model or test: for each (games G, churn b, players per flaw m), 4,000 simulated suites. In each, every flaw's hit probability π_f is drawn from Beta(h_f + 1/2, n_f − h_f + 1/2) given the pilot's h_f hits of n_f players at G; each player's reference outcome is Bernoulli(π_f); the candidate keeps a reference hit with probability 1 − b and turns a miss into a hit with probability a, where a = (0.1 + b π̄)/(1 − π̄) makes the expected change exactly +0.1 (π̄ the mean of the five π_f). A simulation with a > 1 cannot hold the change and counts as not shipped. The decision is the section 9 ship rule on the Newcombe (1998, method 10) interval of the paired difference over all 5m flawed players.
- Guard and tolerance: ADR 0104's guard flags a regression when the 80% Newcombe interval of the paired difference lies wholly below 0. CI does not compute that interval; `bench::results::regressions` fails when recall falls by more than a fixed tolerance. The tolerance is derived from the guard: k is the smallest number of lost detections, with none gained, whose 80% interval lies wholly below 0, for a reference at the pilot's recall at G (H = round(recall × n) hits of n, so the table is n11 = H − k, n10 = 0, n01 = k, n00 = n − H), and the tolerance is (k − 0.5)/n, so a net loss of k detections fails and k − 1 does not. At each budget size the analysis reports, for both the guard and the CI rule with that tolerance, the power for drops of 0.05 to 0.5 and the false-alarm rate when the true change is 0 (the candidate gains at the rate that balances the churn). CI checks coverage by how much further from 0.8 it moves (`|now − 0.8| > |was − 0.8| + tolerance`), so the coverage tolerance is the 80th percentile of `|c' − 0.8| − |c − 0.8|` over 2,000 smoke-sized suites when the true change is 0, with the candidate's intervals flipped player by player (all of a player's misses or hits together, the clustered and wider model): churn alone exceeds it 20% of the time.
- Coverage: one version's coverage is the mean over players of each player's share of covered checkpoint intervals (it equals the benchmark's pooled share when every checkpoint has an estimate row, which the results show). Its 95% half-width per suite size is 1.96 times the standard deviation over 2,000 suites resampled from the pilot's players within each flaw. The paired power for +0.05 uses the same resampling, the candidate's intervals flipped interval by interval (miss to hit at the rate that gives +0.05, hit to miss at the churn, 0.10) and, as the clustered extreme, player by player (all of a player's misses or hits flip together), with a 97.5% t interval of the per-player differences.
- Effect size: the power with its 95% Monte Carlo interval. Because each simulation draws the flaws' hit probabilities from their posterior, the reported power is an assurance (the power averaged over what the pilot allows), and a draw whose change cannot be held (a > 1) counts as not shipped; the pilot's recall at 2 with Wilson 95% intervals; coverage with a 95% cluster-bootstrap interval (2,000 resamples of players within flaw).
- Multiple comparisons: none is needed for the decision, which reads one quantity per suite size (the primary power at churn 0.10). The grid and the secondaries are reported as estimates, not tested.
- Diagnostics: no Bayesian fit. The run stops if a player seed repeats.
- Exclusions and missing data: none. A target with no estimate row at a checkpoint has no coverage at that checkpoint and is left out of coverage only (it is a miss for recall, as in `bench::results::score`).

## 9. Decision rule

The budgets, in games because the seconds a game depend on the machine: the smoke suite runs on every pull request that touches the algorithm and has 15 minutes, 1,650 games at 0.544 seconds a game with 4 engines; the full suite runs weekly inside `bench.yml`'s 180-minute job and has 9,000 games (about 82 minutes on the documented machine, leaving room for a slower runner and the build). The smoke suite keeps 80 games and one clean control; the full suite is 5m flawed players at the smoke rates and two clean controls, at G games with G in {80, 120}. Today's rate sweep leaves the full suite: the scorer's recall pools every flawed player, and the power analysis models the smoke rates only, so the suite that is scored is the suite that was sized. The detection curves come from the checkpoints instead.

- Resize: if some G in {80, 120} has a smallest grid m from which the ship power stays at least 0.80 for every larger grid m (churn 0.10), and it fits the budget, the full suite becomes the cheapest such (m, G) in games, the larger G on a tie.
- Budget-bound: otherwise the full suite becomes the budget's largest m at the G whose power is higher (the larger G when the two are within 0.01), and its MDE is registered as what the weekly suite can detect; the 0.1 target needs a larger budget, a split job or a cheaper suite, written as the follow-up.
- Smoke: the largest m (any integer) that fits 1,650 games, with the recall tolerance (k − 0.5)/n and the coverage tolerance of section 8. Its MDE is reported; it is a tripwire, not a test of 0.1.
- The absolute thresholds (recall at 2 at least 0.8, coverage in [0.65, 0.95]) are not changed by this experiment.

"Need more data" does not apply: the grid answers the question for any outcome. The decision changes `backend/crates/bench/src/suite.rs` and `bench/baseline.json` (the new smoke suite is run once to give the baseline's metrics), and no algorithm component.

## 10. Analysis code

- Entry point: `lab/experiments/EXP-0001/analysis.py` (`python analysis.py --out DIR` writes `DIR/results.json`, with the `algorithm_id` it ran on, and `DIR/figures/`). The algorithm is an input: the cli of `$REMATCH_CLI` is the version that runs, `--algorithm ID` stops the run when it is another one, and `--seed-namespace NS` draws a fresh replication on new seeds (the default, the experiment id, is the registered run).
- Analysis code sha256: `b9ac1812a845c2f334794de51b5bf9b97c9ff71837adcf497e339e6c07e11add`

## 11. Deviations

None during the run. Two findings from the maths audit (2026-11-28, ADR 0118), recorded after the decision; sections 0 to 10, `analysis.py` and `results.json` are unchanged:

1. **The smoke coverage tolerance is one of two lattice points.** Under section 8's own model, the 0.8 quantile of churn-only coverage growth is 4/60 (0.067), not the committed 0.05. A 2,000-suite run returns 0.05 with probability 0.41. The property section 8 states in words still holds for 0.05: churn alone exceeds it 0.202 of the time at the pilot's coverage. `math.md`, Finding 1.
2. **At the installed baseline the coverage rule is two-sided.** The tolerance was calibrated at the pilot's coverage (0.575). `bench/baseline.json` holds coverage 0.8, so the CI rule (`backend/crates/bench/src/results.rs`, `|is − 0.8| > |was − 0.8| + t`) becomes `|is − 0.8| > 0.05`. Under churn alone, that fails 0.19 of the time with interval flips and 0.40 to 0.71 with the player flips the tolerance was registered with. That is up to 3.5 times the intended 0.20. Recalibrating the rule at the baseline is a rule change, so it is not made here. It is filed as EXP-0033. `math.md`, Finding 2.
