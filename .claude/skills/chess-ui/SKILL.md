---
name: chess-ui
description: Use when building or changing anything that shows a chess board, moves, notation, evaluations, or chess charts.
---

# Chess UI conventions

Load `design-system` too: tokens, honesty rules and the done-checklist apply here. Law: `packages/ui/DESIGN.md` ("The board", "Severity and marks", "The eval graph and eval bar", "Pieces", "Type"). Decisions: `packages/ui/design/decisions/board.md`, ADR-WEB-13 (board, pieces, motion), ADR-WEB-15 (annotations), ADR-WEB-14 (weakness timeline), `docs/decisions/0039-piece-set.md`.

## Never

- **Never chessground or chessops** (B12; `pnpm design:check` fails the import). The board is `react-chessboard` with `chess.js`, wrapped by `Board` in `@ci/ui/board`. Never render react-chessboard's built-in pieces; use `boardPieces`.
- Never Unicode chess glyphs for pieces. Never invent chess data (B15): use `@ci/ui/fixtures` or real games.

## The board

- Use `Board` (`packages/ui/src/board/board.tsx`): `fen`, `label` (its accessible name, "Position after 28. Nd2"), `orientation`, `lastMove`, `check`, `annotations`, `interactive`, `onMove`. Do not restyle squares; `boardTheme` and `squareStyles` own the tints.
- Treatment (Task 10, variant c "Ink well"): flat `board-light` and `board-dark` squares, `--radius-board`, no frame, shadow or wood. From 768px the board sits in a `well` with 8px padding; below that it is flush. Coordinates are drawn on the squares, never in outside lanes. Pieces are drawn at 0.84 (phone) or 0.88 of the square so none covers a coordinate.
- **Pieces:** Cburnett (BSD-3-Clause) is the default, Chessnut (Apache-2.0) the option, recoloured at build time by `pnpm --filter @ci/ui pieces` into per-theme sprites (`pieces.generated.ts`). Licence gate: no GPL, AGPL, NC or unlicensed sets; each shipped set is recorded in ADR 0039 and credited on `/licenses`. Never hand-edit the generated file.
- Squares, check, legal moves and focus follow the halo rule below. Check is an `ink` ring, never pen red (it is not a mistake).
- **Keyboard** (`SquareGrid`): the board is a roving-tabindex 8x8 grid. Arrows move focus as drawn, Enter or Space picks up and drops, Escape cancels. Do not take those keys on a focused square. The review adds its own keys through `useReviewKeys` (`REVIEW_KEYS`: arrows, Home, End, up and down for moments, `f` flip, space play, `b` engine), guarded for text inputs.
- Sound: `useBoardSounds()` from `@ci/ui/audio`, one shared context, original synthesised sounds only, never copied. Sound never carries information alone.

## Annotations

Draw marks with `AnnotationLayer` (`@ci/ui/annotations`), passed through `Board`'s `annotations`. Pen strokes are seeded, variable-width and drawn by `pathLength`.

- Tones (`Tone` = `pen` | `good` | `pencil`): **`pen`** for the played mistake (the player's blunder or mistake); **`good`** for the better move or the punishment; **`pencil`** for plans, ideas, threats and the opponent's mistakes. Never any other colour.
- **Glyph set:** `??` blunder, `?` mistake, `?!` inaccuracy, `!` good, `!!` brilliant. Pen marks carry a `?`-family badge and an open two-stroke arrowhead; good marks a `!`-family badge and a solid head. Plan marks carry no badge. Colour never carries meaning alone.
- **Halo rule:** every board mark (ring, arrow, glyph badge, focus ring, check ring, legal-move dot) sits on a 2px `--color-sheet` halo, so a mark's neighbouring colour is its halo and not the square. Not optional: without it night marks fall to 1.01:1.
- Badge in the top-right corner, or the corner farthest from arrow heads, never a corner holding a coordinate. At most 12 marks animate at once. `describeAnnotation` gives the text alternative.

## Notation and evaluations

- Format moves with `formatNotation` (`packages/ui/src/board/notation.ts`), or render with `NotationText`. Never build the string by hand. `31. Rxe6?? −4.2` for White, `31… Kg7` for Black (`…` U+2026 after the number), glyph kerned to the move (`--tracking-glyph`), eval after a space.
- Evals use the real minus **U+2212** (`−`), never a hyphen. `formatEval`: pawns to one decimal, `0.0` for level, mate as `#3` or `#−3`. The eval bar label is `evalBarLabel` (`+5.7`, `M2`).
- Notation is Literata; glyphs in `pen-text` only for the player's own mistakes (opponent's in `ink-soft`). Use `judgementGlyph` for the glyph, not literals.
- Severity thresholds are on the win-chance scale (Lichess): a loss of at least 0.3 is a blunder, 0.2 a mistake, 0.1 an inaccuracy. Never reclassify in the UI.

## Eval bar and eval graph

- **White's point of view always**, stored and shown (`cp` and `mate` are White-POV; the bar's White end flips with the board orientation only).
- The y-axis is **win chance** (`winningChances`), not pawns, so a +8 does not flatten the early mistakes. Mate maps to 100%.
- `EvalBar` (`@ci/ui/board`): vertical beside the board, 8px horizontal bar above it below 768px. Moves on `swing`, `swing-big` for a change of 0.3 or more. Label in the losing side's fill colour.
- `EvalGraph` (`@ci/ui/eval-graph`): full width under the board, ink line over a 10% area, zero line `rule-strong`, an `ink` cursor for the shown move. **Ticks and rings only for mistakes and above** as the red scale (blunder, mistake); inaccuracies are thin rings, labelled only in containers 768px and wider. The player's marks are `pen`, the opponent's `pencil` hairlines. Labels never overlap, never sit on the line, and leaders never cross.
- Charts that show rates (`AccuracyTrend`, `JudgementColumns`, `ResultSplit`, `KpiStat`) obey the honesty rules in `design-system`: **n with every rate**, unknown is a dash and never 0, no `NaN`, intervals rounded outward, and no "improved" or "worse" unless intervals separate.

## Checks

- Tests in `packages/ui/test/` (`board.test.tsx`, `annotations.test.tsx`, `notation.test.ts`, `eval-graph.test.tsx`, `charts.test.tsx`) show the expected behaviour. Add yours first.
- Look at it at 390 and 1440 in day and night, with a fixture game that has a `??`, a mate and a long game, then run the critique loop from `design-system`.
