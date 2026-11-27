# EXP-0030: decision

- Decision: ship `memory-halves` (section 9). Both candidates cleared every condition; the fixed order picks `memory-halves`. All six of its false-alarm cells are within the bound: the worst upper end at 1 − 0.025/2 is 0.136 (falling at 120 games) against 0.15. It beats the champion by 0.35 to 0.69 at each length, catches a 200-point fall at every length, and passes the rough-world, chess.com-like and transfer guards. It ships as `algo-6.0.0` (ADR 0117), with the verdict stated again and the copy of section 9.
- Algorithm: `algo-5.0.1+d9c1f47013b6`, the freeze commit `09b265b1` (tag `exp-0030-freeze`). Results: `results.json` (commit `d9474b6e`), the figure in `figures/false-alarms.png`; `analysis.py` sha256 `85f282962d9d1f2942833b0eb845673b603bfedc0309c0684a12a263cb51f146`, as section 10 states. Lab image `rematch-lab:exp30`, whose cli is the freeze's. Seed namespace `EXP-0030-fb2f150c7574`, drawn from drand quicknet round 34312269 (`review.md`).
- Compute: 4,079,000 simulated games, 65,400 trajectories through `cli lab trajectory` and 3,955,242 cohort lines, exactly the registered budget. 858 seconds a run on one core of the lab container, capped at 3 GB (about 0.24 CPU-hours). Run twice: the two `results.json` and the two figures were byte-identical. No engine, no database, no AI tokens.

## The co-primaries

The share of 2,000 settled constant-strength players (Glicko-2 as on Lichess) told each way, with the Wilson interval at 1 − 0.025/2:

| Rated games | `memory-halves` falling | rising | `memory-before` falling | rising | Champion falling | rising |
|---|---|---|---|---|---|---|
| 60 | 0.103 [0.087, 0.121] | 0.089 [0.074, 0.106] | 0.103 (the same windows) | 0.089 | 0.458 | 0.411 |
| 120 | 0.117 [0.100, 0.136] | 0.084 [0.070, 0.101] | 0.109 [0.092, 0.127] | 0.088 [0.073, 0.105] | 0.640 | 0.294 |
| 250 | 0.105 [0.089, 0.123] | 0.095 [0.079, 0.112] | 0.101 [0.085, 0.119] | 0.101 [0.085, 0.119] | 0.795 | 0.164 |

Every candidate cell reads "within the bound". The champion reproduces EXP-0015 on these fresh seeds (0.44, 0.62 and 0.78 there), which is the positive control: the lab path returns the rule's own `kind`.

The plain world is close to the candidates' own assumption, an Elo-type update at lila's floor deviation, so this table shows that the implementation is right and calibrated where the assumption holds: the 80% interval of the difference has a mean half-width of 26.5, 43.8 and 50.9 points against 1.28 SD of the difference across the players (26.3, 44.6 and 50.9), where the champion's is 3.7. It is not a measurement on real players. The tests of the assumption are the guards below.

At 120 games `memory-halves` says "falling" more often than "rising" (234 against 168 players), and the mean difference is −2.3 points. It is within the bound and reported, not explained; the follow-up looks at it.

## The guards

