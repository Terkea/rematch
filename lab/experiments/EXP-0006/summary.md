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
- The test: both rules on 6,400 fresh synthetic players, paired; ship when the worst coverage's distance from 0.80 shrinks with a 97.5% interval wholly above 0, and five guards hold, including false weaknesses for players at their peers' rate (at most 0.01 more) and Tier 1 recall at 2.
- The risk, written before the run: on the development seeds the false-weakness guard was 0.009 worse, close to its margin.
