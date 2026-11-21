# Styleguide hero: decision

Task 6, lab procedure (LP) step 4. Date: 2026-11-21. Revised in fix round 1 (see the end).

## The moment

All three variants show the same critical moment, which is computed from `games.middlegameBlunder` rather than typed in by hand. It is the player's worst blunder by lost win chance: **28. Nd2?? −1.1** (White, DrNykterstein v Night-King96, Lichess blitz). The evaluation was +2.7 before the move, and win chance fell from 73% to 40%. Black's reply was 28…e4.

The coach note is one sentence built from facts the fixture shows: "Your knight on e4 was holding back Black's e-pawn; the moment it went to d2, the pawn came forward with 28…e4, and +2.7 became −1.1." The fixture has no best move (`bestUci` is null), so the note doesn't name one (`B15`).

Each variant plays the same beat from choreography 1's climax. The fixed times (400, 760, 860 and the 30ms head gap) come from DESIGN.md's table. Every other time comes from the `@ci/ui/motion` tokens:

- 0ms: the row is selected.
- 400ms: the knight lifts over `dur.instant`.
- 490ms: it travels on `settleFor("knight")`.
- 760ms: it lands with its squash on `land`.
- 860ms: the pen ring draws over `dur.draw`. One `ease.pen` progress drives both mask segments, split by arc length (M2). The glyph and evaluation appear at the same time, and in A the scoresheet ring draws over `dur.drawSmall`.
- 980ms (`stagger.pen` later): the pen arrow draws over `dur.draw`.
- 1246ms and 1276ms: the head strokes, each over `dur.instant`. 1246 is when the shaft reaches 85% of its `ease.pen` progress, solved from the bezier.
- 1400ms: the `??` badge lands on `land`.
- 1400–1820ms: the link mark draws over `dur.draw` (A and C).
- 1820ms: the note appears.

Under reduced motion, the end state shows at once (M10).

## Variants

| | A "Scoresheet margin" | B "Book plate" | C "Coach's note" |
|---|---|---|---|
| Where the notation sits | One row of a ruled scoresheet, at `base` | Under the figure, at `4xl` | The `h1`, at `5xl` |
| Board and text | The board heads a ruled scoresheet that opens on the ringed row. The comment sits in the margin, level with that row (under the rows on narrow screens). | The board is a figure with a centred caption, the game is printed as running text, and the note is a footnote | The board is offset beside the headline, lower down |
| How the red pen connects them | A scoresheet ring on the row and a board ring, tied by one link stroke through the gutter | Only the `??` glyph and a footnote numeral | One pen stroke rings the move, then runs through the gutter into d2 along rank 2 |

## Ranking

The implementer's self-critique ranked the variants A, then C, then B. An independent review (controller, fix round 1) agreed with that ranking.

1. **A, the winner.** A matches the system's own contract:
   - DESIGN.md "Styleguide hero" asks for the game as the `h1` at `3xl`, the board at `--board-size`, and the ruled scoresheet. A puts the pen on both the row and the square, which is the "annotated scoresheet" direction at its most literal.
   - The scoresheet ring uses its documented geometry (0.8×, `rx = w/2 + 10`, `ry` 14px) in a 34px row (`--space-scoresheet-row`).
   - Its weakness was that it was the quietest of the three. The link mark taken from C gives it the big beat.
2. **C, second.** C is the boldest and the most memorable: the pen gesture from headline to board is the brand's signature at full volume. But it breaks the design law:
   - `5xl` is landing-only (Type scale), and the cliché review's guard says the styleguide hero "is the game, not a display headline". This drifts toward `B1`.
   - The move is the `h1`, and the game is demoted to a footnote line.
   - Its link stroke was a pen mark DESIGN.md didn't have. The mark is now ruled into DESIGN.md (see below), so that objection applies only to C's first round.
3. **B, third.** B is elegant but editorial:
   - A big serif on light paper with a footnote rule is the look the cliché review warns about (`B1`, `B3` drift).
   - The running text is a bold block, and its five red glyphs dilute the one that matters.
   - The pen barely connects board and text: only the glyph's colour does.
   - At 1440px the right column has a large dead gap.

No variant shows `B5`, `B6`, `B7`, `B8`, `B13` or `B16`, and all of them use only tokens.

## Decision

**A, "Scoresheet margin"**, is the base for the `/styleguide` hero, with C's link mark adopted by controller ruling. The board and annotation lanes (Tasks 11 and 12) should build it with the real board and `@ci/ui/annotations` geometry.

