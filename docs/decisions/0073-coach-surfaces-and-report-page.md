# 0073. Coach surfaces and the report page

Date: 2026-11-22. Status: accepted (M5b Task 6). Builds on ADR 0068 (coach read models and
scope), ADR 0069 (report read model and requests), ADR 0070 (chat hand-off), ADR 0071 (coach
text and states), ADR 0072 (profile encodings), ADR 0075 (beta AI plan, first report on build),
DESIGN.md (choreography 2, M4, M7, M10, B3, B9, B18).

## Context

M5b adds the coach surfaces to the signed-in app. The report is the first of them: it is
assembled by the worker one section at a time, can be caught mid-flight, can lose a section to
the validator, and can be computed-only when the AI is off or out of budget. The page has to say
which of these it is in, keep the engine's numbers on screen in every case, and never present a
section as the coach's when its row is not `ready`.

## Decision

**Routes.** The coach surfaces live under `/app`: `/app/report` (this task), `/app/weaknesses`,
`/app/weaknesses/[seriesKey]`, `/app/areas/[area]`, `/app/progress` and `/app/coach` (Tasks 7–10).
Every one takes `account` and `tc` (ruling R1) and reads only through the read models, which check
the scope against the user's linked accounts; a foreign or malformed choice falls back to the
default scope.

**R7: layout.** The report continues M5a's overview sheet: a serif H1, ruled section heads
(`SectionHead`), the area rulers on a `sheet`, and unruled lists everywhere else. No card grid,
no stat banner (B9), no hairline between every row (B3).

**Section order.** The state banner; "Where you stand" (rating and band, the basis line "Based on
214 analysed rapid games from 3,912 imported" with "backfill still running" while it runs, the
seven area scores on one ruler, an area without a score saying "Need N more games" or, for
Mindset, that it is advice and not a score, then the full AI disclosure); the coach's overview
over the strengths the engine found; "What's costing you games" (by rank: `WeaknessRow`, the
coach's note for that weakness, up to three `ExemplarBoard`s from the report and "Ask your coach
about …" with a `weakness` anchor); openings with their verdict (Keep, Fix, Learn deeper,
Replace); "Your climb plan" (at most three focuses with why, habit and drills by name); the area
notes, each linked to its area page; "This month" (new and fixed changes with their ready change
notes); and the count of statements the validator removed.

**Section states.** A section's coach part is `ready` (validated content), `dropped` (null in the
payload: "Your coach's notes for this section couldn't be checked, so they're not shown"),
`writing` (absent while the run builds: `CoachState` "Your coach is writing this section"),
`missing` (absent after the run ended) or `off` (the AI was off; the banner says so). The
computed part always renders. The compact disclosure sits on every section that shows coach
text, and the full one in the snapshot only when some section is ready.

**State copy.** `none`: "Your coaching report hasn't been built yet." with "Build my report".
`building`: "Building your report: 2 of 5 sections written" over a list of the five sections by
name, with "Waiting for the batch, usually within an hour" for a batch run, "Your coach is busy;
retrying at 14:05 UTC" for a retry, and, when there is no earlier report, "Your first report is
being written" (ruling G3: the worker starts it once the profile is built). `ready` with the AI
off: the no-key line or the budget line. `failed`: the interval date, "There isn't enough
analysed data for a report yet.", the budget line, or "The last rebuild failed. Your previous
report is still shown." ("Building your report failed. Try again." with no previous report).

**Rebuild.** `RebuildButton` posts to `/api/coach/report` and toasts "Rebuilding your report.
Sections appear as they're written." It is disabled while a run builds and, before
`nextManualAt`, reads "Rebuild available on 22 Dec 2026" (a time in UTC when it is the same day).
The page passes `betaReportIntervalDays()` to `getReport`, the same interval the route enforces
(ruling G1). A `throttled` refusal (failure backoff or the daily run cap) says "Your last report
attempts didn't finish, so rebuilding is paused. Try again at 14:05 UTC." and keeps the button
disabled until then.

**Realtime.** `LiveRefreshOn` watches `report(accountId)` and `notes(accountId)`, so each
`report.section.ready` re-renders the server page; a section that turns ready while the page is
open assembles in on its own.

**Reveal.** On the first view of a report version on this device (`localStorage`
`ci.report.seen`, every access in try/catch), the sections assemble as DESIGN.md choreography 2:
each ruled line draws on `scaleX`, `--stagger-base` apart, and its content appears with opacity
only; the weaknesses resolve in rank order. An inline script sets `data-reveal="first"` while
the HTML is parsed, so the CSS reveal in `globals.css` starts from the server's first frame (no
flash, no wait for hydration; the rejected "JavaScript-only roll" in the design-system skill).
Any key or pointer press ends it (M7); later views and reduced motion show the end state (M10).
The brief named the `page` spring; the reveal uses the CSS line draw that DESIGN.md specifies,
because a spring would need JavaScript and reintroduce the flash.

**No "train this" button.** Drills are out of M5b's scope, so the plan names them and links
nowhere. A button to a missing feature would overclaim.

**Weakness links.** A ranked weakness links to `/app/weaknesses/<id>.<variant>` (or `<id>`), the
profile series key. Its weekly sparkline reads the live `weakness_series_point` rows through
`getWeakness`; the rate, interval and counts are the report's.

## Consequences

- The report is honest in every state the worker can leave it in, and its numbers never depend
  on the AI.
- `OverviewSheet` gains an optional `lead` prop so `/app` passes `leadFinding` to `WorkOn`.
- The page makes one `getWeakness` read per ranked weakness for the sparklines (at most a
  handful per report).
