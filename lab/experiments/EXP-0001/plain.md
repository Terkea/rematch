# EXP-0001 in plain words

**Bottom line:** our test bench can afford 22 made-up players per flaw, which catches the improvement we wanted to see only about half the time, so we sized it to the budget and wrote down what it can see.

## What we asked

We test each new version on made-up players with flaws we planted: one hangs pieces, one blunders on a low clock, one misses mates. How many players, and how many games each, does that test need to notice a version that puts the planted flaw in the top two weaknesses more often?

## Why it matters to someone learning

If this test can't see a drop, a worse ranking can reach you.

## What we found

In a pilot of 30 flawed players and 2 clean ones (3,840 games), the planted flaw made the top two for 19 of 30 at 80 games, about 6 in 10: hanging pieces and low-clock blunders for all six players, missing a mate for none. To catch one more player in ten four times in five, we'd need 40 players per flaw at 80 games. The weekly budget of 9,000 games allows 22, which catches it about half the time.

## What changed in Rematch

The benchmark, not the algorithm: the full suite is 22 players per flaw at 80 games and the smoke suite 3, and the smoke suite fails a change whose top-two rate drops by more than 0.1 (two of its 15 flawed players) or whose coverage moves by more than 0.05.

## How this helps us improve Rematch

It tests the weakness ranking end to end and re-runs on every release: every version since has had the overall verdict "no clear difference", though coverage improved on algo-2.0.0. Nearly all of any gain in it has to come from missing a mate and early blunders.

## What we still don't know

The size rests on a guess of how much rankings shuffle between versions, which moves the answer threefold. The full suite had never run until its first run on 28 November 2026. The maths audit found the coverage rule trips on noise alone 19% to 71% of the time, not the 20% we meant; a false alarm blocks a pull request until someone looks, never lets a regression through. EXP-0033 is filed to fix it.
