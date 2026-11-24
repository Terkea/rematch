# Review of the pre-registration (part one)

Reviewer: a fresh agent (Claude) with no part in writing the registration, using the vendored `scientific-critical-thinking` and `peer-review` skills, on 2026-11-24, against commit a57973a9. It read `docs/evaluation.md`, the template, ADRs 0106, 0107 and 0110 (then numbered 0109), `lab/experiments/README.md`, this folder with `dev/` and its git history, EXP-0002's registration, decision and `analysis.py`, EXP-0005's folder, and the variant, estimate and harness code. Nothing was run in Docker; its calculations were plain host Python.

What held up: the selection rule and both margins were committed in badb094a, before the development results in 784bbf85, and between those commits `dev.py` changed only its parallelism; the confirmatory seeds (`EXP-0006/…`) are disjoint from the development ones (`EXP-0006-dev/…`), as `seed()` hashes the whole string; the challenger sits under `profile-core/src`, so the manifest pins it; players are resampled whole and both rules are paired on the same resamples.

## Concerns and what was done

1. The primary cannot fail, so the guards alone decide the ship (major). ΔD was expected near 0.64 against a minimum effect of 0.05; ten of the fifteen development rules would pass it.
   - Done (the review's option a): the primary is now the challenger's own calibration, D(challenger), and a ship needs its 97.5% interval wholly at or below 0.07 (the band's half-width plus EXP-0002's Bonferroni resolution, section 5). ΔD > 0 at the 97.5% level stays as a condition. H0 and H1 are restated on D(challenger).
2. The power analysis did not simulate the registered rule, and the guards make the ship power much lower than stated (major).
   - Done: `power.py` runs the registered profile design on its own namespace (`EXP-0006-power`, disjoint from the development and confirmatory seeds) and applies the whole profile rule in each of 2,000 paired bootstrap replicates; the Tier 1 guards are simulated at no true change. At 200 players a cell the false-weakness guard had about even odds (the review's estimate), so the ρ ≤ 1 cells are now 800 players a cell. The pilot (`dev/power.json`): the profile rule ships in 0.87 of the replicates, recall at 2 holds in 0.88 of the simulations; the whole rule's power is about 0.69, accepted in writing in section 6, with the follow-up named.
3. The clean false-alarm guard was a coin flip on three players, and a single event rejected the change (blocker).
   - Done: the count guard is gone. 20 new clean controls (1,600 games) join the side-by-side run, and the clean alarm is measured at player level on 23 players with a Newcombe interval; the power pilot showed a guard on 23 players would hold only 0.49 of the time at no true change (0.81 at 60, 0.88 at 80), so it is reported, not gated, and the false-weakness guard on 9,600 profile players carries the decision. The smoke suite's own CI rule (`bench::results::regressions`, with its tolerances, against the champion's smoke scorecard from the same run) is the Tier 1 gate the owner asked for; its failure blocks a ship but never rejects alone.
4. The margins had no reason in player terms, and the recall margin equals the effect EXP-0001 says matters (major for the false-credible margin, minor for the others).
   - Done: section 5 gives each margin a sentence in a player's terms (the false-credible margin as at most about one more false weakness in 20 reports for a player at the peers' rate, the price of an honest range), and calls the recall guard a tripwire for about three net losses in 45, not a test of non-inferiority at 0.1.
5. EXP-0005's results would be visible while this registration could still be edited (major).
   - Done: EXP-0005 is closed unrun and this experiment owns the Tier 1 run, so no Tier 1 number exists before this run. `analysis.py` holds the sha256 of sections 0 to 9 as they stand after these fixes (`PREREG_SHA`) and stops if they change.
6. EXP-0005's integrity was not checked before its numbers were used (major).
   - Done: the Tier 1 run is now this experiment's, and `analysis.py` stops unless every player's variant is `challenger`, both sides have the same unweighted counts at every checkpoint, at least one interval differs (a positive control), both sides are deterministic, and the pilot's reference side equals EXP-0001's stored rows field by field.
7. "Fully fixed" for #13 hinged on W5, a key #13 is not about (major, for the #13 decision).
   - Done: #13 is fixed when W1's four coverages are within the band, the #13 cell (W1, peers at 0.0008, true rate 0.023, now 1,600 players) is within the band at every checkpoint, and #13's smoke acceptance holds. Whether all 20 cells are within the band, W5 at 30 games included, is reported.
8. Part of the improvement may be widening the generator rewards, and only one schedule runs (minor).
   - Done: the dispersion clamp's widening is named in section 2; the ρ = 1 coverage and the false-credible share per key, the ρ = 0.5 coverage and the median width per cell are reported. The compressed schedule is not run: under it the effective counts change nothing, and EXP-0002 measured its contrast.
9. A follow-up could not reduce the Tier 1 uncertainty as written (minor).
   - Done: section 9 names the follow-up's new data: fresh profile seeds (`EXP-0006-followup`) and the full suite's players side by side.
10. Description drift (minor).
   - Done: section 2 restated; the Tier 1 population is the pilot, the smoke suite and 20 clean controls, not EXP-0002's "Next" (the full suite), because the pilot's reference side can be checked against EXP-0001's stored results and the full suite costs 8,960 games more for guards whose power the pilot already sets.

The fixes are in the commit after this review; the critic did not re-read them.
