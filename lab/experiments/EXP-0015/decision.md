# EXP-0015: decision

- Decision: hide the verdict until fixed (section 9, outcome 1). Every primary cell is above the hide line: the Bonferroni lower ends are 0.40, 0.58 and 0.75 against 0.20. Players whose strength never changed are told their rating is falling 44%, 62% and 78% of the time at 60, 120 and 250 rated games. The filing covers `falling` and `rising`, the rating row's "Worse" and "Better", and the coach's `kind`; the accuracy and blunders rows keep their verdicts. The production change is its own lane, and the challenger is EXP-0030. Nothing in the algorithm changes here.
- Algorithm: `algo-4.0.1+fcda2762eeeb`. Results: `results.json` (commit `0af39f23`), the figure in `figures/falling.png`; `analysis.py` sha256 `97f34774eee7211a68bd285318a9ba22754d0fc1318dd0d2734355c2fee59518`, as section 10 states; lab image `rematch-lab:labbridge` (`sha256:b75c7f2f2b43…`), whose cli is main's at `ef0789b9`.
- Compute: 1,322,000 simulated games and 8,100 trajectories through `cli lab trajectory`, exactly the registered budget; 98 seconds a run on one core of the lab container (about 0.03 CPU-hours). Run twice: the two `results.json` and the two figures were byte-identical. No engine, no database, no AI tokens.

## The primary

The share of 700 settled constant-strength players (Glicko-2 as on Lichess) whose trajectory is `falling`, with Wilson intervals (Bonferroni: 1 − 0.05/3):

| Rated games | Falling | 95% | Bonferroni | Read |
|---|---|---|---|---|
| 60 | 310 of 700, 0.443 | [0.406, 0.480] | [0.399, 0.488] | above the hide line |
| 120 | 435 of 700, 0.621 | [0.585, 0.657] | [0.577, 0.664] | above the hide line |
| 250 | 549 of 700, 0.784 | [0.752, 0.813] | [0.745, 0.819] | above the hide line |

The rule promises at most 10% at an unchanged strength; it gives four to eight times that, and the longer the history the worse it gets. The `change` shown with a false `falling` averages −39, −50 and −59 points, all of it noise: "peaked at … (−N)" compares a maximum with one draw, so it is negative for a stable player whatever the verdict.

The hide line assumed a fifth of active players truly declining and 80% of them caught. With the detection this run measured (0.735 to 1.0), the break-even false-alarm rate at that share is 0.18 to 0.25, and every lower end is above it; false headlines stop outnumbering true ones only if more than about 28% of players are declining at 60 games, 37% at 120 and 43% at 250. The decision does not hinge on the assumed share.

## Why

The rule's 80% interval of the rating difference has a mean half-width of 3.7 points at every length. Across constant-strength players the difference has an SD of 21 points at 60 games, 34 at 120 and 37 at 250, so an interval that held 80% would be 26 to 47 points each side: the rule's is six to thirteen times too narrow. The world shows why: a settled rating moves about 5 points a game, its error around the true strength has an SD of 31 points and a game-to-game correlation of 0.97, and the mean error over 30 games still has an SD of 29: a window of 30 ratings holds about one independent game's worth of information, and the bootstrap counts 30. At 120 and 250 games the difference is also off-centre (a mean of −9 and −23 points), the peak's selection, so width and bias are two separate failures.

What carries it, approximately and from exploratory reads:

- The bootstrap's independence assumption alone gives about 0.45 false "falling" at every length. At 60 games the windows are fixed halves and false "falling" (0.443) and false "rising" (0.439) are equal; the no-peak read, the production rule on the first and last 30 games of the same series, gives 0.446 [0.409, 0.483] falling and 0.450 rising at 120 games, and 0.449 [0.412, 0.486] and 0.490 at 250. About nine stable players in ten get a verdict either way.
- Peak selection makes it one-sided: at 120 and 250 games it adds about 0.2 to 0.3 to "falling" and takes it from "rising". The no-peak windows sit a fixed distance apart and the primary's do not, so this is not a clean decomposition.
- Both must be fixed: memory alone is already more than twice the hide line, so a rule that only drops the peak still fails.

The split by basis is descriptive: the basis is chosen by the same ratings the verdict compares (at 60 games the windows are the same either way; "start" means the maximum fell in the last window).

## The confirmatory secondaries

- False "rising" (Bonferroni, 3): 0.439 [0.394, 0.484] at 60 games and 0.300 [0.260, 0.343] at 120, above the hide line; 0.171 [0.140, 0.208] at 250, undecided. The hide covers `rising` and "Better" (section 9).
- Accuracy and blunders per game "worse" (Bonferroni, 6): accuracy 0.113, 0.080 and 0.110, blunders 0.114, 0.087 and 0.086, every cell within the bound; the 60-game blunders cell clears by 0.0001 (upper end 0.14988). These rows keep their verdicts. They are within the bound only because the simulated accuracy and blunders have no memory beyond each game's result, and their parameters are assumptions; it is evidence about the mechanism (the same bootstrap is about right on values that are independent game to game), not about real players, whose accuracy has form. A check of the lag-1 correlation of accuracy on a real series comes before anyone relies on these rows.
- Detection, always beside the false-alarm rate at the same length (200 players a cell; Wilson 95%):

