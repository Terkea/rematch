---
id: EXP-0005
title: How much do the rankings move between two real algorithm versions?
status: pre-registered
algorithm: algo-1.1.1+42d749f7c50b
issue: 100
updated: 2026-11-24
---

- The question: how often a real candidate loses a flawed player's correct top-2 weakness that production found, the churn EXP-0001 assumed to be 0.10 when it sized the benchmark.
- The candidate: EXP-0006's challenger, run side by side with production on the same games (`ALGO_VARIANT`), on EXP-0001's 32 pilot players and the 16 smoke players, 5,120 games.
- The rule: a Jeffreys 95% interval of the churn at 80 games wholly below 0.10 resizes the weekly suite by EXP-0001's own rule; wholly above, its detectable change is re-registered; a straddle keeps the suites.
- The limit, written before the run: with 28 reference hits the design can say "below" only for a churn near 0.
