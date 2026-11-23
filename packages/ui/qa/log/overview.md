# Overview `/app`: critique log (M5a Task 8)

Screenshots live in the git-ignored `.superpowers/sdd/2026-11-21-05a-game-pages/shots-t8/` and are not committed. They were taken on a throwaway compose project whose database held a read-only copy of real games (the labs-on-real-data ruling), so no figure from them is written here.

## States captured

Each at 1440 and 390 (390 also as a first-viewport `-fold` shot), light and dark, reduced motion, in a fresh browser context with the theme pinned:

| State | How it was made | Files |
|---|---|---|
| Rich data (several hundred analysed games, two time controls) | the lab user linked to the copied account | `overview-rich-*` |
| Filtered to one class | `/app?tc=rapid` | `overview-rapid-*` |
| A one-game account | a lab user linked to a synthetic account with one analysed game | `overview-onegame-*` |
| Analysing | a lab user whose three games are all pending | `overview-analysing-*` |
| No links | a lab user with no linked account | `overview-nolinks-*` |

Every capture returned 200 with no console errors and no horizontal overflow. The rendered HTML of each state (`overview-<state>.html`, 1440 light) was grepped for `NaN|undefined|Infinity`: no matches. An unknown class (`?tc=bogus`) rendered All.

The first-view reveal was checked on its own (`reveal-early.png`, `reveal-end.png`): `data-reveal="first"` on the first load, the rule's animation is `ci-rule-draw`, the session cookie is set, and a reload renders without the reveal.

## Pass 1 (implementer self-review; the fresh critic is still owed)

`pnpm design:check`: 0 findings.

The CL procedure's step 3 needs a fresh subagent with `critique-prompt.md` and only the PNGs. The implementer was told not to dispatch subagents, so **the controller must run the critic on `shots-t8/` and append its verdict here** (the reviewer gate needs `pass`).

Issues found by the implementer and fixed in this pass:

1. **The trend caption talked about line breaks when there was no line** (one game). It now says "The line starts once you have 10 analysed games." whenever there is no rolling entry.
2. **The sparkline note showed with no sparklines** (every row below the minimum). It now shows only when a row has an estimate.
3. **"0 games analysed" under the title with nothing linked.** The status line is left out when there are no games; the empty state says what to do.
4. **Tabular figures in the sans face spread the decimal point** ("84 . 5%") in the recent table and the sparkline ends. Both use proportional figures now (they are right-aligned or paired, so nothing jitters).
5. **The phone band line said "N games."** on a filtered page. It now names the class ("N rapid games.").

Hand-offs addressed from the T6 critique (critique-t6.md) and review:

- Variant b's headline sentence with both bands, plus a shorter phone sentence at `2xl`.
- A ruled Recent games table (date, result, opponent with rating, accuracy), each row a link to the review.
- The All / class filter as links with `aria-current`.
- The rolling line and band break across four weeks with no games (`gapDays`), and the caption says so.
- No ruler on overview KPIs (ADR 0055); sparklines carry their first and last values and "Lower is better"; the gap note sits beside the rows.
- n stated once per section; a row on the same basis keeps it for screen readers only.
- Dark mode: heavier trend band, firmer dense loss rings, the wins bar steps down to `ink-soft`.
- The judgement columns run the full content width, so at 1440 they no longer scroll with the edge fade.
- "What to work on" is a red-pen slot with no finding until the R2 profile exists.
- "Unverified" links to Accounts.

Tried and rejected:

- **A ruler on the user's own weekly range.** The sparkline already shows that range; a second drawing of it beside the band added ink, not information.
- **Rolling the numbers from JavaScript alone.** A hard load would paint the final page, then hide and reassemble it on hydration. The reveal is CSS keyframes instead, gated by a session cookie the server reads, and `Stat` rolls only while its wrapper is still hidden.

Open for the critic: the per-class score and accuracy rows stack tall in the five-column "By time control" at 1440, leaving space under Recent games; and the nav still says "Dashboard" (the T21 shell's label) above an "Overview" title.

Verdict (pass 1, fresh critic in review-t8.md): **needs changes**.

## Pass 2 (fix round 1, after the fresh critic)

The critic's verdict on pass 1 was **needs changes**, with three blocking issues. All three are fixed, and the shots were retaken on localhost after hydration (the capture waits for the trend island to draw):

1. **The nav said "Dashboard" above an "Overview" title.** The nav label is now "Overview" (ruling), and "dashboard" stays a command-palette keyword.
2. **n and the shortfall repeated in a sparse account.**
   - "Mistakes per game" now says "Across N analysed games." and, below the minimum, "Need N more games before these rates mean anything." once. Its rows show a dash, with the basis and the shortfall kept for screen readers.
   - Each class in "By time control" states "Based on N games in <class>." once, plus its shortfall. Its score and accuracy now sit side by side, which also shortens the 1440 bottom band.
   - A lone game is plotted as its dot, and the caption says the line starts at 10 games.
   - Date ticks are one per day, which fixed a duplicate-key warning on a one-game span.
3. **Recent games rows didn't look like links.** The date is underlined, rows tint on hover and ring on focus, and each row ends in a chevron. On phones the date drops its year and the opponent wraps, with the rating on its own line, so nothing is truncated. A filtered page says "<Class> games only." above the table.

Also from the review: the sparkline note now reads "your latest 12 weeks of play" (not "your last 12 weeks"). The gate's HTML grep now strips `<script>` payloads, because React's `$undefined` tokens are not page text.

Still open (not blocking per the critic): the moiré of the hatched judgement columns at small sizes, the sparse per-game grid at 1440, and the analysing placeholder's squiggle (M4's ProgressPlaceholder).

Verdict: pending the critic's re-check.

## 2026-11-23: the focused overview (ADR 0092)

Shots: `.shots/overview/{1440,390}-{light,dark}.png` (whole page, one tall viewport) and `first-*` (first viewport), taken by `e2e/focus.spec.ts` on the e2e-fixtures seed with a running focus.

Self-critique only: this lane ran without subagents, so no independent critic. The gate still wants a fresh `pass`.

- The first viewport now answers "where am I, what is my one thing, what do I do": Where you are, This week's focus, Practise (one primary button), then How you're doing.
- The time control sits under the title because it scopes the whole page; format, colour, dates and compare sit at the top of Details, with the comparison under them (critic 4 kept).
- Details is quieter: `xl` sub-headings on a `rule` line, the trend no longer draws in (below the fold, M4).
- Fixed in round 1: "What to work on" repeated the focus card's weakness; it is now left out when it names the focus's series.
- Open: at 1440 the top cards are a `max-w-3xl` reading column with space to the right; the below-minimum trajectory card is one line; the focus card's length select stays as tall as before.

Verdict: self-critique `pass` (no blocker or major found); needs an independent critic.
