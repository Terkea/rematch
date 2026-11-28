# EXP-0002 in plain words

**Bottom line:** Rematch's 80% ranges were not 80% ranges for players clearly worse than their peers; for missing a mate at 30 games the range held the true rate about 1 time in 10.

## What we asked

Every weakness rate comes with an 80% range. If Rematch says you miss mates at some rate, give or take, is your real rate inside that range 8 times in 10?

## Why it matters to someone learning

A range that is too narrow, or in the wrong place, tells you we're sure of something we're not. If it sits too close to your peers, it also tells you a weakness is milder than it is.

## What we found

We made 6,000 players with rates we chose, 30 to 250 games each, and scored every range against the truth. For players two to eight times their peers' rate, 12 of the 20 weakness and game-count cells fell outside 75% to 85%. Missing a mate at 30 games held the truth 0.10 of the time, early blunders 0.20, leaving pieces hanging 0.64. The misses were almost all on the high side: the peers' rate pulled the estimate towards them. For missing a mate, which has about one chance a game, the peers weighed as much as a player's first 40 games.

## What changed in Rematch

Nothing in this experiment; it only measured. The fix was EXP-0006, which shipped as algo-2.0.0.

## How this helps us improve Rematch

It tests the weakness estimate and its ranges. It is live and re-runs on every pull request that changes the algorithm version. On the leaderboard algo-2.0.0 came out better than algo-1.1.0, and every version since has shown no clear difference, so a change that breaks the ranges again gets caught.

## What we still don't know

The peers' rates and the chances per game are our assumptions, so real players' ranges are not measured yet. Nor do we know whether a player reads "80%" the way we mean it. The maths audit found two of the power statements in the plan were loose (0.84, not above 0.9, at the worst case); no result moved.
