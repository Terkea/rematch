# 0025. Tiered import and prioritised analysis for large histories

Date: 2026-11-21
Status: Accepted

## Context

Some players have thousands of games. Downloading a game is almost free; engine analysis costs about 15-20 core-seconds per game. Analysing everything would cost hours of CPU per heavy user and delay their first results.

## Decision

Import broadly, analyse selectively (design section 8c).

- Tier 1: import up to 10,000 recent games per account (large limit) or 1,000 (small limit), without the engine. This powers the rating curve, openings, time-control mix, clock and session stats.
- Tier 2: a per-user priority queue for engine analysis:
  - P0 onboarding: 50 (small limit) or 200 (large limit) recent games in the primary time control.
  - P1: 100 per other active time control (large limit).
  - P2: stratified history sample of 15 per month per time control, up to 3 years back (large limit), backfilled at low priority.
  - P3: new games from sync (small limit 20 per week; large limit all, with a fair-use cap of 300 per week).
  - P4: on-demand analysis in the browser.
- P1 and P2 stop adaptively once the weakness posteriors are precise enough.
- Rated games first; skip games under 10 plies; bullet only if primary; games that already have Lichess evals first.
- One analysis protocol across all tiers.
- Every metric states its basis (analysed vs imported counts).

## Alternatives considered

Analyse everything (cost, latency); analyse only the most recent N (no long-term timeline or honest baseline).

## Consequences

Needs a scheduler with per-user priorities and checkpointed, resumable imports. History-based metrics are sample estimates and are labelled as such.
