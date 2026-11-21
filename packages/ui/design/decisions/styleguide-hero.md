# Styleguide hero: decision

Task 6, lab procedure (LP) step 4. Date: 2026-11-21.

## The moment

All three variants show the same critical moment, which is computed from `games.middlegameBlunder` rather than typed in by hand. It is the player's worst blunder by lost win chance: **28. Nd2?? −1.1** (White, DrNykterstein v Night-King96, Lichess blitz). The evaluation was +2.7 before the move, and win chance fell from 73% to 40%. Black's reply was 28…e4.

The coach note is one sentence built from facts the fixture shows: "Your knight on e4 was holding back Black's e-pawn; the moment it went to d2, the pawn came forward with 28…e4, and +2.7 became −1.1." The fixture has no best move (`bestUci` is null), so the note doesn't name one (`B15`).

Each variant plays the same beat from choreography 1's climax:

- 0ms: the row is selected.
- 400ms: the knight lifts.
- 490ms: it travels on `settleFor("knight")`.
- 760ms: it lands with its squash on `land`.
- 860ms: the pen ring draws, and the glyph and evaluation appear.
- 980ms: the pen arrow draws, with its head strokes at 1246ms and 1276ms.
- 1400ms: the `??` badge lands, and the note appears.

Under reduced motion, the end state shows at once (M10).

## Variants

| | A "Scoresheet margin" | B "Book plate" | C "Coach's note" |
|---|---|---|---|
| Where the notation sits | One row of a ruled scoresheet, at `base` | Under the figure, at `4xl` | The `h1`, at `5xl` |
| Board and text | The board sits in the scoresheet's margin column, and the note sits in the right margin, level with the row | The board is a figure with a centred caption, the game is printed as running text, and the note is a footnote | The board is offset beside the headline, lower down |
| How the red pen connects them | A scoresheet ring on the row and a board ring draw in the same beat | Only the `??` glyph and a footnote numeral | One pen stroke rings the move, then runs through the gutter into d2 along rank 2 |

## Ranking (self-critique against DESIGN.md)

The lane rules forbid this implementer from dispatching subagents, so LP step 3 (a fresh subagent ranks the variants) was done as a self-critique. An independent CL pass on `/styleguide` is still owed when the hero lands there (Tasks 11 and 12).

1. **A, the winner.** A matches the system's own contract:
   - DESIGN.md "Styleguide hero" asks for the game as the `h1` at `3xl`, the board, and the ruled scoresheet. A puts the pen on both the board and the row, which is the "annotated scoresheet" direction at its most literal.
   - The scoresheet ring uses its documented geometry (0.8×, `rx = w/2 + 10`, `ry` 14px) in a 34px row (`--space-scoresheet-row`).
   - Ruling stays on the scoresheet surface (`B3` guard), and the panel keeps `--radius-sheet`.
   - Issues:
     - It is the quietest of the three. At 1440px the margin note's 15ch column reads ragged.
     - The margin board is 384px, which is off the `--board-size` table. It needs a mini-board size, or the full size.
     - The arrowheads are hidden under the ring and the badge on a knight's short diagonal.
2. **C, second.** C is the boldest and the most memorable: the pen gesture from headline to board is the brand's signature at full volume, and its motion is the most "huge". But it breaks the design law in three places:
   - `5xl` is landing-only (Type scale), and the cliché review's guard says the styleguide hero "is the game, not a display headline". This drifts toward `B1`.
   - The linking stroke is a pen mark with no `?`-family badge and no open head. That breaks "pen marks always carry a `?`-family badge and an open head" (Severity and marks). It is a new mark type, and DESIGN.md doesn't have it.
   - The move is the `h1` and the game is demoted to a footnote line. On mobile the board has to come first so the stroke can reach it.
3. **B, third.** B is elegant but editorial:
   - A big serif on light paper with a footnote rule is the look the cliché review warns about (`B1`, `B3` drift).
   - The footnote rule is decoration, not a scoresheet.
   - The running text is a bold block, and its five red glyphs dilute the one that matters.
   - The pen barely connects board and text: only the glyph's colour does.
   - At 1440px the right column has a large dead gap.

No variant shows `B5`, `B6`, `B7`, `B8`, `B13` or `B16`. All use only tokens.

## Decision

**A, "Scoresheet margin"**, is the base for the `/styleguide` hero. The board and annotation lanes (Tasks 11 and 12) should build it with the real board and `@ci/ui/annotations` geometry.

Taken from the losers:

- **From C:** the note appears only after the marks land (the comment follows the evidence). Also from C: the cross-surface pen link, as a **proposal for the owner**, not shipped. If it is approved, DESIGN.md needs a new "link" mark (pen, no badge, no head) with its own rule. Until then, A connects the surfaces by drawing both rings in the same beat.
- **From B:** the centred figure caption ("Position after 28. Nd2, Black to move"), which states the side to move. B's running notation also suits print or share views later.

Rejected: the `5xl` notation headline (`5xl` is landing-only), the footnote rule, and bold running text in the app.

Follow-ups for the lanes building on A:

- Size the margin board with `--board-size`, or add a documented mini-board size.
- Widen the margin note at 1440px, or set it under the rows.
- Keep the arrowheads clear of the badge.

## Screenshots

These are in the worktree's `.shots/` (git-ignored), from `pnpm shots --routes lab/styleguide-hero/a,lab/styleguide-hero/b,lab/styleguide-hero/c` with reduced motion, so they show the end state:

- `.shots/lab-styleguide-hero-a/{390,1440}-{light,dark}.png`
- `.shots/lab-styleguide-hero-b/{390,1440}-{light,dark}.png`
- `.shots/lab-styleguide-hero-c/{390,1440}-{light,dark}.png`
- Motion frames for C: `.shots/motion/lab-styleguide-hero-c/*-f00..f11.png` (from `--motion`)

The variant pages stay at `/lab/styleguide-hero/{a,b,c}`. They are dev-only.
