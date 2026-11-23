# Review of the pre-registration (part one)

Reviewer: a fresh agent with the `scientific-critical-thinking` and `peer-review` skills (`.claude/skills/science/`), reading `preregistration.md`, `analysis.py` (sha256 `38f763d3…5222` as registered), `docs/evaluation.md`, `docs/benchmark.md`, ADR 0106, the template, `lab/experiments/README.md`, and `profile-core/src/stats/posterior.rs`, `priority.rs` and `constants.rs`. Nothing was run; the figures below are hand calculations and are marked as such. This is a working draft for the maintainer, who is accountable for the fixes.

What the study is: a simulation of event counts from chosen true rates, passed through the production `compute_estimates`, scoring how often the 80% interval holds the true rate. Strengths: it removes detector error by construction, so it can isolate the posterior; it runs the production code; the player is the unit and every interval of a player is resampled together, which handles the nesting of the four checkpoints; the worst-case standard-error bound in section 6 is correct (a per-player share with mean 0.8 has variance at most 0.16), so the primary is well powered; the expectation is written down before the run, so it can be wrong; the cell-level results, the share missing on each side and the calibration curve are all reported. The concerns are about what the primary averages over, which can hide the miscalibration the study is meant to find, and about whether the grid contains the case that motivated it.

## Concerns

