# 0050. Overview chart encodings

Date: 2026-11-22. Status: accepted. Amended in Task 4 fix round 1 (rolling nulls, dense dots, nearest-dot taps).

## Context

The overview page (M5a Task 8) shows a player's analysed games as three charts in `@ci/ui/charts`: accuracy over time (`AccuracyTrend`), the player's judged moves per game (`JudgementColumns`) and results by time control (`ResultSplit`). The owner has about 500 analysed games, so the accuracy chart must stay readable at 500 dots in a 320px column. DESIGN.md fixes the palette: `pen` red means the player's own mistake, `pencil` is the opponent's or an idea, `good` is the better move, and red and green are never series colours. Colour must never carry meaning alone. The honesty rules (spec §12a) say no number comes from zero evidence and every count is visible.

## Decision

**R11: result by dot shape.** `AccuracyTrend` plots up to the latest 500 analysed games (the page says so when it truncates). Each dot is a game, on a UTC time scale, with accuracy 0–100 and gridlines at 25, 50 and 75:

- a win is a filled `ink` disc;
- a draw is an `ink` ring with its left half filled (a vertical split still reads at 3px);
- a loss is a hollow `ink-soft` ring;
- an unfinished game is a `pencil` cross.

The radius is 3px, or 4.5px on a coarse pointer. Every dot sits on a 1.5px halo of the host surface (`--chart-surface`, default `sheet`), so dense clusters stay countable. **Dense plots** (more than 0.5 games per px of plot, such as 500 games on a phone) keep the 3px radius even on a coarse pointer and step every tone down one: `ink-soft` fills and rings, and `rule-strong` loss rings. The 2px `ink` mean on its halo then stays the strongest mark. Findability on touch comes from the nearest-dot overlay (below), not the drawn radius.

The legend repeats each shape with its count ("Win 244"), so n is always on the page. A game whose accuracy is not a finite number is not drawn and not counted in the legend. A note says so instead ("1 game has no accuracy and isn't shown."). With fewer than two drawable games the chart is replaced by "Not enough games yet". Input may come in any order; the chart sorts by date, so the x order, the keyboard order and the rolling runs always agree. A visually hidden summary gives the games, their span and the latest rolling average with its 80% band, with the ends rounded outward.

**The rolling interface (amends the brief, ruling I0).** `rolling` is `{ id; mean: number; low: number | null; high: number | null }[]`, matching Task 2's `rollingMean`, which returns null ends when a window has no spread. The mean line and the band break separately:

- the mean runs wherever an entry has a finite `mean`;
- the band runs only where both ends are finite and `high > low`.

A null end therefore shows the mean with no band under it. A band is never clamped to 0, never one-sided and never zero-width. Both break where `rolling` has no entry, so a gap or a thin window is never bridged with invented data.

The mean is an `ink` 2px line on a 5px surface halo. The band is `pencil` at 45% alpha. That reads clearly on `sheet` in both themes (the night `pencil` is lighter than `rule-strong`), and the earlier `rule-strong` at 25% vanished at night.

**Pattern redundancy.** Every series differs by shape or pattern as well as tone:

- `JudgementColumns`: one unit cell per move, stacked from the baseline. Blunders are solid `pen`, mistakes are a `pen` hatch with a `pen` outline, and inaccuracies are a `pencil` outline. A clean game is a 1px `rule` tick, so it reads as "zero" rather than "missing". Counting cells gives the exact number, so there is no value axis.
- `ResultSplit`: wins solid `ink`, draws an `ink` hatch, losses an outlined `sheet` segment. The words ("8 wins, 2 draws, 4 losses") always carry the counts; a row with no games is not drawn.
- The hatches come from `<ChartPatterns prefix />` (`hatch-pen`, `hatch-ink`). Each chart renders its own copy under a `useId` prefix, so hiding one chart never removes another's fill.

**Red only for the player's own marks.** `JudgementColumns` counts only the player's moves, so its red means what red means on the board and the eval graph: "your mistake". The opponent's mistakes are not plotted here. Results use the tonal ink ramp, never red or green, because a loss is not a chess mistake and a win is not a "good move".

**Interaction.** Pointer and touch go through one transparent overlay. It resolves the dot **nearest** the tap by Euclidean distance (a binary search on x, then a distance check) within 12px, or 22px on a coarse pointer, and opens the one shared popover anchored at that dot. A tap far from every game does nothing. The chosen dot gets a 2px `ink` selection ring; on a fine pointer the nearest dot under the cursor gets a 1px `ink-soft` hover ring. The popover shows the caption, a "Review game" link, and "Previous game" / "Next game" buttons, so every game in a dense cluster is reachable by touch.

For the keyboard and screen readers each game also has a `pointer-events: none` button, named by its caption. The buttons are one tab stop: arrow keys walk the games in date order, Home and End jump to the ends, and Enter opens the popover. The dot layer and the buttons are memoised, so a key press re-renders two buttons, not 500 dots.

**Judgement columns** are links to their games, named by the game's label and described by their counts ("2 blunders, 1 mistake, 3 inaccuracies"). A visually hidden total gives n ("40 games: …"). The columns are one tab stop with the same keys. The latest game sits at the right edge, under the trend's latest dot. When the columns scroll (below 480px), the chart opens scrolled to the latest games, and the oldest edge fades over 24px to show that there is more history. With no games it shows "No analysed games yet" in a `well`, with no plot and no legend. `ResultSplit` reads counts as whole games and renders nothing when every row is empty; the host owns that state.

**Motion.** `drawIn` sweeps one clip over 2 × `dur.reveal` on `ease.pen`, and each dot fades in as the sweep passes it (opacity only). The clip rect's `width` attribute is set from the motion value rather than through a CSS transform on a `<clipPath>` child, which WebKit has not always repainted. Under reduced motion the clip starts at its final width and nothing animates (M10).

## Consequences

- At 500 games on a phone the dot cloud is dense, so it is toned down; the mean line carries the trend, and the popover steppers carry the detail. The overview page (Task 6/8) defaults narrow screens to the last 100 games, with a "Last 100 / All 500" control.
- Judgement columns keep a 12px least width, so 40 games need 480px; narrower containers scroll inside the chart, never the page.
- Changing a result's shape or a judgement's pattern is a design change: it updates this ADR, DESIGN.md and `packages/ui/test/charts.test.tsx` together.
