# EXP-0002: decision

- Decision: outside the band (section 9). For players with a weakness (ρ in {2, 4, 8}), 12 of the 20 weakness × checkpoint coverages of the 80% range lie wholly outside [0.75, 0.85] at the Bonferroni level, 2 lie inside and 6 are undecided. The 80% ranges are not calibrated for players whose rate is two to eight times their peers' (the primary's population, chosen by ρ, not by what is shown). Nothing in the algorithm changes here; the fix is the subject of EXP-0006.
- Algorithm: `algo-1.1.0+fa5e2186e1dd`. Results: `results.json` (commit 7409f457), figures in `figures/`.
- Compute: 6,000 simulated players on two schedules and the #13 cell, 49,600 profile computations through `cli lab profile`, 1,592 seconds on one core of the lab container. No engine runs.

## The primary

Coverage of the production 80% interval, ρ in {2, 4, 8}, two games a day, pooled over band and dispersion (3,600 players a coverage), with the Bonferroni interval for 20 (every interval below is Bonferroni unless it says otherwise):

| Key (prior strength) | 30 games | 60 | 120 | 250 |
|---|---|---|---|---|
| W1 (300 chances) | 0.64 [0.61, 0.66] | 0.71 [0.69, 0.73] | 0.76 [0.74, 0.78] | 0.80 [0.78, 0.82] |
| W4.missed (40) | 0.10 [0.09, 0.11] | 0.15 [0.14, 0.17] | 0.21 [0.20, 0.23] | 0.26 [0.24, 0.28] |
| W7.traps (40) | 0.20 [0.19, 0.22] | 0.27 [0.26, 0.29] | 0.36 [0.34, 0.38] | 0.43 [0.41, 0.45] |
| W9b (40) | 0.71 [0.69, 0.73] | 0.75 [0.73, 0.78] | 0.78 [0.76, 0.81] | 0.84 [0.82, 0.86] |
| W5 (flat) | 0.83 [0.82, 0.85] | 0.85 [0.83, 0.86] | 0.86 [0.84, 0.87] | 0.90 [0.89, 0.92] |

Outside the band: W1 at 30 and 60 games, W4.missed and W7.traps at every checkpoint, W9b at 30 games (all too low), and W5 at 250 games (too high). Within: W1 at 250 games and W9b at 120. The pooled coverage, reported and not decided on, is 0.572 [0.568, 0.575].

The pattern is the one #13 suspected, and it is not W1's alone. The misses of the low coverages are on the high side: among credible W4.missed estimates the true rate lies above the interval 80% of the time. The posterior mean of a B2 W4.missed player at ρ = 4 (true rate 0.63) is 0.69 of the truth at 30 games and still 0.88 at 250. The likely mechanism, inferred and not tested here (no factor varies the prior strength): the prior's strength is counted in chances, and W4.missed and W7.traps have about one chance a game against a prior worth 40, so the peers' rate weighs as much as a player's first 40 games; W1 has 53 chances a game against 300, about six games' worth, and is still outside the band at 30 and 60 games.

The recency weighting moves coverage in opposite directions by key (the schedule contrast, exploratory cells at the primary ρ): without it, W1 at 250 games falls from 0.80 to 0.73, W9b at 120 from 0.78 to 0.73 and W5 at 250 from 0.90 to 0.81, while W4.missed at 250 rises from 0.26 to 0.39 and W7.traps from 0.43 to 0.54. So the weighting widens the intervals (it treats weighted counts as real ones), which is what puts W1 at 250 and W9b at 120 inside the band, and at the same time it discounts the data and not the prior, which deepens the pull on the low-chance keys. Both are faults to fix, not one.

## The confirmatory margins (Holm, 21; Bonferroni intervals)

