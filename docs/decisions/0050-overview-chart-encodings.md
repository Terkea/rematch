# 0050. Overview chart encodings

Date: 2026-11-22. Status: accepted.

## Context

The overview page (M5a Task 8) shows a player's analysed games as three charts in `@ci/ui/charts`: accuracy over time (`AccuracyTrend`), the player's judged moves per game (`JudgementColumns`) and results by time control (`ResultSplit`). The owner has about 500 analysed games, so the accuracy chart must stay readable at 500 dots in a 320px column. DESIGN.md fixes the palette: `pen` red means the player's own mistake, `pencil` is the opponent's or an idea, `good` is the better move, and red and green are never series colours. Colour must never carry meaning alone. The honesty rules (spec §12a) say no number comes from zero evidence and every count is visible.

## Decision

**R11: result by dot shape.** `AccuracyTrend` plots up to the latest 500 analysed games (the page says so when it truncates). Each dot is a game, on a UTC time scale, with accuracy 0–100 and gridlines at 25, 50 and 75:

- a win is a filled `ink` disc;
- a draw is an `ink` ring with its left half filled (a vertical split still reads at 3px);
- a loss is a hollow `ink-soft` ring;
- an unfinished game is a `pencil` cross.

The radius is 3px, or 4.5px on a coarse pointer. Every dot sits on a 1.5px halo of the host surface (`--chart-surface`, default `sheet`), so dense clusters stay countable. The legend repeats each shape with its count ("Win 244"), so n is always on the page. With fewer than two games the chart is replaced by "Not enough games yet".

The rolling mean is an `ink` 2px line on a surface halo over a `rule-strong` band (25% alpha). It breaks wherever the rolling series has no entry, so a gap or a thin window is never bridged with invented data.

**Pattern redundancy.** Every series differs by shape or pattern as well as tone:

- `JudgementColumns`: one unit cell per move, stacked from the baseline. Blunders are solid `pen`, mistakes are a `pen` hatch with a `pen` outline, and inaccuracies are a `pencil` outline. A clean game is a 1px `rule` tick, so it reads as "zero" rather than "missing". Counting cells gives the exact number, so there is no value axis.
- `ResultSplit`: wins solid `ink`, draws an `ink` hatch, losses an outlined `sheet` segment. The words ("8 wins, 2 draws, 4 losses") always carry the counts; a row with no games is not drawn.
- The hatches come from `<ChartPatterns prefix />` (`hatch-pen`, `hatch-ink`). Each chart renders its own copy under a `useId` prefix, so hiding one chart never removes another's fill.

**Red only for the player's own marks.** `JudgementColumns` counts only the player's moves, so its red means what red means on the board and the eval graph: "your mistake". The opponent's mistakes are not plotted here. Results use the tonal ink ramp, never red or green, because a loss is not a chess mistake and a win is not a "good move".

**Interaction.** Each dot is a Base UI popover trigger (one popover root, payload per dot) with its caption as its accessible name. The dots are one tab stop: arrow keys walk the games in date order, Home and End jump to the ends. The popover shows the caption and a "Review game" link. Judgement columns are links to their games, with the game's label as the name.

**Motion.** `drawIn` sweeps one clip over 2 × `dur.reveal` on `ease.pen`, and each dot fades in as the sweep passes it (transform and opacity only). Under reduced motion the clip starts at its final width and nothing animates (M10).

## Consequences

- At 500 games on a phone the dot cloud is dense; the halo and the mean line keep the trend legible, and the per-dot popover and the review list carry the detail.
- Judgement columns keep a 12px least width, so 40 games need 480px; narrower containers scroll inside the chart, never the page.
- Changing a result's shape or a judgement's pattern is a design change: it updates this ADR, DESIGN.md and `packages/ui/test/charts.test.tsx` together.
