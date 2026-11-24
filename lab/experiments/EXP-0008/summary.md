---
id: EXP-0008
title: Is 300,000 nodes enough? Depth against judgements and cost
status: pre-registered
algorithm: algo-3.0.0+554036a3b27c
issue: 103
updated: 2026-11-24
---

- The question: does the production search, 300,000 nodes a position, give a player the same inaccuracy, mistake and blunder labels as a search a hundred times deeper, and would 100,000 nodes, a third of the engine time, do as well?
- The ground truth: **Stockfish 19** at 30 million nodes with the Syzygy tables, scored by the production code; Lichess's own `[%eval]` comments (CC0) as the secondary.
- The design: 160 rated rapid Lichess games from October 2026 (the most 12 CPU-hours allow; no affordable size reaches 80% power), every position searched at 50,000, 100,000, 300,000 and 1 million nodes, and the deep reference only on the moves where those disagree, so every contrast is exact at a fraction of the cost; a step up must also hold against a reference three times deeper.
- The rule: a step down to 100,000 or up to 1 million needs the change in label disagreement shown to lie on the right side of a tolerance of 1 move in 100; otherwise 300,000 stays.
