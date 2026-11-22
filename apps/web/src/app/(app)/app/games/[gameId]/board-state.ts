import { formatEval, type Score, type Square } from "@ci/ui/board";
import type { EvalGraphPoint } from "@ci/ui/eval-graph";
import type { ScoresheetMove } from "@ci/ui/review";
import { Chess } from "chess.js";
import type { ReviewView } from "./view-model";

// Pure readings of the view at a ply: the position, the last move, check, the eval, the clocks.

type Color = "white" | "black";

const plyAt = (v: ReviewView, ply: number) => (ply > 0 ? v.plies[ply - 1] : undefined);

export function fenAt(v: ReviewView, ply: number): string {
  return plyAt(v, ply)?.fenAfter ?? v.startFen;
}

export function lastMoveAt(v: ReviewView, ply: number): { from: Square; to: Square } | undefined {
  const p = plyAt(v, ply);
  return p ? { from: p.uci.slice(0, 2) as Square, to: p.uci.slice(2, 4) as Square } : undefined;
}

/** The checked king's square, if the side to move is in check. */
export function checkSquare(fen: string): Square | undefined {
  try {
    const chess = new Chess(fen);
    if (!chess.inCheck()) return undefined;
    for (const row of chess.board())
      for (const cell of row)
        if (cell?.type === "k" && cell.color === chess.turn()) return cell.square as Square;
  } catch {
    // an unreadable FEN draws no check
  }
  return undefined;
}

/** The scoresheet rows. "Best" is tagged on the player's own moves only (critic, T7). */
export function sheetMoves(v: ReviewView): ScoresheetMove[] {
  return v.plies.map((p) => ({
    ply: p.ply,
    color: p.color,
    san: p.san,
    judgement: p.analysis?.judgement ?? null,
    best: p.color === v.playerColor && p.analysis?.best === true,
    mine: p.color === v.playerColor,
  }));
}

export function graphPoints(v: ReviewView): EvalGraphPoint[] {
  if (v.status !== "analysed") return [];
  return v.plies.flatMap((p) =>
    p.analysis
      ? [{ ply: p.ply, winWhite: p.analysis.winWhite, san: p.san, judgement: p.analysis.judgement }]
      : [],
  );
}

export function scoreAt(
  v: ReviewView,
  ply: number,
): { score: Score; source: "engine" | "derived" } | null {
  if (v.status !== "analysed") return null;
  if (ply === 0) return v.startScore;
  const a = plyAt(v, ply)?.analysis;
  return a ? { score: a.score, source: a.scoreSource } : null;
}

/** `+0.6`, `#3`, or `≈ −1.2` for a score derived from the stored win% (ADR 0049 R7). */
export function evalText(s: { score: Score; source: "engine" | "derived" } | null): string | null {
  if (!s) return null;
  const text = formatEval(s.score);
  return s.source === "derived" ? `≈ ${text}` : text;
}

/** A side's clock at the shown ply: after its latest move, or the base time before its first. */
export function clockAt(v: ReviewView, color: Color, ply: number): number | null {
  for (let i = Math.min(ply, v.plies.length) - 1; i >= 0; i--) {
    const p = v.plies[i];
    if (p?.color === color && p.clockAfter !== null) return p.clockAfter;
  }
  return v.baseSeconds;
}

/** Time the mover spent on the shown move: the stored value, or its clock drop plus increment. */
export function spentAt(v: ReviewView, ply: number): { seconds: number } | null {
  const p = plyAt(v, ply);
  if (!p) return null;
  if (p.analysis?.timeSpent != null) return { seconds: p.analysis.timeSpent };
  const before = clockAt(v, p.color, ply - 1);
  if (before === null || p.clockAfter === null) return null;
  const seconds = before - p.clockAfter + (v.incrementSeconds ?? 0);
  return seconds >= 0 ? { seconds: Math.round(seconds * 10) / 10 } : null;
}

/** The clock source records tenths when any clock has a fraction. */
export function hasTenths(v: ReviewView): boolean {
  return v.plies.some((p) => p.clockAfter !== null && !Number.isInteger(p.clockAfter));
}
