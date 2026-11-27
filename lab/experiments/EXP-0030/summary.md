---
id: EXP-0030
title: A trajectory verdict that counts the rating's memory
status: decided
algorithm: algo-5.0.1+d9c1f47013b6
issue: "EXP-0030: a trajectory verdict that counts the rating's memory"
updated: 2026-11-27
live: true
rerun: release
run_cpu_hours: 0.24
run_minutes: 15
---

- Ship `memory-halves` as `algo-6.0.0`. On 2,000 fresh simulated players whose strength never changed, it told 10.3%, 11.7% and 10.5% their rating was falling at 60, 120 and 250 rated games, and 8.9%, 8.4% and 9.5% that it was rising, every cell within the bound of 15%. The rule it replaces told 46% to 79% "falling" on the same players.
- How: the rule compares your first 30 games with your last 30, and its 80% range counts how a rating carries its error from one game to the next, from your own rating steps. Nothing is compared with a peak, which is a maximum of the same noise.
- It catches a 200-point fall 72%, 84% and 99% of the time at 60, 120 and 250 games, and a 100-point fall 36% to 72%.
- With breaks, streaks and tilt it runs at 13% to 14%, and a chess.com-like rating at 9% to 11%. On real Lichess players over a month, 36% were flagged either way against the model's 20%: real ratings wander, or truly change, more than the model allows, so the rate on real players is not yet known.
- Next: EXP-0031 tests long histories and new accounts, where the rule looks worse (17% at 250 games from a new account), and EXP-0032 measures the model on real series at the product's 30-game windows.
