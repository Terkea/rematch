# 0079. Explained stats: definition lines, an explanation popover and the weakness story

Date: 2026-11-22. Status: accepted.

## Context

The owner said the stats pages "don't really say much": opening a weakness did not say what was going on, and names such as "Not winning won games" had no explanation. The glossary (`apps/web/src/server/profile/glossary.ts`, kept accurate by its drift test) now holds the meaning, measurement, reason and tip for every weakness, KPI, area, state and term.

## Decision

1. **`ExplainPopover` and `Meaning` in `@ci/ui/profile`.** An "i" button (`aria-expanded`) beside a name opens the glossary entry (meaning, how it's measured, why it matters, what to try) in the existing `sheet` popover. It opens on click or tap, never on hover alone, and Escape or an outside click closes it with focus back on the trigger (Base UI). `Meaning` is the one-sentence meaning in muted text under the name. `WeaknessRow`, `KpiTrendItem` and `KpiStat` take an optional `explain` entry. The package never writes a definition: the host passes glossary text.
2. **Definition lines everywhere a name is shown:** the weaknesses list, the area page's title, measures and weaknesses, the overview's accuracy and blunders, progress (KPI charts, early reads, before and after) and the report's weakness and area sections. A series with no glossary entry (an allowed motif, an opening line, an endgame type) shows none rather than a borrowed one (`seriesGlossary`).
3. **The weakness detail page is a story,** top down: what this is, how often it happens to you (in words with n, the 80% range, and against your level only where a benchmark exists), better or worse (from the stored state and `p_imp`, "too early to tell" below the evidence minimum), examples from your games, and what to do. The previous charts sit under "Details", unchanged.
4. **Examples come from the report first, then the stored occurrences:** `game_metrics` exemplar plies of the series, the player's own moves within the estimate's window, newest game first, one per game, at most three (`WeaknessDetail.examples`). An example shows the move played (pen) and the engine's move (good) as two arrows, one line on the win chance before and after, and a link to the game at that ply. With no engine move stored there is no second arrow, never an invented one.
5. **"What this means for you"** under the overview headline and each area headline comes from the pure functions in `apps/web/src/lib/explain.ts`, built only from the value, interval, n and benchmark state. They translate a number into its meaning in a game or say exactly why they cannot yet; nothing is called better or worse unless the ranges separate.
6. **Opening verdicts explain themselves (I9).** Each verdict in the report carries an "i" with its glossary entry (`VERDICT_GLOSSARY`, whose thresholds mirror `profile-core/src/openings/verdicts.rs` and fail a test when they drift) and one line on why this line got it, from the stored `opening_lines.summary` (`LineSummary`) in the order `decide_verdict` applies its rules: the repeated costly move and its count for Fix; the score against what the rating predicts, in pp with its 80% range, for Keep and Replace; the early exit from theory, the worse position or the low score for Learn deeper. When the stored line no longer gives the verdict the report shows, the line falls back to the verdict's meaning rather than quote evidence for another verdict.

## Consequences

- The overview has no rating-band benchmark for accuracy or blunders, so its line says "Not compared with players at your level yet."
- The detail page's examples no longer name the report they came from, since they may also come from the stored occurrences.
- The visual baselines of the pages with definition lines change.
