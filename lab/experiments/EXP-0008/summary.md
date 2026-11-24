---
id: EXP-0008
title: Is 300,000 nodes enough? Depth against judgements and cost
status: decided
algorithm: algo-3.0.0+554036a3b27c
issue: 103
updated: 2026-11-24
live: true
rerun: release
run_cpu_hours: 0.35
run_minutes: 4
---

- No: 1 million nodes a position gives labels closer to a search a hundred times deeper. Against **Stockfish 19** at 30 million nodes, it disagrees on 2.5 moves in 100 fewer than 300,000 does (95% [2.0, 3.0]), past the tolerance of 1 in 100; against 100 million nodes on 40 of the games, 2.2 fewer [1.2, 3.3].
- Measured on 160 rated rapid Lichess games (CC0), the deep searches spent only on the moves where the cheaper ones disagree, so every contrast is exact.
- For a player: 7.4% of move labels change, mostly fewer inaccuracies, and game accuracy falls by 0.4 of a point on average. The engine costs 39 CPU-seconds a game instead of 12, so a 300-game import goes from about 15 to 49 minutes on 4 engines.
- The reference is not the truth: on the moves where 300,000 and 1 million disagree, 12% of its labels changed again between 30 and 100 million nodes, and the step up shrank by 12% on the same games; the claim is that 1 million agrees more with deeper searches.
- Next: ship 1 million as a major version, test deeper search only where the cheap searches disagree (EXP-0026), and measure whether players' top weaknesses move (EXP-0027).
