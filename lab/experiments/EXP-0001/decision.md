# EXP-0001: decision

- Decision: budget-bound (section 9). No full suite within 9,000 games detects a change of +0.1 in recall at 2 80% of the time under the registered churn of 0.10. The full suite is resized to the budget, and the change it can detect is registered: 0.15.
- Algorithm: `algo-1.1.0+fa5e2186e1dd`. Results: `results.json` (commit 7409f457), figures in `figures/`.
- Compute: the pilot (30 flawed players and 2 clean controls, 120 games each, 3,840 games) took 1,489 seconds with 8 **Stockfish 19** engines at 12,000 nodes: 0.39 seconds a game, 575,567 engine searches and 468,209 served from the cache, about six cores busy (`docker stats` showed 590% for the lab container). The power simulation took under two minutes of that container's time. The wall time is not in `results.json`, which must be byte-identical from run to run.

## The primary: players needed for +0.1 in recall at 2

The smallest players per flaw from which the simulated ship rule (a two-sided 97.5% Newcombe interval of the paired difference wholly above 0) succeeds at least 80% of the time for every larger size:

| Churn | 30 games | 45 | 60 | 80 | 120 |
|---|---|---|---|---|---|
| 0 | 14 | 14 | 14 | 14 | 16 |
| 0.05 | 24 | 28 | 28 | 28 | 32 |
| 0.10 (registered) | 28 | 40 | 40 | 40 | 48 |

At 80 games and churn 0.10, 40 players a flaw (200 flawed players, 16,160 games with two clean controls) give a power of 0.83 [0.81, 0.84] (95% Monte Carlo interval); 32 a flaw give 0.71 [0.69, 0.72]. More games do not help: 120 games need more players, not fewer. Recall at 2 is higher at 120 games (0.73 against 0.63), and under the registered churn model a higher baseline means fewer misses to gain from (the same ordering holds at churn 0), and 0.3% to 0.75% of the 120-game draws cannot hold the change at all; the misses that remain sit in flaws the ranking does not reach (W4.missed is ranked for none of the six players at any checkpoint). The churn is assumed, not measured, and moves the answer by a factor of three between 0 and 0.10.

At the budget:

| Suite | Players a flaw | Games | Total games | Power for +0.1 [95% MC] | Smallest change shipped 80% of the time |
|---|---|---|---|---|---|
| full, 80 games | 22 | 80 | 8,960 | 0.52 [0.51, 0.54] | 0.15 |
| full, 120 games | 14 | 120 | 8,640 | 0.28 [0.27, 0.30] | none in the grid |
| smoke | 3 | 80 | 1,280 | 0.07 [0.06, 0.08] | none in the grid |

The registered rule picks the full suite at 80 games, 22 players a flaw. Its power is an assurance: the flaws' hit probabilities are drawn from their posteriors given the pilot's six players each. The ship curve is not monotone in the change (0.96 at +0.2, 0.003 at +0.5 for the full suite), because a draw whose change cannot be held counts as not shipped; the smallest change shipped 80% of the time lies between 0.10 and 0.15 on the grid's steps.

What the +0.1 means here: at 80 games `hang` and `low_clock_blunder` are found for 6 of 6 players and `no_convert` for 5 of 6, so 10 of the pilot's 11 misses sit in W4.missed (never ranked) and W7.traps. The simulated change is a pooled one, and nearly all of it has to come from those two flaws. A W1 change of the kind section 1 describes cannot show up in recall at 2 at this ceiling; it would show in W1's coverage or its games to detection.

## The CI rule and the tolerances

