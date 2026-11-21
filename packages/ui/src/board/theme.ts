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
 * Pieces are drawn smaller than the square, centred, so the 12px file letters in the first rank's
 * bottom-right corners stay clear of the piece bases (a knight's base reaches 0.86 of the square
 * at full size, a rook's 0.81). The letters are a fixed 12px, so small squares need the smaller
 * piece: 0.84 below 768px (41 to 60px squares) and 0.9 from 768px (56px squares and up). Checked
 * by measurement on b1 and g1 in the T11 screenshots. The board root sets `--piece-scale`.
 */
export const PIECE_SCALE = { phone: 0.84, wide: 0.9 } as const;

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

// Coordinates: Schibsted 500 at `xs`, on the squares. Day `ink`; night `on-ink` on light squares
// and `ink` on dark ones ("Board coordinates"). The light-square colour comes from a variable the
// board root sets per theme.
const coordinate: CSSProperties = {
  fontFamily: "var(--font-sans)",
  fontSize: "var(--text-xs)",
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
  darkSquareNotationStyle: { color: "var(--color-ink)" },
  // Files hug the bottom-right corner and ranks the top-left, so the glyphs sit outside the
  // 90%-scaled pieces' silhouettes.
  alphaNotationStyle: { ...coordinate, bottom: 1, right: 2 },
  numericNotationStyle: { ...coordinate, top: 2, left: 2 },
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
