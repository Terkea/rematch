---
id: EXP-0001
title: How many synthetic players and games the benchmark needs
status: pre-registered
algorithm: algo-1.1.0+fa5e2186e1dd
issue: 91
updated: 2026-11-23
---

- The question: how many synthetic players per flaw, and games per player, the Tier 1 benchmark needs to detect a change of 0.1 in recall at 2 between two algorithm versions at least 80% of the time.
- What we will do: run a pilot of 6 players per flaw and 2 clean controls (120 games each, 3,840 games) through the real pipeline, then simulate the registered decision rule with the player as the unit.
- The result and decision come after the run; the suites and the CI tolerances in `bench/baseline.json` are resized only from it.
