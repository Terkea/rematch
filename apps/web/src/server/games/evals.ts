import type { EngineScore } from "@ci/chess-core";

export type Score = EngineScore;

/** The constant of `winningChances` in `@ci/chess-core` (Lichess). */
const WIN_K = 0.00368208;
const CP_LIMIT = 1000;

/**
 * The centipawn score whose `winPercent` is `winWhite`: the exact inverse of `winningChances`,
 * rounded to an integer and clamped to ±1000 (the cap `winningChances` applies). Used only when
 * `position_evals` holds no row for a position; the caller marks the score `derived` (ADR 0049).
 */
export function scoreFromWin(winWhite: number): Score {
  const w = Math.min(100, Math.max(0, winWhite));
  if (w >= 100) return { cp: CP_LIMIT };
  if (w <= 0) return { cp: -CP_LIMIT };
  const cp = -Math.log(100 / w - 1) / WIN_K;
  // `+ 0` turns -0 into 0, so a level position reads as { cp: 0 }.
  return { cp: Math.max(-CP_LIMIT, Math.min(CP_LIMIT, Math.round(cp))) + 0 };
}

/** A `position_evals` row's score (White's point of view), or null when it carries none. */
export function storedScore(row: {
  scoreCp: number | null;
  scoreMate: number | null;
}): Score | null {
  if (row.scoreMate !== null) return { mate: row.scoreMate };
  if (row.scoreCp !== null) return { cp: row.scoreCp };
  return null;
}
