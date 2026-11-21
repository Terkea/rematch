# ADR-WEB-13. Board: shipped Cburnett pieces, coordinates and piece motion

Date: 2026-11-21
Status: Accepted (Task 11, M4)

## Context

Task 11 builds the product board on react-chessboard 5.12.1 (variant c "Ink well", `packages/ui/design/decisions/board.md`). Three things needed a decision the design law does not spell out:

1. react-chessboard bundles unattributed Cburnett drawings as `defaultPieces`. The controller ruled they never render (ADR 0039), and DESIGN.md asks for build-time recolouring, never runtime `filter`.
2. Coordinates sit on the squares. At full size a piece's base or crown reaches into the corners (a bishop's base to 0.88 of the square, a knight's to 0.86), so on a 41px phone square the letters on b1 and g1 sat on the piece bases.
3. react-chessboard animates moves with one CSS transition of a fixed duration. DESIGN.md asks for per-piece mass (`settleFor`), a lift, a landing squash on `land`, the knights arc and captures knocked off.

## Decision

- **Pieces.** The twelve Commons originals are committed unmodified in `packages/ui/assets/pieces/cburnett/` with a `LICENSE` (sources, SHA-256 and the BSD-3-Clause text). `packages/ui/scripts/build-pieces.ts` (`pnpm --filter @ci/ui pieces`) parses them and maps every black paint to `var(--color-piece-dark)` and every white paint to `var(--color-piece-light)` in `src/board/pieces.generated.ts`. The CSS variables switch with the theme, so one generated set serves day and night and nothing recolours at runtime. A test rebuilds the set from the SVGs and fails on drift, and another checks that no rendered piece is a react-chessboard default.
- **Coordinates and piece scale** (controller ruling). Coordinates are 0.2 of the square with a 2px inset, files bottom right and ranks top left. Pieces are drawn at 0.84 of the square below 768px and 0.88 from 768px (`PIECE_SCALE`, `--piece-scale` on the board root), so no piece covers a coordinate on any square from 41px up. A test flattens every piece's outline (curves, arcs, transforms, half the stroke) and checks it against both glyph boxes. On a tinted night square the coordinate turns `on-ink`, through a scoped rule per tinted square.
- **Motion.** react-chessboard runs with `showAnimations: false`. Every piece is rendered by our `pieces` renderers, which read a motion plan from a React context. The board plans the move in the render that brings the new FEN (from `lastMove` or its own last move), so the piece's first frame is at its origin. The plan is read once, when the piece mounts on its destination. Travel is one progress value on the piece's `settleFor` spring; the knights arc, lift and squash are derived from it. Castling moves the rook 70ms after the king. Reduced motion jumps the piece with one opacity cue of `dur.quick`.
- **Keyboard.** dnd-kit makes every piece a tab stop. The board removes them from the tab order (a `MutationObserver` keeps `tabindex="-1"`), because the `SquareGrid` is the keyboard interface.

## Consequences

- Pieces are slightly smaller than a full-square set (0.84 and 0.88). Boards with squares under 41px (thumbnails) are not covered by the check, and should hide coordinates.
- Drag keeps react-chessboard's overlay: the drop lands with the squash, but the drag tilt and the separate shadow layer (choreography 6) are a follow-up; the Task 11 brief does not specify them.
- The `/licenses` notice for Cburnett is handed to Task 27.

## Addendum (Task 11 review, fix round 1)

- **Our own moves:** a move the board sent through `onMove` animates only if the new FEN's placement is that move played on the previous FEN. Otherwise the board uses `lastMove`. The pending move is spent after one render.
- **In-flight pieces:** a piece still in the air registers where it is drawn now. When a new FEN moves it again, its plan starts from there, already lifted, instead of jumping back to its square (M7). It does not carry the spring's velocity over yet.
- **Marks and coordinates:** on the check square the coordinate is `ink` on a 2px `sheet` halo, drawn over the ring. Capture rings and focus rings on coordinate squares are cut by a mask where the glyph sits.
- **Tracked follow-ups:** underpromotion (the board plays a queen until the drill UI brings a picker); a live region announcing the move played; drag tilt and the drag shadow layer (choreography 6); passing the spring's velocity on when a piece is re-planned mid-flight.
