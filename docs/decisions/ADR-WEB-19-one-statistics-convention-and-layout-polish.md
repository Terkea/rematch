# ADR-WEB-19. One statistics convention, and the critic's layout fixes

Date: 2026-11-22
Status: Accepted (web polish, after the M4 T28 independent critique)

## Context

An independent critic, judging the T28 baselines from the screenshots alone, found 8 Important issues. Two were about honesty:

- The overview said "Need 9 more games" below 10 games. Meanwhile the styleguide's KPI cards showed a 53.0% headline from 6 games, and one card said "90% likely" while every other said 80%.
- The accuracy trend's caption says "your average over the last 20 games", but the line started at game 10 (ADR 0047 `TREND_MIN = 10`), so its first ten points averaged fewer.

The other six were layout issues:

- the auth pages' board sheet stopped 15px short of the right edge;
- the mistake columns filled about 70% of their row;
- the overview's "Mistakes per game" rows had no shared baseline;
- the review had a dead gap above the engine switch;
- the area score drew a "next band" flag below the player's own score;
- the games list had a date heading over nearly every game.

## Decision

- **One rule for every KPI.** ADR 0047 R3 and ADR-R2-5 set the rule: 10 games minimum (`KPI_MIN_GAMES`) and 80% intervals everywhere (`INTERVAL_LEVEL`).
  - `@ci/ui/stats` now exports both constants, and a web test pins the server's `KPI_MIN_GAMES` and `CONFIDENCE` to them.
  - `KpiStat` shows no number, band or trend below 10 games. It shows the basis and "Need N more games", as the overview's `EstimateKpi` does.
  - The interval type is `level: 0.8` only. The 90% path and its 1.2816 / 1.6449 rescale are gone, and the 90% fixture is now an 80% one.
  - "Early estimate" stays, for a KPI with at least 10 games that is still below its metric's own minimum evidence.
- **The rolling line starts on a full window.** `TREND_MIN = TREND_WINDOW = 20`, so "the last 20 games" is literally true for every point. The caption before then reads "The line starts once you have 20 analysed games". The styleguide's synthetic trend follows the same rule. This amends ADR 0047's `TREND_MIN = 10`.
- **The auth pages bleed:** `html:has([data-bleed])` drops `scrollbar-gutter: stable`. The gutter exists to keep the app header still, and the signed-out pages have none.
- **Mistake columns span the row:** each game's slot is the width divided by the number of games. Its block stays at most 24px wide and sits centred in the slot.
- **KPI rows without a ruler** (`scale={false}`, the overview) put the value on the label's baseline, the interval words under the value, and the trend at the right edge.
- **The review's engine switch sits above the verdict**, under the move line, so no reserved space separates the text from it.
  - The verdict keeps its reserved height (I2: the clocks and ledger don't jump), and what is left of it now falls before the clocks.
  - A first try put the switch under the text and reserved height on the whole block. The switch then moved whenever the verdict changed, and the reveal's 390px layout-shift check failed (CLS 0.025 against 0.02).
- **The area score hides the next-band flag once the score reaches it.** The row then says "In the next band (71 and up)", and so does its accessible text.
- **The games list, sorted by date,** names each day once, in a Date cell that spans that day's rows. There is no heading row per day. Phone rows always carry their date inline.

## Consequences

- A user is never told that one game means nothing while six games get a headline, and every interval on screen is at the same level.
- A new account gets its accuracy line at 20 analysed games instead of 10.
- The overview, games, review, sign-in and blocked baselines change, and so do the styleguide's stats, charts, area-score, timeline and sound sections. They were regenerated on the production build.
