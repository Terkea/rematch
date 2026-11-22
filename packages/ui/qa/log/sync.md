# Sync and analysis panel (M5b Task P2)

Surfaces: the compact panel on `/app` (in place of the status line), the full panel on
`/app/games` (above the filters) and the header `ActivityIndicator` line. Shots come from a
temporary, uncommitted lab route with synthetic accounts (`quietknight`, `pawnstorm_88`,
`endgame_owl`; no owner data) at 390 and 1440, in light and dark, with reduced motion (the
`pnpm shots` default).

## Pass 1 (self-critique only; no fresh critic was dispatched in this task)

Findings and fixes:

1. Punctuation sat apart from the numbers ("analysed . 2 analysing now ,") because
   `tabular-nums` widened the full stops and commas in this face. Fix: running sentences drop
   `tabular-nums`; only counters keep it.
2. At 390 the compact panel's quiet "Sync now" wrapped under the sync line with a negative
   margin, so it looked misplaced. Fix: the button now sits on the name row, top right, and the
   sync line spans the full width below it.
3. A 40-game tally stretched across 1060px read as a ruler, not a count. Fix: the tally has a
   fixed pitch (2 spacing units per tick), so a small sample draws a short strip and a large
   one fills the row.

Checked and kept:

- Every tick state has its own shape as well as its colour: done is a full stroke, analysing a
  dashed pencil stroke, failed a broken stroke, waiting a stub. The sentence gives the exact
  counts, and the strip has an accessible name.
- Red is not used anywhere. A failed analysis is not the player's mistake, so it uses the
  `Warning` icon in `ink` with "Retry N games".
- The ETA shows only with a measured pace, and states its basis. An account with nothing
  outstanding shows no ETA.
- Nothing says "live". The checking stroke idles only while an import runs, and it is static
  under reduced motion.
- There is no horizontal scroll at 390, and no NaN, undefined or null text.

Verdict: self-critique pass. **The fresh critic passes (at least 2) are owed by the controller**,
because this task was told not to dispatch subagents.
