# 0090. The missed-moments trainer

Date: 2026-11-23. Status: accepted (lane `trainer`, the owner's top improvement after the
end-user review). Evidence: `docs/research/2026-11-23-what-helps-amateurs-improve.md`, feature 1.
Numbers 0088, 0089 and 0091 are taken by parallel lanes.

## Context

The app diagnoses well but gave the player nothing to practise. The research ranks retrieval
practice on the player's own error positions, spaced and interleaved, with confidence asked
before the answer and task-focused feedback, as the highest-impact addition.

## Decision

1. **Items.** `/app/train` makes items from the scope's stored analysis on each visit (new
   positions only, never rewritten): the player's own moves at a `W4.missed`, `W3.shallow`,
   `W3.deep` or `W7.traps` exemplar ply (`game_metrics.metrics.weakness.<key>.exemplarPlies`) or
   judged a blunder, with a stored `best_uci` other than the move played. Dropped: positions
   already decided (win% ≥ 97 before and ≥ 90 after) and answers that are a free capture
   (`obviousCapture` from the strengths page), except a missed mate. A position in several sources
   keeps all of them (`source_keys`) and is named by the most specific (`source_key`).
2. **Storage.** `drill_items` and `drill_attempts` (migration `20261123040000_drill_items.sql`).
   Items belong to a user (`user_id`), so two users linked to one account never share items or
   attempts. Every read and write also checks the account is one of the user's links.
3. **Grading without an engine.** The accepted set is stored with the item: `best_uci`, plus each
   `position_multipv` line (else `move_features.multipv`) that keeps the outcome. When the best
   move mates in n, a mate in at most n + 1; otherwise at most 5 win% points below the best.
   Anything else is "not one of the engine's good moves here". Promotion is to a queen (the
   board's current behaviour).
4. **The attempt.** The player plays a move, then says "Sure" or "A guess", then sees the verdict.
   The spec listed the confidence before the move; asking it about the chosen move keeps it a
   confidence in an answer (the hypercorrection effect) and still comes before the reveal. The
   verdict is about the move ("Right: Rd8# mates." / "Not this time: the engine's move was …"),
   with the engine line as numbered arrows (`explorerAnnotations`), the move played in the game,
   the source (named only now), and a link to the game at that ply. The answer never reaches the
   page before the attempt: the server grades (`POST /api/train/attempts`).
5. **Scheduling** (UTC days). Each right answer in a row is due again after 1, 3, 7, 21, then 60
   days; a miss resets the streak and makes it due tomorrow. A clean pass is a right, sure answer
   at least 7 days after the previous attempt; three retire the item (a miss resets the count).
   Only the first attempt of a day changes the schedule.
6. **Sessions.** Up to 12 items: those due first, then new ones (newest game first) while fewer
   than 10 were introduced today; never past 24 attempts a day. The items are interleaved by
   source in a seeded order, and the motif is not named before the attempt. `?source=` keeps one
   source: the weakness page's "Practise these N positions" uses it.
7. **Holdout.** About 20% of items (`fnv1a("holdout:<game>:<ply>") % 100 < 20`, decided once,
   stored as `held_out`) are never served or graded.
8. **"Is it working?"** Per source, the weakness rate per chance in the scope's games from the
   first drill of that source on, against before (events and opportunities from `game_metrics`;
   blunders per own move from `moves`), with n and Wilson 80% ranges (`server/stats`). Drilled
   against held back: how often the motifs of drilled items (`move_features.best_line.tags`) were
   found in later games (`missedByMotif`) against those only held-out items have. Below 30 chances
   on a side it says "Too early to tell", and it claims a change only when the ranges separate.

## Consequences

- Visiting `/app/train` or a weakness page writes new items (one short write transaction).
- Positions without stored MultiPV accept the engine's move only, which is strict but honest.
- Regression to the mean and rating drift still confound before/after; the holdout comparison is
  the within-player control. A learned half-life can replace the Leitner boxes once reviews exist.
