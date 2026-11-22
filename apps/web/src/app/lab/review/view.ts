import type { PlyAnalysis, ReviewGame, ReviewPly } from "../../../server/games/review";

// The review lab's client boundary (Task 7 fix round 1): the pages hand their "use client" views
// only what the variants render, never the raw ReviewGame. Dropped: id, platform, sourceUrl, eco,
// openingName, result, analysisStatus and each ply's fenBefore (derived from the previous ply),
// accuracy, phase and decided. The win% drop (`deltaW`) goes only to the variants that print it.

export type ViewAnalysis = Pick<
  PlyAnalysis,
  "winWhite" | "score" | "scoreSource" | "judgement" | "classification" | "bestUci" | "timeSpent"
> & { deltaW?: number };

export type ViewPly = Pick<
  ReviewPly,
  "ply" | "color" | "san" | "uci" | "fenAfter" | "clockAfter"
> & { analysis: ViewAnalysis | null };

export type ReviewView = Pick<
  ReviewGame,
  | "white"
  | "black"
  | "playedAt"
  | "timeClass"
  | "baseSeconds"
  | "incrementSeconds"
  | "rated"
  | "playerColor"
  | "playerResult"
  | "termination"
  | "accuracy"
  | "startFen"
  | "startScore"
  | "analysed"
  | "hasClocks"
  | "phases"
  | "moments"
> & { plies: ViewPly[] };

/** `drops`: include each move's win% drop, for a commentary line or a mistake list. */
export function toReviewView(g: ReviewGame, opts: { drops: boolean }): ReviewView {
  return {
    white: { name: g.white.name, rating: g.white.rating },
    black: { name: g.black.name, rating: g.black.rating },
    playedAt: g.playedAt,
    timeClass: g.timeClass,
    baseSeconds: g.baseSeconds,
    incrementSeconds: g.incrementSeconds,
    rated: g.rated,
    playerColor: g.playerColor,
    playerResult: g.playerResult,
    termination: g.termination,
    accuracy: { white: g.accuracy.white, black: g.accuracy.black },
    startFen: g.startFen,
    startScore: g.startScore,
    analysed: g.analysed,
    hasClocks: g.hasClocks,
    phases: { ...g.phases },
    moments: [...g.moments],
    plies: g.plies.map((p) => ({
      ply: p.ply,
      color: p.color,
      san: p.san,
      uci: p.uci,
      fenAfter: p.fenAfter,
      clockAfter: p.clockAfter,
      analysis: p.analysis
        ? {
            winWhite: p.analysis.winWhite,
            score: p.analysis.score,
            scoreSource: p.analysis.scoreSource,
            judgement: p.analysis.judgement,
            classification: p.analysis.classification,
            bestUci: p.analysis.bestUci,
            timeSpent: p.analysis.timeSpent,
            ...(opts.drops ? { deltaW: p.analysis.deltaW } : {}),
          }
        : null,
    })),
  };
}
