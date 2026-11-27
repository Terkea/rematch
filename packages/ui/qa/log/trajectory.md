# Rating trajectory (Progress first section, Overview card): critique log (F6, ADR 0089)

Surface: `TrajectorySection` (`apps/web/src/app/(app)/app/_trajectory/section.tsx`) with
`RatingTrajectory` (`@ci/ui/charts`), the then | now `CompareBlock`, and `TrajectoryCard` on the
Overview. Styleguide demo: `/styleguide#trajectory` (synthetic games, server-rendered read model),
captured by `e2e/visual.spec.ts` (section `trajectory`).

## Pass 1 (implementer self-critique; self-critique only)

The implementer was told not to dispatch subagents, so the fresh critic pass
(`critique-prompt.md`, PNGs only) is still owed: **the controller runs it and appends the verdict
here.** Docker stopped answering during the visual run, so no screenshots were taken in this pass.

Form (dataviz): one series over time, so a single ink line with no legend (the title names it); a
single categorical role, no palette to validate. The peak and current marks carry direct labels; the
compared windows are `pencil` washes named on the plot; gridlines `rule`, axis `rule-strong`. Text
uses ink tokens only. A crosshair readout follows the pointer and the arrow keys (the plot is one
focusable slider), and "Show as a table" is the text view (month, rated games, last and highest
rating).

`pnpm design:check`: 0 findings.

Issues found and fixed in code review of the render:

1. **Window labels could collide** when the peak window sits next to the last games (a recent
   peak, or a short history). They now shorten to "Then" and "Now", and if they still meet they part
   either side of the gap (`windowLabels`, tested).
2. **A level line read "peaked at 1,500 … and is 1,500 now (±0): no clear change from your peak
   period"**. It now reads "Your rapid rating is 1,500 now, ±0 from its peak of 1,500 in …: no
   clear change since the games around it."
3. **The headline and the "since then" sentence used no arrows** (DESIGN copy rule): "accuracy
   69.1% then, 66.0% now (no clear change)".

Owed: screenshots at 390 and 1440, day and night, the horizontal-scroll check at 360, and the
fresh critic's verdict.

## 2026-11-27: algo-6.0.0, the verdict back (EXP-0030, ADR 0117)

The rating row has its verdict and ranges again. The headline compares the windows' averages ("Your rapid rating averaged 663 over your last 30 games, down from 864 over your first 30 (−201).") and a plain line under it gives the current rating and the highest ("It is 540 now. Your highest was 1,111, in March 2026."). The shaded spans are the first and the last 30 games. Shots of the styleguide's trajectory section at 390 and 1440, day and night (`.shots/trajectory-*.png`, a section-only capture; no horizontal scroll at either width). `pnpm design:check`: 0 findings. A fresh critic saw only the screenshots, DESIGN.md and REFERENCES.md.

- Round 1, revise. Major: the shown differences did not add up from the rounded numbers beside them (864 → 663 shown as −202; 27% → 43% as +17). Minors: the headline ran to three sentences; the subtitle still said "then"; and six that predate this change (the weakness row's change has no range; "clearly worse" in the summary against "Worse" in the table; a heavy day fill on the spans; the then | now captions off the table grid; a narrow Change column; crowded chart labels at 390). Fixed: every shown difference is now taken from the rounded values shown (headline, rating row, weakness row, with a test that they add up); the headline is the comparison only, with the facts on their own line; the subtitle reads "how your first games compare with your last".
- Round 2, revise. Blocker: the 1440 night chart was empty, a capture taken before the chart measured its width. Minor: at 390, "(−201)." wrapped alone. Fixed: a no-break space keeps the difference with its numbers; the shots now settle for 3 seconds.
- Round 3, pass. The six minors that predate this change remain, deferred to the then | now component and the chart, which other surfaces share.
