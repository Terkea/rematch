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

## Critic pass 1 (independent, screenshots only)

I judged this from the PNGs in the session scratchpad `m5b-t4/` (`profile-1440-light.png`, `profile-1440-dark.png`,
`profile-390-light.png`, `profile-390-dark.png`, `crop-390-top.png`, `crop-390-mid.png`) and from my own crops of
them. I read no code. Each location names the screenshot and a region, in that PNG's own pixels.

**Verdict: needs-changes.** 0 Blocker, 6 Important, 9 Polish.

Overall it does not look like generic dashboard slop. The serif display numbers and headings sit over a quiet
sans, with hairline rules and a restrained slate palette. There are no cards or gradients, and the
captions read like a chess player wrote them ("4. Nxe5? Better was Nxd4. Win chance 52% to 36%."). All of
it reads as deliberate. Primary text has enough contrast in both themes, and nothing overflows at 390.

The data display is mostly honest:
- Every weakness rate shows "x of n chances".
- Every KPI headline shows its game count.
- The empty metric says "No games in this range yet".
- The early read says how many more chances it needs.
- No NaN or 0 stands in for an unknown value.

The problems are marks that readers cannot decode, and comparisons that the layout invites but the data
does not support.

### Blocker

None.

### Important

1. **The interval bars use a different scale in each row and have no axis, so they invite a false
   comparison.** `profile-1440-light.png`, weakness rows, middle column (x 590-1030, y 205-520).
   "Leaving pieces undefended" (3.1-5.6%) and "Rook endgames" (8.4-15.2%) draw bars of exactly the same
   length at the same x (about 787-943), stacked in aligned rows. A reader scanning down will read them as
   equally uncertain and at the same level, and both are false. Either share one scale across rows, with
   a few tick labels, or break the aligned stack and give each bar its own end labels (3.1 / 5.6) so it is
   clearly local. The same applies at 390 (`crop-390-top.png`, bars at y 360, 603 and 1057).
2. **A weakness rate never says what it counts.** `profile-1440-light.png`, rate column (x 424-545).
   For "Leaving pieces undefended", "4.2%, 38 of 902 chances" is clear enough. "Rook endgames 11.8%, 31 of
   263 chances" and "Time trouble blunders 1.8%, 14 of 780 chances" are not: 11.8% of what happens in rook
   endgames? A chess player needs the event named in the count, for example "31 errors in 263 rook-endgame
   moves" or "14 blunders in 780 moves under 1 minute". The interval "3.1-5.6%" also has no label. Adding
   one word ("likely 3.1-5.6%") tells a non-statistician that it is a range for the rate.
