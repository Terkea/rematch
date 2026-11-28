# EXP-0006 in plain words

**Bottom line:** we fixed the 80% ranges by letting your peers count for at most two of your own games, and it shipped as algo-2.0.0, at the price of slightly more credible-looking weaknesses for players who have none.

## What we asked

EXP-0002 found the ranges dragged towards your peers. Would a lighter pull from the peers, and counting older games for what they are worth, bring every range back to holding the truth 75% to 85% of the time, without inventing weaknesses or losing real ones?

## Why it matters to someone learning

If you miss mates far more than players at your rating, the range should say so plainly, not soften it towards the average.

## What we found

On fresh made-up players two to eight times their peers' rate, all 20 cells landed inside 75% to 85%; the worst was 0.828. The old rule's worst was 0.103. The trade-off: for players at or below their peers' rate, a credible-looking weakness turned up a little more often, about one more in 110, inside our margin with 0.0007 to spare. For early blunders that went from about 1 in 740 such players to 1 in 59. The benchmark found the same 28 of 45 planted flaws either way.

## What changed in Rematch

algo-2.0.0: the peers' say is capped at two of your games, and recency-weighted games are counted at their real worth. Issue #13 stays open: leaving pieces hanging, against peers far below you, held the truth 0.766 of the time at 30 games, short of 0.80.

## How this helps us improve Rematch

It changed the weakness estimate itself. It is retired from the live arena, because EXP-0002 measures the same coverages on every pull request that changes the version, and the extra credible-looking weaknesses move to EXP-0013.

## What we still don't know

Whether players at their peers' rate see more "worth working on" in practice (EXP-0013). Real players' game-to-game spread; every number here is on made-up players. The maths audit found the recall guard was looser than registered (0.149, not 0.10) and the planned 50 players a cell had power 0.16; the decision stands and the next registration of that guard corrects it.
