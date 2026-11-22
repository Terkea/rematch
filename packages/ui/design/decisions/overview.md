# Overview layout

Date: 2026-11-22. M5a Task 6. Status: **ruled.** The independent critic ranked the variants **b > c > a** from screenshots alone (no code and no reports): `.superpowers/sdd/2026-11-21-05a-game-pages/critique-t6.md`. That ranking matches the implementer's, and variant b is the pick.

Lab routes: `/lab/overview/a`, `/lab/overview/b` and `/lab/overview/c`. All three read the signed-in user's real games through `loadLabOverview` (`requireUser` plus `getOverview`), and the lab still 404s in production (`labEnabled`). They were built and shot on the owner's analysed games, restored read-only into a throwaway compose project. Neither the owner's data nor its figures are committed. The screenshots are in the git-ignored ledger folder:

`.superpowers/sdd/2026-11-21-05a-game-pages/shots-t6/overview-{a,b,c}-{390,1440}-{light,dark}.png`

All 12 shots were taken on localhost after hydration and after the trend's draw-in sweep had finished. Phones (390) used a touch context. None had horizontal scroll. Figures below, such as "N analysed games" and "X%", are placeholders.

## The three variants

| Axis | a, "Scoresheet column" | b, "Chart-led" | c, "By time control first" |
|---|---|---|---|
| Spine | one reading column (`max-w-3xl`) on a sheet, ruled like a scoresheet | the accuracy trend, full width on a `sheet` band | the time-control rows |
| Head | a scoresheet's header fields: Account, Last import and Games, each over a `rule-strong` line | one Literata sentence with the headline numbers and n, plus their bands in words | a status line and one sentence for all time controls |
| KPI rows | each ruled row carries its chart: accuracy with the trend, blunders with the judgement columns | from `lg`, the KPI rows (7 of 12 columns) sit beside the judgement columns and the results (5 of 12) | inside each open row: that time control's accuracy and trend beside its judgement rows and columns |
| Time controls | a ruled row near the end: result bars, then a score row per class | the right column, under the judgement columns | the spine itself: name and games, result bar, and score with band and n; the largest class opens first |
| Recent games | ruled list rows | not shown | not shown |

They share the honest pieces:

- `EstimateRow` shows one of three things, always with its n: a value with its band; a value with no band and a sentence, when the interval cannot be estimated; or "Need N more games" with no number.
- The empty states: no linked account, no games, and none analysed.
- `TrendWindow`: phones show the latest 100 games, with "Show all" (the T4 ruling).

Variants b and c are client components. They receive only a narrow, JSON-plain view model (`toViewB` and `toViewC` in `view.ts`), built on the server, so no raw rows, opponents or account ids cross the boundary. `lab-overview-boundary.test.ts` holds them to an allowlist of keys (fix round 1, review-t6 Important 1).

## The critic's ranking (critique-t6.md)

1. **b.** It is the only variant with a clear point of view. The serif lead sentence, with its bands directly under it, "reads like a coach's note on a scoresheet", and it is premium and honest at once. The full-width timeline gives the dots room to breathe, and the two-column body uses 1440 properly.
2. **c.** It makes the most analytically honest structural choice, because it stops blending time controls. But as a page it falls short: the rounded panel reads as a generic card, the left column dies under the scatter, and half the data hides behind a chevron.
3. **a.** It has the most scoresheet idiom, but it is a narrow column floating in 1440. Its scatter is crammed, every metric has the same weight, and on a phone it runs to about 3000px.

The implementer ranked them the same way, for the same reasons.

## Decision

**Variant b** for Task 8. These are taken from the losers, as the critic recommends:

- **From c:** the time-control split, as a ruled segmented filter (All, then each class with its count) that drives the headline, the timeline and the per-game rates. Not as an accordion. Also c's summary row for "By time control": the results bar, a large score, its band and n.
- **From a:** the Recent games list, as a ruled scoresheet table (date, result, opponent and rating, accuracy), and the note "Score counts a draw as half a win". The ruled idiom goes on section heads and table rows, not on header metadata.

The critic's other changes for b are hand-offs in `task-8-brief.md`, not built here:

1. Answer "what to work on" with one red-pen lead line.
2. Fix the KPI rulers and sparklines, which mislead.
3. Stop the rolling average from bridging months with no games, and align the chart's y axis to the content column.
4. Cut the repeated n and the repeated headline numbers.
5. Use a smaller lead size on phones.
6. Readability fixes to the judgement columns and to dark-mode contrast.

## Notes for Task 8

- `bandText` and the other `@ci/ui/stats` helpers live in a client module. A server component cannot call them; b's first render failed with "Attempted to call bandText() from the server". Either map to view models and keep the islands small (as b and c now do), or move the pure text helpers out of the `"use client"` module.
- `AccuracyTrend` draws in over 2 × `dur.reveal`. Under `next dev` the sweep can still be running at `networkidle`, so screenshots must wait for the clip width to reach the SVG width (the T4 deferred note).
- Task 2's per-game points carry no opponent, so a trend dot's caption is "<day>, win, accuracy X%". Task 8's `gameCaption` (with "v Opponent (1520)") applies to `recent` rows only.
