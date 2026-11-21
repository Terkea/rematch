# Board treatment

Date: 2026-11-21. Task 10 (M4). Status: decided by the lab, with three proposals for the controller.

Lab routes: `/lab/board/a`, `/lab/board/b` and `/lab/board/c`. Each shows the same position, `games.middlegameBlunder` after ply 55 (28. Nd2??, e4 to d2), with one pen arrow (e4 to d2), one pen ring (d2) and one `??` badge (d2), plus the last-move tint on e4 and d2. Screenshots at 390 and 1440 px in day and night are in the worktree's `.shots/lab-board-{a,b,c}/` (git-ignored).

## Decision

**Variant c, "Ink well"**, with one change: the well applies from 768px up only.

| Axis | Chosen |
|---|---|
| Squares | flat `board-light` and `board-dark`, nothing drawn on them |
| Frame | none. From 768px the board sits in a `well` with 8px padding, and both have `--radius-board`. Below 768px the board is flush, at the full `--board-size`. |
| Coordinates | on the squares: ranks top left of the a-file, files bottom right of the first rank, Schibsted 500 at `xs`. Day: `ink`. Night: `on-ink` on light squares and `ink` on dark squares (DESIGN.md, "Board coordinates"). |
| Stroke | the DESIGN.md variable-width outline, filled and tapered, on a 2px `sheet` halo |
| Pieces | Cburnett (see "Piece set") |

### Stroke

All values are at a 56px square and scale with the square (in board units, one square = 1).

- **Width profile:** 2.2px at the start (0.039 of a square), 3.2px from 35% to 65% of the length (0.057), and 1.4px at the end (0.025), eased with smoothstep. The ends are round caps. This is DESIGN.md's profile unchanged.
- **By mark:** pen and good 1.0×, pencil 0.7×, the thin inaccuracy ring 0.75×, and arrowhead strokes 0.9× of the shaft.
- **Halo:** 2px of `sheet` on each side (a 4px stroke under the fill), drawn per segment so a later segment's halo cuts over an earlier one.
- **Ring:** the DESIGN.md spiral (ry 0.38, rx/ry 1.12, tilt 8–12°, −60° to 380°, 2.5px of growth per revolution), drawn as two segments, 0–360° and the 20° overrun, so the tail draws over the start.

### Arrowhead geometry

- **Shaft:** starts 0.22 of a square from the origin centre and bows 3% of its length (2–4%, seeded) as a quadratic.
- **Shaft end:** 0.16 short of the target centre when the target is bare, as DESIGN.md says. **Proposal 1:** when the target also carries a ring, the tip stops 0.46 from the centre, on the ring's outer edge. At 0.16 the head was buried inside the ring and under the knight in every variant.
- **Open head (pen, pencil):** two separate strokes, 0.27 and 0.33 of a square (0.9 and 1.1 of the 0.30 base), at 26° and 32° to the shaft, starting at the tip and following the shaft's end tangent. Which side gets which is seeded. Each stroke uses the same taper profile at 0.9× width.
- **Solid head (good):** a filled triangle with the same unequal sides, on the halo (not exercised in this lab).

### Badge

**Proposal 2:** the badge sits top right (DESIGN.md) unless the move's arrow arrives from the right of the target, in which case it moves to the top-left corner. In the lab position (e4 to d2) the arrow comes in from the upper right, and the top-right badge covered the whole arrowhead.

### Well below 768px

**Proposal 3:** no well on phones. At 390px the well's 8px padding costs 16px of board (336px against 352px). Coordinates on the squares already cost nothing, so a flush board is the largest board any variant gives on a phone.

## Critique of each variant

DESIGN.md says the board "has `--radius-board` and no frame, shadow or wood", and its squares are `board-light` and `board-dark`, so a and b start by breaking a rule. They were run because the brief asked for real alternatives on each axis.

### a, "Printed diagram"

