# ADR-R2-6. Timeline change detector and states

Date: 2026-11-21
Status: Accepted. This file has the detector part, from R2 Task 13 (`profile-core::timeline::{bocpd, cusum, ewma}`). R2 Task 14 appended the state machine, the confirmation rule, the onset rows and the cause hints (`timeline::{states, series, causes}`). The number is a placeholder: the controller renumbers R2 ADRs before merging.

## Context

The weakness timeline has to say when a weakness started improving or relapsing. It has to say this without ever claiming a change from zero evidence. Three detectors are involved (TS M2 Task 17, improvement-engine §6.6):
- **BOCPD** (Adams–MacKay with a Beta-binomial predictive): the main change detector, with an onset estimate.
- **CUSUM**: an early warning on the current segment.
- **EWMA**: drift monitoring for resolved weaknesses.

## Decision (detectors, Task 13)

1. **BOCPD in log space**, exactly as the TS pseudocode: predictive, grow, change point via logsumexp, normalise.
   - Defaults: hazard 1/200, `max_run` 500, window 50 games, 10 prior games, onset level 0.8.
   - Run length r after game t means the segment began at game t − r + 1.
   - `onset_low` and `onset_high` come from the smallest highest-mass run-length set covering the level. Ties go to the longer run, i.e. "no change".
   - `Bocpd::step(k, n)` is the streaming form, with O(max_run) memory. `run_bocpd` is the batch form the brief specifies.

2. **Truncation merges; it does not drop.** Run lengths above `max_run` are merged into a final bucket that means "at least `max_run` games". That bucket keeps the parameters of the heavier of the two merged runs.
   - The textbook alternative drops the tail. That throws away the no-change mass of a long stable series and pushes the posterior onto short runs. The result is a change claimed from nothing.
   - The bucket never counts towards `p_change`, so the effective change window is `min(window, max_run)`.
   - When the bucket is the MAP run or lies inside the highest-mass set, `onset_low = 1`: the onset is then only an upper bound.
   - Measured on seeded stationary series (3 %, 40 opportunities per game, prior Beta(1.5, 48.5)). The reference is the untruncated posterior over the same resolvable window:

   | Series | max_run | Exact p_change | Merge | Drop tail | Worst merge error over all t |
   |---|---|---|---|---|---|
   | 300 games | 30 | 0.086 | 0.071 | **0.821** | 0.081 |
   | 1 200 games | 500 | 0.083 | 0.083 | 0.094 | 0.006 |
   | 2 000 games | 500 | 0.108 | 0.110 | 0.117 (and MAP run 1, a false "changed after the last game") | 0.020 |

   Merging stays within 0.1 of the exact value and errs low (no false claim). Drop-tail reads a stable series as 82 % "changed" once `max_run` is below the window. With `max_run = 500` it moves the MAP onset to the last game.
   - Before this fix, the capped bucket was counted inside the window when `max_run < window`. That reported `p_change ≈ 1` for a stable series (Task 13 review, fix round 1).

3. **`p_change` includes the prior hazard mass.** With zero evidence it is 1 − (1 − H)^L, about 0.22 for H = 1/200 and L = 50.
   - It must never be shown, or acted on, without the evidence gate: opportunities since the onset ≥ minEvidence (Task 14).
   - The window must stay well below ~320 games. For H = 1/200 the prior mass alone reaches 0.8 at L ≈ 321.

4. **CUSUM.** This is Page's one-sided CUSUM on the binomial log-likelihood ratio, with S floored at 0. It alarms at the first S > h (default h = 4) and stops there.
   - Task 14 runs it on the current segment with **p0 = the segment rate and p1 = 1.5 · p0**, after at least 10 games, and resets it after each confirmation.
   - The TS flat-series test does not name its parameters. The up parameters (0.02, 0.03) on a series at 0.03 would rightly alarm. So the port tests the flat series with p0 = 0.03 and p1 = 0.045, the Task 14 configuration.

5. **EWMA.** z₀ = level and z_t = λx_t + (1 − λ)z_{t−1}. It alarms at the first z > level + k·σ·√(λ/(2 − λ)), with defaults λ = 0.1 and k = 2, and stops there.

