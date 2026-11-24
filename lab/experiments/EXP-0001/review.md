# Review of the pre-registration (part one)

Reviewer: a fresh agent with the `scientific-critical-thinking` and `peer-review` skills (`.claude/skills/science/`), reading `preregistration.md`, `analysis.py` (sha256 `9f5320c9…abd3` as registered), `docs/evaluation.md`, `docs/benchmark.md`, ADR 0106, the template, `lab/experiments/README.md` and `backend/crates/bench/src/results.rs`. Nothing was run; the numbers below are hand calculations and are marked as such. This is a working draft for the maintainer, who is accountable for the fixes.

What the study is: a sizing study. A pilot of 30 flawed and 2 clean synthetic players (120 games each) gives each flaw's hit probability for recall at 2; a Monte Carlo then estimates, for each suite size, how often a paired Newcombe 97.5% interval would ship a candidate that is 0.1 better. Strengths: the player is the unit throughout; the pilot's uncertainty about each flaw's hit probability is propagated (Jeffreys draws) instead of plugged in; the coverage secondary brackets the within-player correlation between an interval-by-interval and a player-by-player extreme; seeds are named and checked against the named suites; the full grid is reported, not one number. The concerns below are mostly about assumptions that decide the outcome but are not informed by the pilot, and about places where `analysis.py` evaluates a rule other than the one the decision installs.

## Concerns

1. The churn assumption decides the outcome, and nothing measures it.
   - Where: section 2 (second rival), section 6 inputs, section 8 model, section 9 "churn 0.05"; `analysis.py` `CHURN`, `PRIMARY_CHURN`.
   - Seen: the share of reference hits a candidate loses is fixed at 0.05 for the decision, with 0 and 0.10 as sensitivity, and no source is given. The pilot runs one algorithm version, so it carries no information about churn.
   - Why it matters: the power of a paired rule is driven by the discordant pairs. With π̄ about 0.6, churn 0.05 gives losses of about 0.03n and gains of about 0.13n per n flawed players; churn 0 gives no losses at all. By a McNemar-style hand approximation the required n moves by a factor of roughly two to three between churn 0 and 0.05, so the registered choice can decide between "resize" and "budget-bound". The decision then follows from an unexamined number while the 3,840-game pilot estimates only the quantity (π_f) the power is least sensitive to. Churn for ranking changes may also be much larger than 0.05: ADR 0101 changed which weakness ranks first for several players at once.
   - Severity: major.
   - Fix: estimate churn before the run from real version pairs on the same seeds, for example the smoke and full suites under the algorithm before ADR 0101 and under `algo-1.1.0` (the `bench-history` RunRecords or a re-run), and register the observed discordance (losses among reference hits, gains among misses) as the primary scenario, with its uncertainty. If that is not possible, register the decision under the pessimistic churn (0.10) and report 0.05 as sensitivity, and say in section 9 that the result is conditional on it.

2. The rule whose false-alarm rate and minimum detectable drop are reported is not the rule that is deployed.
   - Where: section 8 "Guard and tolerance"; `analysis.py` `simulate_recall` (`flagged`), the `at_budget` loop; `backend/crates/bench/src/results.rs` `regressions`.
   - Seen: the analysis evaluates a "CI guard" that flags when the 80% Newcombe interval of the paired difference lies wholly below 0, and reports its power and false-alarm rate. What CI actually does (`regressions`, `fell`) is compare the new metric with the stored baseline number: it fails when `now < was - tolerance`, a threshold on the net change in recall with no interval. The tolerance (k - 0.5)/n is derived from the guard, but the guard's operating characteristics are then attributed to a different rule.
   - Why it matters: a net-count threshold and a Newcombe interval react differently to the same data. The interval widens with the number of discordant pairs; the net-count rule does not. Under churn, a candidate with 3 losses and 2 gains has a net loss of 1 (passes the deployed rule at k = 2) but its Newcombe interval differs from the "k lost, none gained" case. The registered false-alarm rate and the MDE of the guard therefore do not describe what CI will do.
   - Severity: major.
   - Fix: simulate the deployed rule itself: in each simulated suite, flag when (n10 - n01)/n < -tolerance, and report that rule's false-alarm rate at true change 0 and its power for drops of 0.05 to 0.5. Keep the Newcombe guard only if `regressions` is changed to use it, and say so in section 9.

