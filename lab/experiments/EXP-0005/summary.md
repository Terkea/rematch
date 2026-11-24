---
id: EXP-0005
title: How much do the rankings move between two real algorithm versions?
status: decided
algorithm: algo-1.1.1+42d749f7c50b
issue: 100
updated: 2026-11-24
live: false
retired: closed unrun by the value gate; no branch of its rule could change the suites (review concern 1)
---

- Closed unrun by the value gate: no branch of its decision rule could change the benchmark's suites, so it was not run (ADR 0107).
- Why: with 28 reference hits, even no hit lost gives a churn interval reaching 0.085, at which EXP-0001's own rule keeps today's 22 players a flaw.
- What remains: EXP-0006's side-by-side run reports the churn of its prior change descriptively, as a lower bound for other kinds of change.
- Next: measure the churn again on the full suite with the first detector or ranking candidate, where the result can move the suites.
