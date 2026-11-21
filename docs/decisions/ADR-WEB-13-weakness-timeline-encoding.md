# ADR-WEB-13. Weakness timeline and area score encoding

Date: 2026-11-21
Status: Accepted

## Context

Task 15 builds the area score row and the weakness timeline swimlane (spec §12b). The owner wants to see when a mistake starts, when it improves and when it comes back. Six states must read apart without colour alone (DESIGN.md), and red and green keep their chess meanings.

## Decision

- **State encoding.** Red means the weakness is a problem now, and green means it is getting better. Inside each hue, pattern and bar shape separate the states:
  - `not_an_issue`: a `rule-strong` hairline, with no bar.
  - `emerging`: `pen` dots.
  - `active`: a solid `pen` bar.
  - `relapsed`: a `pen` cross-hatch.
  - `improving`: a `good` hatch at 45°.
  - `resolved`: a solid `good` band, thinner than a bar.

  The patterns are SVG `<pattern>` elements filled with token colours over the lane's `well`: `pen` on `well` is 4.14:1 day and 6.11:1 night, and `good` on `well` is 5.56:1 and 10.20:1. Each lane label also shows its current state as text ("Now relapsed"). A hidden paragraph per lane lists every state and date for assistive tech.
- **Calendar axis.** Every month is the same width, and days spread evenly inside their month. Reports and plans run by the month, the month ruling stays regular, and the brief's check holds: one month in a two-month range is exactly 50%. The cost is a skew of under 2% between months of different lengths.
- **Geometry in percentages.** Lane x positions are SVG percentages, so nothing is measured and the layout never shifts. Below a 640px container, the lanes scroll sideways inside their own named, focusable region. The labels stay pinned, and the region opens on the latest months.
- **Events.** A plan is a `pencil` diamond and a drill a filled `pencil` square. Each runs as a dashed `pencil` rule through every lane. Its label is a numbered note under the chart, so labels never collide.
- **One clock each.** The timeline reveal is a `sheet` cover. It slides off on `--wipe` (`dur.reveal`, `ease.pen`) as a transform only. The area score needles share one `--sweep` value. Under reduced motion both values are 1 from the start. Input during the reveal finishes it over `dur.quick`.
- **Area score.** The list is unruled: DESIGN.md keeps ruling for scoresheet surfaces. Its numbers sit on one shared 0 to 100 scale. Each mark has its own shape:
  - the player: a tall `ink` needle;
  - the band median: a short `pencil` tick that hangs below the scale;
  - the next band: a `good` flag that stands above it.

  An `ink`/15 tint spans the gap between the median and the player.

## Consequences

- When real data lands (M2), very long ranges thin the month labels to about 12, but lanes stay one row per series. Twelve or more series make a tall chart. Filtering is left to the Progress page.
- The hover readout is pointer-only. Keyboard and screen reader users get the same dates from the per-lane text.
