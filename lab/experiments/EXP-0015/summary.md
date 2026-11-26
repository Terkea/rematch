---
id: EXP-0015
title: Is "clearly worse than your peak" a false alarm?
status: pre-registered
algorithm: algo-4.0.1+fcda2762eeeb
issue: 110
updated: 2026-11-26
---

- The question: how often Rematch tells a player whose strength has not changed that their rating is falling ("Your rapid rating peaked at 1,562 and is 1,488 now"), and how often it catches a real fall of 100 or 200 points, at 60, 120 and 250 rated games (#110).
- The players are simulated, so the truth is exact: 700 players of constant strength rated by Glicko-2 as on Lichess, and 1,200 whose strength falls, each series sent through the production trajectory (`cli lab trajectory`).
- The acceptable false-alarm rate is 0.15, the 10% the rule's 80% interval promises plus 0.05. Above 0.15 at any length, a challenger rule is registered; above 0.20, where false "falling" headlines would outnumber true ones, the verdict is also hidden until one clears.
- About 3 CPU-minutes, no engine and no AI tokens. A fresh critic's 15 concerns were fixed before the run (`review.md`).
