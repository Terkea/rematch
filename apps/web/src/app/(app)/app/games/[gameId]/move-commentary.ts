import { judgementGlyph, moveNumberText } from "@ci/ui/board";
import { Chess } from "chess.js";
import type { JudgementBasis, ReviewView } from "./view-model";

// The words beside the board: the shown move, its verdict sentence, and the "Where it went wrong"
// ledger. A judgement that rests on a forced mate says so, so the mark never contradicts the
// win chances printed beside it (M5a T10; see JudgementBasis in view-model.ts).

type Judgement = "inaccuracy" | "mistake" | "blunder";

const ARTICLE: Record<Judgement, string> = {
  inaccuracy: "an inaccuracy",
  mistake: "a mistake",
  blunder: "a blunder",
};

/** The reason a mate rule gives, after "is a blunder". */
const BASIS_CLAUSE: Record<JudgementBasis, string> = {
  win: "",
  missedMate: ": it let a forced mate go",
  allowedMate: ": it allows a forced mate",
};

/** The ledger's note for a mate-rule judgement. */
export const BASIS_NOTE: Record<JudgementBasis, string | null> = {
  win: null,
  missedMate: "marked for losing a forced mate, not for the drop",
  allowedMate: "marked for allowing a forced mate, not for the drop",
};

const pct = (w: number) => `${Math.round(w)}%`;

export function fenBefore(v: ReviewView, ply: number): string {
  return ply > 1 ? (v.plies[ply - 2]?.fenAfter ?? v.startFen) : v.startFen;
}

/** `31. Rxe6??`: the move number, the move and its glyph; null at the start. */
export function moveText(v: ReviewView, ply: number): string | null {
  const p = ply > 0 ? v.plies[ply - 1] : undefined;
  if (!p) return null;
  const j = p.analysis?.judgement;
  return `${moveNumberText(Math.ceil(ply / 2), p.color)} ${p.san}${j ? judgementGlyph[j] : ""}`;
}

/** A UCI move as SAN in `fen`, or null when it isn't legal there. */
export function sanOf(fen: string, uci: string): string | null {
  try {
    return new Chess(fen).move({
      from: uci.slice(0, 2),
      to: uci.slice(2, 4),
      ...(uci.length > 4 ? { promotion: uci.slice(4, 5) } : {}),
    }).san;
  } catch {
    return null;
  }
}

/**
 * "31. Rxe6?? is a blunder. Win chance 64% to 22%. The engine preferred Bxe6." For the opponent's
 * judged moves it names them and gives the player's win chance. A best move "matches the engine's
 * choice". Anything else, and the start, is null.
 */
export function moveCommentary(v: ReviewView, ply: number): string | null {
  const p = ply > 0 ? v.plies[ply - 1] : undefined;
  const a = p?.analysis;
  const text = moveText(v, ply);
  if (!p || !a || !text) return null;
  if (!a.judgement || !a.drop) return a.best ? `${text} matches the engine's choice.` : null;
  const best = a.bestUci && a.bestUci !== p.uci ? sanOf(fenBefore(v, ply), a.bestUci) : null;
  const preferred = best ? ` The engine preferred ${best}.` : "";
  const verdict = `${text} is ${ARTICLE[a.judgement]}`;
  const { before, after, basis } = a.drop;
  if (p.color === v.playerColor)
    return `${verdict}${BASIS_CLAUSE[basis]}. Win chance ${pct(before)} to ${pct(after)}.${preferred}`;
  const by = ` by ${v[p.color].name}`;
  return `${verdict}${by}${BASIS_CLAUSE[basis]}. Your win chance ${pct(100 - before)} to ${pct(100 - after)}.${preferred}`;
}

/** The engine's better move for a judged ply, as SAN: the green arrow on the board. */
export function engineAlternative(v: ReviewView, ply: number): string | null {
  const p = ply > 0 ? v.plies[ply - 1] : undefined;
  const a = p?.analysis;
  if (!p || !a?.judgement || !a.bestUci || a.bestUci === p.uci) return null;
  return sanOf(fenBefore(v, ply), a.bestUci);
}

export type LedgerRow = {
  ply: number;
  text: string;
  judgement: Judgement;
  /** The player's win%, rounded as printed. */
  before: number;
  after: number;
  basis: JudgementBasis;
};

/** "Where it went wrong": the player's judged moves in game order. */
export function ledgerRows(v: ReviewView): LedgerRow[] {
  return [...v.moments]
    .sort((x, y) => x - y)
    .flatMap((ply) => {
      const a = v.plies[ply - 1]?.analysis;
      const text = moveText(v, ply);
      if (!a?.judgement || !a.drop || !text) return [];
      return [
        {
          ply,
          text,
          judgement: a.judgement,
          before: Math.round(a.drop.before),
          after: Math.round(a.drop.after),
          basis: a.drop.basis,
        },
      ];
    });
}
