import type { Annotation, Glyph, Square } from "@ci/ui/annotations";
import { formatEval, judgementGlyph, moveNumberText, type Score } from "@ci/ui/board";
import type { EvalGraphPoint } from "@ci/ui/eval-graph";
import type { ScoresheetMove } from "@ci/ui/review";
import { Chess } from "chess.js";
import type { ReviewView as ReviewGame } from "./view";

// Pure helpers for the review lab (Task 7). Lab only: Task 10 builds the product's reducer,
// annotations and commentary (ADR 0051), informed by the pick recorded in ADR 0054.

type Color = "white" | "black";

const plyAt = (g: ReviewGame, ply: number) => (ply > 0 ? g.plies[ply - 1] : undefined);

export function fenAt(g: ReviewGame, ply: number): string {
  return plyAt(g, ply)?.fenAfter ?? g.startFen;
}

export function lastMoveAt(g: ReviewGame, ply: number): { from: Square; to: Square } | undefined {
  const p = plyAt(g, ply);
  return p ? squares(p.uci) : undefined;
}

function squares(uci: string): { from: Square; to: Square } {
  return { from: uci.slice(0, 2) as Square, to: uci.slice(2, 4) as Square };
}

/** The scoresheet rows. Keep the result stable (memoise it on the game). */
export function sheetMoves(g: ReviewGame): ScoresheetMove[] {
  return g.plies.map((p) => ({
    ply: p.ply,
    color: p.color,
    san: p.san,
    judgement: p.analysis?.judgement ?? null,
    best: p.analysis?.classification === "best",
    mine: p.color === g.playerColor,
  }));
}

export function graphPoints(g: ReviewGame): EvalGraphPoint[] {
  if (!g.analysed) return [];
  return g.plies.flatMap((p) =>
    p.analysis
      ? [{ ply: p.ply, winWhite: p.analysis.winWhite, san: p.san, judgement: p.analysis.judgement }]
      : [],
  );
}

export function scoreAt(
  g: ReviewGame,
  ply: number,
): { score: Score; source: "engine" | "derived" } | null {
  if (!g.analysed) return null;
  if (ply === 0) return g.startScore;
  const a = plyAt(g, ply)?.analysis;
  return a ? { score: a.score, source: a.scoreSource } : null;
}

/** A side's clock at the shown ply: after its latest move, or the base time before its first. */
export function clockAt(g: ReviewGame, color: Color, ply: number): number | null {
  for (let i = Math.min(ply, g.plies.length) - 1; i >= 0; i--) {
    const p = g.plies[i];
    if (p?.color === color && p.clockAfter !== null) return p.clockAfter;
  }
  return g.baseSeconds;
}

/** Time the mover spent on the shown move: the stored value, or its clock drop plus increment. */
export function spentAt(g: ReviewGame, ply: number): { seconds: number } | null {
  const p = plyAt(g, ply);
  if (!p) return null;
  if (p.analysis?.timeSpent != null) return { seconds: p.analysis.timeSpent };
  const before = clockAt(g, p.color, ply - 1);
  if (before === null || p.clockAfter === null) return null;
  const seconds = before - p.clockAfter + (g.incrementSeconds ?? 0);
  return seconds >= 0 ? { seconds: Math.round(seconds * 10) / 10 } : null;
}

/** The clock source records tenths when any clock has a fraction. */
export function hasTenths(g: ReviewGame): boolean {
  return g.plies.some((p) => p.clockAfter !== null && !Number.isInteger(p.clockAfter));
}

/**
 * The marks at the shown ply (R8): a pen ring on the destination (the player's mistakes and
 * blunders), the pen arrow of the played move (the player's blunders), the `good` arrow of the
 * engine's move when it differs, and the glyph badge (pen for the player, pencil for the opponent).
 */
export function annotationsAt(g: ReviewGame, ply: number): Annotation[] {
  const p = plyAt(g, ply);
  const a = p?.analysis;
  if (!g.analysed || !p || !a) return [];
  const mine = p.color === g.playerColor;
  const played = squares(p.uci);
  const out: Annotation[] = [];
  if (mine && (a.judgement === "mistake" || a.judgement === "blunder"))
    out.push({ kind: "circle", square: played.to, tone: "pen" });
  if (mine && a.judgement === "blunder") out.push({ kind: "arrow", ...played, tone: "pen" });
  if (a.judgement && a.bestUci && a.bestUci !== p.uci)
    out.push({ kind: "arrow", ...squares(a.bestUci), tone: "good" });
  if (a.judgement)
    out.push({
      kind: "mark",
      square: played.to,
      glyph: judgementGlyph[a.judgement] as Glyph,
      tone: mine ? "pen" : "pencil",
    });
  return out;
}