- By key at the primary ρ: W1 0.73 [0.71, 0.74] outside, W4.missed 0.18 [0.17, 0.19] outside, W7.traps 0.32 [0.30, 0.33] outside, W9b 0.77 [0.76, 0.79] within, W5 0.86 [0.85, 0.87] undecided.
- By ρ, every key: 0.5 0.78 within; 1 0.87 [0.86, 0.88] outside (too wide: a player at the peers' rate is covered more than 80% of the time, as the prior is then right); 2 0.72 outside; 4 0.54 outside; 8 0.46 outside.
- By band (which only moves the peers' rate by about 20%): B1 0.56, B2 0.57, B3 0.58, all outside; the band barely matters.
- By checkpoint: 0.50 at 30 games, 0.55 at 60, 0.60 at 120, 0.65 at 250, all outside.
- By dispersion: none 0.59, de1.5 0.56, both outside; overdispersion costs about three points.
- By schedule: two games a day 0.572, every game in the last day 0.566, both outside. Pooled, removing the recency weighting changes little; per key it does (above), in both directions.

All 21 Holm-adjusted p values are below 0.001; with 1,200 to 3,600 players a margin they would be for any visible difference, so the classifications above are what count.

## Secondaries

- Among the estimates Rematch would show (`credible`), coverage is W1 0.73, W4.missed 0.20, W7.traps 0.25, W9b 0.77, W5 0.83 (Wilson 95% intervals over intervals, too narrow because a player gives up to four). Shown ranges of the two low-chance keys hold the truth one time in four or five; W9b's and W5's shown ranges are inside the band, W1's below it.
- The #13 case (W1, peers 0.0008, true rate 0.023; Wilson 95%): with no dispersion, coverage 0.53 [0.46, 0.60] at 30 games, 0.65 at 60, 0.65 at 120 and 0.73 [0.66, 0.79] at 250; with de1.5, 0.53, 0.615, 0.675 and 0.755. Every estimate was credible, and the posterior mean is 0.83 of the truth at 30 games. The benchmark's W1 under-coverage is reproduced without any detector error, so the prior accounts for at least part of it; the detector's own under-read (0.019 against 0.023, `docs/benchmark.md`) is not measured here.
- Calibration curve (central Beta interval, ρ in {2, 4, 8}): the 50% interval holds the truth 34% of the time, the 80% 57%, the 95% 74%.

## Deviations

1. Section 6 says the schedule margins pool 7,200 players; analysis.py pools them within cells of 200 players and the margins hold 1,200 (band, ρ), 1,800 (dispersion) or 3,600 players (key, checkpoint, schedule). The precision claims of section 6 were for the larger counts; every margin's interval is still narrower than 0.025 a side, so no classification depends on it.

Otherwise the run used the registered seeds, sizes and code (sha256 in section 10).

## What this does not show

- The peer rates and chances per game are assumptions (section 7); a real per-band reference could move every key. For W4.missed and W7.traps the under-coverage appears whenever the player's rate is well above the peers' (at ρ = 1 they are covered 0.93 of the time), so it depends on how far real weak players sit from their peers, which we have not measured. EXP-0001's end-to-end W4.missed coverage was 0.79, but its injected rate sits close to its tiny measured rate.
- The KPI snapshots' bootstrap intervals were not measured (lab-v1 has no step for them).
- Whether a player reads a range as "80%" at all is not something this experiment asks.

## Next

1. **EXP-0006: fix the 80% ranges' prior and weighting.** A comparison experiment. The candidate is developed on development seeds of EXP-0002's design (`--seed-namespace EXP-0006-dev`), disjoint from the confirmatory ones, among three changes the evidence points at: a lighter prior for every key (W1 is outside the band at 30 and 60 games, W4.missed and W7.traps everywhere), a prior that decays with the data's recency weights, and an interval that uses the weighted counts' effective sample size (`(Σw)² / Σw²`) instead of treating weighted counts as real ones. The chosen candidate is frozen on its branch and then registered. Primary: the worst of the 20 coverages above, on fresh confirmatory seeds. Guarded: the ρ = 1 coverage, and Tier 1 recall at 2 and clean false alarms on EXP-0001's full suite (a lighter prior can rank more noise). Drafted as a v0.3 issue linking #13.
2. A `lab-v2` step for the KPI snapshot intervals, so their bootstrap ranges can get the same measurement. Named in the EXP-0006 issue's pointers, not a separate experiment yet.
