---
id: EXP-0002
title: Do the 80% ranges hold the true rate 80% of the time?
status: pre-registered
algorithm: algo-1.1.0+fa5e2186e1dd
issue: 92
updated: 2026-11-23
---

- The question: whether the 80% range Rematch shows for a weakness rate holds the player's true rate about 80% of the time (#13).
- What we will do: feed event counts drawn from rates we chose (five weaknesses, three rating bands, 30 to 250 games, 6,000 players) to the production estimate, and score every range against the truth.
- Calibrated means every weakness × sample-size coverage lies inside [0.75, 0.85] for players with a weakness; the result and decision come after the run.
