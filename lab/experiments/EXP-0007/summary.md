---
id: EXP-0007
title: The motif tagger against the Lichess puzzle themes
status: decided
algorithm: algo-1.1.0+fa5e2186e1dd
issue: 102
updated: 2026-11-24
live: true
rerun: release
run_cpu_hours: 0.02
run_minutes: 1
---

- The question: how often is a position Rematch calls a fork really a fork, and how many forks does it miss? Measured on 54,987 Lichess puzzles (CC0), against Lichess's generated themes, with each puzzle's solution standing in for the engine's best line.
- Trusted, 4 of 10 motif tags: fork (precision 0.98, recall 0.72), discovered attack (0.99, 0.86), promotion (1.00, 0.82) and smothered mate (1.00, 1.00), the last two close to pattern checks; the five mate-length tags pass as replay checks.
- Hidden until fixed: deflection (0.56, 0.33), which mostly fires on attraction. Hidden pending a human audit, because the themes cannot tell our errors from their gaps: skewer (0.81, not shown to clear 0.80), back-rank mate (0.61), hanging piece (0.57), sacrifice (0.53) and pin (0.40, with recall 0.20, so it needs a fix anyway).
- Recall falls as the puzzles get harder: fork from 0.86 below 1000 to 0.54 above 2000.
- Next: stop naming the six hidden tags, a blind human audit (EXP-0019), fixes for deflection and pin (EXP-0020, EXP-0021), the window past the first gain (EXP-0022), and the engine's own line (EXP-0023).
