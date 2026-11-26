# EXP-0015: review

## Part one: the pre-registration, before the run

A fresh critic (an agent with the `scientific-critical-thinking` and `peer-review` skills, no part in the design) read `preregistration.md`, `analysis.py`, `power.py` and the production rule (`trajectory.rs`) on 2026-11-26, before anything ran on the registered seeds. It also ran its own rough approximation, an Elo world with this design's opponents and a normal stand-in for the bootstrap, not the cli: the share told "falling" came out 0.37 to 0.82 across K = 12 to 28 and the three lengths, and false "rising" at 60 games about 0.42. The bound and the hide line were not moved towards or away from that; it changed the value case and what the run carries. Each concern, and what was done:

1. **The outcome is close to foregone, and the value case said uncertainty 3.** Section 0 now scores uncertainty 2 (value 6) and says why the run is still worth its 3 CPU-minutes: the rate itself is what we publish, which mechanism carries it chooses the challenger, and the arena needs the champion's baseline on registered seeds. Section 2 states the expected range and its source.
2. **The hide line's argument gave 0.20, not 0.30.** With a share b of players truly declining and detection d, false headlines outnumber true ones past b × d / (1 − b), 0.20 at b = 0.2 and d = 0.8. The hide line is now 0.20 (section 5, `HIDE`), with the formula and its sensitivity to b; the bound of 0.15 is labelled a convention (EXP-0002's tolerance), not a cost.
3. **False "rising" was exploratory, so a hide would leave the same verdict live the other way.** False "rising" is now a confirmatory secondary read like the primary, and a hide filing covers `rising`, "Better" and the rating row's words when it is above the hide line (section 9).
4. **At 60 games "then" is always the first 30 games.** True: the clamp leaves no room. The peak game is still inside the window whenever the basis is `peak` (it lies in the first 30), so the "peaked at" copy is not wrong, and no issue is filed. The point is used instead: the 60-game cell carries the bootstrap's independence assumption alone, and a no-peak read (the production rule on the first and last 30 games of the 120- and 250-game series) separates what the peak adds (section 2).
5. **The estimand was not stated.** Section 1 now says it is the player's strength, and why the product reads the verdict that way (ADR 0089 §7, the coach's "likeliest place to look").
6. **The Glicko-2 grew the deviation by a full rating period a game.** lila grows it by elapsed periods, `RatingPeriodsPerDay(0.21436)` (`modules/rating/src/main/Glicko.scala`, which also has `minDeviation = 45`, checked); at two games a day that is 0.107 a game, now in `glicko2` (`PERIODS_PER_GAME`). The deviation and the mean step a game are reported, and the Elo arms run at all three lengths.
7. **Transfer to chess.com.** An exploratory chess.com-like Glicko-1 arm is added, with the stored rating taken after the game and `chesscom: true`, so the loader derives the pre-game rating as production does; its constants are assumptions. The accuracy and blunders parameters are labelled assumptions with no source, and section 7 states the transfer limits (no streaks or breaks, which would add memory).
8. **The power for "keep" holds only at exactly 0.10.** The grid now runs from 0.05 to 0.85 with 0.12 to 0.14 in it; section 6 says 200 players would decide in the expected regime and that the 700 are for the regime a challenger is measured in, and that a rule at 0.12 to 0.15 will read "need more data", which is intended.
9. **The burn-in check was computed as an absolute error, not the registered signed one, and not for the declines.** `settle()` now reports the signed mean error at the first observed game, the absolute one, and the 25-point flag, for the settled, decline and other-rating populations.
10. **The rule's calibration was not recorded.** Each trajectory now keeps the half-width of its 80% interval of the rating difference, and the diagnostics compare its mean with 1.28 times the SD of the difference across constant-strength players, per length.
11. **The accuracy and blunders secondaries tested a null the primary calls acceptable, with no action.** They are now read by the primary's bound and hide line (Bonferroni across six), and a hide filing covers their rows when above the line; the binomial tests are gone.
12. **The account id changed between checkpoints.** The player id no longer carries the length, so a player has one account id and one production seed at every checkpoint, as in production.
13. **The arena rule could not tell a fix from a loss.** "Better" now needs all three primary cells better; shipping still needs the challenger's own cells within the bound. The detection guard is an absolute floor, worse when the upper end of the challenger's own 80% interval at a 200-point decline lies below 0.50, since the champion's detection is inflated by its false alarms; the paired change is reported.
14. **A budget abort wrote nothing.** It now writes a `results.json` that says it failed, with its `compute`.
15. **No baseline rate in the power inputs.** Section 6 now gives the expected regime and its source.

The critic found the Glicko-2 iteration, the measured event (`kind == "falling"` is `rating.verdict == Worse`), the request construction and the absence of pseudoreplication sound. It did not recommend killing the experiment.

## Part two: the result, after the run

A second fresh critic (the same skills) read the frozen registration, `analysis.py`, `arena.py`, `results.json` and the figure on 2026-11-26, after both runs, and recomputed every primary interval from the counts (they match to six decimals). It found no deviation in what `analysis.py` computed, and the decision is outcome 1 by section 9 with no room to hedge. Its points, and what `decision.md` does with them:

1. **State the rule mechanically, with the lower ends.** Done: 0.40, 0.58 and 0.75 against 0.20.
2. **Say exactly what the hide covers.** `falling` and `rising`, the rating row's "Worse" and "Better", and the coach's `kind`; false "rising" at 250 games is undecided, not fine; the accuracy and blunders rows keep their verdicts, the 60-game blunders cell by 0.0001.
3. **The accuracy and blunders result is a property of the assumed world.** Stated: their simulated values have no memory beyond the result; the rows work as an internal control of the mechanism, not as evidence about real players.
4. **The mechanism split is approximate.** Worded as such: memory alone gives about 0.45 at every length; the peak makes it one-sided and adds about 0.2 to 0.3; both must be fixed.
5. **Width and bias are separate failures.** Stated, with the difference's off-centre means at 120 and 250 games.
6. **The world diagnostics.** Cited in one paragraph; the deviation pinned at the floor means the world never exercises breaks.
7. **Detection must not be published without the false-alarm rate beside it.** Done: one table with both, the likelihood ratios and the share of false headlines; no claim that detection improves with games.
8. **The hide line against the measured detection.** A robustness sentence: the decision holds for a declining share up to about 0.28 at every length.
9. **The rate rises with history, and "peaked at … (−N)" is negative for a stable player.** Stated, and named for EXP-0030.
10. **The split by basis is not a mechanism.** Stated as descriptive.
11. **New accounts.** One line; no rate is claimed.
12. **The Elo and Glicko-1 arms.** Robust within this world, not evidence about chess.com; Glicko-1's 60-game asymmetry is noted to be checked in the loader.
13. **Record the sha, the image and the determinism.** In `decision.md`.
14. **The limits.** A section of their own.
15. **The arena rule's gaps** (no "rising" flags, pooled detection, a lax floor, no deviation per game). Listed in `decision.md` and given to EXP-0030's registration; `arena.py` follows section 9 as registered and is not changed.
16. **The challenger.** EXP-0030's Next item takes every design point and pitfall.
17. **The figure.** Its unlabelled lines and missing intervals are noted; the frozen `analysis.py` is not changed for it.
