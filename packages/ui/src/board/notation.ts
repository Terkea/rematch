// Notation formatting (DESIGN.md, "Type": notation). Evaluations use the real minus sign, U+2212.

export type Score = { cp: number } | { mate: number };
export type NotationJudgement = "inaccuracy" | "mistake" | "blunder";

export interface NotationMove {
  moveNumber: number;
  color: "white" | "black";
  san: string;
  judgement?: NotationJudgement | null;
  score?: Score;
}

const MINUS = "−";

export const judgementGlyph: Readonly<Record<NotationJudgement, "?!" | "?" | "??">> = {
  inaccuracy: "?!",
  mistake: "?",
  blunder: "??",
};

/** Pawns to one decimal (`Math.round(|cp| / 10) / 10`), signed, with `0.0` for a level position. */
export function formatEval(score: Score): string {
  if ("mate" in score) return score.mate < 0 ? `#${MINUS}${-score.mate}` : `#${score.mate}`;
  const pawns = Math.round(Math.abs(score.cp) / 10) / 10;
  if (pawns === 0) return "0.0";
  return `${score.cp < 0 ? MINUS : "+"}${pawns.toFixed(1)}`;
}

/** The move number and dot: `31.` for White, `31…` for Black. */
export function moveNumberText(moveNumber: number, color: "white" | "black"): string {
  return color === "white" ? `${moveNumber}.` : `${moveNumber}…`;
}

/** `31. Rxe6?? −4.2`, `31… Kg7`, `12. Nf5?!`: the glyph kerned to the move, the eval after a space. */
export function formatNotation(m: NotationMove): string {
  const glyph = m.judgement ? judgementGlyph[m.judgement] : "";
  const move = `${moveNumberText(m.moveNumber, m.color)} ${m.san}${glyph}`;
  return m.score ? `${move} ${formatEval(m.score)}` : move;
}

/** The eval bar's label (`+5.7`, `M2`): it sits at the winning side's end, so mates drop the sign. */
export function evalBarLabel(score: Score): string {
  if ("mate" in score) return score.mate === 0 ? "#" : `M${Math.abs(score.mate)}`;
  return formatEval(score);
}
