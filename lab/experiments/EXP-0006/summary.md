---
id: EXP-0006
title: Does a lighter, honestly counted prior fix the 80% ranges?
status: decided
algorithm: algo-1.1.1+42d749f7c50b
issue: 101
updated: 2026-11-24
live: false
retired: a comparison of two prior rules inside one build; EXP-0002 (live, per pull request) measures the same coverages for every later version, the smoke suite gates the Tier 1 guards, and the false-weakness guard moves to EXP-0013
---

- The question: whether a fix brings EXP-0002's 20 key × sample-size coverages of the 80% ranges into [0.75, 0.85] (#13) without more false weaknesses or lost detections.
- The fix, chosen on development seeds first: the peers' prior is worth at most two of the player's own games, and recency-weighted games count at their effective sample size, divided by their game-to-game spread.
- The result on fresh synthetic players, averaged over players two to eight times their peers' rate: every one of the 20 coverages is inside the band (the worst 0.828, W5 at 30 games), against production's worst of 0.103 (W4.missed at 30 games); the coverage at the peers' rate moves from 0.874 to 0.804; credible estimates for players at their peers' rate rise by 0.009, inside the 0.01 margin with 0.0007 to spare; Tier 1 recall at 2 is unchanged and the smoke suite's coverage rises from 0.65 to 0.80.
- The decision: ship, as `algo-2.0.0`. #13's own case (W1 against peers at 0.0008) reaches 0.766 [0.738, 0.791] at 30 games, below 0.80 and not shown inside the band, so #13 stays open.
- Next: EXP-0025 for #13's remaining cell, and EXP-0013 for what the extra false weaknesses mean for clean players.
