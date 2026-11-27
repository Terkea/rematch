# EXP-0029: review

## Part one: the pre-registration, before any new month is read

The critic was a fresh agent (Claude, no part in the design). It read the stage 1 registration at d37dce69 (`preregistration.md`, `power.py`, `power.json`, `summary.md`, `inputs.sha256`, the placeholder `analysis.py`), EXP-0009's registration, decision, review and results, issue #131, the template, ADR 0114 and `trainability`. It re-ran `power.py`'s functions read-only in the lab image and downloaded nothing. Its verdict was not ready: 4 blocking, 8 should-fix and 7 minor concerns.

Nothing has been streamed, and no month from 2025-09 to 2026-06 has been read, before or after this review. Every fix below uses only EXP-0009's committed results.

### Blocking

1. **B1. The power took the lower bound once, but the run takes it again.** Stage 1 planned at the lower bound and then read the minimum at that same value. The run, though, computes its own lower bound below whatever the truth is. Under the assumption the text stated, two keys reached the target, not six, and the closed form ignored the correlation between keys and the joint requirement on the control. Done: `power.py` now simulates section 9 end to end. It draws the run's own split-half correlation at its n, takes the lower bound and the floor, draws correlated coefficients (ρ = 0.3; 0 and 0.6 as the sensitivity), applies Holm across the eight keys, the control's own test and the three outcomes. The plan assumes EXP-0009's point reliability through Spearman-Brown, with the run's lower bound as the only penalty, and the lower bound as the truth is reported as a sensitivity. Outcome 2's list is cut, from the simulation and before any data, to W4.allowed, W5, W6, W7.traps and W9a. W7.prep (0.52 to 0.77) and W4.missed (0.58 to 0.79) are out with W9b, and section 9 says outcome 2 covers none of the three (sections 6 and 9).
2. **B2. The primary leaked the true level, a positive bias.** With only q₁ adjusted, q₂ still carries a stable level, which EXP-0009 showed predicts later climbing. Done: the primary adjusts for the window-mean level q̄ = (q₁ + q₂)/2, and the F1-level model is a sensitivity. Outcome 1 is read on the primary, and a key that reads "precedes climbing" only in the F1-level model is called a level artefact. Section 2 no longer says the bias works "never for it": it names the level leak and its size, about +0.8 to +1.0 per observed SD for accuracy and W7.prep by the critic's estimate (sections 2, 8 and 9).
3. **B3. The control's power ignored the residual luck bias, so outcome 3 was likely and unstated.** Done: the expected bias is registered for each quantity. It is EXP-0009's primary-minus-adjusted shift, scaled by the memory ratio at April's games and by 1/√3 for the window's dilution: for accuracy −1.53 at 20 games, −1.02 at 30 and −0.70 at 40. It enters the simulation, and the sizing step requires the control to hold 0.80 at 7 Elo per true SD after it. The review's three options were weighed:
   - (a) a two-month gap was simulated as design B. It removes about two thirds of the luck at 20 games, but with one outcome month a lasting link has half the Elo, and the control's power is lower (0.54 against 0.65), so it is not taken;
   - (b) reading the control on the adjusted model would not vouch for the keys' model under the same bias, so it decides nothing;
   - (c) the outcome probabilities are stated in section 0.

   The result: at the plan's 20 games in April the run would close unrun at the sizing step. It goes ahead at about 25 or more, and there, with no key truly linked, outcome 3 is about 0.29 (under 0.01 to 0.66 over a control of 10 to 5 Elo per true SD). If the reliabilities sit at EXP-0009's lower bounds, outcome 3 is about 0.73. The value gate is applied in section 0. A real link is found almost always, a false candidate comes up less than 2% of the time, and the worst case costs the streaming alone, so the experiment proceeds to stage 2 with the sizing step as its stop. The case for closing it now is set out in the lane report.
4. **B4. The kill rule's budget test was ambiguous, and good news could kill the run.** Done: the sizing step is a function, `power.py --sizing` (`decide_size`), tested on a fixture (`lab/tests/test_exp0029_sizing.py`). It tests the games against the 150% cap (5,550,000), and it projects CPU-hours and wall time from the measured streaming and EXP-0009's rates. Before it kills, it falls back to smaller per-window caps in a fixed order: (144, 120), then (96, 80), then (72, 60). It has no free parameter on the day (sections 0 and 6).

