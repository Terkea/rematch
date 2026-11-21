import type { Orientation, Square } from "../board/theme.js";

export type { Orientation, Square };

/**
 * The colour of a mark (DESIGN.md, "Severity and marks"): `pen` for the player's mistakes, `good`
 * for better moves and good play, and `pencil` for ideas and the opponent's mistakes (controller
 * ruling, Task 13: the opponent's marks are drawn in pencil, the player's in pen).
 */
export type Tone = "pen" | "good" | "pencil";

export type Glyph = "??" | "?" | "?!" | "!" | "!!";

export type Annotation =
  | { kind: "arrow"; from: Square; to: Square; tone: Tone }
  | { kind: "circle"; square: Square; tone: Tone }
  | { kind: "mark"; square: Square; glyph: Glyph; tone: Tone };

export interface AnnotationLayerProps {
  annotations: readonly Annotation[];
  orientation: Orientation;
  /** Draw the marks on (default). False, or reduced motion, renders them fully drawn. */
  animate?: boolean;
}

/** A point in board units: one square is 1, (0, 0) is the top-left corner as drawn. */
export interface Point {
  x: number;
  y: number;
}
