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

7. **Full deterministic recompute.** `evaluate_series` replays the whole series on every profile build, ordered by `(played_at, game_id, events, opportunities)`; the counts only break ties between duplicate keys, so even duplicates cannot make the output depend on input order. Its output depends only on the set of points, not their input order (a property test shuffles the input). No incremental state can drift. P_imp uses the fixed seed of ADR-R2-5.

8. **Detector configuration.**
   - BOCPD: hazard 1/200, `max_run` 500, streaming `Bocpd::step`. The prior is `Beta(50·mu, 50·(1 − mu))` around the band's pooled rate. Shapes are floored at 1e-3 when mu is 0 or 1.
   - The change window is 50 games (`SERIES_WINDOW_GAMES`, which a test caps at 100). At least 10 games of the current segment must precede a change.
   - The current segment is the run of games since the last confirmed change. A change is searched only inside it, over run lengths `1 ≤ r < limit` with `limit = min(50, games in segment − 10)`. `p_change` is `change_summary` with `min_prior_games = segment start + 10`, so the capped bucket and older segments never count.
   - **Onset = the MAP run restricted to `r < limit`.** Its bounds come from the smallest highest-mass subset of that range covering 0.8. Ties go to the longer run.
     - The coverage is **80 % given a change in the window**: the subset covers 0.8 of the restricted mass, so its unconditional coverage is at least 0.8 × p_change ≥ 0.64.
     - `change_summary`'s `p_change` includes the r = 0 mass (about the hazard, 1/200), while `candidate` excludes it. r = 0 is a change starting after the current game, so it has no games after the onset to date or to meet the evidence gate. The confirmation level therefore sees a `p_change` at most about 0.005 above the restricted mass, which is accepted and documented rather than special-cased.
     - **Structural limit:** the onset lies within the last 49 games, so a change can be confirmed only when `minEvidence ≤ 49 × opportunities per game`. Keys with fewer opportunities per game (for example 1000 / 49 ≈ 21 per game at the default gate) can never confirm a change. They get no onset rows and no confirmed `improving`, only states from confidence and the streak. This is the price of the window cap of 100 or less (the prior hazard mass) and is accepted. The per-key minEvidence (ADR-R2-5) must keep it in mind.
   - **The onset window is never zero-width** (`MIN_ONSET_WINDOW_GAMES = 2`). When one run alone covers 0.8 (a sharp jump), the next most likely run is added, which only raises the coverage. With fewer than two candidate runs nothing is confirmed yet. Test `onset_window_is_never_zero_width`, and every onset row in the property test.

9. **Confirmation rule.** A change is confirmed only when all three hold:
   - `p_change ≥ 0.8`;
   - `|rate_after / rate_before − 1| ≥ 0.15`, using raw rates either side of the onset. A zero rate before counts as an infinite change only if the rate after is positive;
   - the opportunities from the onset game to the current game are at least `minEvidence`.

   A confirmation starts a new segment at the onset game. The `before` counts of the last confirmed change are kept for P_imp.

10. **Evidence gate (honesty).** BOCPD's `p_change` includes the prior hazard mass, which is about 0.22 at window 50 with zero evidence. Therefore:
    - `p_change`, `rate_before` and `rate_after` are `Some` only on the rows of a confirmed change, and `None` on every other row.
    - `confidence_above` is 0 while the segment holds fewer than `minEvidence` opportunities or no events. One bad game, or 19, does not make a weakness active; 20 games of 50 opportunities at the same rate do (test `few_games_claim_no_active_state`). This extends TS, where confidence alone could claim `active` after one game.
    - **Every non-neutral state needs `minEvidence`** (fix round 1, ruling C3, a deliberate deviation from TS). `active` needs it through the confidence gate, a confirmed change through the confirmation rule, `resolved` through the streak gate, `relapsed` through the drift window (§11), and a CUSUM `emerging` through its own gate (§11). Below the gate, CUSUM is an internal signal at most, never a state.
    - The P40 streak counts only games since the current state was entered, and only while those games hold `minEvidence` opportunities (§11).
    - A NaN `minEvidence` can never be met, and a negative one is 0. A non-finite or negative `n0` is 0.
    - Tests: zero evidence (150 × (0,0) on its own, and after a real segment), a change below the gate (750 of 1 000 opportunities), and a NaN or infinite gate. All of them report no change and no `p_change`.

