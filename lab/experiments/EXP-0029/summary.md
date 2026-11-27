---
id: EXP-0029
title: Does a fall in a weakness over three months come before climbing?
status: pre-registered
algorithm: algo-5.0.0+3cabedf4c935
issue: 131
updated: 2026-11-27
---

- The question: EXP-0009's, asked again with three-month windows on new months (October 2025 to June 2026), with players EXP-0009 never followed. Does a fall in one of eight weaknesses from one three-month window to the next come before a rise in rating over the months after?
- The fix: EXP-0009's outcome started from a rating built from the same games as the weakness rates, so luck in those games pulled every estimate down. Here the outcome starts a month later, at the end of April 2026, and runs to the end of June.
- What it costs: eight monthly dumps streamed once (about 240 GB, never stored), about 3 million games measured with Lichess's own evaluations, about 13 CPU-hours, no engine time and no AI tokens.
- Sized from EXP-0009's reliabilities at their lower bound: about 20,000 players. That is close to what the hash range allows, so a count taken before any game is measured can stop the run unrun.
- What it can change: a weakness that comes before climbing becomes a candidate for a new teaching order, tested in its own experiment. Anything else keeps the order as it is, and this is the last follow-up.