3. **"Band" is jargon, and the band is never named.** `profile-1440-light.png`, comparison text
   (x 592, y 238 and 340), reads "Band median 2.1%, below your whole range". A player does not know which
   band. Name it, for example "Players rated 1400-1600: 2.1%" (or the band's own label), and say "below
   your likely range". The same applies to the area score key ("Band median", y 1718) and its title.
4. **The early-read sparkline looks broken, and the pending value looks settled.**
   - Sparkline: `crop-390-top.png` y 875-915 and `profile-1440-light.png` x 1060-1185, y 410-450 ("Missed
     forks"). Under the "Weekly rate" label there is one speck of 1-2 px and an end dot, with no line. It
     looks like a rendering failure, not like "too little data". For an early read, hide the sparkline or
     replace it with the same pending wording ("Weekly trend after 180 more chances").
   - Headline: the 9.0% is set at the same size as the confident rows. In light it also has the same
     weight and colour, so it reads as a settled value. Demote it visibly, with a muted colour or "~9%".
     The dark theme (`profile-390-dark.png`) mutes it slightly; the light theme does not.
5. **The KPI small multiples have marks the key does not explain.** `profile-1440-light.png`, KPI trends.
   - In each chart a small solid dot sits just left of the hollow ring (for example Accuracy x 185-220,
     y 810-818; also in the other two charts and in `crop-390-mid.png` y 203-208). The key explains the
     ring ("Fewer than 5, off the line") and the line, but not this dot. Explain it in the key or remove it.
   - The line has a gap between 31 Aug and the first report rule. A reader will ask whether that means
     missing data or zero. Add "Gap: no games that week" to the key.
   - The report key ("1 21 Sep 2026: Coaching report") sits under the fourth, empty cell (y 1020-1045),
     far from the rules it explains. Move it next to the key at the top of the section.
6. **The area score has a mark nobody can read, no interval, and a detached axis at 390.**
   `profile-1440-light.png` y 1700-1820, and the bottom of `profile-390-light.png` (y about 3260-3420 in
   the 358 px PNG).
   - The key lists "Your score" (thick tick) and "Band median" (thin tick). The chart shows the thick tick
     at 62, plus an open bracket from 50 to about 61 hanging under the axis. I cannot find a thin median
     tick, and the bracket is not in the key. If the bracket is the band median or a range, the key must
     draw it.
   - The score "62" from "22 rapid games" has no interval, unlike every other value on the page.
   - At 390 the 30 / 50 / 70 labels sit above the "Endgame / 62" line, about 70 px above the axis they
     label, so they read as a separate row.
   - The section title "Area score, next band not measured" reads like the name of a test state, not a
     heading.

### Polish

1. **The move glyphs are hard to place.** `profile-1440-light.png`, board 1 at x 205, y 1275 and board 2
   at x 480, y 1363. The `?` and `??` badges sit on a corner shared by four squares (d5/e5/d6/e6, and
   g5/h5/g6/h6), not clearly on the destination square. Anchor each badge to the top-right corner of its
   destination square.
2. **The board coordinates are tiny.** On all boards the rank digits and file letters are about 7 px at
   1440, and smaller at 390. In the light theme the file letters at the bottom-right of the dark squares
   have low contrast. Raise them to about 10-11 px or move them outside the board.
3. **The exemplars have little context.** The captions are good chess prose, but they do not say which
   weakness the position illustrates or which game it came from (opponent, date). On the profile page,
   tie each exemplar to its weakness row. The flipped second board is right for "4...", but a small
   "Black to move" or orientation cue would help.
4. **A caption splits its number pair.** `profile-1440-light.png` y 1530-1560: at 1440, in a 370 px
   column, "Win chance / 52% to 36%." breaks across two lines. Keep "52% to 36%" together with
   non-breaking spaces, or widen the column.
5. **Empty space at 1440.** The example positions use two 370 px boards and leave about 400 px empty on
   the right. The area score axis runs the full 900 px for three labels. A third column or a max-width for
   the section would look more intentional.
6. **Onset dates wrap mid-date at 390.** `crop-390-top.png` y 310-333 ("Onset detected 14 Sep / 2026"),
   and the same for 20 Aug and 2 Jul. Keep the date on one line with non-breaking spaces, or drop the year
   when it is the current year.
7. **The KPI y-axis labels are awkward.** The extent values (78.9%, 67.7%, 1.99, 0.91) are not round
   numbers. The bottom label crowds the first x label ("67.7% 31 Aug", `crop-390-mid.png` y 262). Round to
   tidy ticks and give the x labels their own row. In the light theme the stored-range band has low
   contrast on the grey plot panel: darken the band slightly or drop the panel fill.
8. **Some small labels have low contrast.** "Weekly rate" (about 11 px, muted grey) in the light theme
   and "Onset detected ..." in the dark theme are near the limit. Check them against 4.5:1.
9. **Touch targets and affordance.** If the weakness rows or the report numbers (1, 2) link anywhere,
   nothing shows it, and the report markers are well under 44 px. If they are not interactive, this is
   fine.

### Where I differ from the implementer's pass 1 (read after forming the above)

- The implementer left the lone dots of the early-read sparkline open as "the Sparkline's existing
  treatment". I rate them Important (item 4).
- "AreaScore without a next band draws no next tick" is correct. The bracket that remains is still not in
  the key (item 6).
- The implementer says the KPI key draws every mark. The lone solid dot before the ring and the gap in the
  line are not in it (item 5).
- I agree with the implementer on the exemplar chess (the Blackburne Shilling Nxe5? and Qg5, and the h6??
  Nxf7 idea), and the arrows match the captions.

## Fix round 1 (implementer)

Inputs: critic pass 1 above (6 Important, 9 Polish) and the code review `review-t4.md` (2 Important, 8 Minor).
Shots in the session scratchpad `m5b-t4/fix1/` (`profile-{390,1440}-{light,dark}.png`, 390 also cut into
`-partN` crops), reduced motion. No overflow at 360, 390 or 1440; no `NaN|undefined|Infinity` in the section.
`pnpm design:check`: 0 findings.

Important, all fixed:

1. **Interval bars on one scale.** `weaknessScaleMax(rows)` gives a list one scale from 0; rows take
   `scaleMax`. Each strip labels its ends (`0%`, `20.0%`), so equal ranges no longer look equal by default.
2. **Rates say what they count; the range is labelled on screen.** Visible "Estimated % of <chances>" over
   the figure, "80% range 3.1–5.6%", and counts with nouns ("31 errors in 263 rook-endgame moves") via a
   `counts` prop (default "times in chances").
3. **The band is named.** `band.label` ("Players rated 1400–1600"; default "Players in your rating
   band"): "Players rated 1400–1600: 2.1%, lower than your whole likely range". A `WeaknessRowKey` draws
   the strip's three marks. No reference: "No rating-band reference yet".
4. **Early reads look provisional.** "Early estimate" label, the figure smaller, muted and dash-underlined,
   no strip, and no sparkline: "Weekly trend after 180 more fork chances". With no chances at all: "No
   chances yet", no figure. A settled row with fewer than two measured weeks says "Weekly trend after more
   weeks" instead of the sparkline's specks.
5. **KPI key explains every mark**, in the grid's unit (`nUnit`): line, "A week on its own, between gaps"
   (the lone dot), hollow ring, "Gap: no games that week", stored 80% range (R2 bands are 80%:
   `INTERVAL_LEVEL`/`Z80` in `profile-core`), report rules. The numbered report list moved up beside the key.
6. **Area score.** Optional `interval` (`{low, high, level}` drawn as a `pencil` band and stated "80% range
   55–68"; `null` says "No interval yet"; absent says nothing, for existing hosts). The key adds "Distance to
   the band median" (the connector) and "Likely range". Below 560px each ruler carries its own numbering
   under it, so the scale no longer floats over the labels. Title now "Area scores without a measured next
   band".

Polish and review minors:

- Fixed: badges sit inside their destination square (inset), the reply is named ("Their reply, Qg5"), the
  key names played and better moves, "52% to 36%" never splits (no-break spaces), win chances clamped to
  0–100, onset dates never wrap mid-date, KPI y-axis ends are tidy numbers with the date row under the
  plot only, the KPI band is a step darker (35%), the headline date is a `<time>`, `headingLevel` prop,
  the narrow weakness row stacks in one column, small labels moved from `ink-faint` to `ink-soft`.
- Parked: board coordinate size (Polish 2) is the board's own ruled treatment (DESIGN.md "The board"), not
  this kit's; exemplar context (game, opponent, which weakness; Polish 3) belongs to the host pages T6–T8,
  which place each board under its weakness; the empty space at 1440 (Polish 5) is the styleguide demo's
  layout; report markers are not links (Polish 9), so no target is needed. "Your win chance" wording (M8)
  stays as the brief's "Win chance".
