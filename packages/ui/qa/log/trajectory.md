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
