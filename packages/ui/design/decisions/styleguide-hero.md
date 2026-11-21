# Styleguide hero: decision

Task 6, lab procedure (LP) step 4. Date: 2026-11-21. It was revised in fix rounds 1 and 2; the corrections are listed at the end.

## The moment

All three variants show the same critical moment. It is computed from `games.middlegameBlunder`, not typed in by hand: the player's worst blunder by lost win chance, **28. Nd2?? −1.1** (White, DrNykterstein v Night-King96, Lichess blitz).

- The evaluation was +2.7 before the move.
- White's win chance fell from 73% to 40%, which is 0.66 on the win-chance scale.
- Black's reply was 28…e4.

The coach note is one sentence built from facts the fixture shows: "Your knight on e4 was holding back Black's e-pawn; the moment it went to d2, the pawn came forward with 28…e4, and +2.7 became −1.1." The fixture has no best move (`bestUci` is null), so the note doesn't name one (`B15`).

Each variant plays the same beat, which is the climax rows of choreography 1. DESIGN.md's table fixes four values: 400, 760 and 860ms, and the 30ms head gap. Every other time follows from the `@ci/ui/motion` tokens.

- **0ms:** the row is selected. The eval bar holds the pre-move value, and the graph cursor sits on ply 54 (variant A).
- **400ms:** the knight lifts over `dur.instant`.
- **490ms:** it travels on `settleFor("knight")` along the **knights arc**. Its lift grows to 1.13 and rises 0.2 of a square at mid-travel, so it visibly jumps between d3's queen and e3's bishop.
- **760ms (contact):**
  - The knight squashes and lands on `land`.
  - The eval bar swings to −1.1 on `swing-big` (the change is 0.66, which is at least 0.3), so it overshoots and comes back.
  - The graph cursor steps to ply 55 on `swing` (A).
- **860ms:**
  - The board ring draws over `dur.draw`. One `ease.pen` progress drives both mask segments, split by arc length (M2).
  - In the same beat, the scoresheet ring and the graph's ring on 28. Nd2 draw over `dur.drawSmall`.
  - The graph label and its leader tick fade in from the ring's start + 120ms (A).
- **980ms (`stagger.pen` later):** the pen arrow draws over `dur.draw`. It ends 46% short of the ringed square (Task 10 ruling).
- **1246 and 1276ms:** the head strokes, over `dur.instant`. 1246 is when the shaft reaches 85% of its `ease.pen` progress, solved from the bezier.
- **1400ms:** the `??` badge lands on `land`. It is in the top-left corner, because the arrow arrives from the right (Task 10 ruling).
- **1400–1820ms:** the link mark draws over `dur.draw` (A and C). In A it draws only if both ends are in view.
- **1820ms:** the note appears.

Under reduced motion, the end state shows at once (M10).

## Variants

| | A "Scoresheet margin" | B "Book plate" | C "Coach's note" |
|---|---|---|---|
| Where the notation sits | One row of a ruled scoresheet, at `base` | Under the figure, at `4xl` | The `h1`, at `5xl` |
| Board and text | The eval bar and the board in its well head a ruled scoresheet that opens on the ringed row. The header and eval graph sit beside the board. The comment sits in the sheet's right margin, level with that row. | The board is a figure with a centred caption, the game is printed as running text, and the note is a footnote. | The board is offset beside the headline, lower down. |
| How the red pen connects them | A scoresheet ring and a board ring, tied by one link stroke through the gutter. The graph's ring on the same move draws in the same beat. | Only the `??` glyph and a footnote numeral. | One pen stroke rings the move, then runs through the gutter into d2 along rank 2. |

## Ranking

The implementer's self-critique ranked the variants A, then C, then B. An independent review (controller, fix round 1) agreed.

1. **A, the winner.** A is the closest fit to the system's own contract:
   - The game is the `h1` at `3xl`, and the board is at `--board-size` in its well.
   - The scoresheet is ruled per reference 3 and carries its comment in the margin.
   - The pen is on the row, the square and the graph.
   - The scoresheet ring uses its documented geometry (0.8×, `rx = w/2 + 10`, `ry` 14px) in a 34px row (`--space-scoresheet-row`).

   One thing does not follow the original DESIGN.md hero sketch: that sketch had the scoresheet to the right of the board, and A puts it underneath. Fix round 2 rewrote the sketch by controller ruling (see "Composition" below). The first version of this file claimed A met the sketch as it stood, which was wrong.
2. **C, second.** C is the boldest: the pen gesture from headline to board is the brand's signature at full volume. But it breaks the design law:
   - `5xl` is landing-only (Type scale), and the cliché review's guard says the styleguide hero "is the game, not a display headline". This drifts toward `B1`.
   - The move is the `h1`, and the game is demoted to a footnote line.

   Its link stroke is now a ruled mark ("Link" in "Severity and marks"), so that part of C survives in A.
3. **B, third.** B is elegant but editorial:
   - A big serif on light paper with a footnote rule is the look the cliché review warns about (`B1`, `B3` drift).
   - The running text is a bold block, and its red glyphs dilute the one that matters.
   - The pen barely connects board and text: only the glyph's colour does.

No variant shows `B5`, `B6`, `B7`, `B8`, `B13` or `B16`, and all three use only tokens.

## Decision

**A, "Scoresheet margin"**, is the base for the `/styleguide` hero, with C's link mark adopted by controller ruling. The board and annotation lanes (Tasks 11 and 12) should build it with the real board, eval bar and eval graph, and with the `@ci/ui/annotations` geometry.

### Composition (controller ruling, fix round 2)

The board heads the scoresheet. No legal side-by-side route exists for the link in this position:

- Rank 2 to the right of d2 is occupied (Ke2, f2, Bg2, Qh2).
- With the board beside the scoresheet, every path from the ring crosses text: the move number on one side, or Black's 28…e4 on the other.

