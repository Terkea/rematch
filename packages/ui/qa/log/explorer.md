# Games explorer (M5a Task 9)

Date: 2026-11-22. Surface: `/app/games` in its default, filtered-to-empty and page-2 states, plus the
open Select, the open phone filters, a two-account Account filter, keyboard focus on a row, and the
no-links and no-games empty states.

## Screenshots

These were taken at 390 × 844 and 1440 × 1000 (plus 360 for the pager), in day and night, on a
lab compose project after hydration. They are not committed. The copies are in
`.superpowers/sdd/2026-11-21-05a-game-pages/shots-t9/` (git-ignored), named
`<state>-<width>-<theme>.png`. The data was the owner's real games, restored into the lab project's
own database, so no figures from it are quoted here. Every capture measured 0 px of horizontal
overflow.

## Round 1: self-critique against `../critique-prompt.md`

The implementer was told not to dispatch subagents, so this round is a self-critique. An independent
critic should still run in review.

```json
{
  "issues": [
    { "rank": 1, "severity": "major", "screenshot": "draft-1440-light.png",
      "observation": "Sort by wrapped onto a second line on its own, under the date fields, leaving a ragged hole beside it; the count sat alone further down.",
      "fix": "The filters keep one row. Below a rule, a second row holds the count, Clear filters and Sort by, so the sort and the count read together and stay visible on phones." },
    { "rank": 2, "severity": "major", "screenshot": "page2-1440-dark.png",
      "observation": "At night the Black swatch (piece-dark on sheet, with a piece-dark border) all but vanished, so the colour column read as random white squares.",
      "fix": "Both swatches take a rule-strong border; only the fill differs." },
    { "rank": 3, "severity": "major", "screenshot": "filters-open-390-light.png",
      "observation": "On a phone 'Previous page' and 'Next page' wrapped onto two lines each, and at 360 px the pager overflowed by 6 px.",
      "fix": "The steps never wrap; below sm, 'Page 2 of 20' takes its own line above them." },
    { "rank": 4, "severity": "major", "screenshot": "(keyboard pass)",
      "observation": "Typing a date digit by digit made the year 0002 a valid value, which navigated, and the re-keyed input lost focus mid-entry.",
      "fix": "A DateFilter keeps its own value and navigates only on a day the URL accepts (2000–2100); it is never remounted, so focus stays. Both behaviours are unit-tested." },
    { "rank": 5, "severity": "minor", "screenshot": "draft-1440-light.png",
      "observation": "Rows were 70 px tall, so a page of 25 games ran very long.",
      "fix": "Cell padding drops from py-3 to py-2.5." }
  ],
  "verdict": "pass"
}
```

## What the design commits to

- **A ruled scoresheet, not a data grid.** The list sits on one `sheet` with `rule` lines between
  games and a `rule-strong` line under the column heads. It has no zebra striping, no card per game
  and no shadows. Hover steps one tone down the ladder, and focus draws a 2px ink ring around the
  whole row (the link's `::after` covers the row).
- **The red pen marks only mistakes.** In the table, the blunder count is `pen-text` when it is
  above zero. On phones, the counts become the coach's notation, `2??` and `1?`, with the glyphs in
  the pen. The words ("2 blunders, 1 mistake") are always there for screen readers.
- **Who you played and how well** lead each row: the opponent and rating, then the accuracy in
  Literata 600 with tabular figures. The result word is Literata too, so a column of
  Win/Loss/Draw reads like a crosstable.
- **The colour played** is a tiny piece-coloured square, using the piece tokens, so it inverts
  correctly at night. Screen readers hear "as White" or "as Black".
- **Pending never looks like a number.** "Waiting for analysis", "Couldn't analyse" and "Skipped"
  replace the accuracy, and there is no % and no count.
- **Loading** is the list's ruled shape in static wells (`loading.tsx`). A filter change keeps the
  old list in place at 60% opacity with `aria-busy` until the new page renders. No spinner (B18).

## Known limits

- On a 360 px phone the second line ("Win, Rapid 10+0, 12 Nov 2026, …") truncates the opening name
  entirely, while at 390 px it shows the first word or two. The table (from `md`) always shows it.
- Native date inputs show the browser's own format (mm/dd/yyyy in an en-US browser) and put four
  tab stops in each field. That is the platform's behaviour, which was kept for its accessibility.

## Round 2: review-t9 (critic and code review), fix round 1

The critic's verdict was "needs changes"; these are its five points and what changed.

1. **Phone overflow.** It was re-measured as `scrollWidth - clientWidth` with classic scrollbars at
   360, 390 and 1440, in day and night, for the default, page 2 (accuracy sort), no-match and
   open-filters states: 0 in every case. The earlier 6 px came from the pager, whose steps no
   longer wrap; at sm and below, "Page 2 of 20" takes its own line. `apps/web/e2e/games.spec.ts`
   now asserts `scrollWidth === clientWidth` at 390 and 360 on synthetic games. A `w-100` mutant on
   the list fails it ("/app/games at 390: expected 0, received 26").
2. **Colour.** The swatch beside the opponent is gone. The result is now a scoresheet mark: the
   player's score (1, ½ or 0) on a square in the colour they played, as a crosstable writes it,
   beside the result word. Its `title` and the row's link name say "you played White" or "you
   played Black". The square uses the piece tokens with a `rule-strong` border, so it reads the
   same way at night.
3. **Scoresheet idiom.**
   - Sorted by date, the games sit under day headings (Literata), and the Date column goes.
     Any other sort keeps a Date column.
   - The last two columns form a ruled margin: accuracy, then the red-pen marks (`2??`, `1?`).
     The column head carries the legend.
   - The opening is the quietest cell (`ink-faint`).
4. **Empty and waiting states.**
   - **No match** names the filters ("You asked for Classical draws since 1 Oct 2026."). It
     offers each filter dropped, with its count ("Any time control: 3 games"), plus Clear
     filters.
   - **No games** says "Importing your games from chess.com name…" while an account has never
     synced or an import is running. Once every account has synced, it says
     "chess.com name has no games yet." with a way to Accounts.
   - **No links** gained a second line.
   - **Loading** reads "Fetching your games" over the list's shape; it is captured in
     `loading-{390,1440}-light`.
5. **Phone filters.**
   - With filters set, the chips ("Rapid ×", "Losses ×", "From 1 Oct 2026 ×") show under the
     Filters button whether or not the panel is open; each removes its filter.
   - Account moved into the toolbar row beside Sort by, so it no longer sits alone at 1440.
   - Native date fields still show the browser's format. That is left as a platform behaviour.

Verdict after round 2 (self): pass, pending the independent critic.
