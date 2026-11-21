// The shape of a shipped piece set, shared by the board and the build-time recolouring script.

export type PieceCode =
  | "wK"
  | "wQ"
  | "wR"
  | "wB"
  | "wN"
  | "wP"
  | "bK"
  | "bQ"
  | "bR"
  | "bB"
  | "bN"
  | "bP";

export interface PieceNode {
  tag: "g" | "path" | "circle";
  /** geometry and transform, as SVG attributes */
  attrs: Record<string, string>;
  /** presentation, as a React style object (camelCase) */
  style: Record<string, string>;
  children: PieceNode[];
}

export type PieceSet = Record<PieceCode, PieceNode[]>;