Taken from the losers:

- **From C, the link mark.** One pen stroke runs from the scoresheet ring across the gutter to the board ring. The ruling is recorded in DESIGN.md "Severity and marks" (the Link row and its rules):
  - colour: `pen` only between two pen-marked ends, with no badge or head
  - shape: 0.9× with a halo
  - routing: never across text or occupied squares, at least 16px clear, entering through the gutter
  - timing: 1400–1820ms on `--dur-draw`/`--ease-pen`, retracting with the other marks
  - reduced motion: fully drawn
  - budget: one per view, counting toward the 12 marks, and dropped on mobile when both ends aren't in view together

  A implements every one of these rules.
- **From C, the note's timing.** The note appears only after the marks land, so the comment follows the evidence.
- **From B, the side to move.** It moves into A's header ("The board shows the position after 28. Nd2, Black to move."), not into a caption under the board. A caption there would sit in the link's path through the gutter.

Rejected: the `5xl` notation headline (landing-only), the footnote rule, and bold running text in the app.

Why the board now heads the scoresheet, instead of sitting beside it: the link must reach d2 over empty squares. Rank 2 to the right of d2 is full (Ke2, f2, Bg2, Qh2), so the link can only enter from the left (a2–c2) or from below (d1). With the board beside the scoresheet, any path from the ring crosses text: the move number on one side, or Black's 28…e4 on the other. With the board above a scoresheet that opens on row 28, the link rises 16px or more clear of the evaluation, crosses the gutter, and enters through c1–d1. That is short, and it touches no text.

Follow-ups for the lanes building on A:

- The arrowheads still hide under the ring and the badge on a knight's short diagonal.
- At 1440px the right column has a quiet stretch between the header and the margin comment, beside the board. The eval graph slot (DESIGN.md hero sketch) is the natural thing to fill it.
- The pen geometry here is a lab approximation: fixed drift phases instead of the FNV-1a and mulberry32 seed, and a constant mask width per segment.

## Screenshots

These are in the worktree's `.shots/` (git-ignored). The static shots use reduced motion, so they show the end state.

- `.shots/lab-styleguide-hero-{a,b,c}/{390,1440}-{light,dark}.png`, from `pnpm shots --routes lab/styleguide-hero/a,lab/styleguide-hero/b,lab/styleguide-hero/c`
- `.shots/lab-styleguide-hero-{a,b,c}/360-{light,dark}.png`, from Playwright directly. `scrollWidth` is 360 in all six, so there is no horizontal scroll.
- Motion frames from `--motion`: `.shots/motion/lab-styleguide-hero-{a,c}/*-f00..f11.png`. These 12 frames end at about 1.2s, before the badge.
- Timed motion frames from Playwright directly, at fixed times from 0 to 2200ms: `.shots/motion-timed/lab-styleguide-hero-{a,c}/1440-light-tNNNN.png`. Contact sheets are at `.shots/motion-timed/{a,c}-sheet.png`. They show, in order, the knight in flight, the landing, the ring and arrow draw, the badge landing in place on d2, and the link drawing from 1500 to 1820ms.

The variant pages stay at `/lab/styleguide-hero/{a,b,c}`. They are dev-only.

## Fix round 1 (corrections to the first version of this file)

- **Wrong claim: "motion confirmed".** The `--motion` frames stop at about 1.2s, so they never showed the badge. The badge had a transform-origin bug in all three variants. It set `originX` and `originY` in board pixels, but Motion renders SVG transforms with `transform-box: fill-box`, so the origin landed far outside the badge, and the badge flew in from the bottom right. It now scales from its own centre (`originX: 0.5, originY: 0.5`). The timed frames past 1400ms confirm this.
- **Wrong claim: "ruling stays on the scoresheet".** A had a vertical rule between the board column and the scoresheet, which is page ruling (`B3`). It is gone. The only rules left are reference 3's own: row rules, and the vertical rules that set off the number and White columns inside the scoresheet.
- **Wrong board size.** A's board was 384px, which is off the `--board-size` table. It is now `w-(--board-size)` (560px at 1440).
- **Invented easings and raw durations.** The pen easings and raw ms literals were invented. They are replaced with `ease.pen` and `dur.draw`, `dur.drawSmall` and `dur.instant`. One progress drives both ring mask segments (M2).
- **Proportions.** The columns now use reference 3's widths (56, 208 and 208px; 208 is the nearest spacing step to the reference's 210). The margin comment is 40 to 52ch, and the move number uses a no-break space in B.
