# Strengths (`/app/strengths`): critique log

Screenshots are not committed. They are taken through the `qa` container of the throwaway compose
project `ci-t6` (no host ports), from the e2e stack's `web` dev server, signed in as the adult
e2e user and seeded with `e2e/helpers/seed-strengths.ts` only: the coach seed's synthetic rows,
plus strength plies on real moves of two public fixture games (DrNykterstein's games from
lichess, the player as Black), with synthetic `move_features` (critical, Maia pending). States
shot, at 390 and 1440, light and dark, reduced motion:

- `/app/strengths` (rapid, the default): no band reference (own-baseline wording), too few
  games for the own comparison, one confirmed improving weakness, no judged opening line, the
  report's strengths, no hard spot.
- `/app/strengths?tc=blitz`: the critical-position fallback with three boards.

## Pass 1 (implementer self-critique; self-critique only)

The implementer was told not to dispatch subagents, so the fresh critic pass
(`critique-prompt.md`, PNGs only) is still owed: **the controller runs it and appends its verdict
here.**

At 360 and 390 neither route scrolls sideways (`scrollWidth` equal to the viewport), and the
page text has no `NaN|undefined|Infinity`. Measuring 640 to 1023px found the header nav wider
than the viewport (687px at 640, 918px at 768; already 826px at 768 before Strengths was added),
so below `lg` the header now keeps only the phone-row sections and moves the rest into "More
pages" (fixed in round 1 below).

`pnpm design:check`: 0 findings.

Issues found and fixed:

1. **The report's area showed as `(openings)`.** `areaLabels` comes from a client module, so a
   server component read a client reference and fell back to the raw id. The page now names
   areas from the server glossary (`AREA_GLOSSARY[area].name`).
2. **The header scrolled sideways from 640 to 1023px** with eight sections (and from 768 to
   about 880px with seven). Below `lg` the nav keeps Overview, Games, Report and Coach; the rest
   are in the account menu's "More pages", as on a phone.
3. **A weakness row drew an "interval" strip from a single after-rate**: a 1px sliver beside a
   tick, which read as a broken chart. Weakness rows now have no strip; their words carry
   before, after, the date, the likelihood and the counts since.
4. **Rows led with a signed change in green** ("+12.0 percentage points" wrapped at 390). Rows
   now lead with the value and its unit ("76.1%", "0.80 a game"); the change and its 80% range
   are in the line under it.
5. **"you have 0 games with these measures"** read as an accusation when the scope's metrics
   rows carry no measures. The parenthesis is left out at zero.
6. **"The only good move, found."** read as a caption fragment. Now "You found the only good
   move." (fallback) and "You found a hard move." (Maia).

Open, for the fresh critic: the green value on an improving weakness ("9.0%") is the rate now,
not the drop; check whether it reads as the improvement.

## Pass 2: fix/strengths-polish (N11 to N14; self-critique only)

The implementer was told not to dispatch subagents, so this is **self-critique only**; the fresh
critic pass is still owed. Shots: `/app/strengths`, `/app/strengths?tc=blitz` and
`/app/weaknesses/W3.shallow` at 390 and 1440, light and dark, through the `qa` container of
`ci-fixh` (base URL `http://localhost:3100`, the e2e adult's stored session), on the strengths
seed (whose spots are now three quiet moves in balanced games and one free capture after a
blunder, which must not show).

Changed and checked:

1. **Fallback title.** Without a Maia rating the section reads "Critical moments you got right",
   never "hard"; the lede keeps "Difficulty pending: Maia …" and names the new rules (win chance
   25% to 75%, free captures and gifts after the opponent's mistake left out). Round 1 repeated the
   title inside the lede ("these are critical moments you got right: …"); now "these are moments
   where the engine's best move …".
2. **Boards are position explorers** (Strengths and the weakness examples): the key reads
   "Played, Qb6, the engine's move" with a `good` arrow and `!` badge, then Expand and "Open the
   game at 8… Qb6". The host's own "Open … in the review" link was dropped so there is one link
   per board. Each board fills its grid column (`w-full`), as before.
3. **Peer claims and the promising opening tier** are covered by unit and page tests; the e2e seed
   has no peer benchmark and no line with 10 games, so their calm lines show.

Open, for the fresh critic: at 390 a full-page capture shows the second and third boards as the
explorer's lazy `well` placeholder (they load within a screen of the viewport; the coach shows
the same); the caption under each board ("8… Qb6! You found the only good move.") partly repeats
the key above it; the scope switch is clipped at 390 (pre-existing).