### Should-fix

1. **S1. Outcome 2's statement claimed more than the estimand.** Done: it now says "did not come before further climbing … once the rating had a month to catch up". It also says the catch-up already in April's rating reads zero, and that three keys are not covered (section 9).
2. **S2. The analysis code would have been written after the data was on disk.** Done: stage 2 writes `analysis.py` against a synthetic fixture, a critic reviews it, and its sha256 is committed in section 10, all before the first byte is streamed. The wall-time cap now runs from the sizing commit (sections 0, 7 and 10).
3. **S3. R_end of a window and R_start were undefined for many players.** Done: section 4 defines a month's and a window's end and start ratings, and what happens with fewer than five games in a month.
4. **S4. A mean-games ratio overstated the Spearman-Brown gain.** Done: k is the harmonic-mean ratio E_0009[1/m] / E_0029[1/m] of the capped game counts. EXP-0009's side comes from its own players files, whose checksums are in its `inputs.sha256` (section 6).
5. **S5. With no history month, new accounts entered in October.** Done: the September 2025 players file is streamed (about 30 GB and an hour, no games). The activity rule and the history covariates come from it, as July's did in EXP-0009. This adds one month to issue #131's range, still disjoint from EXP-0009's (sections 2, 7 and 8).
6. **S6. Selection on F2 activity is selection on the exposure window.** Done: the flow from F1 to F2 is reported by F1 level, rating and activity. Staying into F2 is modelled and enters the inverse-probability weights. The population is named as players active for six months (sections 1, 2, 7 and 8).
7. **S7. The minimum effect is not in EXP-0009's unit.** Done: "unchanged, so the follow-up asks the same question" is gone. Section 5 says the true change is now between three-month means, that Spearman-Brown assumes a fixed true-change variance, and which way that errs.
8. **S8. `summary.md` overclaimed the fix.** Done: it says the gap helps only for players active in April, states the run's condition on that, and gives the likely results.

### Minor

- M1 (Holm): the control is out of the keys' Holm family and is tested on its own. The simultaneous level is 1 − 0.05/8 (section 8).
- M2 (windows): O is May and June, read from the end of April (section 4).
- M3 (version): the sizing commit pins the algorithm id (section 3).
- M4 (planned edits): section 10 lists the only edits planned after stage 2's review.
- M5 (leakage): the run counts the games between two players of the slice as well (section 7).
- M6 (Docker share): if the share is lost, the run stops and the fix is recorded as a deviation; the cohort is never copied (section 0).
- M7 (holdout): the unused part of [0.3, 0.9) is said not to be a holdout (section 6).

### Round 2 of part one: the critic's re-review of 764ce29a, and the fixes

The same critic re-read the revised registration and re-ran `power.py` on a scratch copy, which reproduced the committed `power.json` byte for byte. Of the 19 round 1 findings, 15 were resolved and 4 partly (B3, B4, S4, S8). There were 6 new ones: 2 blocking, 3 should-fix, 1 minor. Its value-gate call was to close the experiment unless a free pre-check on EXP-0009's own players files showed a median of about 40 or more rapid games in October 2026. The coordinator ran that check (`dev/precheck.py`, on EXP-0009's own cohort, development data EXP-0009 had already read). It passes: of 17,891 players, the median is 118 and 82.8% played at least 40. Still, it measures EXP-0009's cohort in October, not EXP-0029's in April, and the April gate decides on the day. No month of EXP-0029 has been read.

