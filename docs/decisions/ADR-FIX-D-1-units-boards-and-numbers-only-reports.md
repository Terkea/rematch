# ADR-FIX-D-1. Numbers keep their units, answers show their positions, numbers-only reports say why

Date: 2026-11-22. Status: accepted (browser check on the owner's real data, batch D). Builds on
ADR-AI-8 (segments), ADR 0060 and 0071 (coach text), ADR 0073 (report states) and ADR 0078
(coach availability).

## Context

- **N1.** A coach answer read "Your win chance was about 86.5 before it … You spent 40.7 on that
  move". The validator forbids `%` next to a token (it is glue) and allows `pct0`/`pct1` only for
  fractions in [0, 1], while win chances, accuracies and clock times are stored on their own
  scales (0..100, seconds). `format_number` knew no units, so `int`/`dec1` printed a bare number
  and the prompt told the model not to add units. The unit was lost in the formatter, not in the
  schema or the renderer.
- **N2.** A report built computed-only before the coach was switched on said "Your coach's notes
  are off for now" while `ai_status` was fresh and enabled.
- **N5.** Answers cite moves but showed no position.

## Decision

1. **The server prints a number with its unit.** `Evidence::unit_of(path)` gives a path's unit:
   a metric's stated `unit` (`pct` is `%`, `seconds` is ` s`), else the field name (`winBefore`,
   `winAfter`, `accuracy`: `%`; `clockBefore`, `clockAfter`, `timeSpent`, `baseSeconds`,
   `incrementSeconds`: ` s`; `cp`: ` cp`). `format_num_at` appends it for every format but
   `pct0`/`pct1` (already `%`) and `signed1` (pawns), so the stored `Segment::Num.display` and the
   resolved text agree. A no-break space keeps `40.7 s` on one line. Counts stay bare: the text
   names them ("games"). `core/tokens` v2 tells the model the unit is the server's; v1 stays for
   stored hashes.
2. **"Numbers only" has two readings.** With no coach now (`ai_status`), the report banner and the
   area pages' "Coach's note" say "Coach not available right now" (ADR 0078). With the coach on,
   they say "This report was built from your numbers only. Your coach's notes come with the next
   rebuild, available <when>" (`nextManualAt`, else "now"). The budget keeps its own copy.
3. **Answers show the positions they cite.** `getThread` returns `positions`, keyed
   `platformGameId#ply`, for every `move`, `best` and `pv` segment of a done answer (at most 60),
   read in the same read-only transaction from `moves` on the conversation's own account while
   the user is linked to it. The board is the move's `fen_before`, from the player's side, with
   the move played (pen for the player's own mistake or blunder, else pencil) and the engine's
   `best_uci` (good). One board per ply, under the paragraph that first cites it; the board loads
   lazily near the viewport, a click opens the review at that ply, and the caption's link is the
   one tab stop. Its name reads "Position before 30. Rxg7; engine preferred Qe3".

## Consequences

- Stored answers keep the display they were stored with; new answers carry units.
- The thread API grows by the cited positions; no request per board.
- Copied text reads "better move: Qe3" and the composer hint "Shift + Enter" (one "+").