1. The primary averages over miscalibration in opposite directions, so it can land in the band while most of its parts are outside it.
   - Where: section 4 (primary over every key, band, dispersion and checkpoint at ρ in {2, 4}, cells weighted equally), section 9 (decision on the primary only); `analysis.py` lines 266-283.
   - Seen: the primary pools W5 (flat prior, where a Beta posterior is close to nominal by construction), W4.missed and W7.traps (n0 = 40), W1 (n0 = 300) and W9b, over checkpoints from 30 to 250 games, with and without overdispersion. The expected errors have different signs. By hand: the prior pull gives under-coverage at ρ > 1 and short histories; the recency weighting treats weighted counts as real counts, which overstates the posterior variance (the true variance of the weighted rate scales with Σw²c, the posterior's with Σwc), so at 250 games, with weights from 0.24 to 1, coverage rises to roughly 0.88; the 1.5 design effect has the opposite sign and brings W1 and W9b down to roughly 0.7 at 30 games. An equal-weighted average of 0.65, 0.7, 0.8 and 0.88 sits inside [0.75, 0.85].
   - Why it matters: coverage is a calibration property of each interval a player reads, not of an average over players with different keys and histories. A "within the band" decision would be read as "the ranges are calibrated for the players they are shown to" (section 9), while W1 at 30 to 60 games, the case of #13, could be well below the band. The confirmatory margins have the same problem: each margin averages over all ρ, so the over-coverage at ρ ≤ 1 cancels the under-coverage at ρ ≥ 2 within, for example, the W1 margin.
   - Severity: major.
   - Fix: make the decision depend on the worst part, not the mean. For example, register as primary the set of key × checkpoint coverages at ρ in {2, 4} (20 numbers), with the rule "within the band" only if every one's Bonferroni interval lies inside [0.75, 0.85], and "outside" if any lies wholly outside. Alternatively keep one number but make it a miscalibration distance, such as the largest |coverage - 0.80| over key × checkpoint, with a bootstrap interval. Restrict the confirmatory margins to ρ in {2, 4} (or report key × ρ margins), so they do not average over-coverage and under-coverage.

2. The case that motivated the study lies outside the grid.
   - Where: section 1 (#13), section 7 (W1 peers 0.004 "the contract fixture's value"; ρ up to 8); `docs/benchmark.md`.
   - Seen: in the benchmark, W1's interval misses because it is shrunk towards the opponents' rate of 0.0008 while the true rate is 0.023, an odds ratio of about 29. Here W1's prior mean is 0.004 and the largest ρ is 8 (true rate about 0.031); the primary uses ρ up to 4 (about 0.016). The prior pull scales with the distance between the prior mean and the truth relative to the posterior width, so the grid understates the #13 situation severalfold.
   - Why it matters: a "within the band" result here would not answer #13, and could be cited as if it did. The peer rates of all keys (0.004, 0.02, 0.30, 0.10) are labelled assumptions; they decide where the prior sits, and so decide the result for the keys with a strong prior, contrary to the section 7 remark that they "set where the prior sits, not whether it is right".
   - Severity: major.
   - Fix: take the peer rates from a real band reference (the `lichess-db --band-reference` output, or the bench's opponents' rows) and record them in `inputs.sha256`; add the benchmark's own condition (W1 peers 0.0008, true rate 0.023) as a registered cell; and either extend ρ to 16 and 32 or justify why players above ρ = 8 do not occur.

3. The primary's population is chosen by ρ, not by whether the player is told about the weakness.
   - Where: section 4 ("These are the players Rematch tells about a weakness"); `analysis.py` keeps `reason` but not `credible`.
   - Seen: Rematch shows a weakness when the estimate is `credible` (80% interval above the peers' median and an excess of at least 0.02 points a game, `priority.rs`). Some players at ρ = 2 will not be credible, and ρ = 8 players, who are, are excluded from the primary. Conditioning on being shown is also a selection on the interval itself: players whose interval happened to land high are more likely to be shown, which changes coverage among them (a winner's-curse effect).
   - Why it matters: the estimand in section 4 is described as the ranges "a player reads", but the code computes something else. The excluded ρ = 8 is also the cell the stated expectation predicts to be worst, which makes the choice look tuned towards passing even though it is pre-registered.
   - Severity: major.
   - Fix: record `credible` (and the weighted `k`, `n`) in each row, and add, as a confirmatory secondary or as the primary, the coverage among estimates that are credible, at every ρ. If the primary stays ρ-based, include ρ = 8, or give a reason grounded in the product for leaving it out.

4. The recency rival is mis-stated and cannot be separated from the prior pull.
   - Where: section 2 ("it widens intervals, so it cannot cause under-coverage by itself").
   - Seen: `estimate_weakness` down-weights the data with the 60-day half-life but not the prior strength n0 (`priority.rs`, `posterior(k, n, pooled, n0)`). Weighting therefore increases the prior's relative weight and adds to the pull at ρ > 1. It also overstates the posterior variance (concern 1), so it pushes coverage both up and down at once. No condition in the grid switches recency off.
   - Why it matters: if the primary misses, the study cannot say whether the prior strength, the recency weighting, or their interaction is to blame, and the proposed fix (a weaker W1 prior) may be the wrong one.
   - Severity: major.
   - Fix: correct the sentence, and add a registered contrast in which every game is played within a day of `now` (spacing of minutes, so every weight is about 1), at least for W1 and W9b at all checkpoints. That isolates the prior from the weighting through the same production function, with no schema change.

5. The statement that the gap between EXP-0001 and EXP-0002 is "the detector's share" does not follow.
   - Where: section 1, last sentences of the first paragraph.
   - Seen: EXP-0001's end-to-end coverage comes from bench players with a different peer row (W1 0.0008), different chances, different ρ, only 30 to 120 games and the clock model; EXP-0002's from this grid. The two coverages differ in many design factors besides the detector.
   - Why it matters: subtracting one from the other would attribute every design difference to the detector.
   - Severity: minor.
   - Fix: either drop the sentence, or register a matched cell here that copies the pilot's conditions (its peer rows, chances per game, true rates and checkpoints), and compare only with that cell.

6. Two-sided tests of coverage = 0.80 add nothing, and their family is inconsistent with the classification.
   - Where: section 8 (z tests, Holm) and section 9 (classification by Bonferroni intervals).
   - Seen: with 2,400 or more players per margin, almost every margin will reject coverage = 0.80 even when it is inside the band. The decision is made on intervals against the band, not on the p values.
   - Why it matters: Holm-adjusted p values against a point null will be quoted as "significantly miscalibrated" for margins that are within the band the study itself calls fine.
   - Severity: minor.
   - Fix: drop the z tests, or replace them with two one-sided tests against the band's edges (0.75 and 0.85), Holm-adjusted, which match the classification.

7. The overdispersion model covers one source and one level.
   - Where: section 7 (de1.5 for W1 and W9b only; θ constant over 250 games).
   - Seen: only within-game clustering at one design effect is simulated. The true rate is constant for 125 days, while the product's use case is a player who trains and changes. The design effect for Poisson-sized games is 1 + (E[m²]/E[m] - 1)ρ_icc, about m + 1 rather than m, a small difference.
   - Why it matters: the conclusion "calibrated" would hold for stationary players with mild clustering, which should be what the decision says.
   - Severity: minor.
   - Fix: add a design effect of 2 (the top of the cited 1.3 to 2 range) as a third level, or state the limit in section 9's wording; optionally add one exploratory drift condition (a rate falling by half over the window), scored against the rate at `now`.

8. Small factual and plan-code points.
   - Where: sections 6, 7 and 8; `analysis.py`.
   - Seen: (a) Section 7 says W4.missed at B1 and ρ = 8 is 0.94; by the stated odds scaling it is 0.811 (0.3/0.7 × 1.25 × 8 = 4.29 in odds). (b) The Bonferroni z for 19 two-sided margins at 0.05 is about 3.01, not 3.02 (the code computes it, so only the text is off). (c) The production interval is the 10% and 90% quantiles widened to include the mean (`posterior.rs`), while the PIT curve uses the plain Beta CDF; the two differ only for tiny shapes, but the text should say the curve is of the central Beta interval. (d) "Bands" vary only the peers' rate (the trainability is irrelevant to coverage), so the band margin is a peer-rate margin and should be named so.
   - Severity: minor.
   - Fix: correct (a), (b) and the naming in (d); add one sentence for (c).

9. The intervals measure Monte Carlo precision only.
   - Where: sections 6, 8 and 9.
   - Seen: the bootstrap intervals capture the sampling of synthetic players. The quantities that set the answer (peer rates, chances per game, the ρ grid, checkpoints, weights over cells) are fixed by design and carry no uncertainty in the interval.
   - Why it matters: a narrow 95% interval inside the band may be read as strong evidence that the ranges are calibrated, when it says the average over this particular grid is precisely estimated.
   - Severity: minor.
   - Fix: state in section 9 that the decision is conditional on the registered grid and inputs, and report the primary's sensitivity to the two inputs most likely to move it (the W1 peer rate and the design effect).

## Overall verdict

The experiment is well constructed in the mechanics that usually go wrong: it calls the production function, treats the player as the unit, has enough players, and reports everything. Its main risk is a decision rule that can pass when the ranges are not calibrated: the primary is a mean over keys, checkpoints and dispersion levels whose errors have opposite signs, the margins average over ρ in the same way, and the grid does not contain the #13 condition (a W1 peer rate of 0.0008 against a true rate of 0.023). Before the run, the decision should be tied to the worst key × checkpoint coverage (or every one of them) among the players who are actually shown the weakness, the peer rates should come from a real reference with the benchmark's case added, and a no-recency contrast should be added so a miss can be attributed. Each fix changes `analysis.py`, so section 10's sha256 must be recomputed and each edit listed here before anything runs.

## What was done before the run

Every edit below is in the commit after the review, before any result; section 10's sha256 was recomputed.

1. The primary is now the 20 key × checkpoint coverages at ρ in {2, 4, 8}, decided on the worst: "within" only if every Bonferroni interval lies inside [0.75, 0.85], "outside" if any lies wholly outside (sections 2, 4, 8, 9). The pooled number is still reported, not decided on. The margins by key, band, checkpoint and dispersion are restricted to the primary ρ.
2. The #13 case is a registered cell (W1, peers 0.0008, θ = 0.023, both dispersions). No per-band peer reference is committed, so the other peer rates stay assumptions, now said so in section 7.
3. `credible` is recorded, and coverage among credible estimates is a descriptive secondary per key at every ρ, with the selection caveat. ρ = 8 is in the primary.
4. Recency: the rival is restated (it adds to the pull and can widen the interval), and a schedule factor separates it: the same draws with every game inside the last day. Its margins are confirmatory.
5. Minor: the detector-share sentence is now descriptive only; the z tests are kept for Holm but the classification is what the decision uses; the intervals are said to carry simulation noise only; 0.811 and z = 3.03 (21 margins) corrected; the calibration curve is named the central Beta interval; the band margin is named a peer-rate margin. Overdispersion stays at one level and rates do not drift, both stated in section 7.
6. Owner direction after the review: `--algorithm` and `--seed-namespace` make the algorithm and the seeds inputs (section 10); the defaults are the registered run.

The analysis was exercised once before the run with a seed namespace `DRY-` that is not the registered one and 2 players a cell, to catch software errors. No registered seed was used.