const ARTICLE = { inaccuracy: "an inaccuracy", mistake: "a mistake", blunder: "a blunder" };

/** `31. Rxe6??`: the move number, the move and its glyph. */
export function moveText(g: ReviewGame, ply: number): string | null {
  const p = plyAt(g, ply);
  if (!p) return null;
  const glyph = p.analysis?.judgement ? judgementGlyph[p.analysis.judgement] : "";
  return `${moveNumberText(Math.ceil(ply / 2), p.color)} ${p.san}${glyph}`;
}

function sanOf(fen: string, uci: string): string | null {
  try {
    const m = new Chess(fen).move({
      from: uci.slice(0, 2),
      to: uci.slice(2, 4),
      ...(uci.length > 4 ? { promotion: uci.slice(4, 5) } : {}),
    });
    return m.san;
  } catch {
    return null;
  }
}

/**
 * The coach's line for the shown move: "31. Rxe6?? is a blunder. Win chance 64% to 22%. The
 * engine preferred Bxe6." For the opponent's mistakes it names them and gives the player's win
 * chance. A best move "matches the engine's choice". Anything else is null.
 */
export function commentaryAt(g: ReviewGame, ply: number): string | null {
  const p = plyAt(g, ply);
  const a = p?.analysis;
  const text = moveText(g, ply);
  if (!g.analysed || !p || !a || !text) return null;
  if (!a.judgement)
    return a.classification === "best" ? `${text} matches the engine's choice.` : null;
  if (a.deltaW === undefined) return null;
  const mine = p.color === g.playerColor;
  // The mover's win chance after the move, and before it (the drop is `deltaW`).
  const after = p.color === "white" ? a.winWhite : 100 - a.winWhite;
  const before = after + a.deltaW;
  const pct = (w: number) => `${Math.round(w)}%`;
  const best = a.bestUci && a.bestUci !== p.uci ? sanOf(fenAt(g, ply - 1), a.bestUci) : null;
  const preferred = best ? ` The engine preferred ${best}.` : "";
  if (mine)
    return `${text} is ${ARTICLE[a.judgement]}. Win chance ${pct(before)} to ${pct(after)}.${preferred}`;
  const opponent = g[p.color].name;
  return `${text} is ${ARTICLE[a.judgement]} by ${opponent}. Your win chance ${pct(100 - before)} to ${pct(100 - after)}.${preferred}`;
}

const TIME_CLASS: Record<string, string> = {
  bullet: "Bullet",
  blitz: "Blitz",
  rapid: "Rapid",
  classical: "Classical",
  correspondence: "Daily",
  daily: "Daily",
};

const DAY = new Intl.DateTimeFormat("en-GB", {
  day: "numeric",
  month: "short",
  year: "numeric",
  timeZone: "UTC",
});

/** "12 Nov 2026", in UTC (ruling R6). */
export function dayText(d: Date): string {
  return DAY.format(d);
}

/** "Rapid 10+0, rated, 12 Nov 2026". */
export function metaLine(g: ReviewGame): string {
  const tc = TIME_CLASS[g.timeClass] ?? g.timeClass;
  const control =
    g.baseSeconds !== null && g.baseSeconds % 60 === 0
      ? ` ${g.baseSeconds / 60}+${g.incrementSeconds ?? 0}`
      : "";
  return `${tc}${control}, ${g.rated ? "rated" : "casual"}, ${dayText(g.playedAt)}`;
}

export function resultLine(g: ReviewGame): string {
  return g.playerResult === "win" ? "You won" : g.playerResult === "loss" ? "You lost" : "Draw";
}

export function accuracyLine(g: ReviewGame): string | null {
  const mine = g.accuracy[g.playerColor];
  const theirs = g.accuracy[g.playerColor === "white" ? "black" : "white"];
  const parts = [
    mine === null ? null : `Your accuracy ${mine.toFixed(1)}%`,
    theirs === null ? null : `opponent ${theirs.toFixed(1)}%`,
  ].filter((s): s is string => s !== null);
  if (parts.length === 0) return null;
  const [first, ...rest] = parts;
  return [first && first[0]?.toUpperCase() + first.slice(1), ...rest].join(", ");
}

/** The eval readout: `+0.6`, `#3`, or `≈ −1.2` for a score derived from the stored win%. */
export function evalText(s: { score: Score; source: "engine" | "derived" } | null): string | null {
  if (!s) return null;
  const text = formatEval(s.score);
  return s.source === "derived" ? `≈ ${text}` : text;
}