3. The k for the recall tolerance is computed on a configuration that cannot occur.
   - Where: section 8 ("the smallest number k of lost detections, with none gained"); `analysis.py` lines 530-538.
   - Seen: the loop calls `newcombe_paired(n11=0, n10=0, n01=k, n00=n-k)`. That is a reference with exactly k hits, all lost, and a candidate with recall 0. The real situation is a reference with H hits (about 0.6n), of which k are lost: n11 = H - k, n01 = k, n10 = 0, n00 = n - H. The two give different marginal proportions, different Wilson widths and a different φ (0 in the code's case, strongly positive in the real one).
   - Why it matters: k sets `bench/baseline.json`'s recall tolerance. By hand, at the smoke size n = 15 with H = 9, both configurations give k = 2, so the smoke tolerance is probably unaffected; at other n (the full suite's at_budget rows report k too) they can differ, and the plan's wording does not match the code.
   - Severity: major as a plan-code mismatch, minor in its likely effect at n = 15.
   - Fix: compute k with n11 = H - k, n01 = k, n10 = 0, n00 = n - H, taking H from the smoke suite's reference hits (the pilot's recall at 80 games times n, rounded), or better, drop the Newcombe construction and define the tolerance directly from the deployed net-count rule (concern 2).

4. The coverage tolerance is derived for a statistic CI does not compute, with a mis-stated error rate and the least conservative dependence model.
   - Where: section 8 ("The coverage tolerance becomes 1.2816 times the standard deviation … the drop churn alone exceeds 20% of the time"); `analysis.py` lines 557-574; `results.rs` `regressions` coverage branch.
   - Seen: (a) CI's coverage check is not a drop: it fails when |coverage - 0.8| grows by more than the tolerance, a two-sided distance test against the stored baseline. The analysis simulates a signed paired difference. (b) A drop beyond 1.2816 standard deviations happens 10% of the time one-sided, not 20%. (c) The null is simulated by independent flips of every interval, which section 8 itself treats as the optimistic extreme; real version changes move a player's checkpoint intervals together (a prior change shifts all of W1's intervals at once), so the standard deviation and therefore the tolerance are too small, and false alarms more frequent than intended.
   - Why it matters: the tolerance written into `bench/baseline.json` would not have the false-alarm rate the text claims.
   - Severity: major (it is one of the two outputs the decision installs).
   - Fix: simulate the deployed statistic, |c_new - 0.8| - |c_base - 0.8|, under the null; compute the tolerance from both the interval-flip and the player-flip models and register the larger; state the error rate it gives (the share of null runs that fail) instead of a z multiple.

5. The simulated recall is not the recall the resized full suite will compute.
   - Where: section 9 (the full suite is 5m flawed players "and the 8 other rates of today's sweep at one seed each" and two clean controls); `results.rs` `score` (`recall_at_2` is the share over all flawed players).
   - Seen: the power is simulated over the 5m smoke-rate players only, but the bench scorer's recall includes every flawed player, so the 8 sweep players at other rates enter the weekly metric.
   - Why it matters: the weekly suite's recall would mix players the power analysis did not model (at 13 players per flaw, 8 of 73 flawed players, about 11%), with different and unknown hit probabilities. A candidate's change on them is unmodelled, and the comparison the decision sized is not the one the suite reports.
   - Severity: major.
   - Fix: either register that the primary comparison in the weekly suite reads recall over the 5m smoke-rate players only (and add that metric to the scorer before the change lands), or include the sweep players in the simulation with their own hit probabilities (which needs them in the pilot).

6. The uniform-gain model, and the example in the question, do not describe a typical candidate.
   - Where: section 1 (the chess example), section 2 (the ceiling rival), section 8 (a = (0.1 + bπ̄)/(1 − π̄) applied to every miss).
   - Seen: the candidate converts misses to hits at the same rate in every flaw. A real candidate, such as the section 1 example of a new W1 detector, changes one detector and so moves one flaw's players. With five flaws, +0.1 overall means +0.5 within that flaw, which is infeasible when its hit rate is already above 0.5 (W1 is ranked first today). Most misses today sit in W4.missed and W7.traps, which are not ranked for structural reasons (no peer row, cost attribution), so the model credits the candidate with fixing them.
   - Why it matters: the MDE of 0.1 pooled over flaws is what the suite is sized for, but the question frames it as a single-detector change it cannot represent. The total discordant counts, and so the power, are roughly the same whether a gain is spread or concentrated, so the numbers may stand; the claim does not.
   - Severity: minor.
   - Fix: rewrite the example as a pooled change (for example, a ranking fix that brings W7.traps into the top 2 for half of its players), add a sensitivity scenario where all gains fall on one flaw's misses, and report per-flaw feasibility.

7. The power reported is assurance, and infeasible draws are counted as failures.
   - Where: section 6 and section 8; `simulate_recall` (`feasible`, `ship = (lo_s > 0) & feasible`).
   - Seen: power is averaged over draws of π_f from the Jeffreys posterior, which is Bayesian assurance (expected power), not power at fixed inputs. Draws where the required gain exceeds 1 count as "not shipped".
   - Why it matters: both choices lower the number relative to the conventional power at the point estimate, and the second does so for a reason unrelated to the suite size (a draw where +0.1 is impossible says nothing about whether m players detect it). Each is defensible, but the reader of section 9 should know which quantity crossed 0.80.
   - Severity: minor.
   - Fix: name the quantity "assurance" in sections 4, 6 and 9, report the infeasible share next to every power (it is computed and stored), and also report power conditional on feasibility.

8. The first grid crossing of 0.80 is read from a noisy, non-monotone curve.
   - Where: section 9 ("a smallest grid m with ship power at least 0.80"); `required_m`.
   - Seen: the first m whose Monte Carlo estimate is at least 0.80 is taken. Power curves of discrete interval rules are saw-toothed, and the Monte Carlo error is up to 0.008.
   - Why it matters: a cell at 0.802 with a true value of 0.795 would set the suite size; the choice is pre-registered, so this is not p-hacking, but it is noise in the decision.
   - Severity: minor.
   - Fix: require the lower end of the 95% Monte Carlo interval to be at least 0.80, or require the next grid point to pass as well.

9. Small plan-code and text inconsistencies.
   - Where: section 8, section 9, `analysis.py`.
   - Seen: (a) section 8 says the mean of per-player coverage "equals the benchmark's pooled share" because checkpoints are equal per player, but `coverage_vectors` drops checkpoints without an estimate row, so sizes can differ; and in `simulate_coverage` the gain is calibrated on the pooled share `cbar` while the effect is measured on the mean of per-player means. (b) Section 9's smoke rule is "the largest m that fits 1,650 games", but the code restricts m to `M_GRID` (it gives m = 3 either way today; it would not if the budget changed). (c) The Newcombe method 10 implementation should be checked against Newcombe's published worked examples (1998, table II), including how φ is handled when ad > bc, since a unit test is cheap and the decision depends on it. (d) The pilot's 80-game checkpoint stands in for an 80-game player; this holds only if a checkpoint's build sees exactly what an 80-game run would (the same clock and window), which should be stated or tested.
   - Severity: minor.
   - Fix: correct the sentence in (a) or weight players by intervals; use `range(1, …)` for the smoke m as for the full one; add a test of `newcombe_paired` against published values; add one sentence (or a check) for (d).

10. H0 and H1 are not tested.
   - Where: section 2.
   - Seen: H0 and H1 are statements about the outcome of a deterministic computation given the assumptions; the experiment has no inferential step that could reject H0, and "need more data" is declared not to apply.
   - Why it matters: it is honest to say this is a sizing calculation whose result is conditional on the churn and gain model; dressing it as a hypothesis test invites the decision to be read as evidence about the benchmark.
   - Severity: minor.
   - Fix: state in section 2 that the hypotheses label the two branches of section 9 and are not tested, and that the result holds under the registered churn and gain assumptions only.

## Overall verdict

The design gets the hard parts right: the player is the unit, the pilot's uncertainty is propagated, the coverage secondary brackets clustering, and every output is reported. Its weakness is that the decision is driven by an assumption the experiment does not measure (churn), and that the two tolerances it would write into `bench/baseline.json` are derived from rules that differ from what `results.rs` actually checks (a Newcombe guard instead of a net-count threshold for recall, a signed drop instead of a distance from 0.8 for coverage), with one of them computed on an impossible configuration. None of these needs a bigger pilot; they need the churn to be estimated from two real algorithm versions (or the decision taken at the pessimistic value), the deployed rules to be simulated as they are, and the full suite's recall to be defined over the players that were sized. Each fix changes `analysis.py`, so section 10's sha256 must be recomputed and each edit listed here before anything runs.

## What was done before the run

Every edit below is in the commit after the review, before any result; section 10's sha256 was recomputed.

1. Churn: not measurable without a second version on the same seeds (a pre-ADR-0101 pilot would cost another 3,840 games). The decision is now taken at the conservative end, churn 0.10 (sections 2, 8 and 9, `PRIMARY_CHURN`); 0 and 0.05 stay as sensitivity.
2. The CI rule: `simulate_recall` now also runs the rule `regressions` applies (recall below the baseline by more than the tolerance), and the analysis reports its false-alarm rate and MDE next to ADR 0104's guard (section 8).
3. The tolerance's k is computed on a reference at the pilot's recall (n11 = H − k, n01 = k, n00 = n − H), not on a candidate at recall 0.
4. The coverage tolerance is now the 80th percentile of `|c' − 0.8| − |c − 0.8|`, the statistic CI checks, under the player-by-player flips; the "20% of the time" sentence now matches it.
5. The rate sweep leaves the full suite, so the suite that is scored is the suite that was sized (section 9, `FULL_SWEEP_PLAYERS = 0`).
6. Minor: the power is named an assurance and infeasible draws are said to count as not shipped (section 8); `required_m` needs the power to stay at least 0.80 for every larger grid size; the smoke m is any integer; the pooled-share sentence is conditioned on every checkpoint having an estimate row; section 2 says H0 and H1 name the branches and are not tested. Not done: a test of `newcombe_paired` against Newcombe's published table (the paper's values were not to hand); it was checked on fabricated inputs for symmetry, a zero difference and a pure-gain table. The 80-game checkpoint standing in for an 80-game player holds by construction: the harness builds each checkpoint with the clock one hour after its last game (`bench::harness::run_player`).
7. Owner direction after the review: `--algorithm` and `--seed-namespace` make the algorithm and the seeds inputs (section 10); the defaults are the registered run.

