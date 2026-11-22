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
}
