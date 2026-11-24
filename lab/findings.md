# Findings

The running log of every decided experiment on the algorithm, newest first, negative and inconclusive ones included. Each line links its folder in `lab/experiments/`; the method is in [docs/evaluation.md](../docs/evaluation.md#how-we-do-science-on-the-algorithm).

| Date | Experiment | Algorithm version | Result | Decision | Link |
|---|---|---|---|---|---|
| 2026-11-23 | EXP-0002: do the 80% ranges hold the true rate 80% of the time? | `algo-1.1.0+fa5e2186e1dd` | For players with a weakness, 12 of 20 weakness × sample-size coverages lie outside [0.75, 0.85], all but one too low: W4.missed 0.10 [0.09, 0.11] at 30 games, W7.traps 0.20 [0.19, 0.22], W1 0.64 [0.61, 0.66] (Bonferroni intervals); the misses are on the high side, the prior pulling the estimate towards the peers. The #13 case: 0.53 [0.46, 0.60] at 30 games with exact counts. | Outside the band; the fix is EXP-0006 (#13). | [EXP-0002](experiments/EXP-0002/decision.md) |
| 2026-11-23 | EXP-0001: how many synthetic players and games the benchmark needs | `algo-1.1.0+fa5e2186e1dd` | Detecting +0.1 in recall at 2 80% of the time needs 40 players a flaw at 80 games (power 0.83 [0.81, 0.84], 95% Monte Carlo) under the assumed churn of 0.10; the 9,000-game weekly budget allows 22 (power 0.52 [0.51, 0.54], 0.15 detected 80% of the time). Pilot recall at 2 at 80 games 0.63 [0.46, 0.78] (Wilson), coverage 0.575 [0.467, 0.683] (cluster bootstrap). | Budget-bound: full suite 22 a flaw, smoke 3 a flaw, CI tolerances 0.1 (recall) and 0.05 (coverage); churn to be measured (EXP-0005). | [EXP-0001](experiments/EXP-0001/decision.md) |

EXP-0003 (partial pooling, #11) and EXP-0004 (the Tier 2 design) are on the v0.3 roadmap, with EXP-0005 (the churn between two versions) and EXP-0006 (a fix for the 80% ranges' prior and weighting). A follow-up line names the experiment it follows.
