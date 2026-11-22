# Progress page (`/app/progress`): critique log (M5b Task 10)

Screenshots are not committed. They were taken from the compose `web` dev server (project
`ci-m5b-t10`, no host ports) through the `qa` container, of a temporary, uncommitted lab route that
rendered `ProgressView` with synthetic read-model objects (no player data), at 390 and 1440, light
and dark, reduced motion. Two cases, as the plan asks:

- **rich:** 214 analysed rapid games, 14 KPI weeks with stored bands, a week with no games, thin
  weeks (hollow), one report date as an event, three weaknesses (improving, relapsed, emerging), and
  three weekly reviews (one written, one being written, one that could not be checked);
- **new:** 9 analysed blitz games over two weeks, one metric with no games, no weaknesses, no
  report and no weekly review (the "first review on 23 Nov 2026" state).

At 360 and 390 neither case had page-level horizontal overflow (`scrollWidth` equal to the
viewport), and the page text had no `NaN|undefined|Infinity`.

## Pass 1 (implementer self-critique; self-critique only)

The implementer was told not to dispatch subagents, so the fresh critic (`critique-prompt.md`,
PNGs only, at least 2 passes) is still owed: **the controller runs it and appends its verdicts
here.**

Issues found and fixed:

1. **Five copies of the chart key.** `KpiTrendGrid` takes one `minN` and one `nUnit` per grid, and
   the KPIs differ in both (20 games, 15 games with an episode, 40 low-clock moves, 30 chances), so
   the page needs one grid per threshold. Each grid drew its own key and report list. Now only the
   first group ("Accuracy and blunders") shows the key and the report list; the others hide theirs
   and state the one thing that differs in a line ("hollow weeks had fewer than 40 low-clock
   moves"). The rating says what its rule 1 is in its own note.
2. **The rating sat in a third of an empty sheet**, with a heading that repeated the section's. The
   section is "Rating over time"; the chart takes a 36rem column and the note sits beside it.
3. **One rate group alone on a row.** The three rate groups are a 2-column grid at `lg`, with
   "Converting and saving" spanning two rows beside the two single-metric groups.
4. **Tabular figures in the before-and-after sentence** spread the stops and colons ("16 . 0% ,").
   The sentence uses proportional lining figures.

Checked and kept:

- Before and after says "detected improving" and "detected relapsed", never "fixed", and gives the
  rates from the transition row with the chances behind the estimate. The ring and dot share one
  scale from 0 for every row, and the state mark repeats the timeline's pattern and colour.
- The rating is labelled "Your recorded rating; it moves for many reasons." and nothing on the page
  links it to training.
- Weekly reviews show model text only when the review is `ready` with its stored disclosure
  ("Written by AI", the date, and the note behind the info button).

## Pass 2 (review-t10 pass B, needs-changes; fixes by the implementer)

Retaken shots (rich, empty two-week account, pending review; 390 and 1440, light and dark) are in
the T10 scratchpad `fix1/`. No overflow at 360 or 390, and no bad text.

1. **Thin reads looked settled (Important).** A metric with no week at its minimum is no longer a
   chart with a big headline. It is an early-read row at body weight ("Early read: 78.0% from 5 games
   in the week of 16 Nov 2026. A week counts from 20 games."). With fewer than 3 weeks the page draws
   no charts at all: one note ("Trends appear after 3 weeks of games. You have 2 so far.") and every
   metric's early read.
2. **The dumbbell was illegible for small rates (Important).** Each row now has its own scale from 0,
   with a 0 tick and both values labelled above their marks (the lower one ending at its mark, the
   higher one starting at its mark), and the legend says "Each row on its own scale from 0".
3. **Before and after date and n.** The date is the change point from the same-game onset row, with
   its likely range. The after side shows its stored count ("After: 12 of 460 chances since 14 Oct
   2026"). The before side says plainly that its count isn't stored. It no longer cites the six-month
   estimate's n.
4. **Pending review.** The state line is at body weight under the week heading, and the shot data
   uses a finished week (9 Nov).
5. **Group captions** name what the threshold counts ("fewer than 15 games with a winning or lost
   position").

Left to `@ci/ui` (hand-offs): bands that run through hollow weeks, the decimals on y-axis ticks and
their 11px `ink-faint` labels, and a `showKey` or per-item `minN` prop to replace the CSS hide.
