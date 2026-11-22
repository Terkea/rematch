import { type Judgement, judge } from "./judgement.js";
import type { Color, ParsedGame } from "./pgn.js";
import { detectPhases, type Phase } from "./phase.js";
import { type EngineScore, flipScore, moveAccuracy, winPercent } from "./score.js";

export interface PositionEval {
  /** Score from white's point of view. */
  score: EngineScore;
  bestUci: string | null;
  pv: string[];
}

export interface MoveRecord {
  ply: number;
  color: Color;
  san: string;
  uci: string;
  fenBefore: string;
  bestUci: string | null;
  /** Mover's win% before and after the move. */
  winBefore: number;
  winAfter: number;
  /** max(0, winBefore - winAfter) */
  deltaW: number;
  accuracy: number;
  judgement: Judgement | null;
  phase: Phase;
  clockBefore: number | null;
  timeSpent: number | null;
  /** The game was already decided: |winBefore - 50| >= 40. */
  decided: boolean;
}

const DECIDED_THRESHOLD = 40;

/**
 * `evals[i]` is the evaluation of the position before ply i + 1, so it has
 * `plies.length + 1` entries; the last one is the final position.
 */
export function buildMoveRecords(
  game: ParsedGame,
  evals: PositionEval[],
  clock: { baseSeconds: number; incrementSeconds: number } | null,
): MoveRecord[] {
  const { plies } = game;
  if (evals.length !== plies.length + 1) {
    throw new Error(`Expected ${plies.length + 1} evals, got ${evals.length}`);
  }
  const phases = detectPhases(plies.map((p) => p.fenBefore));
  // Without any clock comment in the PGN there is no clock to report, even when a time control
  // is known (R1 Task 24 parity: the Rust port's rule, so a clockless game reads as unknown).
  const pgnHasClocks = plies.some((p) => p.clockSeconds !== null);
  const base = clock && pgnHasClocks ? clock.baseSeconds : null;
  const previousClock: Record<Color, number | null> = { white: base, black: base };

  return plies.map((ply, i): MoveRecord => {
    const evalBefore = evals[i] as PositionEval;
    const evalAfter = evals[i + 1] as PositionEval;
    const povScore = (s: EngineScore) => (ply.color === "white" ? s : flipScore(s));
    const before = povScore(evalBefore.score);
    // A delivered checkmate has no meaningful engine score: the mover has won.
    const deliversMate = game.terminal === "checkmate" && i === plies.length - 1;
    const after: EngineScore = deliversMate ? { mate: 1 } : povScore(evalAfter.score);

    const winBefore = winPercent(before);
    const winAfter = winPercent(after);

    const clockBefore = previousClock[ply.color];
    const timeSpent =
      clock !== null && clockBefore !== null && ply.clockSeconds !== null
        ? Math.max(0, clockBefore - ply.clockSeconds + clock.incrementSeconds)
        : null;
    previousClock[ply.color] = clock === null ? null : ply.clockSeconds;

    return {
      ply: ply.ply,
      color: ply.color,
      san: ply.san,
      uci: ply.uci,
      fenBefore: ply.fenBefore,
      bestUci: evalBefore.bestUci,
      winBefore,
      winAfter,
      deltaW: Math.max(0, winBefore - winAfter),
      accuracy: moveAccuracy(winBefore, winAfter),
      judgement: judge(before, after),
      phase: phases[i] as Phase,
      clockBefore,
      timeSpent,
      decided: Math.abs(winBefore - 50) >= DECIDED_THRESHOLD,
    };
  });
}