- Recall at 2, smoke suite: the tolerance is (k − 0.5)/n with k = 2 lost detections of n = 15 (the smallest net loss whose 80% Newcombe interval lies wholly below 0, from a reference at the pilot's recall), so 0.1. With churn 0.10 and no true change, the CI rule fails 12% of the time (0.118); it catches a true drop of 0.2 81% of the time and a drop of 0.1 49% of the time.
- Coverage, smoke suite: 0.05, the 80th percentile of the growth of `|coverage − 0.8|` that churn alone produces at the smoke size.
- The full suite's tolerance by the same rule would be 0.014, with a false-alarm rate of 0.34: it is the weekly report, not a gate, and `bench.yml` does not compare it with a baseline.
- ADR 0104's guard (the 80% interval of the drop wholly below 0), for comparison: false alarms 0.098 (smoke), 0.093 (full, 80 games) and 0.090 (full, 120); smallest drop caught 80% of the time 0.3, 0.1 and 0.1. The CI rule at the full sizes: smallest drop caught 0.05 (80 games) and 0.1 (120), false alarms 0.34 and 0.30.
- `top1_precision`, `plan_hit_rate` and `top2_agreement` were not in the experiment; their tolerances stay at 0.2, 0.2 and 0.25, not derived.

## Secondaries and what the pilot showed

- Coverage of the target's 80% intervals over the checkpoints up to 80 games: 0.575 [0.467, 0.683] (95% cluster bootstrap over players), from 120 intervals; up to 120 games 0.58 [0.48, 0.67]. Within-player ICC 0.30. W1 held the true rate in 0.42 of its intervals, W4.missed 0.79, W7.traps 0.38, W9b 0.63, W5 0.67. This is the end-to-end number with the real detectors; EXP-0002 measures the posterior alone.
- Coverage precision: at 20 players a flaw (the nearest grid size to the full suite's 22) the 95% half-width is 0.057 (0.091 at 8 a flaw, 0.153 at the smoke size); the paired power for +0.05 in coverage at 20 a flaw is 0.52 with interval-by-interval flips and 0.23 with player-by-player flips.
- Recall at 2 in the pilot, 80 games: 19 of 30, 0.63 [0.46, 0.78] (Wilson 95%): `hang` 6 of 6, `low_clock_blunder` 6 of 6, `no_convert` 5 of 6, `opening_blunder` 2 of 6, `miss_mate` 0 of 6. At 120 games 22 of 30, 0.73 [0.56, 0.86]. The opening players' rank 1 is W1 at 29 of their 30 checkpoints (one had nothing ranked at 30 games), the precedence effect `docs/benchmark.md` describes, so W7.traps reaches the top 2 only as second.
- Clean controls: one of the two had a rank-1 weakness at 45 games (W9a) and the other at 80 games (W5); neither at 30, 60 or 120. The smoke suite's clean control has none, so two of 10 clean checkpoints here (against none of four in the smoke suite) is the first sign that the ranking floor lets a clean player's noise through. It is exploratory and decides nothing.
- Every player's CoachingInput was the same when built twice (`deterministic`).
- Against the absolute thresholds this experiment leaves unchanged: the pilot's recall at 2 (0.63 at 80 games) is below 0.8, and its coverage (0.575) is below 0.65.

## Deviations

1. The pilot ran with the cli of commit 4536649d (`--suite-file`); the suite resize that follows from this decision changes the named suites only, not the cli the pilot used, and the algorithm id is the same.
2. Implementing the decision, the full suite uses 16 opponents (the pilot's) instead of 20, and its checkpoints stop at 80 games. Neither was registered; both follow from sizing the suite on the pilot as it ran.
3. `bench.yml`'s timeout rises from 180 to 300 minutes and `lab.yml`'s from 90 to 300 (with a release cli), for a hosted runner slower than the documented machine. The 9,000-game budget of section 9 is kept: the 180-minute job was its basis, and the decision is the one that budget gives. At the documented 0.544 s a game, the 40-a-flaw suite (16,160 games) would take about 147 minutes on the documented machine, so a 300-minute job could hold it; raising the budget is the owner's call (Next).
4. None in sections 1 to 10 after the results.

## What changes

- `backend/crates/bench/src/suite.rs`: the smoke suite is three players per flaw and one clean control at 80 games (1,280 games; the first player of each flaw keeps its old name and seed), and the full suite 22 players per flaw and two clean controls at 80 games (8,960 games); the rate sweep is gone from the full suite.
- `bench/baseline.json`: the new smoke suite's metrics, a recall tolerance of 0.1 and a coverage tolerance of 0.05.
- No algorithm component changes, so no version bump.

## Next

1. **EXP-0005: measure the churn.** The decision rests on an assumed churn. When the first behaviour change has a candidate branch (EXP-0006 is the first), run both versions on this pilot's registered players and count, per flaw, the reference hits the candidate loses and the misses it gains. One candidate's churn is an estimate for one kind of change, with its own interval; EXP-0005 registers its own analysis (the churn is a constant in this frozen one) and re-sizes the suites at the measured value in either direction. Drafted as a v0.3 issue.
2. The 0.1 target needs about 16,000 games a week at 80 games a player, about 147 minutes on the documented machine. That is a budget choice, not an experiment: raise the weekly budget, split the full suite across parallel jobs in `bench.yml`, or accept 0.15. Left to the owner.
