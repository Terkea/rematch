# EXP-0008: decision

- Decision (section 9, rule 1): **300,000 falls short; the setting is 1 million nodes**, with one thread and 16 MB of hash, as measured. Against the 30-million-node reference, 1 million nodes disagrees on 0.0246 of moves fewer than 300,000 does, 95% [0.0197, 0.0296] (80% [0.0214, 0.0279]), wholly above the tolerance of 0.01; against the 100-million-node reference on the seeded 40 games, 0.0221 [0.0115, 0.0327] (80% [0.0153, 0.0289]), also wholly above it. We ship it as a major algorithm version, in its own pull request (Next, item 1).
- Algorithm: `algo-3.0.0+554036a3b27c`. Results: `results.json` (commit 1cf0520b), two runs byte-identical (sha256 `9fe1f487b9b2c291cc45593eef6bc6b4ff1c92a1ae7d78f85f8b1c7cb71379e6`). The second run read the cache, so that shows the analysis is deterministic; the engine's determinism rests on the 72 re-searched positions (12 a setting), every one identical to its cached search. Figure: `figures/contrasts.png` (the exact contrasts, from `results.json`); `figures/disagreement.png` is the exploratory absolute curve.
- Ground truth: **Stockfish 19** at 30 million nodes with the Syzygy 3-4-5 tables (all 290 files checked), on 160 rated rapid Lichess games of October 2026 (CC0, many thanks to Lichess and its players), 10,596 moves, 9,997 positions. It is a deeper search, not the truth: on the 192 moves where 300,000 and 1 million disagree in the 40 checked games (the moves that decide the contrast, not a sample of all moves), 12.0% of the reference's labels changed again between 30 and 100 million nodes. On those same 40 games the step up is 0.0252 against 30 million and 0.0221 against 100 million, 12% smaller, with a lower end 0.0015 above the tolerance: the claim is that 1 million agrees more with deeper **Stockfish 19** searches, not that its labels are correct. The scope is Lichess rapid games with a server analysis, rated 800 to 2199; 1 million still disagrees with the 30-million-node reference on about 10% of moves.
- Compute (section 11): 14.5 billion candidate nodes (97% of the budget), 53.9 billion reference nodes (115%: 1,797 positions against a planned 1,536), 33.6 billion branch nodes (129%: the step up's check took 336 positions at 100 million against a planned 258) and 1.6 billion recheck nodes (99%); every unit under the 150% cap. About 19.5 CPU-hours and 133 minutes on 10 engines from a cold cache (the plan was 11.6 and a 5.5 branch); the second run read the cache and took 4 minutes (about 0.35 CPU-hours, the re-searches). The pilot took about 6.7 CPU-hours over three runs. No AI tokens.

## The primary

| Contrast | Games | Estimate | 95% interval | Reads |
|---|---|---|---|---|
| D(300,000) − D(1 million), 30M reference | 160 | 0.0246 | [0.0197, 0.0296] | above 0.01: the step up |
| D(300,000) − D(1 million), 100M reference | 40 | 0.0221 | [0.0115, 0.0327] | above 0.01: the check holds |
| D(100,000) − D(300,000), 30M reference | 40 | 0.0122 | [0.0014, 0.0230] (99% [−0.0022, 0.0267]) | not read: the step up came first; neither shown within nor shown worse |

By rating band (exploratory), the step up is above zero in every band: 0.034 [0.024, 0.044] at 800 to 1199 (47 games), 0.019 [0.010, 0.028] at 1200 to 1599 (59), 0.024 [0.016, 0.032] at 1600 to 1999 (44) and 0.023 [0.006, 0.039] at 2000 to 2199 (10).

## What changes for a player, 1 million against 300,000

- Labels: 7.45% of moves change their label, 95% [6.75%, 8.14%], about five of a game's 66 moves, 0.86 a player and game, for 3.3 times the engine time. A player's game shows, on average, 2.59 blunders, 1.51 mistakes and 2.75 inaccuracies at 1 million, against 2.56, 1.46 and 3.04 at 300,000: fewer inaccuracies, the same blunders.
- Accuracy: 0.42 of a point lower on average, 95% [0.22, 0.62]; 1.06 points in absolute terms [0.93, 1.19]; 45% of a player's games [39%, 51%] move by a point or more.
- Exploratory, against the reference (stratified, bootstrap 95%): label disagreement 12.3% [7.9%, 17.5%] at 300,000 and 9.8% [5.7%, 14.6%] at 1 million; the best move agrees 80.7% and 83.8% of the time; the mover's win% before the move is off by 2.3 and 1.9 points. These absolute shares, and every absolute figure for 50,000, 100,000 and Lichess, rest on the 100 sampled moves of the unreferenced remainder, each weighing about 96.5, which is why 100,000's (12.6%) sits close to 300,000's while the exact contrast on the 40 step-down games separates them.
- Top weaknesses: not measured. A ranking reads every game of a player and the enrichment's MultiPV and threat searches; it is EXP-0027 (Next, item 3).
- Lichess's own evaluations: their labels differ from 300,000's on 9.9% of moves [9.1%, 10.8%], and from the reference on 12.3% [7.4%, 18.0%], about as often as 300,000's do.

## What it costs

Measured engine CPU, one thread, per game of this sample (62.5 positions): 4.0 seconds at 100,000 nodes, 11.8 at 300,000 and 39.1 at 1 million, 3.3 times as much. A 90-day import of 300 rapid games is 1.0 CPU-hour at 300,000 and 3.3 at 1 million: about 15 and 49 minutes of wall time on the 4 engines a worker runs, the engine stage only (the enrichment's own searches come on top and are not re-costed here). The ETA and the queue-lag advice need re-costing with the change.

## The cheaper middle option

Not settled by this experiment, and not choosable by it: section 9 chooses only among registered candidates. Two options:

1. **A middle node count** (600,000, say): not searched, so nothing can be said; the curve between 300,000 and 1 million is not measured.
2. **Deeper search only where the cheap searches disagree** (post hoc, exploratory, from this run's cached searches; `review.md`, part two): search 100,000 and 300,000 everywhere and 1 million only on the moves whose labels differ between them, 15.9% of positions, about 22 CPU-seconds a game instead of 39. It recovers 0.0147 [0.0111, 0.0183] of 1 million's 0.0246, and stays 0.0099 [0.0059, 0.0139] behind 1 million, an interval straddling the tolerance, so it is not shown to match. It also swaps labels move by move, where a real re-search of a position changes the neighbouring move that shares it, so it is not yet the protocol that would ship. Because its rule was found after seeing these results and is scored on the same games, it is EXP-0028's challenger on new games (Next, item 2), not a decision.

## Deviations and process

- None from sections 0 to 10 as frozen at commit 1cf0520b. The design changed after the pilot and after the pre-run critique, before any confirmatory search (`review.md`, part one, commits d8951e09 and 3012b7ab); the revised design had no second independent critique before the run.
- The compute plan in section 0 was exceeded in CPU-hours (19.5 against 17.1 with the branch), within every node unit's cap.
- The step down was not read: rule 1 decides first. Its estimate, 0.0122 on 40 games, is reported for the record.
- The cache is keyed by the engine crate's source digest. The 1 million protocol changes `ENGINE_NODES` in that crate without changing a search at a stated node count, so the arena's first re-run on it searches everything again (about 19.5 CPU-hours) unless the cache files are copied to the new key; the run's own re-searches then check them.

## Next

1. Issue: "Analyse at 1 million nodes: the protocol EXP-0008 chose": `ENGINE_NODES` 1,000,000 and `sf19-n1m-v1`, a major algorithm version with its ADR, the manifest, the replay goldens and the arena against `algo-3.0.0`, the pipeline re-costed. EXP-0009 runs at 1 million.
2. EXP-0028: search deeper only where the cheap searches disagree, registered against 1 million on new games, with the reference's convergence measured.
3. EXP-0027: do a player's top weaknesses change between 300,000 and 1 million nodes? Whether stored analyses are redone when the protocol ships.
