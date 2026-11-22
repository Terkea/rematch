# Weaknesses (`/app/weaknesses`, `/app/weaknesses/[seriesKey]`): critique log (M5b Task 7)

Screenshots are not committed. They were taken from a `next dev` in the throwaway compose project
`ci-m5b-t7` (no host ports), pointed at its `chess_web_test` database and seeded with synthetic rows
only (T1's `seedProfile`, T2's `seedNote`, `seedSyntheticGame`; no player data). The three example
positions are real opening traps (3… Nd4 Blackburne Shilling with 4. Nxe5, 4… h6 against the
Fried Liver set-up, 3… Nf6 against Qh5 and Bc4); their win chances are made up. States shot, at
390 and 1440, light and dark, reduced motion:

- `/app/weaknesses` rich: eight series across all three groups, an early read, a non-`W` series
  without a band reference, 4 of 32 positions waiting for Maia.
- `/app/weaknesses` empty: 6 analysed games, no estimates.
- `/app/weaknesses/W1` sufficient: stored interval, band median, onset row, timeline, 12 weeks
  (two under 20 chances), a ready change note, three exemplars from a report.
- `/app/weaknesses/W1` early read: 120 of 1,000 chances, no history, no note, no report.

At 360 and 1440 no page scrolls sideways (`scrollWidth` 360, and 1425 with the scrollbar at 1440).
The visible text of all four was checked for `NaN|undefined|Infinity|null`: none. The morph was
driven in Chromium with and without `startViewTransition`: a client navigation (one document
load), no console errors, the detail header present afterwards.

## Pass 1 (implementer self-critique; self-critique only)

The implementer was told not to dispatch subagents, so the two fresh critic passes
(`critique-prompt.md`, PNGs only) are still owed: **the controller runs them and appends the
verdicts here.**

`pnpm design:check`: 0 findings.

Issues found and fixed:

1. **The exemplar boards overflowed their columns by the well's padding** at 1440 (the board sits
   in an 8px well from 768px). The board size is now the column less 16px from 768px.
2. **An early read's interval scale looked settled** (a solid range and an ink mean). It now draws
   the range as a dashed outline and the mean in `ink-soft`, matching the list's early-read row.
3. **The detail's scale had no key**, and the band line repeated the median twice ("Band median
   2.1%" then "Players in your rating band: 2.1%, …"). The list's key sits under the scale, and the
   second line says only where the median falls ("Players in your rating band, lower than your
   whole likely range.").
4. **"Week by week" was one small multiple in a full-width sheet**, mostly empty at 1440. It now
   shares a row with "What changed" from 1024px.
5. **Scale end labels used tabular figures** ("7 . 0 %"); they use proportional lining figures.
6. **"Getting better" lede overclaimed a cause** ("the rate has dropped"). It now says only what
   was detected ("Detected as improving, not yet settled enough to call resolved.").

Checked and kept:

- Arrows against the FEN, one White and one Black position. 4. Nxe5 (White to move, board from
  White): pen f3→e5, good f3→d4, pencil d8→g5 (Qg5). 4… h6 (Black to move, board from Black): pen
  h7→h6, good d7→d5, pencil g5→f7 (Nxf7). 3… Nf6: pen g8→f6, good g7→g6, pencil h5→f7 (Qxf7#).
- Non-`W` series (`endgame:rook`) never reach `min_evidence`, so they always read "Early read" with
  "More chances needed": that is T1's ruling (no Rust minimum), stated honestly.
- Rows without weekly points say "Weekly trend after more weeks", never an empty sparkline.

## Fix round 1 (review-t7.md; self-critique only)

Shots retaken (same synthetic seed, plus a 0-chance week and the note's `input.timeClass`) into
the session scratchpad `m5b-t7\fix1\`.

1. **Early reads had confident group headings.** Rows below their evidence minimum now sit only in
   a fourth group, "Early reads" ("Not enough chances yet to tell what these are doing. Each says
   how many more it needs."), never under "Costing you games" or "Getting better".
2. **List sparklines joined thin and missing weeks.** The rows now span the scope's newest 12
   calendar ISO weeks; a week with fewer than 20 chances, or none, is null, so the line breaks.
   A line under the key says so.
3. **The view-transition mid-frame blanked the app nav.** The shell header (`data-app-chrome`) gets
   its own `view-transition-name` for the morph with no animation, so it holds still; the wait for
   the new page dropped from 1200 to 400ms.
4. **Change note from another time class.** The detail shows only notes whose job input names the
   scope's time class.
5. Polish: the early read's key now draws the provisional marks (dashed range, soft tick); the
   detail scale ends read "0%" and "7%"; a series with no evidence threshold says so; the review
   link is at least 24px tall. Left to T4 (its components): the list strip's "25.0%" label, the
   doubled "Early read" in a row, the dark "Not an issue" swatch, crossing arrow heads.
