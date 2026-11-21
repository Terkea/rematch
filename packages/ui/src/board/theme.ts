import type { CSSProperties } from "react";

// Board styling from the tokens (DESIGN.md, "The board"; design/decisions/board.md, variant c
// "Ink well"). `board/**` is one of the places raw geometry is allowed besides tokens.css.

export type BoardFile = "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h";
export type BoardRank = 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8;
export type Square = `${BoardFile}${BoardRank}`;
export type Orientation = "white" | "black";

export const FILES: readonly BoardFile[] = ["a", "b", "c", "d", "e", "f", "g", "h"];
export const RANKS: readonly BoardRank[] = [1, 2, 3, 4, 5, 6, 7, 8];

/** The viewport width from which the board sits in its 8px `well` (ruling 3, Task 10). */
export const WELL_MIN_WIDTH = 768;

/**
 * Coordinates (controller ruling, Task 11): Schibsted 500 at 0.2 of the square with a 2px inset,
 * files in the bottom-right corner and ranks in the top-left. `advance` is the widest glyph's
 * advance in em (Schibsted Grotesk 500: "6" 0.601, "b" and "d" 0.600), measured in Chromium.
 */
export const COORDINATE = { size: 0.2, inset: 2, advance: 0.61 } as const;

/** The two coordinate glyph boxes on a square of `px` pixels, in square units (0 to 1). */
export function coordinateBoxes(px: number) {
  const height = (COORDINATE.size * px) / px;
  const width = (COORDINATE.size * COORDINATE.advance * px) / px;
  const inset = COORDINATE.inset / px;
  return {
    rank: { top: inset, left: inset, bottom: inset + height, right: inset + width },
    file: { top: 1 - inset - height, left: 1 - inset - width, bottom: 1 - inset, right: 1 - inset },
  };
}

/**
 * Pieces are drawn smaller than the square, centred, so no piece covers a coordinate on any
 * square: the widest reach is a bishop's base toward the bottom-right and a queen's crown toward
 * the top-left. The 2px inset weighs more on small squares, so phones (41 to 60px squares) take
 * 0.84 and boards from 768px (56px squares and up) 0.88. board.test.tsx checks every point each
 * piece paints against the glyph boxes. The board root sets `--piece-scale`.
 */
export const PIECE_SCALE = { phone: 0.84, wide: 0.88 } as const;

/** Column (0 = left as drawn) and row (0 = top as drawn) of a square in the given orientation. */
export function squareToCell(
  square: Square,
  orientation: Orientation,
): { col: number; row: number } {
  const file = square.charCodeAt(0) - 97;
  const rank = Number(square[1]) - 1;
  return orientation === "white" ? { col: file, row: 7 - rank } : { col: 7 - file, row: rank };
}

export function cellToSquare(col: number, row: number, orientation: Orientation): Square {
  const file = orientation === "white" ? col : 7 - col;
  const rank = orientation === "white" ? 8 - row : row + 1;
  return `${FILES[file] as BoardFile}${rank as BoardRank}`;
}

/**
 * How far short of the target centre an arrow shaft ends, in squares: 0.16 for a bare target, and
 * 0.46 when the target carries a ring, so the tip stops on the ring's outer edge (ruling 1).
 */
export function shaftClearance({ ringed }: { ringed: boolean }): number {
  return ringed ? 0.46 : 0.16;
}

/**
 * The glyph badge's corner: top right, unless the move's arrow arrives from the right of the
 * target as drawn (the origin's centre is right of the target's in this orientation), so the badge
 * never covers the arrowhead (ruling 2).
 */
export function badgeCorner(
  arrow: { from: Square; to: Square } | null,
  orientation: Orientation,
): "top-left" | "top-right" {
  if (!arrow) return "top-right";
  const from = squareToCell(arrow.from, orientation);
  const to = squareToCell(arrow.to, orientation);
  return from.col > to.col ? "top-left" : "top-right";
}

// Coordinates: Schibsted 500 at 0.2 of the square (the board is 8 squares, so 0.025 of
// --board-size), on the squares. Day `ink`; night `on-ink` on light squares and `ink` on dark ones,
// and `on-ink` on both when tinted ("Board coordinates"). The colours are variables the board root
// and its tinted squares set.
const coordinate: CSSProperties = {
  fontFamily: "var(--font-sans)",
  fontSize: `calc(var(--board-size) * ${COORDINATE.size / 8})`,
  lineHeight: 1,
  fontWeight: 500,
  position: "absolute",
  userSelect: "none",
  zIndex: 1,
};

export const boardTheme = {
  lightSquareStyle: { backgroundColor: "var(--color-board-light)" },
  darkSquareStyle: { backgroundColor: "var(--color-board-dark)" },
  lightSquareNotationStyle: { color: "var(--board-coord-light)" },
  darkSquareNotationStyle: { color: "var(--board-coord-dark)" },
  alphaNotationStyle: { ...coordinate, bottom: COORDINATE.inset, right: COORDINATE.inset },
  numericNotationStyle: { ...coordinate, top: COORDINATE.inset, left: COORDINATE.inset },
  boardStyle: { borderRadius: "var(--radius-board)", overflow: "hidden" },
  dropSquareStyle: { boxShadow: "inset 0 0 0 2px var(--color-sheet)" },
  // The origin keeps a 30% ghost while the piece is dragged (DESIGN.md, "Board states").
  draggingPieceGhostStyle: { opacity: 0.3 },
  // The lift scale of a knight or bishop; the drag tilt and shadow layer are Task 12b's.
  draggingPieceStyle: { transform: "scale(1.05)" },
} as const satisfies Record<string, CSSProperties>;

/** A flat tint over the square's own colour: one solid layer, never a gradient wash (B7). */
const tint = (token: string): CSSProperties => ({
  backgroundImage: `linear-gradient(var(${token}), var(${token}))`,
});

/**
 * Check: a 3px `ink` inset ring on a 2px `sheet` halo on both sides, so the ring never touches a
 * square (the halo rule). Never pen red: check isn't a mistake. No radial gradient (B7).
 */
export const CHECK_RING =
  "inset 0 0 0 2px var(--color-sheet), inset 0 0 0 5px var(--color-ink), inset 0 0 0 7px var(--color-sheet)";

export interface SquareState {
  lastMove?: { from: Square; to: Square } | undefined;
  check?: Square | undefined;
  selected?: Square | null | undefined;
}

/** Per-square styles for react-chessboard's `squareStyles`: last move, selection and check. */
export function squareStyles({
  lastMove,
  check,
  selected,
}: SquareState): Record<string, CSSProperties> {
  const styles: Record<string, CSSProperties> = {};
  const merge = (sq: Square, style: CSSProperties) => {
    styles[sq] = { ...styles[sq], ...style };
  };
  if (lastMove) {
    merge(lastMove.from, tint("--color-board-lastmove"));
    merge(lastMove.to, tint("--color-board-lastmove"));
  }
  if (selected) merge(selected, tint("--color-board-select"));
  if (check) merge(check, { boxShadow: CHECK_RING });
  return styles;
}
