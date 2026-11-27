---
id: EXP-0029
title: Does a fall in a weakness over three months come before further climbing?
status: pre-registered
algorithm: algo-6.0.0+f40a2b27677a
issue: 131
updated: 2026-11-27
---

- The question: EXP-0009's, asked again on new months (October 2025 to June 2026) and players EXP-0009 never followed, with three-month windows. Does a fall in one of eight weaknesses from one three-month window to the next come before further climbing, once the rating has had a month to catch up?
- The fix and its limit: EXP-0009's outcome started from a rating built from the same games as the weakness rates, so luck in those games pulled every estimate down. Here the outcome starts at the end of April 2026, a month later. That helps only for players who play enough in April. EXP-0009's own players played a median of 121 rapid games in October, enough to leave a tenth to a sixth of the luck (the check uses the worse), and a check on April's games stops the run before any game is measured if too much is left.
- What it costs: nine monthly dumps streamed once (about 270 GB, never stored), then about 3.4 million games measured with Lichess's own evaluations for about 24,000 players, about 18.5 CPU-hours in all, with no engine time and no AI tokens.
- The likely results, from a simulation of the rule at that size: if no weakness is linked, "no further climbing from these five weaknesses" about 94% of the time and "need more data" about 5%, or up to 30% if the luck left is twice the estimate. A real link of the minimum size is found almost always. In the tail case where the reliabilities are as low as EXP-0009's lower bounds, "need more data" becomes the likeliest result (82% to 93%).
- What it can change: a weakness that comes before further climbing becomes a candidate for a new teaching order, tested in its own experiment. Anything else keeps the order as it is, and this is the last follow-up.