- **Squares:** flat, quiet and correct. The 1px `rule-strong` frame does read as a printed diagram, and it gives the board a crisp edge on `paper` in day, where `board-light` against `paper` alone is soft.
- **Coordinates:** outside, in Schibsted `xs` (`ink-soft`), so no square carries text. But the lanes cost 24px of board at 390px (328px against 352px), and the reader's eye has to jump off the board to read a square.
- **Stroke:** a fine, even 0.08-square line (4.5px at 56px). It is legible and calm, but it reads as a vector tool, not a hand. Constant width loses the pen's signature, and at 41px squares the ring looks like a UI focus ring.
- **Verdict:** the best static diagram and the dullest mark. Not taken. The frame idea is noted but rejected for the "no frame" rule.

### b, "Book hatching"

- **Squares:** dark squares are 1px diagonal `ink` hatching (42% `ink` in `color-mix`, 4px pitch) over `board-light`. In day at 1440px it is charming and clearly bookish. It fails on the job: the line texture competes with every piece outline on a dark square, so the eye works harder to read the position, and in night the pale hatching over the night `board-light` nearly disappears, so the board reads as almost one tone at 390px. It also collides with the last-move tint, which is hard to see over hatching.
- **Coordinates:** Literata on all four sides, like an old book. It looks the part but costs 32px of board on a phone (320px against 352px), the most of any variant.
- **Stroke:** a broad 0.11-square marker (6.2px at 56px). It is loud enough to be seen at once, but it is blunt. The open head turns into a blob at 41px squares, and the ring swallows most of the knight.
- **Verdict:** the most distinctive look and the worst board to read. Not taken.

### c, "Ink well"

- **Squares:** flat token squares, so pieces and marks carry the eye, as DESIGN.md asks. The `well` gives the board a set-in, pressed feel without a drawn frame, and in night the dark well around the livelier night squares makes the board the brightest object on the page, which is right for the hero.
- **Coordinates:** on the squares, using DESIGN.md's per-square colours. They are readable in both themes and cost no pixels.
- **Stroke:** the tapered outline is the only one of the three that looks drawn by a hand. It is thin at 41px squares (3.2px peaks become 2.4px at 42px squares). That is acceptable, because the halo carries it, and the motion (the draw-on reveal) is what sells the pen. Revisit with the animated reveal in Task 11 before changing the widths.
- **Weaknesses:** the well costs 16px on phones (Proposal 3), and a ring, an arrowhead and a badge on one square are crowded on any variant (Proposals 1 and 2).
- **Verdict:** taken.

## Piece set

**Cburnett, under its BSD-3-Clause option, as ADR 0039 decided. Chessnut (Apache-2.0) stays the option.** No piece assets were added in this task (the brief lists none).

### react-chessboard licence check (Step 1)

- `npm pack react-chessboard@5.12.1` was unpacked in a scratch directory and read, never run. `package.json` says `"license": "MIT"`, and `LICENSE` is MIT, "Copyright (c) 2022 Ryan Gregory".
- Its default pieces (`dist/index.esm.js`, `defaultPieces`) are Cburnett: 45×45 viewBox, 1.5 stroke, and path data identical to the Commons file. The `wP` path matches `Chess_plt45.svg` from upload.wikimedia.org character for character.
- The package carries no attribution to Colin M.L. Burnett and no BSD notice. The MIT grant covers Ryan Gregory's code. It cannot relicense Burnett's drawings, and their provenance in the package (Commons or Lichess's GPL-labelled copy) is unstated.
- **Result:** the code is MIT and fine. The default piece art must not be shipped from the package. The product takes Cburnett from Wikimedia Commons under BSD-3-Clause with the notice on `/licenses` (ADR 0039), recoloured at build time, and passes it to react-chessboard through its `pieces` option.
- The lab pages use the package's embedded copy, recoloured at runtime to `--color-piece-dark` and `--color-piece-light`, only to compare the treatments. The lab is not served in production (`LAB_ENABLED`), so nothing ships from it.
- No copyleft source was used: no chessground, no Lichess assets.

## Follow-ups

- Task 11 (board wrapper): implement c, the three proposals if approved, and the build-time Cburnett sprites from Commons with `LICENSE` and attribution, in the location its brief names.
- DESIGN.md: if the proposals are approved, update "Pen stroke geometry" (shaft end with a ring), "Glyph badge" (corner rule) and the board's well rule.
