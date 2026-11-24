---
id: EXP-0007
title: The motif tagger against the Lichess puzzle themes
status: pre-registered
algorithm: algo-1.1.0+fa5e2186e1dd
issue: 102
updated: 2026-11-24
---

- The question: how often is a position Rematch calls a fork really a fork, and how many forks does it miss? The same for each of the 15 tags with a Lichess counterpart.
- The ground truth: the themes of the Lichess puzzle database (CC0), a snapshot pinned by sha256, restricted to puzzles played at least 1,000 times with a popularity of at least 50.
- The design: 54,987 puzzles drawn by theme and puzzle rating, sized by a power analysis on a separate pilot of 1,874; the real Rust tagger reads each puzzle's solution as a move's best line, as production does.
- The rule: a tag is named to players only when its precision is shown to be at least 0.80, and trusted when its recall is also shown to be at least 0.50 (Bonferroni over 30 intervals); a tag the themes cannot judge is hidden pending a human audit.
- The pilot's hand check found most disagreements on fork, pin, skewer, sacrifice and back-rank mate to be motifs Lichess did not name, and most on deflection to be our errors.
