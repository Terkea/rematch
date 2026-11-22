import type { ReviewGame } from "../../../../../server/games/review";

// The review page's client boundary (M5a T10, T7 review I1): the server component hands the
// "use client" view only what it draws. No id, platform, source URL, opening, result, termination,
// accuracy, rating history or per-ply accuracy, phase or decided flag; the header is rendered on
// the server. The win% before and after a move is sent only for judged plies, where it is printed.

type Judgement = "inaccuracy" | "mistake" | "blunder";
type Score = { cp: number } | { mate: number };

/**
 * What a judgement rests on. The stored rule (`judge` in packages/chess-core/src/judgement.ts,
 * ported to Rust as is) is Lichess's: the mover's win% drop of 15, 10 or 5 points makes a blunder,
 * a mistake or an inaccuracy, EXCEPT where a forced mate changes hands. Losing a forced mate is a
 * blunder unless the move still leaves +7 or more, and walking into one is a blunder unless the
 * position was already −7 or worse. So "98% to 90%" (8 points) can be a blunder: the 98% was a
 * forced mate (the win% of a mate is the cp clamp, 97.54%). The page says so beside the number.
 */
export type JudgementBasis = "win" | "missedMate" | "allowedMate";

/** The win% of a forced mate for (high) and against (low) the mover: `winPercent(±1000 cp)`. */
const MATE_HIGH = 97.5447;
const MATE_LOW = 2.4553;
/** Stored drops are the difference of the same doubles; this only absorbs float noise. */
const EPS = 1e-9;

const RANK: Record<Judgement, number> = { inaccuracy: 1, mistake: 2, blunder: 3 };

function byDrop(drop: number): number {
  if (drop >= 15 - EPS) return 3;
  if (drop >= 10 - EPS) return 2;
  if (drop >= 5 - EPS) return 1;
  return 0;
}

/** `before` and `after` are the mover's win% (0 to 100). */
export function judgementBasis(j: Judgement, before: number, after: number): JudgementBasis {
  // The mate rules only ever judge more severely than the drop; a judgement the drop explains
  // rests on the drop.
  if (RANK[j] <= byDrop(before - after)) return "win";
  if (before >= MATE_HIGH) return "missedMate";
  if (after <= MATE_LOW) return "allowedMate";
  return "win";
}

export type ViewAnalysis = {
  /** White's win% after the ply. */
  winWhite: number;
  score: Score;
  scoreSource: "engine" | "derived";
  judgement: Judgement | null;
  /** The move played was the engine's choice. */
  best: boolean;
  bestUci: string | null;
  timeSpent: number | null;
  /** Judged plies only: the mover's win% before and after, and what the judgement rests on. */
  drop?: { before: number; after: number; basis: JudgementBasis };
};

export type ViewPly = {
  ply: number;
  color: "white" | "black";
  san: string;
  uci: string;
  fenAfter: string;
  clockAfter: number | null;
  analysis: ViewAnalysis | null;
};

export type ReviewStatus = "analysed" | "waiting" | "failed" | "skipped";

export type ReviewView = {
  /** "<White> v <Black>". */
  title: string;
  white: { name: string; rating: number | null };
  black: { name: string; rating: number | null };
  playerColor: "white" | "black";
  startFen: string;
  startScore: { score: Score; source: "engine" | "derived" } | null;
  status: ReviewStatus;
  hasClocks: boolean;
  baseSeconds: number | null;
  incrementSeconds: number | null;
  phases: { middlegameStartPly?: number; endgameStartPly?: number };
  /** The player's judged plies, worst first. */
  moments: number[];
  plies: ViewPly[];
};

function statusOf(g: ReviewGame): ReviewStatus {
  if (g.analysed) return "analysed";
  if (g.analysisStatus === "failed") return "failed";
  if (g.analysisStatus === "skipped") return "skipped";
  return "waiting";
}

export function toReviewView(g: ReviewGame): ReviewView {
  return {
    title: `${g.white.name} v ${g.black.name}`,
    white: { name: g.white.name, rating: g.white.rating },
    black: { name: g.black.name, rating: g.black.rating },
    playerColor: g.playerColor,
    startFen: g.startFen,
    startScore: g.analysed ? g.startScore : null,
    status: statusOf(g),
    hasClocks: g.hasClocks,
    baseSeconds: g.baseSeconds,
    incrementSeconds: g.incrementSeconds,
    phases: g.analysed ? { ...g.phases } : {},
    moments: g.analysed ? [...g.moments] : [],
    plies: g.plies.map((p) => {
      const a = g.analysed ? p.analysis : null;
      let analysis: ViewAnalysis | null = null;
      if (a) {
        analysis = {
          winWhite: a.winWhite,
          score: a.score,
          scoreSource: a.scoreSource,
          judgement: a.judgement,
          best: a.classification === "best",
          bestUci: a.bestUci,
          timeSpent: a.timeSpent,
        };
        if (a.judgement) {
          const after = p.color === "white" ? a.winWhite : 100 - a.winWhite;
          const before = after + a.deltaW;
          analysis.drop = { before, after, basis: judgementBasis(a.judgement, before, after) };
        }
      }
      return {
        ply: p.ply,
        color: p.color,
        san: p.san,
        uci: p.uci,
        fenAfter: p.fenAfter,
        clockAfter: p.clockAfter,
        analysis,
      };
    }),
  };
}
