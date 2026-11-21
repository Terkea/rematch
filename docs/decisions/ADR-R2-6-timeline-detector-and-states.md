# ADR-R2-6. Timeline change detector and states

Date: 2026-11-21
Status: Accepted. This file has the detector part, from R2 Task 13 (`profile-core::timeline::{bocpd, cusum, ewma}`). R2 Task 14 appends the state machine, the confirmation rule and the onset rows. The number is a placeholder: the controller renumbers R2 ADRs before merging.

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