The two analysis scripts were exercised once on fabricated inputs before the run (a fake bench result, and a seed namespace `DRY-` that is not the registered one, at toy sizes), to catch software errors. No registered seed and no pilot data were used.

# Review of the result (part two)

Reviewer: a fresh agent with the `scientific-critical-thinking` and `peer-review` skills, reading `preregistration.md`, `analysis.py`, part one above, `results.json` (queried, not read whole), `decision.md`, `summary.md`, the EXP-0001 line in `lab/findings.md`, and the uncommitted implementation in the worktree (`backend/crates/bench/src/suite.rs`, `.github/workflows/bench.yml`, `.github/workflows/lab.yml`, `docs/benchmark.md`, `bench/baseline.json`). Nothing was rerun. Every number in `decision.md`, `summary.md` and the findings line was checked against `results.json`; those not named below match it to the rounding shown. The compute figures (1,489 seconds, 575,567 searches, 468,209 from the cache) are not in `results.json` and could not be checked. This is a working draft for the maintainer, who is accountable for the decision.

What holds: the decision follows section 9. At churn 0.10 the required players per flaw are 40 at 80 games and 48 at 120 (`required_players_per_flaw`), above the budget's 22 and 14, so "resize" cannot apply; at the budget the ship power is 0.525 at 80 games and 0.283 at 120, more than 0.01 apart, so the rule picks 80 games and 22 a flaw. The smoke m (3), the recall tolerance ((2 − 0.5)/15 = 0.1, from 10 reference hits) and the coverage tolerance (0.05) are the ones `results.json` records. The churn is called an assumption throughout, and the power is called an assurance.

