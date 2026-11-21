/** Engine score from one side's point of view. Positive mate means that side mates. */
export type EngineScore = { cp: number } | { mate: number };

const WIN_K = 0.00368208;
const CP_LIMIT = 1000;

export function scoreToCp(s: EngineScore): number {
  if ("mate" in s) return s.mate > 0 ? CP_LIMIT : -CP_LIMIT;
  return Math.max(-CP_LIMIT, Math.min(CP_LIMIT, s.cp));
}

export function flipScore(s: EngineScore): EngineScore {
  return "mate" in s ? { mate: -s.mate } : { cp: -s.cp };
}

/** Lichess winning chances in [-1, 1]. */
export function winningChances(s: EngineScore): number {
  return 2 / (1 + Math.exp(-WIN_K * scoreToCp(s))) - 1;
}

/** Lichess win percentage in [0, 100]. */
export function winPercent(s: EngineScore): number {
  return 50 + 50 * winningChances(s);
}

/** Per-move accuracy from the mover's win% before and after the move. */
export function moveAccuracy(winBefore: number, winAfter: number): number {
  if (winAfter >= winBefore) return 100;
  const raw =
    103.1668100711649 * Math.exp(-0.04354415386753951 * (winBefore - winAfter)) -
    3.166924740191411 +
    1;
  return Math.max(0, Math.min(100, raw));
}
