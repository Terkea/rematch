---
id: EXP-0009
title: What actually helps people climb?
status: pre-registered
algorithm: algo-4.0.0+51733d23895f
issue: 104
updated: 2026-11-24
---

- The question: among anonymised Lichess rapid players rated below 2000, does a fall in a rate Rematch measures come before a rise in rating, and by at least 14 Elo a month per standard deviation of change?
- The cohort: the same players followed over four monthly dumps (July to October 2026) by a salted hash of their name alone, with no names stored (ADR 0114).
- Stage 1 spends no engine time: it reads the evaluations Lichess already published, for the eight weakness keys that need no further analysis, the blunder rate and accuracy.
- Observational: the result is a list of candidates to teach, never causes, and it controls for the rating's own drift back, the starting rating, games played and time control.
