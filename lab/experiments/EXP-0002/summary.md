---
id: EXP-0002
title: Do the 80% ranges hold the true rate 80% of the time?
status: decided
algorithm: algo-1.1.0+fa5e2186e1dd
issue: 92
updated: 2026-11-23
---

- The question: whether the 80% range Rematch shows for a weakness rate holds the player's true rate about 80% of the time (#13).
- What we did: fed event counts drawn from rates we chose (five weaknesses, three rating bands, 30 to 250 games, 6,000 players) to the production estimate, and scored every range against the truth.
- The result: for players with a weakness, 12 of 20 weakness × sample-size coverages lie outside [0.75, 0.85]; the worst is W4.missed at 30 games, 0.10 [0.09, 0.11], and W1 at 30 games is 0.64 [0.61, 0.66]. The ranges miss on the high side: the prior pulls the estimate towards the peers.
- The decision: outside the band, so the 80% ranges are not calibrated for the players they are shown to. The algorithm is unchanged by this experiment.
- Next: EXP-0006 tests a prior counted in games instead of chances, which would weaken it for the keys with about one chance a game.