From 1024px the page has two columns: 512 + 40 (bar and well) + 32 + 392 at 1024, and 560 + 40 + 64 + 552 at 1440.

- **Left column:** the eval bar, then the board in its well.
- **Right column:** the header at the top and the eval graph with its label lane at the bottom, level with the board's bottom edge.
- **The sheet** spans both columns. The ruled table sits under the board and fills its column. The comment sits in the sheet's right margin, level with row 28 (reference 3).

On a phone the order is: board (with the 8px horizontal bar above it), scoresheet, note, graph. DESIGN.md "Styleguide hero" and the plan pass's layout line now say the same.

### The link mark

The link runs from the scoresheet ring:

1. straight up to the middle of the gutter,
2. along it,
3. up through d1 (empty) into d2,

and it stops short of the board ring. It follows DESIGN.md "Severity and marks", Link:

- **Colour:** pen, joining two pen-marked ends, with no badge or head.
- **Width:** 0.9× the board pen at the current square size.
- **Clearance:** at least 16px from any text.
- **Timing:** 1400–1820ms.
- **Reduced motion:** fully drawn.
- **Visibility:** it is drawn only when an `IntersectionObserver` reports both ends fully in view at 1400ms (at once under reduced motion). Otherwise it is omitted.
- **No legal route:** the link is omitted.

### The excerpt

The excerpt opens on the ringed move and ends before the next blunder (31…Nf4??), so no second `??` competes with the key move.

The hero's label lane names only the key move. The game's other 18 marks are rings on the line. This is a **proposal recorded in DESIGN.md's hero section**: the game has 19 marks, and their labels can't fit a 392–552px lane even with the mobile rule (7 blunders and mistakes, six of them inside about 90px). Please confirm or overrule it.

### Taken from the losers

- **From C:** the link mark, and the note's timing (the comment follows the evidence).
- **From B:** the side to move. It sits in A's header ("The board shows the position after 28. Nd2, Black to move."), not in a caption under the board, because a caption there would sit in the link's path.

### Rejected

The `5xl` notation headline (landing-only), the footnote rule, and bold running text in the app.

### Follow-ups for the lanes building on A

- **Right column at 1279 and 1440:** there is a quiet stretch between the header and the 120px graph, about 300px beside the board. Filling it would take a hero-specific graph height, which is not ruled. The graph stays at its real size.
- **Graph rings:** the cluster of rings after the key move (plies 62–70) overlaps at this width. DESIGN.md's "scroll or compress" rule for long games will decide that in the eval graph task.
- **Pen geometry:** it is still a lab approximation (fixed drift phases instead of the FNV-1a and mulberry32 seed, and a constant mask width per segment).
- **Coordinates on the squares:** these are not drawn on the placeholder board. When they arrive, the link's entry through d1 must keep 16px from the "c" and "d" file labels. Entering at d1's centre does that.

## Screenshots

These are in the worktree's `.shots/` (git-ignored). The static shots use reduced motion, so they show the end state.

- **Standard widths:** `.shots/lab-styleguide-hero-{a,b,c}/{390,1440}-{light,dark}.png`, from `pnpm shots --routes lab/styleguide-hero/a,lab/styleguide-hero/b,lab/styleguide-hero/c`.
- **Extra widths** (Playwright directly): `.shots/lab-styleguide-hero-{a,b,c}/360-{light,dark}.png` and `.shots/lab-styleguide-hero-a/{1024,1279}-{light,dark}.png`. `scrollWidth` equals the viewport in every shot, so there is no horizontal scroll.
- **Timed motion frames for A:** at 390 and 1440, day and night, from 0 to 2400ms: `.shots/motion-timed/lab-styleguide-hero-a/{390,1440}-{light,dark}-tNNNN.png`.
  - Contact sheets: `.shots/motion-timed/a-{390,1440}-{light,dark}-sheet.png`, and a close-up of the knight arc and bar swing in `.shots/motion-timed/a-1440-bar-knight.png`.
  - The 1440 frames lag their nominal time by up to 200ms, because each full-viewport capture is slow. The script logs the real times.

The variant pages stay at `/lab/styleguide-hero/{a,b,c}`. They are dev-only.

## Corrections

### Fix round 1

- **Wrong claim: "motion confirmed".** The `--motion` frames end at about 1.2s, so they never showed the badge, which had a transform-origin bug in all three variants. Its origin was in board pixels under Motion's `fill-box`, so the badge flew in from the bottom right. It now scales from its own centre.
- **Wrong claim: "ruling stays on the scoresheet".** A had a page divider between the board and the scoresheet (`B3`). It was removed.
- **Board size:** A's 384px board was off the `--board-size` table. It now uses `--board-size`.
- **Invented values:** invented pen easings and raw durations were replaced with `ease.pen`, `dur.draw`, `dur.drawSmall` and `dur.instant`.

### Fix round 2

- **The fix-round-1 claim that A implemented every link rule was wrong.**
  - Its visibility check measured the viewport height, not whether both ends were actually in view. It is now an `IntersectionObserver`.
  - Its width did not scale with the square. It is now 0.9 × square / 56.
- **The claim that A matched the hero contract was overstated.** A's stacked composition needed a DESIGN.md change, which is now ruled.
- **Second loud surface:** the eval graph, the label lane and the eval bar were added.
- **Margin comment:** the note moved into the sheet's margin, which removed the dead strip.
- **Excerpt:** it now ends before 31…Nf4??.
- **Spacing:** "−1.1" now sits at least 8px from the column rule, and the White and Black cells have 20px of left padding (at 360 the number column is 40px).
- **Note text** uses `text-wrap: pretty`.
- **Table** fills its column.