| Rated games | False alarm | 100-point fall | 200-point fall |
|---|---|---|---|
| 60 | 0.443 | 0.735 [0.670, 0.791] | 0.935 [0.892, 0.962] |
| 120 | 0.621 | 0.945 [0.904, 0.969] | 1.000 [0.981, 1.000] |
| 250 | 0.784 | 0.980 [0.950, 0.992] | 1.000 [0.981, 1.000] |

  The detection is inflated by the false alarms: a rule that says `falling` to most stable players says it to nearly every falling one. The likelihood ratios are 1.25 to 2.1, and if a fifth of players were truly declining, about three in four "falling" headlines would be false. The cells' declines are spread over different numbers of games, so "detection improves with games" is not a valid reading.

## Exploratory

- At 30 games (W = 15): 0.411 falling and 0.417 rising.
- The score row's "worse" is 0.05 to 0.10 at every length; any of the rating, accuracy, blunders and score rows "worse": 0.60 at 60 games, 0.69 at 120, 0.83 at 250.
- New accounts (no burn-in, starting at 1500), 300 players, uncorrected: 0.43, 0.53 and 0.60 falling. Below a true 1500 the rating really falls while it settles (0.58 falling at 60 games, against 0.26 at or above 1500), so these are false only about strength; no new-account rate is claimed.
- Other ratings in the same simulated world (Wilson 95%): Elo K = 10 0.42, 0.58, 0.77; Elo K = 32 0.415, 0.65, 0.785; a chess.com-like Glicko-1 with the stored rating after the game 0.365 [0.301, 0.434], 0.625 [0.556, 0.689], 0.815 [0.755, 0.863]. The answer is robust to the rating system's step within this world; it is not evidence that it holds on chess.com. Glicko-1 at 60 games gives more "rising" (0.495) than "falling" (0.365), where peak selection cannot act; it may be noise or the loader's pre-game derivation under `chesscom: true`, and is to be checked there before any chess.com claim.
- The burn-in settled in every population: the signed mean error at the first observed game is −2.5 points for the settled players, within ±3.1 for the declines and −3.9 to +4.5 for the other arms (the line was 25). The settled deviation sits at lila's floor of 45 throughout.

## Deviations

None from sections 0 to 10. The fixes the pre-run critic asked for were made before the run, in their own commit (`review.md`, part one). The figure's bound and hide lines are unlabelled and its detection lines carry no intervals; the table above has them.

## What this does not show

- Real players. The world is simulated: Glicko-2 with lila's constants, two games a day with no breaks, streaks or tilt, opponents paired at Normal(rating, 75), and a deviation pinned at its floor. Breaks and streaks add movement and memory to a rating, so the real rate is likely higher, not lower; that is an argument, not a measurement.
- chess.com: the Glicko-1 arm's constants are assumed, and its 60-game asymmetry is unexplained.
- Real accuracy and blunders: their rows' result rests on memoryless, unsourced assumptions.
- The share of real players who are declining (assumed 0.2; the decision holds up to about 0.28 at every length).
- Any version but `algo-4.0.1`, and the weakness rows of then against now (not sent).

## The arena rule's limits

`arena.py` follows section 9: the paired Newcombe interval at 1 − 0.05/3 per cell, "better" only when all three are, and the detection floor on the challenger's own pooled 200-point rate. Passing it does not ship a challenger: shipping needs its own registered experiment, with its cells within the bound. It compares false "falling" only (`results.json` keeps no per-player "rising" flags), it pools detection across lengths, the floor admits a challenger whose upper end reaches 0.50, and the requests carry no rating deviation per game. EXP-0030's registration closes these.

## Next

1. **EXP-0030: a trajectory verdict that counts the rating's memory.** The challenger, in its own lane. It must fix both failures together: an interval that respects the series' memory (a block bootstrap over games, an effective-n correction, or the rating's own deviation) and no selected peak (fixed halves, or a slope with an autocorrelation-robust error); one without the other fails here. Candidates are developed on an `EXP-0015-dev` namespace and judged on a fresh registered one, never tuned on `EXP-0015`; no threshold is copied from this world's difference SDs (21 to 37 points); false "rising" is a co-primary with its per-player flags kept; detection is guarded per length and reported with the false-alarm rate beside it; and a world arm with breaks and streaks, and a check of a real series' autocorrelation, come with it. The "peaked at … (−N)" copy is reconsidered whatever the rule.
2. **Hide the rating verdict.** Issue: "Hide the trajectory's rating verdict until a rule clears EXP-0015". The Progress and Overview headlines state the peak, the current rating and the change as facts, the rating row shows no "Worse" or "Better", and the coach's input withholds `kind`; the accuracy and blunders rows keep theirs. A major algorithm version in its own lane.