- **N1 (blocking). The gate's result turned on φ 0.97 and on using the same game count in both terms of the memory ratio.** Done: the denominator is measured on EXP-0009's own players with an outcome (`dev/exp09_activity.py`: 16,206 players, median 121 October rapid games, mean share −0.87 at φ 0.97 and −0.81 at 0.98). The gate uses the worse of φ 0.97 and 0.98. At the plan the ratio is 0.17 (φ 0.98) and accuracy's bias −0.31 Elo per observed SD (section 4).
- **N2 (blocking). The smallest passing w ran at the edge of the target, and the quoted probabilities were for another w.** Done: the size is the largest w within the games, CPU-hour and wall-time caps (w = 0.6 at the plan), and the target is tested there. Every outcome probability in section 0 and `summary.md` is restated at that size, for the point and the lower-bound truth, biases of ×1, ×1.5 and ×2, and the control at 5, 7 and 10 (`power.json`, `simulation.outcomes_at_w`).
- **N3 (should-fix). The games projection and the fallback caps used the wrong k.** Done: for each cap, the sizing step measures E[min(m, read)] for the games and E[1/min(m, kept)] for k. The plan takes both from a stand-in window on EXP-0009's files (70.4, 62.7 and 55.7 games read; k = 2.64, 2.50 and 2.31).
- **N4 (should-fix). The gate's list and section 9's list could differ.** Done: the gate tests the five registered keys (`OUTCOME2_KEYS`), each needing 0.80 for "below" and 0.80 alone for "precedes climbing"; the list never changes.
- **N5 (should-fix). April's statistic was underspecified, and a median hides a convex tail.** Done: the gate reads the mean, over eligible players with an outcome (5 rapid games in April and in June), of each player's luck share φ^g_A (φ^(2 g_J) − 1), June standing in for the unstreamed May. That weights the low-games tail by its size.
- **N6 (minor). With unequal noise, q̄ does not separate the noise.** Done: a sensitivity weights q̄ by each window's inverse binomial variance. Section 2 now says the remaining leak is small and negative, not "not at all".
- **B3 (partly, now resolved).** The bias is at the worse φ, from measured activity, with ×1.5 and ×2 sensitivities. The gate closes only above a memory ratio of about 0.6 to 0.8, against 0.17 at the plan.
- **B4 (partly, now resolved).** The cost projection uses the measured games per cap (N3). The fallback order is fixed, and a smaller cap is tried only when no w fits the caps at the larger one.
- **S4 (partly, now resolved).** k and the games are each measured per cap (N3).
- **S8 (partly, now resolved).** `summary.md` gives the outcome probabilities at the size the rule chooses, including the lower-bound case.

