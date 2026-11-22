# 0055. The overview page: honest rows, a broken trend line, a server-safe band, a session reveal

Date: 2026-11-22. Status: accepted (M5a Task 8). Builds on 0047 (honest statistics), 0050 (chart encodings) and 0053 (variant b).

## Context

Task 8 turns the overview lab's variant b into the real `/app`. The T6 critic and review left binding hand-offs: break the rolling line across months with no games, fix the misleading range rulers and bare sparklines, state n less often, a shorter phone headline, dark-mode weight, empty states, a red-pen "What to work on" slot that stays empty until the R2 profile exists, a time-control filter, a recent-games table, `bandText` usable on the server, and a narrow view model for client code.

## Decision

1. **No ruler on overview KPIs.** `KpiStat` gains `scale={false}`, and `EstimateKpi` always passes it. A per-game rate has no meaningful reference range here (zero to "15% past the band" put every marker at about 75–80% across, which reads as "near the worst"), so the band is stated in words only. The styleguide and other hosts keep the ruler by default.
2. **Sparklines carry their ends and direction.** `KpiStat` gains `trendEnds` (the first and last weekly values under the line) and `better` ("Lower is better"): a convention, never a verdict. A one-line note beside the rows says a gap is a week with no games.
3. **The trend breaks across empty time.** `AccuracyTrend` gains `gapDays`; the overview passes 28, so the rolling line and band stop where four weeks or more passed with no games, and the caption says so. The chart's default is unchanged.
4. **`bandText` and its siblings move to `@ci/ui/stats/band.ts`,** a module with no `"use client"`, re-exported from the barrel. A server component can call them. Constants shared across the boundary (`REVEAL_COOKIE`) live in plain modules for the same reason.
5. **The page is a server component.** Only the accuracy trend is a client island, and it gets `AccuracyView`: ids, ISO dates, accuracies, results, captions without opponents, review links. `KpiStat` receives numbers only. The recent-games table is server-rendered.
6. **n once per section, for estimates and shortfalls alike** (fix round 1). The headline states n; "Mistakes per game" states it once, and each class in "By time control" states its own once. A row on the section's basis keeps its basis for screen readers only (`basisHidden`). Below the minimum, the section states "Need N more games…" once and its rows show a dash, with the basis and the shortfall kept for screen readers. A row on another basis (for example a class's accuracy when some games have none) shows its own basis and shortfall visibly.
7. **The filter is links.** `?tc=` with `aria-current`, so it survives a reload and needs no client code. An unknown or unanalysed class falls back to All.
8. **The report reveal is CSS, gated by a session cookie.** The server renders `data-reveal="first"` when `ci_overview_seen` is absent; a client effect then sets it (no max-age, so it ends with the browser session). The ruled lines draw on `scaleX` and the heading and figures appear at 60% (CSS keyframes, so the server render is already the first frame and nothing waits for JavaScript), the trend draws in, and `Stat` rolls up from zero only while its `[data-reveal-fig]` wrapper is still transparent. First viewport only; nothing under reduced motion.
9. **"What to work on" stays empty.** It names the slot and says the coaching profile is still being built. No finding is guessed from raw judgement counts.
10. **A lone game is plotted.** `AccuracyTrend` gains `minPoints` (default 2, unchanged); the overview passes 1, so a one-game account shows its dot while the line waits for 10 games. Date ticks are now one per day, on midnight, so a short span never repeats a label.
11. **The nav says "Overview"** (ruling), matching the page's title; "dashboard" stays a command-palette keyword.
12. **Dark mode.** The trend band steps to `pencil/60`, dense loss rings to `ink-faint`, and the wins bar to `ink-soft`, so the white bar is no longer the heaviest mark on the page.

## Consequences

- One more prop each on `KpiStat`, `Stat` and `AccuracyTrend`, all optional and off by default.
- The headline is rendered twice (a short phone sentence and the long one), one hidden per breakpoint.
- The lab under `/lab/overview` is unchanged and still dev-only.
