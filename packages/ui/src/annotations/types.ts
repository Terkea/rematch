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
  | {
      kind: "arrow";
      from: Square;
      to: Square;
      tone: Tone;
      /**
       * Stop the tip on the target square's edge, as for a ringed target, so no ink covers the
       * piece there (the player's move in red pen, M5a T10).
       */
      toEdge?: boolean;
    }
  | { kind: "circle"; square: Square; tone: Tone }
  | {
      kind: "mark";
      square: Square;
      glyph: Glyph;
      tone: Tone;
      /**
       * `full` (default): a solid disc. `light`: the tone's outline on paper, for a lesser mark
       * in the same colour (the player's `?` beside a full-red `??`, M5a T10 fix round 1).
       */
      weight?: "full" | "light";
      /**
       * `inset` (default): inside the square's freest corner. `edge`: centred on one of the
       * square's top corners, half outside it, so it never sits on the piece (M5a T10).
       */
      place?: "inset" | "edge";
    };

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