What is not fixed, and why: the bias model itself (EXP-0009's shift × the memory ratio × 1/√3) is still an estimate from development data. Only its scale is varied, by ×1.5 and ×2, because no data before the run can check its form. The lower-bound-reliability case remains the main risk. There, outcome 3 is 0.82 to 0.93 whatever the bias, and no design change within nine months and the caps removes it: the five keys' reliabilities are what they are.

### Round 3 of part one: the critic's re-review of 012ca9a6 and 636f8c37

Verdict: ready after fixes, and the value gate's call is now to run. One should-fix and three minor findings, all fixed:

- **R3-1 (should-fix). The fallback caps could not fire when they were needed.** w = 0.1 always fit the first cap, so the fallback was reached only when nothing could fit. Done: for each cap in the fixed order, `decide_size` takes that cap's largest fitting w and tests the target there, and kills only when every cap fails. A fixture test covers a streaming overrun. At 20 streaming CPU-hours the first two caps fail at w = 0.2, and cap 72/60 passes at w = 0.3.
- **R3-2. The value score.** Impact is 2, not 3, because no outcome changes the product by itself: 2 × 3 ÷ 2 = 3.0 (section 0).
- **R3-3. `summary.md`.** "A tenth to a sixth of the luck (the check uses the worse)".
- **R3-4. The lower-bound case.** It is stated as a tail case, with the critic's interpolation (a quarter of the way to the point: outcome 3 about 0.30; halfway: about 0.06; three quarters: about 0.01). The real remaining risk is named as the form of the bias model and of Spearman-Brown.

## Stage 2a: the analysis code, before any month is streamed

Written by the lane with no EXP-0029 month read. A critic reviews it before the freeze.

- `analysis.py` implements sections 6 to 9 as registered. It imports EXP-0009's frozen `analysis.py` for the definitions that are the same: the evaluations, the game key, a month's measures and the split halves, the orientation, the bands and the start search. Its sha256 (EXP-0009's own section 10) is in `inputs.sha256`. The sizing step (`analysis.py sizing`) reads the players files only and calls `power.py`'s `decide_size`. The run reads the size it chose.
- Tests: `lab/tests/test_exp0029_analysis.py` runs on a synthetic cohort in the cohort command's file format (`lab/tests/exp0029_synthetic.py`), with `cli lab observe` replaced by a generator of known rates. The tests cover these cases:
  - an injected W7.traps effect reads outcome 1 on W7.traps alone;
  - a null at the fixed seed reads outcome 2 (see stage 2b: a null key reads "precedes climbing" up to 5% of the time, family-wise);
  - the control holds when accuracy is linked and fails when it is not, which gives outcome 3;
  - two runs give byte-identical output, and EXP-0009's range is excluded;
  - the budget cap stops the run;
  - the sizing step works with the games files deleted.

  The synthetic seed is 1. At seed 29 a null key (W5) read "precedes climbing" by chance (p 0.002 before Holm), and over seeds 1 to 6 no null key did.
- Clarifications made before any data, none a change of design. The registration said the flow into F2 is reported "by F1 level", and that staying into F2 is modelled "on the F1 terms". Players who leave before F2 are never observed, so their F1 level does not exist. The flow is therefore reported by F1 rating band and F1 activity tercile, and the staying-into-F2 model uses the players-file F1 terms (sections 2, 7 and 8). The inverse-probability weight is the product of the two staying probabilities, each floored at 0.05. The analysis writes no figure.
- `lichess-db cohort`: no Rust change. The history month and the outcome months are players files only, which the command writes without `--games`. `--hash-range` is not built. `--player-rate 0.9` with [0, 0.3) dropped on reading needs about 59 GB, inside the 70 GB cap. No image on this host has the `cohort` subcommand yet (the stack's `rust-worker` image predates it), so the streaming needs a `rust-worker` image built from this branch.

## Stage 2b: the critic's review of stage 2a (f1eaec51), and the fixes

The verdict was "fix first", with nothing blocking. All of the following are fixed before the sha is frozen. No EXP-0029 month has been read and nothing has been streamed.

- **S2a-1 (should-fix). The pinned id was not enforced.** Fixed: `cmd_run` defaults the required id to the one the sizing step recorded (`sizing.algorithm_id`), and `run` refuses a cli that reports another, with a test. Section 3 pins `algo-6.0.0+f40a2b27677a`, because `exp/0030-memory` (algo-6.0.0) is being merged to main today and the lab image is rebuilt from main after that. The difference from algo-5.0.0 is the trajectory only (`coaching-input/trajectory.rs`, `profile/coaching/trajectory.rs`), not any function section 3 lists. `summary.md` names the same id.
- **S2a-2. Holm's m depended on which keys could be estimated.** Fixed: `read_family` gives a key that cannot be estimated p = 1, so Holm always runs over eight. Tested.
- **S2a-3. Rank deficiency was not flagged.** Fixed: each fit records `rank_deficient`. The synthetic opponents now have ratings that vary (SD 100), so the opponent-gap families are tested on a full-rank design, which a test asserts.
- **S2a-4. The sizing's density counted players before attrition.** Fixed: `density` now counts players with an outcome against EXP-0009's 16,204 / 0.3, because `power.py` scales EXP-0009's n with an outcome. `density_observed` (before the outcome rule, against 17,138 / 0.3) projects the games. The plan in `power.json` is unchanged.
- **S2a-5 (should-fix). "A null never reads outcome 1" is not a property of the rule.** Fixed in the wording: Holm caps at 5% the family-wise chance that a key with no link reads "precedes climbing". The test docstrings, `read_family`, `review.md` (stage 2a) and section 9 now say so. `decide` reports each candidate with its raw and Holm p, and names the follow-up as replication on new months. A new test runs 20 null cohorts of 400 players and asserts that the number with any false candidate stays within the binomial 99.9% bound for a 5% rate. It costs about two minutes in CI. It catches only gross inflation: the bound allows up to 5 of 20, so a true family-wise rate of 0.10 fails it with probability about 0.01 and one of 0.30 about 0.58. The 5% guarantee rests on Holm itself and on the direct Holm test of `read_family`, not on this check (S2b-2).
- **S2a-6 (should-fix). The exclusion test was too weak.** Fixed: the test asserts `flow.history == 900` exactly (100 of 1,000 synthetic players are in [0, 0.3)) and checks every analysed and F1-eligible pseudonym's hash fraction directly.
- **S2a-7. There were no direct tests of the rule.** Fixed: the null at the fixed seed asserts outcome 2 with the five keys below the minimum. Hand-built tests of `read_family` and `decide` cover the following cases:
  - Holm rejects p 0.002 among eight (Holm p 0.016) and does not reject 0.01;
  - a key that cannot be estimated still counts in the eight;
  - one of the five keys undetermined gives outcome 3;
  - W4.missed "precedes falling", with the five below and the control holding, still gives outcome 2;
  - a key that climbs only in the F1-level model is reported and never a candidate;
  - no control gives outcome 3.
- **The streaming plan.** Done: `stream.sh` (POSIX sh, shellcheck clean) makes the critic's four changes:
  1. the CPU is read inside each container from `/sys/fs/cgroup/cpu.stat` (`usage_usec`) into `stream-cpu.tsv`, and the sum is the streaming CPU-hours;
  2. the months stream one at a time; a 60 s watcher stops the container if E: falls below 50 GB free; the df guard must parse to a number or the script exits 2;
  3. every exit status is checked: on failure the month's `*.part` files are deleted, logged, and the stream stops; a month whose folder exists is never resumed;
  4. the image digest and the image cli's algorithm id are logged.

  `--dry-run` runs the guards and prints the nine commands. `lab/tests/test_exp0029_stream.py` tests it with a stand-in for docker: the dry run, an unparsable df, the CPU sum, a failed month and a refused resume. The `ci-exp29-rust-worker` image is not built yet.

## Stage 2c: the critic's review of stage 2b (4987e2c7), and the fixes

The verdict was to freeze `analysis.py` at 0547be05 as is and to fix `stream.sh` first. `analysis.py` is not touched. S2b-1 (`cmd_sizing` does not compare the lab cli's id with section 3's) is left to the coordinator, who checks it by hand at the sizing step. The new lab image `rematch-lab:exp29`, built from this branch, reports `algo-6.0.0+f40a2b27677a`. The branch now includes main's merge of algo-6.0.0 (aaad8b88). No EXP-0029 month has been read and nothing has been streamed.

- **ST-1. There was no way to carry on after a failure.** Fixed: a month whose `meta.json` has EXP-0009's salt and the 0.9 rate, whose data files are there (`players.jsonl`, plus `games.jsonl` for a games month) and which has a row in `stream-cpu.tsv` is complete and is skipped, with a log line. Any other existing folder is a partial month and is refused; it is deleted by hand and the re-stream recorded. The tsv header is written only when the file is new; otherwise rows are appended. Tested: a stream that failed at 2025-11 resumes from 2025-11, and the tsv holds the nine months once.
- **ST-2. Nothing checked the built image before month 1.** Fixed: in both dry and real mode, the preflight runs `sh` in the image, reads `usage_usec` from `/sys/fs/cgroup/cpu.stat` and refuses to stream unless it parses. It also checks that the cohort command is there. Tested with an image without cpu.stat. The dry run against the built image (`ci-exp29-rust-worker`, sha256:5208c69f…) passes every preflight.
- **ST-3. There was no trap.** Fixed: on INT, TERM or HUP the script stops the container, kills the client and the watcher, removes the month's folder, logs the stop and exits 130. `docker run` runs in the background with `wait`, so the trap fires at once. Tested with SIGTERM during a month.
- **ST-4. On failure, only `*.part` was removed.** Fixed: the whole month folder is removed. A month without `usage_usec` is removed too.
- **ST-5. The cli id parse was greedy.** Fixed: the id is the top-level `id` of `cli algo show --json`, parsed with python (the image has none, so the host's python reads the output). It is checked once against the registered `algo-6.0.0+f40a2b27677a`, and a mismatch stops a real run. Tested with nested JSON and with another id.
- **ST-6. The 70 GB cap was not enforced.** Fixed: before each month, the size of `data/exp29-cohort` (`du`) plus the month's reserve must stay within 70 GB, and the watcher stops the container if the folder passes 70 GB or E: falls below 50 GB free. Tested with a 1 GB cap.
- **S2b-2. The binomial test's wording.** Fixed above (stage 2b, S2a-5).
- `stream.sh` is shellcheck clean (`koalaman/shellcheck:stable`, `-s sh`). Its helper is named `month_complete`, because `complete` is a bash builtin.
