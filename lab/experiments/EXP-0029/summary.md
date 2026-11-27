---
id: EXP-0029
title: Does a fall in a weakness over three months come before further climbing?
status: pre-registered
algorithm: algo-5.0.0+3cabedf4c935
issue: 131
updated: 2026-11-27
---

- The question: EXP-0009's, asked again on new months (October 2025 to June 2026) and players EXP-0009 never followed, with three-month windows. Does a fall in one of eight weaknesses from one three-month window to the next come before further climbing, once the rating has had a month to catch up?
- The fix and its limit: EXP-0009's outcome started from a rating built from the same games as the weakness rates, so luck in those games pulled every estimate down. Here the outcome starts at the end of April 2026, a month later. That helps only for players who play enough in April: at 20 rapid games it removes little of the luck, so the run goes ahead only if the median player has played about 25 or more.
- What it costs: nine monthly dumps streamed once (about 270 GB, never stored), then 1.8 to 3.7 million games measured with Lichess's own evaluations, 13 to 20 CPU-hours in all, with no engine time and no AI tokens.
- The likely results, from a simulation of the rule: if no weakness is linked, "no further climbing from these five weaknesses" about 70% of the time and "need more data" about 30%. A real link of the minimum size is found almost always. If the reliabilities are as low as EXP-0009's lower bounds, "need more data" becomes the likeliest result.
- What it can change: a weakness that comes before further climbing becomes a candidate for a new teaching order, tested in its own experiment. Anything else keeps the order as it is, and this is the last follow-up.
