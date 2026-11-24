# EXP-0006: decision

- Decision: ship (section 9). The challenger's worst coverage of the 20 is 0.828 (W5 at 30 games), D(challenger) = 0.028 [0.022, 0.043] (97.5% paired bootstrap), wholly at or below the tolerance of 0.07; the champion's is 0.103 (W4.missed at 30 games), D = 0.697, so ΔD = 0.668 [0.650, 0.679]; every guard holds. `PriorRule::CHALLENGER` becomes production's rule as `algo-2.0.0`.
- #13: improved, not fixed by the registered secondary. W1's four coverages are within the band and the smoke suite's acceptance holds (coverage 0.65 to 0.80, `bench-hang` still ranks W1 first), but the #13 cell at 30 games is 0.766 [0.738, 0.791] (Wilson, Bonferroni for 4): below 0.80, and not shown inside [0.75, 0.85]. #13 stays open for that cell.
- Algorithm: both rules ran on `algo-1.1.1+42d749f7c50b` (`ALGO_VARIANT` unset and `challenger`). Results: `results.json`, figures in `figures/`.
- Compute: 118,400 of 118,400 profile computations and 6,720 of 6,720 games (100% of the budget, cap 150%). One run took 3,254 seconds in the lab container at about 6.5 cores (about 6 CPU-hours): the side-by-side Tier 1 run with 8 **Stockfish 19** engines at 12,000 nodes, then the profiles on 6 processes. The run was made twice, byte-identical. The power pilot (`dev/power.json`, 118,400 profile computations) and the development grid (180,000) ran before registration and are not in the budget.

## The primary

| Key | Rule | 30 games | 60 | 120 | 250 |
|---|---|---|---|---|---|
| W1 | champion | 0.645 | 0.721 | 0.764 | 0.814 |
| | challenger | 0.786 | 0.804 | 0.793 | 0.794 |
| W4.missed | champion | 0.103 | 0.161 | 0.209 | 0.273 |
| | challenger | 0.812 | 0.808 | 0.824 | 0.817 |
| W7.traps | champion | 0.191 | 0.276 | 0.357 | 0.419 |
| | challenger | 0.791 | 0.779 | 0.786 | 0.808 |
| W9b | champion | 0.697 | 0.755 | 0.784 | 0.842 |
| | challenger | 0.783 | 0.795 | 0.798 | 0.810 |
| W5 | champion | 0.832 | 0.837 | 0.869 | 0.899 |
| | challenger | 0.828 | 0.811 | 0.815 | 0.805 |

Coverage of the 80% range for players two to eight times their peers' rate in odds, two games a day, 3,600 players a coverage (each cell's intervals are in `results.json`, `per_rule`). These are averages over ρ in {2, 4, 8}, the three bands and the two dispersions, as registered; coverage by ρ alone is not in the results. Every one of the challenger's 20 Bonferroni intervals lies inside [0.75, 0.85]; the champion's has 2 inside and 13 outside, close to EXP-0002 on its seeds (2 inside, 12 outside; W4.missed at 30 games 0.10 then, 0.103 now). The challenger's ranges still lean towards the peers: the truth lies above them in 0.13 to 0.17 of W1, W4.missed, W7.traps and W9b intervals at 30 games (the champion's: 0.33, 0.90, 0.81 and 0.27), more than the 0.10 an equal-tailed range misses on each side. That lean is the input to EXP-0025.

## The guards

| Guard | Champion | Challenger | Worse by [80% interval] | Margin | Holds |
|---|---|---|---|---|---|
| Coverage at the peers' rate (its distance from 0.80: 0.074 and 0.004) | 0.874 | 0.804 | −0.070 [−0.071, −0.069] | 0.05 | yes |
| Credible estimates among players at or below the peers' rate | 0.0249 | 0.0338 | +0.0089 [+0.0086, +0.0093] | 0.01 | yes |
| Recall at 2, 45 flawed players at 80 games | 28 of 45 | 28 of 45 | 0 [−0.009, +0.009] (Newcombe) | 0.10 | yes |
| Smoke suite's CI rule | – | – | no regression | `bench/baseline.json` | yes |

- The false-weakness guard holds with 0.0007 to spare (its 80% upper end 0.0093 against 0.01), the same difference as on the development seeds and the power pilot (0.0089, 0.0093). Pooled, the credible share for players at or below their peers' rate rises from 0.0249 to 0.0338, one more credible estimate in about 110; per key it rises most for W4.missed (0.006 to 0.029) and W7.traps (0.001 to 0.017), whose strong prior had held them down: a player at their peers' rate gets a credible W7.traps estimate about one time in 59 instead of one in 740. A credible estimate still needs ADR 0101's excess floor before it ranks. The margin holds on the generator's peer rates and chances; on real players it is not measured (EXP-0013). On the Tier 1 players the evidence is consistent with a rise and does not measure it: one more of the 23 clean controls has a rank-1 weakness at 80 games (6 against 5; difference 0.043 [−0.017, 0.109], reported, not gated), and the pilot's scorecard counts one new clean false alarm at its last checkpoint (`exp1-clean-0`, W9a at 120 games).
- The smoke suite's coverage rises from 0.65 to 0.80 with every other metric equal (recall at 2 0.6, top-1 precision 0.643, plan hits 0.6, top-2 agreement 0.927 on both sides, no clean false alarm). The champion's smoke scorecard equals `bench/baseline.json` field for field, and its pilot equals EXP-0001's stored rows field for field (rank 1, target rank, hit, estimate, coverage at every checkpoint): no drift between runs or builds.
- The integrity checks all passed: every player's variant was `challenger`, both sides had the same unweighted counts at every checkpoint, and the challenger moved 4,245 estimate intervals (a positive control).