- Beats the champion (EXP-0015's arena rule, Newcombe at 1 − 0.05/3): false "falling" changes by −0.35 [−0.38, −0.33], −0.52 [−0.55, −0.50] and −0.69 [−0.71, −0.66] at 60, 120 and 250 games. With the champion at 0.46 to 0.79 this could not realistically fail; ADR 0109 asks for it.
- Detection of a 200-point fall (300 players a length, the lower end of the 80% Wilson interval at least 0.50), always with the same length's false-alarm rate:

| Rated games | False "falling" | `memory-halves`, 200-point fall | 100-point fall | `memory-before`, 200-point fall |
|---|---|---|---|---|
| 60 | 0.103 | 0.72 [0.68, 0.75] | 0.36 | 0.72 |
| 120 | 0.117 | 0.84 [0.81, 0.87] | 0.50 | 0.88 |
| 250 | 0.105 | 0.99 [0.98, 0.99] | 0.72 | 0.94 |

  A 100-point fall is caught one time in three at 60 games. If a fifth of players were falling 200 points, about 37% of "falling" headlines at 60 games would still be false (0.08 false against 0.14 true); for 100-point falls about 53%. That is the price of any verdict this early, and the copy says what the verdict means: the rating moved by more than its own wander, not that the player's chess got worse.
- Rough world (sessions, breaks, a slow form, a day's form and tilt; 1,000 players): `memory-halves` 0.125, 0.131 and 0.131 falling and 0.137, 0.141 and 0.144 rising, the largest upper end of an 80% interval 0.159, under 0.20: it passes. The development run failed this guard in one cell (0.166, falling at 120 games); on the confirmatory seeds every cell is at most 0.144. Under streaks and form the rule runs at about 1.3 to 1.4 times its nominal rate, and with the slow form at SD 0 or 20 at 0.09 to 0.12. This is a pass by the registered rule, not a claim that the rule is unaffected by streaks.
- chess.com-like world (Glicko-1 with assumed constants, the stored rating after the game; 1,000 players): 0.086 to 0.107 per cell. It guards the loader's path and a second rating system. chess.com publishes neither its constants nor its deviations, so it is not a measurement of chess.com.
- Transfer (the anonymised Lichess cohort, the `8` slice, one month per player, 16,445 settled series):
  - 36% of real settled players were flagged two-sided, 0.356 [0.349, 0.363] (95%), against the model's nominal 20%. 11% were falling (0.113) and 24% rising (0.243). The mean z² is 2.37, and 3.10 with one pooled RMS step, so the noise of a ten-step estimate does not explain it.
  - It passes the registered gross-failure line of 0.40. But real ratings wander, or really change, more than the model allows: within a month a real rating moves about one and a half times as far as the model expects.
  - This check cannot tell the two apart. It uses five-game windows a median of 74 games apart, not the product's 30 against 30. So the rule's false-alarm rate on real players is not known: somewhere between its nominal rate and about twice it. A pass shows only that the model is not grossly wrong.
  - Real steps are a little streaky: the lag-1 correlation of consecutive steps is 0.036 (0.029 for their signs), against −0.008 in the plain world, 0.020 in the rough world and −0.026 in the chess.com-like one. The rough world is milder than real series.

## Exploratory

- At 30 games (W = 15) the candidates say 0.094 falling and 0.090 rising; the champion 0.41 each way.
- Accuracy and blunders "worse" under `memory-halves` 0.087 to 0.107, as under the champion.
- In the rough world a 200-point fall over 120 games is caught 0.81 of the time.
- The mean `change` when `falling` is −37, −62 and −70 points at 60, 120 and 250 games.
- Elo K = 10 and K = 32 (300 players a cell, 95% intervals about ±0.04): `memory-halves` 0.07 to 0.14 per cell, with 0.14 at K = 10 rising at 120 games and K = 32 falling at 250 games. These are reads, not evidence that the rule adapts to any step size.
- New accounts (no burn-in): `memory-halves` 0.09 to 0.18 per cell, 0.177 falling and 0.167 rising at 250 games. Its "then" is the account's first 30 games, its provisional period, so it is worst exactly where production often looks.
- Where `memory-before` did better without the order letting it ship, recorded and not argued: the rough world's worst cell 0.141 against 0.144; detection at 120 games 0.88 against 0.84; new accounts about 0.03 against 0.17 at 250 games; K = 32 at 250 games 0.11 against 0.14.
- The burn-in settled everywhere: the signed mean error at the first observed game is +1.0 point for the settled players, −3.3 to +4.3 for the declines, +2.0 for the chess.com-like world and −13.7 for the rough world (its tilt pulls the rating below the long-run strength; the line was 25). The settled deviation is at lila's floor of 45; the rough world's averages 49.8 over a median of 218 days.

## Deviations

None from sections 0 to 10. One clarification made before the development run and the freeze: section 7 now writes `σ² = s_rms² / (a (2 − a))` with `a = min(q s_rms / 2, 1)`, where the registration wrote `s_rms / (q (1 − a / 2))`. The two are the same for any step below 347 points; the change only covers that limit. The figure has no error bars on the champion and a stray tick on its log axis; the frozen `analysis.py` is not changed for it.

## What this does not show

- Real players. Every gating population is simulated, and the one real check bounds only gross failure. The false-alarm rate on real series at the product's windows is the next experiment's.
- Long histories and new accounts. Every confirmatory series was settled and at most 250 games; production reads every rated game of a time class, often far more, starting in the provisional period, where `memory-halves` already looks worse (0.17 to 0.18 at 250 games).
- chess.com. The chess.com-like world's constants are assumed.
- Why a rating moved. The verdict says the rating moved by more than its own wander, not what changed in the player's chess.

## Next

1. **EXP-0031: the trajectory verdict on full histories and new accounts.** The shipped rule on histories of 500, 1,000 and 2,000 games that start with the provisional period, on Glicko-2 and the chess.com-like Glicko-1, including the 120-game asymmetry. It decides whether the verdict needs a settled-history gate; such a gate is a new rule and is registered there, not improvised.
2. **EXP-0032: the memory model on real Lichess series.** Real full series of the anonymised cohort at the product's 30-against-30 windows, with z² compared at short and long gaps, since real change grows with the gap and a wrong model inflates every gap alike. It answers the cohort's 36% two-sided against the nominal 20%, and brings a rough world calibrated to the real streakiness (lag-1 0.036) as a guard to re-run.
