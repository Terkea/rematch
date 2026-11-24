---
id: EXP-0006
title: Does a lighter, honestly counted prior fix the 80% ranges?
status: pre-registered
algorithm: algo-1.1.1+42d749f7c50b
issue: 101
updated: 2026-11-24
---

- The question: whether a fix brings every one of EXP-0002's 20 key × sample-size coverages of the 80% ranges into [0.75, 0.85] (#13), without more false weaknesses or lost detections.
- The challenger, chosen on development seeds by a rule written first: the peers' prior is worth at most two of the player's own games, and the recency-weighted data count at their effective sample size, divided by their game-to-game dispersion. On those seeds its worst coverage is 0.85 (W5 at 30 games) against production's 0.11.
- The test: both rules on 14,800 fresh synthetic players and, side by side, on 68 engine-generated players (6,720 games); ship when the challenger's worst coverage is within 0.07 of 0.80 at the 97.5% level and four guards hold, among them false weaknesses for players at their peers' rate (at most 0.01 more), Tier 1 recall at 2 and the smoke suite's CI rule.
- The risk, written before the run: the false-weakness guard is close to its margin (0.009 against 0.01 on the power pilot), so the whole rule's power is about 0.69.
