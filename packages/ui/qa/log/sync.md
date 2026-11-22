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

## Fix round 1 (review-p2.md, visual Important 1 to 3 and Polish; self-critique only)

Shots (synthetic, temporary lab route) are in `scratchpad/m5b-tp2/fix1/{a,b,c}-{390,1440}-{light,dark}.png`:
- a: compact overview (checking, held back, failed games) and full panel (waiting to retry), with the header "Checking for new games" ("Syncing" at 390);
- b: full panel with analysis waiting and no ETA, paused, last sync failed with retrying and failed games, and not synced yet, with the header "Analysing 60 / 108 games";
- c: compact and full for one account with everything analysed, profile updating and a cooldown, with the header "Updating your profile".

Changes:
1. The compact panel is now two lines at 1440 (about 45 px) and about 80 px at 390: the totals ("All time controls"), the state, "Details", and one button. The coaching headline stays on the first screen.
2. An unavailable "Sync now" shows a dashed edge and faint text, uses `aria-disabled`, and gives its reason as a tooltip and accessible description.
3. The working stroke now leads its line, so it never wraps alone. Failed ticks are a broken full-height stroke. The tally caps at 36 ticks on a phone, and there is no tally once all games are done.
4. Asides are now `text-sm ink-soft`. "Unverified" has a touch target. "Fewer than two moves played" replaces the old wording, and the header at 390 says "Syncing".

Verdict: self-critique pass. **The fresh critic passes are still owed by the controller.**