## Reported and exploratory

- Per key at the peers' rate (ρ = 1), the champion's ranges were too wide where its prior was strong and right (W4.missed 0.942, W7.traps 0.931); the challenger's are 0.787 to 0.827. The credible share there rose most for W4.missed (0.006 to 0.029) and W7.traps (0.001 to 0.017), the keys whose prior had held them down, and fell slightly for W1 and W9b. Coverage at half the peers' rate is 0.800 (champion 0.783).
- The #13 cell (W1, peers at 0.0008, true rate 0.023, 1,600 players): challenger 0.766, 0.787, 0.802 and 0.786 at 30, 60, 120 and 250 games, against the champion's 0.523, 0.639, 0.716 and 0.783.
- The churn of this prior change (EXP-0005's measure, descriptive): of the reference's 28 top-2 hits at 80 games the challenger lost none and gained none, a churn of 0 [0, 0.085] (Jeffreys 95%). Recall at 2 is unchanged at 30, 80 and 120 games, and one lower at 45 and 60 (one hit lost of 22 and of 30, none gained). The 17 misses at 80 games sit in `miss_mate` (9 of 9), `opening_blunder` (7 of 9) and `no_convert` (1 of 9) under both rules; why they are not ranked is not measured here. Rank 1 moves more than the hits do: it changes for 5 of 32 pilot, 3 of 16 smoke and 4 of 20 clean players at 30 games, and for 1, 0 and 2 at 80 games. It is the churn of one prior change and a lower bound for detector or ranking changes; it replaces nothing in EXP-0001's sizing (EXP-0005's decision, EXP-0024).
- The pilot's end-to-end coverage (EXP-0001's players, the real detectors) rises from 0.58 to 0.78; the smoke suite's from 0.65 to 0.80.

## What this does not show

- The ranges are calibrated on EXP-0002's generator: its peer rates, chances per game and design effect of 1.5 are assumptions, and the dispersion factor is part of the fix. Real players' game-to-game spread is not measured here; Tier 2 (EXP-0004) is where real games come in.
- Every coverage here is on synthetic players drawn from EXP-0002's model; the 80% ranges of real players are not measured. The ρ = 1 coverage is now 0.80 pooled, but the price of calibration is a measured rise in false weaknesses (0.009 a credible estimate); whether players at their peers' rate see more "worth working on" in practice is EXP-0013's question.
- The timeline's own posteriors (`timeline/series.rs`) keep the old prior; the change states are EXP-0014's.

## Deviations

1. None in sections 0 to 10. The critic's changes before the run (`review.md`) are part of the registration, committed (15b44a58) and frozen by hash before any confirmatory number existed.
2. The production change (`algo-2.0.0`) was prepared while the second run went and committed as work in progress before `results.json`; it was finished only after the second run matched the first and after the result review.
3. `dev/power.json` was written by `power.py` before its clean-alarm guard was dropped, so it reports `ship_power_approx` 0.37 with that guard; section 6's 0.69 uses the same file's numbers without it.

## What changes

- `backend/crates/profile-core/src/stats/variant.rs`: `PriorRule::CHAMPION` is the challenger's rule; the old one is `PriorRule::ALGO_1` (`ALGO_VARIANT=algo-1`). `algo-2.0.0`, with its `algorithm/CHANGELOG.md` entry and ADR 0111.
- The replay goldens and the lab's golden profile response are regenerated; `bench/baseline.json` is rebuilt from a smoke run of `algo-2.0.0`.
- #13 is not closed: its remaining case is W1 at 30 games against peers far below the player.

## Next

1. **EXP-0025: the #13 cell at 30 games.** W1 against peers at 0.0008 with a true rate of 0.023 is covered 0.766 [0.738, 0.791] at 30 games, below 0.80 and not shown inside the band, and the ranges still lean towards the peers; the next candidate is a prior cap below two games for keys with many chances a game, measured on the #13 cell and EXP-0002's grid.
2. **EXP-0013: how often is a player with no weakness shown one?** The shipped rule raises credible estimates for players at their peers' rate by 0.009, with 0.0007 to spare on its margin; EXP-0013 measures what that means on clean players' reports, and takes the false-credible share as a live guard, since EXP-0006 is not live.