## Concerns

1. A wrong count of clean checkpoints.
   - Where: `decision.md`, "Secondaries", clean controls: "two of 64 clean checkpoints here (rather than zero of four in the smoke suite)".
   - Seen: `observed.clean_rank1` has 2 clean players at 5 checkpoints, 10 clean checkpoints, with a rank-1 weakness at 2 of them (`exp1-clean-0` at 45 games, W9a; `exp1-clean-1` at 80 games, W5).
   - Why it matters: 2 of 64 reads as a 3% rate; it is 2 of 10, 20%, which makes the exploratory signal about the ranking floor stronger than the text says.
   - Severity: minor.
   - Fix: "two of 10 clean checkpoints".

2. "The opening players' rank 1 is W1 at every checkpoint" is not quite so.
   - Where: `decision.md`, "Secondaries", recall at 2.
   - Seen: `exp1-openingblunder-0` has no rank-1 weakness at 30 games; the other 29 opening checkpoints have W1 first.
   - Severity: minor.
   - Fix: "at every checkpoint but one (nothing ranked at 30 games for one player)".

3. A coverage half-width attributed to a size that was not simulated.
   - Where: `decision.md`, "Secondaries": "at 22 players a flaw the 95% half-width is about 0.057".
   - Seen: the coverage grid has no m = 22; 0.057364 is the m = 20 cell (32 a flaw gives 0.046). The other values (0.091 at 8, 0.153 at 3, paired power 0.52 and 0.23 at 20) match.
   - Severity: minor.
   - Fix: "at 20 players a flaw, the nearest simulated size, 0.057".

