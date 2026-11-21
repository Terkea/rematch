# ADR-WEB-13. Board: shipped Cburnett pieces, coordinates and piece motion

Date: 2026-11-21
Status: Accepted (Task 11, M4)

## Context

Task 11 builds the product board on react-chessboard 5.12.1 (variant c "Ink well", `packages/ui/design/decisions/board.md`). Three things needed a decision the design law does not spell out:

1. react-chessboard bundles unattributed Cburnett drawings as `defaultPieces`. The controller ruled they never render (ADR 0039), and DESIGN.md asks for build-time recolouring, never runtime `filter`.
2. The file letters sit in the bottom-right corner of the first rank at a fixed 12px (`xs`). At full size a knight's base reaches 0.86 of the square and a rook's 0.81, so on a 41px phone square the letters on b1 and g1 sit on the piece bases.
3. react-chessboard animates moves with one CSS transition of a fixed duration. DESIGN.md asks for per-piece mass (`settleFor`), a lift, a landing squash on `land`, the knights arc and captures knocked off.

## Decision

- **Pieces.** The twelve Commons originals are committed unmodified in `packages/ui/assets/pieces/cburnett/` with a `LICENSE` (sources, SHA-256 and the BSD-3-Clause text). `packages/ui/scripts/build-pieces.ts` (`pnpm --filter @ci/ui pieces`) parses them and maps every black paint to `var(--color-piece-dark)` and every white paint to `var(--color-piece-light)` in `src/board/pieces.generated.ts`. The CSS variables switch with the theme, so one generated set serves day and night and nothing recolours at runtime. A test rebuilds the set from the SVGs and fails on drift, and another checks that no rendered piece is a react-chessboard default.
- **Piece scale.** Pieces are drawn at 0.84 of the square below 768px and 0.9 from 768px (`PIECE_SCALE`, set as `--piece-scale` on the board root). Coordinates keep DESIGN.md's corners and size, inset 2px and 1px.
- **Motion.** react-chessboard runs with `showAnimations: false`. Every piece is rendered by our `pieces` renderers, which read a motion plan from a React context. The board plans the move in the render that brings the new FEN (from `lastMove` or its own last move), so the piece's first frame is at its origin. The plan is read once, when the piece mounts on its destination. Travel is one progress value on the piece's `settleFor` spring; the knights arc, lift and squash are derived from it. Castling moves the rook 70ms after the king. Reduced motion jumps the piece with one opacity cue of `dur.quick`.
- **Keyboard.** dnd-kit makes every piece a tab stop. The board removes them from the tab order (a `MutationObserver` keeps `tabindex="-1"`), because the `SquareGrid` is the keyboard interface.

## Consequences

- The start position's knights on b1 and g1 clear their letters at 360, 390 and 1440px (measured in the T11 screenshots). A rook, queen or king on the first rank's b to g files still touches its letter on 41–44px squares. Clearing those too needs pieces at about 0.75 or a smaller coordinate step on phones, which DESIGN.md would have to allow.
- Drag keeps react-chessboard's overlay: the drop lands with the squash, but the drag tilt and the separate shadow layer (choreography 6) are not built yet.
- The `/licenses` notice for Cburnett is still owed by the task that builds `/licenses`.