11. **Signals per game.** They are computed on the current segment, with the posterior `posterior(k, n, pooled, n0)` (ADR-R2-5):
    - `confidence_above = P(θ > band median)`, gated as above;
    - `at_or_below_median`: the posterior mean is ≤ the median (true when there are no opportunities);
    - `below_p40_streak` (ruling C1): consecutive games at which the games **since the current state was entered** hold at least `minEvidence` opportunities and their posterior mean is < P40. It resets to 0 on every state transition and on any game that breaks the condition. So after a relapse, or a brief `emerging`, the old low games no longer count toward resolving. The explicit reset on a transition is belt and braces: the state window restarts at the transition, so the streak is rebuilt from a fresh window anyway. A mutant that removes the reset survives as equivalent in every test, and this is recorded rather than tested;
    - `cusum_up` (ruling C3): Page's CUSUM (h = 4) over the segment's games with `p0` = the segment's posterior mean and `p1 = 1.5·p0`, run without stopping, so the statistic is the one **at the current game**. It is true only when all of these hold:
      - the statistic is above h now (an alarm somewhere in the past does not count);
      - the segment has at least 10 games;
      - the segment meets `minEvidence`;
      - the segment's posterior mean is above the band median.

      It is an early warning only: it can raise `emerging`, never `active`. It resets with each confirmation and each relapse because it runs on the segment;
    - `confirmed_up` / `confirmed_down`: a confirmation at this game;
    - `p_imp = P_imp(current segment, segment before the last confirmed change)`. It is computed only when it can matter (rule 3) and for the final outcome;
    - `drift` (ruling I1): EWMA (λ 0.1, k 2) over the per-game rates of the games after entering `resolved`. The level L is the segment's posterior mean at entry. σ = √(L(1 − L)/n̄) is the per-game binomial SD at the segment's mean opportunities n̄. Games without opportunities are skipped. Drift is true only when all of these hold:
      - the EWMA is above its limit **at the current game** (not sticky);
      - the most recent games since resolution reach `minEvidence` opportunities (the *relapse window*; none before resolution count);
      - the window's raw rate exceeds L by at least `RELAPSE_MIN_ABS_CHANGE = 0.005` **and** by at least the relative threshold 0.15;
      - P(θ > band median) on the window's posterior is at least 0.7, the same bar as `active`. A relapse means the weakness is back, not just that the rate moved.
    - **Relapse rebaseline** (ruling C2): entering `relapsed` starts a new segment at the start of the EWMA excursion (the first game after the EWMA was last at or below L) and clears the last confirmed change. P_imp is therefore `None` until a confirmed fall inside the relapse segment, and then compares against the relapse level. CUSUM and BOCPD also restart on the relapse segment. A relapsing player is never called `improving` against the old peak.

12. **State rules** (`next_state`, TS M2 Task 18, first match wins, with two R2 deviations marked):
    ```
    resolved                                          -> drift ? relapsed : resolved
    active|improving, streak >= 10                    -> resolved      (R2; TS also emerging|relapsed)
    active|emerging|relapsed, confirmedDown or pImp >= 0.85 -> improving
    improving, !confirmedUp                           -> improving
    confidenceAbove >= 0.7                            -> active
    (confirmedUp and !atOrBelowMedian) or cusumUp     -> emerging      (R2; TS any confirmedUp)
    active|emerging|relapsed, !atOrBelowMedian        -> prev
    otherwise                                         -> not_an_issue
    ```
    - **Deviation 1 (ruling C1):** `resolved` is reached only from `active` or `improving`. An `emerging` weakness was never established, so it cannot be "resolved"; it simply falls back to `not_an_issue`. A `relapsed` one must first improve (a confirmed fall, or P_imp against the relapse segment) or become `active`. Under TS, the brief's relapse series flapped `relapsed ↔ resolved` five times while the player was at 3× the median, and a strong player's 5-game burst went `emerging → resolved` on the next game.
    - **Deviation 2 (found by the C3 stationary test):** a confirmed rise raises `emerging` only when the segment's rate is above the band median. Without this, BOCPD-confirmed random rises from about 0.002 to 0.017 (still below the median of 0.02) labelled 2 of 200 stationary players at half the median as `emerging`. Consequence: `improving` with a confirmed rise at or below the median now falls to `not_an_issue`. That is honest: the rate rose, but it is still no weakness against peers.
    - The six TS table cases are unchanged and still pass.
    - A NaN, infinite or out-of-range probability fails every comparison.
    - A table covering every (previous state, rule) pair and a 20 000-case property test pin the reachable transitions. In particular:
      - `resolved` leads only to `relapsed`, and is entered only from `active` or `improving` (also checked on 60 random series);
      - `relapsed` is reached only from `resolved`;
      - leaving `improving` needs a confirmed rise: `emerging` or more above the median, `not_an_issue` at or below it.

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

    Descriptions state what changed, never that it caused anything ("Most games moved from 300+0 to 180+0"). A test rejects causal wording. They also never overstate a share:
    - "Most games moved" is used only when the new time control has a majority after the change. A plurality reads "The most played time control moved from X to Y".
    - Opening shares print with one decimal ("rose from 9.5% to 40.0%"), so a share under the 10 % rule never reads as "10%". Values are cut to 120 chars, descriptions to 600, and the list to the contract's six.

