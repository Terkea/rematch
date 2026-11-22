# Profile kit (`/styleguide`, Profile section): critique log (M5b Task 4)

Screenshots are not committed. They were taken from a local `next dev` with `LAB_ENABLED=1`, of the
`#profile` section only, at 390 and 1440, light and dark, reduced motion, with the synthetic data in
`apps/web/src/app/styleguide/sections/profile.tsx` (no player data). The two exemplar positions are
real opening traps (the Blackburne Shilling trap after 3… Nd4, and 4… h6?? against the Fried Liver
set-up); their win chances are made up.

At 360, 390 and 1440 the page had no horizontal overflow (`scrollWidth` equal to the viewport, 1425
at 1440 with the scrollbar). The section's text was checked for `NaN|undefined|Infinity`: none.

## Pass 1 (implementer self-critique; self-critique only)

The implementer was told not to dispatch subagents, so the fresh critic (`critique-prompt.md`,
PNGs only) is still owed: **the controller runs it and appends its verdict here.**

`pnpm design:check`: 0 findings.

Issues found and fixed:

1. **Tabular figures in running text** spread stops and commas ("2 . 1%,", "2026 , 14 games").
   Tabular lining figures now only set the large values; sentences, ranges, counts and axis
   labels use proportional lining figures.
2. **"38 of 902 chances" wrapped** in the rate column at 1440. The column is 9rem and the
   comparison column wider, so the strip has room.
3. **The weakness trend had no name on screen.** A small "Weekly rate" label sits over the
   sparkline (its accessible name already said so).

Checked and kept:

- Weakness state chips carry colour, the timeline's pattern and bar weight, and the state's name;
  an early read is a dashed empty box named "Early read", with "180 more chances needed" in place
  of the band comparison.
- The band comparison says "above" or "below" only when the stored interval clears the median.
- KPI multiples: stored bands as given, gaps for the null week, hollow rings for the thin week
  (which also break the line), one shared axis, numbered report rules listed with their dates, a
  key drawing every mark. An empty metric says "No games in this range yet".
- Exemplars: `pen` played arrow with its `?`/`??` badge, `good` better move (solid head), `pencil`
  reply; the key names each, so colour is never the only cue. Night marks keep their halos.
- AreaScore without a next band draws no next tick and its key drops the entry.
- Reduced motion: the KPI grid and weakness rows do not animate; the exemplar arrows use the
  board's own reduced-motion rule (drawn at once).

Open for the critic: the lone dots of a sparse weekly trend (the "Missed forks" early read) are
small `pencil` dots; they are the Sparkline's existing treatment.
