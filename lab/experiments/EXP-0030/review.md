# EXP-0030: review

## Part one: the pre-registration, before the run

A fresh critic (an agent with the `scientific-critical-thinking` and `peer-review` skills, no part in the design) read `preregistration.md`, `analysis.py`, `power.py` and `power.json` as registered (commit `4f59cdef`), EXP-0015's registration, decision, review, analysis and arena rule, the production rule (`trajectory.rs`) and the protocol, on 2026-11-27, before any confirmatory run. It checked the memory model's derivation (the reversion, the stationary variance, the block sum and the cross term), the budget, the seed names, the sign of the arena comparison and the cohort's fields, and found them right. It recommended running after fixes. Each concern, and what was done:

1. **The confirmatory namespace was not fixed by frozen code, and could be ground.** The registration named it from the freeze commit's sha, which its author controls, while `analysis.py` defaulted to a namespace nobody registered. Now the namespace comes from a public beacon nobody here controls: the first drand quicknet round at least 10 minutes after the freeze commit's time, with a Bitcoin block hash as the fallback (section 7). It is written to `namespace.txt`, listed in `inputs.sha256`, and `analysis.py` runs only that namespace or `EXP-0015-dev`, so its sha does not change once the namespace is known. Section 10 is not used to name the freeze; the tag `exp-0030-freeze` and this file do.
2. **The formula was not said to be frozen before the freeze.** Section 7 now says it is frozen at registration: development is implementation and its checks, never a change of formula, constants or windows; a change is a deviation that needs a new review.
3. **The plain world is close to the model's own assumption, and the guards that could really fail were lenient.** The rough-world guard was read at 1 − 0.05/12, which made it easier to pass, not harder. The guards are now uncorrected: a world fails when any cell's share is above 0.15 or its 80% upper end above 0.20 (section 8), which passes 0.84 of the time at a true 0.13 in every cell and 0.015 at 0.15 (`power.json`). Section 2 says the plain world mostly tests the implementation and the sample size. An exploratory run of the rough world with a slow form of SD 0 and 20 is added (500 players each).
4. **The transfer guard's "upper bound" claim was false.** Real improvement lowers a one-sided falling share, and real change of either sign inflates it whatever the model. The guard now reads the two-sided share `|z| > 1.2816`, which independent real change can only raise, and fails only on gross failure, a lower 95% end above 0.40 (twice the nominal two-sided 0.20). Section 7 says a pass bounds the model and a fail cannot tell a wrong model from real change. Series that look provisional (any of the ten steps above 30 points) are left out of the guard and reported with it.
5. **The real series' autocorrelation was biased and not comparable.** Centring each five-step block on its own mean biased the lag-1 correlation by about −1/4, and the simulated worlds used 249-step blocks. Both now use the same blocks (the steps of the first and last five games), centred on the pooled mean, and the lag-1 of the steps' signs (the results) is reported too.
6. **One candidate could veto the other's follow-up.** "Need more data" now applies when any candidate's only failure is an undecided cell, and the follow-up carries only those candidates.
7. **"Hidden for good" did not fit the champion/challenger loop.** A fail now keeps the verdict hidden, states which condition each candidate failed, and leaves any later challenger to its own registration that answers that failure (sections 0 and 9).
8. **Nothing confirmatory covered chess.com.** The chess.com-like Glicko-1 world (stored rating after the game, `chesscom: true`, the loader deriving the pre-game rating) is now a guard with 1,000 players, read like the rough world. Its constants are still assumed; section 7 says it guards the loader's path and a second rating system, not chess.com itself.
9. **The follow-up had no split of α.** The level is now 1 − 0.025/2 per cell in this run and in the one follow-up, on fresh seeds and not pooled.
10. **Bonferroni across twelve over-corrected.** "All six within" is an intersection-union test, so only the two candidates and the two looks split the 5%. Section 6 is recomputed: 2,000 players clear with probability 0.99 at a true 0.11 in every cell, 0.60 at 0.12 and 0.015 at 0.13. Section 3 notes that the two candidates' 60-game cells are identical.
11. **The correlation argument in the power analysis was overstated.** It now says the simulation is approximate.
12. **The beats-the-champion guard cannot realistically fail.** Said so in section 8; it stays because ADR 0109 compares every challenger with the champion.
13. **No positive control that `kind` arrives unmasked.** The run now stops unless the champion tells more than half of the settled players "falling" at 250 games (EXP-0015 measured 0.78).
14. **The cohort's bookkeeping.** Skips are counted, a pseudonym whose first qualifying month is skipped is not used later in either case, and a sensitivity read with one pooled RMS step is reported, since ten steps estimate it with noise.
15. **The value case overstated the payoff.** It now says 200 points, which is what the detection guard measures, and states the share of false "falling" headlines a calibrated rule still gives at 60 games (about 37% at 0.10, 47% at the bound, if a fifth of players decline). The copy says what a verdict means.
16. **The feasibility script.** It is committed as `dev/feasibility.py`, and the cost estimate is now 10 to 20 CPU-minutes.

The critic found the conditions of EXP-0015's Next item met except the namespace mechanics (1) and the autocorrelation check (5), both fixed above.

## Development runs, between registration and freeze

Every run on the development namespace `EXP-0015-dev` and the cohort's `0` slice, through the cli built from this branch:

1. 2026-11-27, `analysis.py` as committed in `49000613`, `--seed-namespace EXP-0015-dev`, cohort slice `0`, with the cli built from `2b8cbd29` (it reported `algo-5.0.0+3cabedf4c935`, being built before the version bump). 857 seconds in the lab container, capped at 3 GB; budget spent exactly as registered. A first attempt the same night was stopped by the host running out of memory and wrote nothing. What it showed, on development data only:
   - the code runs end to end, the positive control holds (the champion told 0.78 of the settled players "falling" at 250 games), and every burn-in settled;
   - both candidates' six plain-world cells are 0.10 to 0.11, with the interval's mean half-width within 1% to 2% of 1.28 SD of the difference at each length;
   - the rough world gives 0.11 to 0.17 per cell (`memory-halves` 0.166 at 120 games falling, which fails its guard; `memory-before` 0.148 at most, which passes); with the slow form at SD 0 or 20 it is 0.10 to 0.14;
   - the chess.com-like world gives 0.09 to 0.10 per cell;
   - the cohort's development slice gives a two-sided share of 0.36 on 16,511 settled series (lower end 0.350, under the 0.40 line), falling 0.11 and rising 0.25, mean z² 2.4: real players wander, or change, more than the model allows within a month, and mostly upwards; the step lag-1 correlation is 0.037 (0.032 for the signs), against −0.004 in the plain world and 0.020 in the rough one.
   Nothing was changed after it: the formula, the constants, the windows and the guards are as registered.

## The freeze and the confirmatory namespace

- Freeze: commit `09b265b1398e4b42763edfa06801633662974267`, tagged `exp-0030-freeze`, committer time 1795739570 (2026-11-27 00:32:50 UTC); the cli built from it is `algo-5.0.1+d9c1f47013b6`.
- Beacon: drand quicknet (chain `52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971`, genesis 1692803367, a round every 3 seconds). The first round at least 10 minutes after the freeze is 34312269 (time 1795740171); its randomness is `fb2f150c7574e48ca31070e87962c48ecdc3d3f68440f63f52871a6ff0bbb064`.
- Namespace: `EXP-0030-fb2f150c7574`, in `namespace.txt`, listed in `inputs.sha256`. No run used it before the confirmatory run.