6. **Honesty rules.**
   - Invalid observations are skipped and counted:
     - BOCPD and CUSUM skip k > n. BOCPD treats such a game as a no-information step, so game indices stay aligned.
     - EWMA skips non-finite values.
   - Invalid options are a typed `timeline::DetectorError` in `Bocpd::new` and the `*_counted` functions. The brief-signature wrappers then return values that claim nothing.
   - No NaN or infinity reaches any output. Zero evidence never alarms in CUSUM or EWMA.

## Consequences

- Long histories cost O(max_run) per game, and their stability is preserved.
- A series longer than `max_run` reports "the segment started at or before game t − max_run + 1" rather than an exact onset.
- `p_change` is not an evidence measure on its own. Task 14's gate is mandatory.

## Decision (states, onsets and cause hints, Task 14)

`DETECTOR_VERSION = "tl-v1"` is stored on every history row. Any change below bumps it.

7. **Full deterministic recompute.** `evaluate_series` replays the whole series on every profile build, ordered by `(played_at, game_id)`. Its output depends only on the set of points, not their input order (a property test shuffles the input). No incremental state can drift. P_imp uses the fixed seed of ADR-R2-5.

8. **Detector configuration.**
   - BOCPD: hazard 1/200, `max_run` 500, streaming `Bocpd::step`. The prior is `Beta(50·mu, 50·(1 − mu))` around the band's pooled rate. Shapes are floored at 1e-3 when mu is 0 or 1.
   - The change window is 50 games (`SERIES_WINDOW_GAMES`, which a test caps at 100). At least 10 games of the current segment must precede a change.
   - The current segment is the run of games since the last confirmed change. A change is searched only inside it, over run lengths `1 ≤ r < limit` with `limit = min(50, games in segment − 10)`. `p_change` is `change_summary` with `min_prior_games = segment start + 10`, so the capped bucket and older segments never count.
   - **Onset = the MAP run restricted to `r < limit`.** Its bounds come from the smallest highest-mass subset of that range covering 0.8. Ties go to the longer run.

9. **Confirmation rule.** A change is confirmed only when all three hold:
   - `p_change ≥ 0.8`;
   - `|rate_after / rate_before − 1| ≥ 0.15`, using raw rates either side of the onset. A zero rate before counts as an infinite change only if the rate after is positive;
   - the opportunities from the onset game to the current game are at least `minEvidence`.

   A confirmation starts a new segment at the onset game. The `before` counts of the last confirmed change are kept for P_imp.

10. **Evidence gate (honesty).** BOCPD's `p_change` includes the prior hazard mass, which is about 0.22 at window 50 with zero evidence. Therefore:
    - `p_change`, `rate_before` and `rate_after` are `Some` only on the rows of a confirmed change, and `None` on every other row.
    - `confidence_above` is 0 while the segment holds fewer than `minEvidence` opportunities or no events. One bad game, or 19, does not make a weakness active; 20 games of 50 opportunities at the same rate do (test `few_games_claim_no_active_state`). This extends TS, where confidence alone could claim `active` after one game.
    - The P40 streak counts only games at which the segment meets `minEvidence`.
    - A NaN `minEvidence` can never be met, and a negative one is 0. A non-finite or negative `n0` is 0.
    - Tests: zero evidence (150 × (0,0) on its own, and after a real segment), a change below the gate (750 of 1 000 opportunities), and a NaN or infinite gate. All of them report no change and no `p_change`.