16. **Measured false-alarm rates** (fix round 1; ref {median 0.02, P40 0.017, pooled 0.02}, n0 300, minEvidence 1000, 50 opportunities per game, 200 seeded series of 300 games unless stated):

    | Scenario | Before the fix (review) | After |
    |---|---|---|
    | stationary 0.01 (half the median): ever `emerging` | 27/200 | **2/200 (1 %)**; test bound < 2 % |
    | stationary 0.01: any non-neutral state | — | 2/200, final state always `not_an_issue` |
    | stationary 0.005: any non-neutral state | 5/200 | 0/200 |
    | 60 × 0.02, 60 × 0.06, 380 × 0.01, 100 players: false relapse in the stable phase | 68/100 | **0/100** (test: exactly 0) |
    | same with a true return to 0.06 for 60 games: relapse detected | — | 100/100 |
    | brief's relapse series: `resolved` after the first `relapsed` | 5 flaps | 0 |

    Both seeded tests (`stationary_half_median_has_under_2_percent_false_emerging`, `a_stable_low_rate_after_resolution_never_relapses`) pin these numbers. The two `emerging` labels left at half the median are BOCPD-confirmed rises to a segment rate just above the median (0.0214 and 0.022). They pass every gate and are within the stated 2 % allowance.

17. **Data exactly at the median** (review M-c, not changed). Stationary data at the true median rate reaches `active` at some point in 147 of 200 series of 300 games, and ends non-neutral in 93/200. This is because `confidence ≥ 0.7` is re-tested every game, and a posterior at the boundary crosses 0.7 by chance. It is inherent to the TS rule at the boundary. A player at the median is, by definition, borderline, so the label is not a false claim of strength or weakness. Hysteresis would cut the churn and is left to a later detector version (it would bump `tl-v1`).

18. **Cost** (review M-g). The CUSUM statistic is recomputed over the segment each game, so a series costs O(n²); the EWMA is now incremental. P_imp runs 10 000 draws per game only in tracked states. A 500-game series takes well under 1 s in debug. This is fine for the beta, and incremental CUSUM is a later optimisation.

## Consequences (Task 14)

- `weakness_state_history.p_change` and `rate_before`/`rate_after` are `real` in the schema. `evaluate_series` is a pure recompute and never reads them back to compare them with f64 values, so Task 14 does not need to widen them. If Task 33 (persistence) ever compares stored rows with recomputed ones, it must first widen them to double precision in a coordinator migration.
- `active` needs `minEvidence` opportunities in the segment, so a new account shows `not_an_issue` until there is enough evidence. The UI must present that as "no claim yet", not "all clear".
- The per-game CUSUM rerun over the segment costs O(segment), so a whole series costs O(n²) (§18). That is negligible for series of a few thousand games.
- Every non-neutral state now needs `minEvidence` (§10), so first warnings for new accounts come later than under TS. That is consistent with the gate on `active`, and the UI shows "no claim yet" in the meantime.