4. An unlisted change to the premise of the budget.
   - Where: `decision.md`, "Deviations" and "Next" item 2; preregistration section 9 ("the full suite runs weekly inside `bench.yml`'s 180-minute job and has 9,000 games"); the worktree's `.github/workflows/bench.yml` (timeout 180 to 300 minutes) and `lab.yml` (90 to 300 minutes, release build).
   - Seen: the implementation raises the weekly job to 300 minutes. The budget was registered in games, so the branch taken is still right, but the 9,000 games were justified by the 180-minute job. At the documented 0.544 seconds a game, the resize suite of 40 a flaw at 80 games (16,160 games, which `decision.md` itself computes) takes about 147 minutes, inside the new limit on the documented machine, though not on a runner twice as slow. "Next" item 2 leaves the split-or-accept choice to the owner while the workflow change has already moved the limit.
   - Why it matters: a reader sees "budget-bound" and a 300-minute job side by side and cannot tell whether the 0.1 target was given up for compute or by choice. A change to a registered input of the decision rule must be listed.
   - Severity: major.
   - Fix: add a deviation stating the timeout change and its reason (slower hosted runners), and state that the registered budget of 9,000 games is kept and the extra time is headroom, not budget; if the owner wants the budget raised, record that as a new decision (it would make "resize" reachable), not as a workflow edit.

