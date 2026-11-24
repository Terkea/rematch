# 0110. Prior-rule variants behind `ALGO_VARIANT`, and the fix for the 80% ranges

Date: 2026-11-24. Status: accepted (lane `exp56`, branch `exp/0005-0006`). Builds on ADRs 0100, 0101, 0104, 0105 and 0106. Experiments EXP-0005 (#100) and EXP-0006 (#101), for #13.

## Context

EXP-0002 found the 80% ranges of the weakness rates miscalibrated for players well above their peers: 12 of 20 key × sample-size coverages outside [0.75, 0.85], down to 0.10 for W4.missed at 30 games (`lab/experiments/EXP-0002/decision.md`). Two causes were named: the band prior's strength `n0` is counted in chances, so for keys with about one chance a game it outweighs the player's first 40 games; and the recency weighting treats weighted sums as real counts and discounts the data but not the prior. EXP-0006 compares a fix with production, and EXP-0005 measures, on the same run, how much a real candidate moves the Tier 1 rankings. Both need two versions of the estimate on the same data. The evaluation spec (section 5) names the mechanism, `ALGO_VARIANT=<name>`, but nothing implemented it.

## Decision

1. **The prior rule is a value, `profile_core::stats::variant::PriorRule`.** Three options: `prior_games` (the band prior is worth at most that many of the player's own games, counted as the data are), `effective_counts` (the weighted rate over Kish's effective sample size, `(Σ w n)² / Σ w² n`), and `dispersion` (that divided by the Pearson dispersion of the per-game counts, clamped to [1, 10]). `PriorRule::CHAMPION` is production; `PriorRule::CHALLENGER` is EXP-0006's frozen candidate; `dev:<options>` names a development rule. `estimate_weakness_with` takes a rule explicitly; `estimate_weakness` uses the process's.
2. **Only tools select one.** A process-wide rule, `variant::set`, is called by `cli lab` (every estimate of the call) and by the bench harness, never by the worker or the web. `cli` reads `ALGO_VARIANT`; an unknown name stops it before anything runs. With the variable unset every output is what it was, which the replay goldens prove.
3. **The bench runs a variant side by side.** With `ALGO_VARIANT`, `cli bench run` inserts every player's games into a second account and builds that account's profiles under the variant, checkpoint by checkpoint and for the perturbed account too, so both versions see the same games for the engine time of one. `bench-results.json` gains each player's `variant` checkpoints and the run's `variant` scorecard. `backend/crates/bench/tests/variant.rs` shows the champion run as the variant reproduces the main run exactly; the rule is process-wide, so that test has its own binary.
4. **The lab passes it through.** `rematch_lab.profile`, `rank` and `bench` take `variant=`; a call without one clears any `ALGO_VARIANT` in the lab's environment.
5. **Production's rule is EXP-0006's challenger** (`algo-2.0.0`): the band prior worth at most two of the player's own games, and the weighted data at their effective sample size divided by their dispersion. EXP-0006 decided "ship" on fresh seeds: all 20 of EXP-0002's coverages inside [0.75, 0.85] (worst 0.828, against the old rule's 0.103), the coverage at the peers' rate 0.874 to 0.804, false weaknesses for players at or below their peers' rate up by 0.009 a credible estimate (margin 0.01), Tier 1 recall at 2 unchanged and the smoke CI rule clean (`lab/experiments/EXP-0006/decision.md`). The old rule stays selectable as `algo-1`.
6. **EXP-0005 was closed unrun** by the value gate: with 28 reference hits no branch of its rule could change the suites. EXP-0006's run measured the churn of this prior change descriptively (none of 28 hits lost at 80 games, 0 [0, 0.085]), a lower bound for detector and ranking changes (EXP-0024).

## Options considered

1. **Two builds, one per version.** Rejected: the bench would generate every game twice (twice the engine time), and a profile-only comparison would need two lab images.
2. **A rule parameter on every function down from `build-profile`.** Rejected: it threads a lab concern through the production handlers. A process-wide rule that only tools set keeps the handlers as they are.
3. **Read `ALGO_VARIANT` inside `profile-core`.** Rejected: an environment variable on a production worker would then silently change its output.

## Consequences

- The variant code is under `profile-core`, so the manifest's `profile.source_digest` moved; `algo-1.1.1` was a patch with byte-identical goldens, and shipping the challenger is the major `algo-2.0.0` (`algorithm/CHANGELOG.md`).
- A comparison of two prior rules costs one bench run, and a profile-only one costs nothing beyond the profile calls.
- On synthetic players drawn from EXP-0002's model, players well above their peers now see ranges that hold their rate about four times in five; the share of W4.missed ranges at 30 games lying wholly below the truth falls from 0.90 to 0.13. The ranges still lean towards the peers (the truth lies above them in 0.13 to 0.17 of intervals at 30 games, against 0.10 for an equal-tailed range). The generator's peer rates and game-to-game spread are assumptions; real players' ranges are Tier 2's to measure.
- The cost is measured on the same model: credible estimates for players at their peers' rate rise by 0.009 (one in about 110), with 0.0007 to spare on the registered margin, most for W4.missed and W7.traps; on the bench, one more of 23 clean players shows a rank-1 weakness at 80 games, which is consistent with a rise and does not measure it. EXP-0013 carries the false-weakness guard from here.
- #13's own case, W1 against peers at 0.0008, is covered 0.766 [0.738, 0.791] at 30 games, below 0.80 and not shown inside the band; #13 stays open for it (EXP-0025).
- Whether real players' game-to-game spread matches the generator's design effect of 1.5 is not measured; Tier 2 is where it would be.
