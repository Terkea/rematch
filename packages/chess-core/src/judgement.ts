import { type EngineScore, scoreToCp, winningChances } from "./score.js";

export type Judgement = "inaccuracy" | "mistake" | "blunder";

function isMate(s: EngineScore): s is { mate: number } {
  return "mate" in s;
}

/** Judge a move. Both scores are from the mover's point of view. */
export function judge(before: EngineScore, after: EngineScore): Judgement | null {
  // Mate created against the mover.
  if (!isMate(before) && isMate(after) && after.mate < 0) {
    const cp = scoreToCp(before);
    if (cp < -999) return "inaccuracy";
    if (cp < -700) return "mistake";
    return "blunder";
  }
  // Mate lost by the mover.
  if (isMate(before) && before.mate > 0 && !(isMate(after) && after.mate > 0)) {
    const cp = scoreToCp(after);
    if (cp > 999) return "inaccuracy";
    if (cp > 700) return "mistake";
    return "blunder";
  }
  const drop = winningChances(before) - winningChances(after);
  if (drop >= 0.3) return "blunder";
  if (drop >= 0.2) return "mistake";
  if (drop >= 0.1) return "inaccuracy";
  return null;
}
