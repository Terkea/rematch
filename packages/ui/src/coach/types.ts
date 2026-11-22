// Structural mirrors of the server types (Rust `ai::segments::Segment`, `ai::output::disclosure`),
// redeclared here so @ci/ui never imports from the web app. The web's `Segment` and
// `AiDisclosure` (server/coaching/types.ts) are assignable to these.

export type Segment =
  | { kind: "text"; text: string }
  | { kind: "move"; gameId: string; ply: number; san: string }
  | { kind: "best"; gameId: string; ply: number; san: string }
  | { kind: "pv"; gameId: string; ply: number; side: "best" | "refutation"; n: number; san: string }
  | { kind: "line"; lineKey: string; n: number | null; san: string }
  | { kind: "num"; path: string; value: number; display: string }
  | { kind: "date"; path: string; date: string }
  | { kind: "square"; square: string }
  | { kind: "eng"; evalId: string; line: number | null; n: number | null; text: string };

export type SegmentKind = Segment["kind"];

/** The stored `ai_disclosure`. Only `label`, `note` and `generatedAt` are shown. */
export interface AiDisclosureData {
  label: string;
  note: string;
  model?: string;
  promptVersion?: string;
  generatedAt: string;
}

/**
 * The fixed label while an answer is still streaming and no disclosure is stored yet. Mirrors the
 * Rust `AI_DISCLOSURE_LABEL` byte for byte.
 */
export const AI_LABEL = "Written by AI";

export interface StreamedChunk {
  seq: number;
  segments: Segment[];
  interim: boolean;
}

export interface EngineCheck {
  status: "pending" | "done" | "failed";
  moves: string[];
  /** The game ply of the first move (1 = White's first move). Without it the moves are not numbered. */
  startPly?: number | null;
}

/**
 * A position an answer cites (Fix D, N5), read on the server with the thread: the position before
 * the move at `ply` of the game the segments name by its platform id (`gameId`), keyed
 * `${gameId}#${ply}`.
 */
export interface CitedPosition {
  gameId: string;
  ply: number;
  /** The review at that ply (`/app/games/<games.id>?ply=N`). */
  href: string;
  /** The position before the move. */
  fen: string;
  /** The player's side. */
  orientation: "white" | "black";
  /** "30. Rxg7" or "30… Rxg7". */
  moveText: string;
  played: { san: string; uci: string };
  /** The engine's better move, or null when it is the move played or unknown. */
  best: { san: string; uci: string } | null;
  /** The player's own mistake or blunder: the played move is drawn in pen. */
  mistake: boolean;
}

/** The key of a cited position: the segment's game and ply. */
export const citedKey = (gameId: string, ply: number): string => `${gameId}#${ply}`;
