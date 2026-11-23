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

## Amendment 1 (2026-11-23): a fraction always reads as a percentage

Final QA Q9: a live answer read "As White you have 50 games and score 0.45, with a range of 0.36
to 0.54". A score, rate or probability is stored as a fraction in 0..1, and `dec1`/`dec2` printed
it bare.

1. **A rate path renders as a percentage in every format.** `format_num_at` prints a path in
   `Evidence::rate_paths` with `%` whatever the token's format: `pct0`/`pct1` as asked; any other
   format as a whole percentage from 10% and with one decimal below it, without a trailing `.0`
   (`0.45` → `45%`, `0.096` → `9.6%`, `0.0017` → `0.2%`), so a rare event never reads `0%`.
2. **Every fraction of coaching-input v2 is a rate path** (ADR 0085): the schema's `0..1` leaves
   (a weakness's `posteriorRate`, `intervalLow`/`High`, `bandMedian`, `confidence`, `pImp`; an
   opening's `score` and alternatives' `bandScore`; a change's `rateBefore`, `rateAfter`,
   `posteriorProb`; a colour weakness's `rate`, `low`, `high`, `peerRate`), an opening's
   `performanceVsExpected` (a score difference, `-1..1`, which reads `-9%`), and every metric or
   colour metric (`byColor.*.score`) whose `unit` is `rate`. A colour metric's unit now comes from
   its `unit` field, as a metric's does. The chat tools `get_weakness` and `get_opening_line`
   mark the same fields of what they return. `pct0`/`pct1` stay refused outside `[0, 1]`.
3. **A test walks the schema.** `every_fraction_path_of_the_schema_renders_as_a_percentage` lists
   the `CoachingInput` JSON schema's number leaves and checks that every fraction leaf the v2
   fixture holds is a rate path and renders with `%` in `dec1`, `dec2` and no format; a new
   fraction field fails it until it is marked.

Stored answers keep the display they were stored with.
