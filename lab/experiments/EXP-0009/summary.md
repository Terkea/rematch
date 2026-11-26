---
id: EXP-0009
title: What actually helps people climb?
status: decided
algorithm: algo-4.0.0+51733d23895f
issue: 104
updated: 2026-11-26
live: true
rerun: release
run_cpu_hours: 1.5
run_minutes: 48
---

- Need more data, and the teaching order stays as it is: we have no data-driven teaching order yet. Among 16,204 anonymised Lichess rapid players followed from July to October 2026, no weakness whose rate fell from August to September came before a rise in October's rating.
- The check that the design can see such a link failed too: a rise in accuracy did not come before climbing (−0.1 Elo per SD, 95% −1.1 to 0.8), though the interval was narrow enough. The likeliest reason is in the design: September's rating, the outcome's starting point, comes from the same games as September's rates, so October's drift back from September's luck pulls every estimate down. The follow-up must start the outcome away from those games.
- The August level of accuracy, early blunders and leaving the opening worse predicts the rating two months on (+4 to +6 Elo per SD), which is a rating catching up with play, not evidence that teaching helps.
- Observational, no causal claims: the players are those who ask Lichess for analysis, followed by a salted hash of their name with nothing else kept (ADR 0114), and only the eight weakness keys that need no further engine analysis were measured, with no engine time.
- Next: the one follow-up the rule allows, with three-month windows on new months (EXP-0029); our own 1 million-node analysis of the other nine keys would cost about 4,560 CPU-hours at this design and does not run.