11. **Signals per game.** They are computed on the current segment, with the posterior `posterior(k, n, pooled, n0)` (ADR-R2-5):
    - `confidence_above = P(θ > band median)`, gated as above;
    - `at_or_below_median`: the posterior mean is ≤ the median (true when there are no opportunities);
    - `below_p40_streak`: consecutive games with posterior mean < P40;
    - `cusum_up`: CUSUM (h = 4) over the segment's games, with `p0` = the segment's posterior mean and `p1 = 1.5·p0`, after at least 10 segment games. It is an early warning only: it can raise `emerging`, never `active`. It resets with each confirmation because it runs on the segment;
    - `confirmed_up` / `confirmed_down`: a confirmation at this game;
    - `p_imp = P_imp(current segment, segment before the last confirmed change)`. It is computed only when it can matter (rule 3) and for the final outcome;
    - `drift`: EWMA (λ 0.1, k 2) over the per-game rates of the games after entering `resolved`. The level is the segment's posterior mean at entry. σ = √(level(1 − level)/n̄) is the per-game binomial SD at the segment's mean opportunities n̄. Games without opportunities are skipped.

12. **State rules** (`next_state`, TS M2 Task 18, first match wins):
    ```
    resolved                                          -> drift ? relapsed : resolved
    active|emerging|relapsed|improving, streak >= 10  -> resolved
    active|emerging|relapsed, confirmedDown or pImp >= 0.85 -> improving
    improving, !confirmedUp                           -> improving
    confidenceAbove >= 0.7                            -> active
    confirmedUp or cusumUp                            -> emerging
    active|emerging|relapsed, !atOrBelowMedian        -> prev
    otherwise                                         -> not_an_issue
    ```
    - A NaN, infinite or out-of-range probability fails every comparison.
    - A table covering every (previous state, rule) pair and a 20 000-case property test pin the reachable transitions. In particular:
      - `resolved` leads only to `relapsed`;
      - `relapsed` is reached only from `resolved`;
      - leaving `improving` needs a confirmed rise, so it never falls straight to `not_an_issue`.

13. **Rows.**
    - **Transition rows:** one at the first game (`from = None`) and one at every state change. They carry the segment's `k`, `n` and confidence.
    - **Onset rows:** one per confirmation, appended after that game's transition row, if there is one. An onset row dates the change behind the current state:
      - `from` is the state at the game before the onset;
      - `to` is the state after the confirming game;
      - `onset` holds the onset game and its `[low, high]` dates;
      - the evidence counts are those since the onset.

      So a state entered before confirmation gets its dated onset when the change is confirmed. For example, `active` is entered from confidence at game 64, and the onset (game 61) is recorded at confirmation (game 80).
    - With no reference, or an invalid one (non-finite or outside [0, 1]), the only state is `not_an_issue`.
    - Points with `events > opportunities` are skipped and counted in `SeriesOutcome::invalid_points`. They do not shift any other row.

14. **W9d and W12** (continuous metrics) are not run through BOCPD in R2, as in M2.

15. **Cause hints.** `cause_hints` compares the games before and after a change. The kinds, in output order:
    - `time_control_mix`: the dominant time control changes and the new one gains ≥ 30 pp;
    - `new_opening`: a line under 10 % of games before and ≥ 30 % after;
    - `session_length`: games per active UTC day rise by ≥ 50 %;
    - `rating_level`: the mean opponent rating moves by ≥ 75;
    - `protocol_version`: the protocol sets differ.

    Share thresholds are compared in exact integer arithmetic, and ties for the dominant value go to the smallest key.

    Each side needs at least `MIN_CAUSE_GAMES = 5` games (rated games for `rating_level`). This is an addition to TS, so that a hint is never claimed from one or two games.

    Descriptions state what changed, never that it caused anything ("Most games moved from 300+0 to 180+0"). A test rejects causal wording. Values are cut to 120 chars, descriptions to 600, and the list to the contract's six.

## Consequences (Task 14)

- `weakness_state_history.p_change` and `rate_before`/`rate_after` are `real` in the schema. `evaluate_series` is a pure recompute and never reads them back to compare them with f64 values, so Task 14 does not need to widen them. If Task 33 (persistence) ever compares stored rows with recomputed ones, it must first widen them to double precision in a coordinator migration.
- `active` needs `minEvidence` opportunities in the segment, so a new account shows `not_an_issue` until there is enough evidence. The UI must present that as "no claim yet", not "all clear".
- The per-game CUSUM and EWMA reruns over the segment cost O(segment) each, so a whole series costs O(n²). That is negligible for series of a few thousand games.
