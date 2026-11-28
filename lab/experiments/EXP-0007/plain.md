# EXP-0007 in plain words

**Bottom line:** when Rematch says "fork", it is a fork about 98 times in 100, but six of the ten motif names didn't clear the bar, so we stopped showing them.

## What we asked

If Rematch calls a position a fork, is it a fork, and how many forks does it miss? We checked every motif name on 54,987 Lichess puzzles against Lichess's own themes.

## Why it matters to someone learning

"You keep missing pins" sends you off to study pins. If the label is wrong, that's study time spent on the wrong pattern.

## What we found

Four names are trusted: fork (right 98 times in 100, finds about 7 in 10), discovered attack, promotion and smothered mate. Deflection was right only 56 times in 100 and often fired on attraction instead, so it's our error. Pin was right 40 times in 100 and found only 1 in 5, so it needs fixing whatever else we learn. Skewer (0.81, not shown to clear 0.80), back-rank mate, hanging piece and sacrifice are less clear: hanging piece finds almost every puzzle Lichess labels and fires on many more, and a wider rule isn't the same as a wrong one. Forks also get harder to spot as puzzles get harder: found 0.86 of the time below 1000, 0.54 above 2000.

## What changed in Rematch

algo-3.0.0 stopped naming the six that didn't clear, to players and to the coach. The tagger still computes them.

## How this helps us improve Rematch

It tests the motif names that players, the coach and the trainer's pattern rates use. It is live and re-runs on every release; every version since has trusted the same nine tags: the four motifs and the five mate-in-N checks. It led to a blind human audit (EXP-0019) and experiments to fix deflection and pin (EXP-0020, EXP-0021).

## What we still don't know

Whether the four unclear tags are really wrong or Lichess's themes just miss them; that's EXP-0019. The test uses puzzle solutions, not the engine's own line in your games (EXP-0023). The maths audit found our theme shares were over all tagged puzzles, not over the false positives, which can be up to about twice as large; the decision stands.