5. The gain model that the pilot contradicts is not discussed, and part one's concern 6 is neither fixed nor listed as not done.
   - Where: section 1 (the W1 detector example), section 8 (gains spread uniformly over every flaw's misses); part one, concern 6 and "What was done", item 6 (concern 6 is in neither the done nor the not-done list); `decision.md`, primary.
   - Seen: at 80 games `hang` and `low_clock_blunder` are 6 of 6 and `no_convert` 5 of 6, so the flaws a detector fix would most plausibly move are at or near the ceiling; 10 of the 11 misses are `miss_mate` (0 of 6; W4.missed is never ranked for any player at any checkpoint, which `decision.md` attributes to structure) and `opening_blunder` (4 of 6 missed, held back by the W1 precedence). The simulated candidate gains almost all of its +0.1 on exactly the players the decision says the ranking cannot reach, and the section 1 example (a W1 detector finding more hanging-piece players) cannot happen at the pilot's ceiling. The Jeffreys draw for 0 of 6 (Beta(0.5, 6.5), mean about 0.07) also gives simulated reference hits on `miss_mate` that the pipeline does not produce.
   - Why it matters: the MDE of 0.15 and the 40 players a flaw are the power for a hypothetical pooled change under this model, not for the detector or ranking fixes the benchmark will meet. Part one judged the total discordant count roughly model-free, which is plausible, but the report does not say which kind of change the 0.15 refers to.
   - Severity: major (interpretation; it does not change the branch taken).
   - Fix: say in `decision.md` that the power and the MDE are for a change spread over the flaws' misses, that at 80 games the misses are almost all in W4.missed and W7.traps, and that the section 1 example is unreachable on this pilot; list concern 6 as not done in part one's record. A concentrated-gain sensitivity run belongs in the EXP-0005 registration, not here.

6. A causal explanation of the 80- against 120-game result stated as a finding.
   - Where: `decision.md`, primary: "under the registered churn model a higher baseline means more hits to lose and fewer misses to gain".
   - Seen: the same ordering appears at churn 0 (16 a flaw at 120 games against 14 at 80), where no hits are lost, so the loss of hits cannot be the whole explanation. At 120 games 0.3 to 0.75% of the draws are infeasible (counted as not shipped), against none at 80; the grid steps are coarse (14, 16; 40, 48) and the Monte Carlo error is about 0.008. No analysis isolated a cause.
   - Severity: minor.
   - Fix: report the ordering and give the explanation as a conjecture, naming the infeasible share and the grid's coarseness as alternatives.

7. Registered secondaries left out of the decision.
   - Where: preregistration section 4 (confirmatory: the MDE of the ship rule, of ADR 0104's guard and of the CI rule, and the false-alarm rate of both guards, at the smoke, full-80 and full-120 sizes); `decision.md`, "The CI rule and the tolerances".
   - Seen: `results.json` `at_budget` gives the guard's MDE (0.3 smoke, 0.1 full-80, 0.1 full-120) and false-alarm rate (0.098, 0.093, 0.090), the CI rule's MDE at the full sizes (0.05, 0.1) and its false-alarm rate at full-120 (0.30). The decision reports the smoke CI rule and the full-80 tolerance only. At the smoke size ADR 0104's guard needs a drop of 0.3 to fire four times in five, against the CI rule's 0.2.
   - Severity: minor (the numbers are in `results.json`; section 4 says every metric is reported whatever it shows).
   - Fix: add the three-size table of both guards' MDE and false-alarm rate.

8. The ship curves are not monotone, and the MDE is read off the rising part without saying so.
   - Where: `decision.md`, budget table ("smallest change shipped 80% of the time"); `at_budget[].curve`.
   - Seen: at full-80 the ship power is 0.83 at +0.15 and 0.96 at +0.2, then falls to 0.64 at +0.3 and 0.003 at +0.5; at full-120 it peaks at 0.58 (+0.2) and falls to 0; the smoke CI rule's detection of drops also falls from 0.99 at 0.4 to 0.90 at 0.5. This is the infeasible draws (a > 1) counted as not shipped, an artefact of the gain model, not a property of the suites. The grid step is 0.05, so the MDE lies in (0.10, 0.15].
   - Why it matters: "none in the grid" for the 120-game suite partly reflects infeasibility at large changes; a reader could take the falling curve for a defect of the rule.
   - Severity: minor.
   - Fix: one sentence explaining the fall and the grid's resolution; report the infeasible share next to the curve.

9. The decision states as done a change that is not in the tree.
   - Where: `decision.md`, "What changes" (`bench/baseline.json`: the new smoke metrics, tolerances 0.1 and 0.05); section 9 ("the new smoke suite is run once to give the baseline's metrics"); the worktree's `docs/benchmark.md` already describes the new tolerances.
   - Seen: `bench/baseline.json` still holds the six-player metrics and tolerances of 0.2 (recall) and 0.15 (coverage).
   - Severity: minor (implementation pending).
   - Fix: commit the baseline from one run of the new smoke suite with the decision, or word "What changes" as pending.

10. The pilot sits below the absolute thresholds the decision keeps, and the decision does not say so.
    - Where: section 9 (recall at 2 at least 0.8, coverage in [0.65, 0.95], unchanged); `observed`.
    - Seen: recall at 2 at 80 games is 0.63 [0.46, 0.78] and coverage 0.575 [0.467, 0.683], both below their thresholds, the coverage interval mostly below 0.65. The resized suites at the smoke rates should be expected to miss both absolute thresholds (reported, not enforced, `docs/benchmark.md`).
    - Severity: minor.
    - Fix: one sentence under "Secondaries".

11. The follow-up does not follow cleanly from the evidence.
    - Where: `decision.md`, "Next" item 1; `.github/settings/roadmap-issues.json`, EXP-0005.
    - Seen: (a) The rule is one-sided: "if the churn is below 0.10, re-size". A measured churn above 0.10 means the weekly suite is weaker than registered, which also needs its MDE re-registered. (b) One candidate (EXP-0006's prior change) gives one churn; a prior change moves many ranks at once and may not stand for detector fixes, so it estimates the churn of that change, not "the churn". (c) "Re-size with this experiment's analysis at the measured value" changes `PRIMARY_CHURN` in a frozen `analysis.py`; a re-size is a new registered analysis (EXP-0005's), not a rerun of EXP-0001.
    - Severity: minor.
    - Fix: make the EXP-0005 rule two-sided, say that it measures one change's churn and that the suite is re-sized at the upper end of its interval, and register the re-size in EXP-0005's own pre-registration and `analysis.py`.

## Verdict

The decision is the one section 9 gives, and the primary numbers (the required sizes, the powers with their Monte Carlo intervals, the tolerances) are reported correctly in `decision.md`, `summary.md` and `lab/findings.md`. Three statements of fact in `decision.md` are wrong or loose: two of 64 clean checkpoints (it is two of 10), W1 first at every opening checkpoint (29 of 30), and the half-width "at 22" (the m = 20 cell). The larger issues are about interpretation and process: the weekly job's timeout was raised from 180 to 300 minutes without a listed deviation, though the 180-minute job was the stated basis of the 9,000-game budget, and the MDE of 0.15 is for a pooled change that, on this pilot, falls almost entirely on flaws the ranking cannot reach today. Both can be fixed in `decision.md` without touching the frozen sections.

## What was done after part two

Edits to `decision.md`, `summary.md`, `lab/findings.md` and the EXP-0005 issue only; sections 1 to 10 and `analysis.py` are unchanged.

- Wrong numbers corrected: 2 of 10 clean checkpoints; the coverage half-width is the 20-a-flaw cell; W1 is the opening players' rank 1 at 29 of 30 checkpoints.
- Major 1: the timeout change is listed as a deviation; the 9,000-game budget is kept and its basis stated, with the 147-minute estimate for the 40-a-flaw suite left to the owner.
- Major 2: the decision now says that 10 of the 11 pilot misses sit in W4.missed and W7.traps, so the +0.1 is a pooled change that would have to come from them, and that a W1 change cannot show in recall at 2 at this ceiling. Part one's concern 6 (the W1 example) is answered by this, not by a change to the frozen sections.
- Minors: the 120-game explanation is stated as the model's mechanics, with the churn-0 ordering and the infeasible draws; the guard and CI-rule secondaries are reported; the non-monotone ship curve and the MDE's grid step are stated; the pilot's results are set against the unchanged absolute thresholds; EXP-0005 re-sizes in either direction with its own analysis. Concern 6 (the baseline) is done in the commit that resizes the suites.
