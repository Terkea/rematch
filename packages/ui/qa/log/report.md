# Report page `/app/report` and "What to work on": critique log (M5b Task 6)

Screenshots are not committed. They were taken from a local `next dev` of the worktree on a
temporary, uncommitted lab route that renders the real `ReportView` and `WorkOn` with R3's
synthetic payload fixtures (`backend/fixtures/ai/payload/*.json`), never owner data. Reduced
motion, full page, at 390 and 1440 in day and night:
`<state>-{390,1440}-{light,dark}.png` in the session scratchpad (`m5b-t6/`).

## States captured

`complete` (a done run with every section, a ready change note), `building` (a rebuild that has
written two of five sections; openings and the climb plan are still being written), `first` (the
first report queued and in a retry, no payload yet), `computed` (computed only, AI off for no
API key), `none` (never built), `failed` (a failed rebuild over the previous report), `partial`
(a done run whose openings section the validator dropped, with 2 removed statements), and the
overview slot `lead-weakness`, `lead-none` and `lead-building`.

Each capture: no page-level horizontal overflow at 390 or 1440 (also checked at 360), and the
main HTML has no `NaN|undefined|Infinity|null|{{`.

## Pass 1 (implementer self-review; the fresh critic is still owed)

The implementer was told not to dispatch subagents, so **the controller owes the independent
critic passes** (at least 2, on the PNGs, DESIGN.md and REFERENCES.md only).

Found and fixed in this pass:
- The exemplar boards took the global `--board-size` (560px at 1440), so three boards overflowed
  the page. The report grid now sets 20rem boards (rounded to 8px), three across from `lg`.
- The scope switch (Task 5's `CoachScopeSwitch`) is wider than a phone with two options. It now
  scrolls inside its own row; hand-off to T5: the segmented labels need a narrow form.
- The disabled header button read "Rebuilding your report" during the first build; it now reads
  "Building your first report".

Open for the critic:
- Each section still being written shows `CoachState` writing (a full list skeleton). With three
  sections pending this is a lot of skeleton; a slimmer per-section form may be better.
- Weakness rows without weekly points say "Weekly trend after more weeks" (the kit's copy).
- `pnpm design:check`: 0 findings.
