---
id: EXP-0001
title: How many synthetic players and games the benchmark needs
status: decided
algorithm: algo-1.1.0+fa5e2186e1dd
issue: 91
updated: 2026-11-23
---

- The question: how many synthetic players per flaw, and games per player, the Tier 1 benchmark needs to detect a change of 0.1 in recall at 2 between two algorithm versions at least 80% of the time.
- What we did: ran a pilot of 30 flawed players and 2 clean controls (3,840 games, 25 minutes with 8 engines) through the real pipeline, then simulated the registered decision rule with the player as the unit.
- The result: 40 players a flaw at 80 games give a power of 0.83 [0.81, 0.84] (95% Monte Carlo) under an assumed churn of 0.10; the weekly budget of 9,000 games allows 22, a power of 0.52 [0.51, 0.54], and a change of 0.15 is detected 80% of the time. Most of the pilot's misses sit in two flaws the ranking does not reach, so that change would have to come from them.
- The decision: budget-bound. The full suite is now 22 players a flaw and the smoke suite 3, at 80 games; the CI tolerances are 0.1 for recall at 2 and 0.05 for coverage.
- Next: measure the churn between two real versions (EXP-0005), since the size rests on it.
