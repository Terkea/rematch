import { fenKey, type ParsedGame, parsePgn } from "@ci/chess-core";
import { type Db, games, moves, positionEvals } from "@ci/db";
import { and, asc, eq, inArray, sql } from "drizzle-orm";
import { type Score, scoreFromWin, storedScore } from "./evals";
import { readOnly } from "./read-only";
import { playerResult } from "./result";
import { scopedAccountIds } from "./scope";
import { sourceUrl } from "./source-url";
import type { AnalysisStatus, PlayerResult, TimeClass } from "./types";

export type { Score } from "./evals";

export type Classification = "best" | "inaccuracy" | "mistake" | "blunder" | null;

export type PlyAnalysis = {
  /** White's win% after the ply (0 to 100), whoever moved. */
  winWhite: number;
  /** White's point of view: from `position_evals`, or derived from `winWhite` when absent. */
  score: Score;
  scoreSource: "engine" | "derived";
  judgement: "inaccuracy" | "mistake" | "blunder" | null;
  classification: Classification;
  /** The engine's best move in the position before the ply. */
  bestUci: string | null;
  accuracy: number;
  /** The mover's win% drop, `max(0, winBefore - winAfter)`. */
  deltaW: number;
  phase: "opening" | "middlegame" | "endgame";
  timeSpent: number | null;
  decided: boolean;
};

export type ReviewPly = {
  ply: number;
  color: "white" | "black";
  san: string;
  uci: string;
  fenBefore: string;
  fenAfter: string;
  /** The mover's clock after the move, in seconds, from `[%clk]`. */
  clockAfter: number | null;
  analysis: PlyAnalysis | null;
};

export type ReviewGame = {
  id: string;
  platform: "lichess" | "chesscom";
  sourceUrl: string | null;
  playedAt: Date;
  timeClass: TimeClass;
  baseSeconds: number | null;
  incrementSeconds: number | null;
  rated: boolean;
  playerColor: "white" | "black";
  white: { name: string; rating: number | null };
  black: { name: string; rating: number | null };
  result: "1-0" | "0-1" | "1/2-1/2" | "*";
  playerResult: PlayerResult;
  termination: string | null;
  eco: string | null;
  openingName: string | null;
  analysisStatus: AnalysisStatus;
  /** Set only for a finished analysis. */
  accuracy: { white: number | null; black: number | null };
  startFen: string;
  startScore: { score: Score; source: "engine" | "derived" } | null;
  plies: ReviewPly[];
  analysed: boolean;
  hasClocks: boolean;
  phases: { middlegameStartPly?: number; endgameStartPly?: number };
  /** The player's judged plies, worst first by `deltaW`, ties by the lower ply. */
  moments: number[];
};

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const STANDARD_START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

function parseOrEmpty(pgn: string): ParsedGame {
  try {
    return parsePgn(pgn);
  } catch {
    // A variant or an unreadable PGN: the page shows the header and no moves, never a 500.
    return { headers: {}, startFen: STANDARD_START, plies: [], terminal: null };
  }
}

type MoveRow = {
  ply: number;
  color: "white" | "black";
  uci: string;
  bestUci: string | null;
  winBefore: number;
  winAfter: number;
  deltaW: number;
  accuracy: number;
  judgement: "inaccuracy" | "mistake" | "blunder" | null;
  phase: "opening" | "middlegame" | "endgame";
  timeSpent: number | null;
  decided: boolean;
};

/** White's point of view of a mover-POV win%. */
const toWhite = (color: "white" | "black", w: number) => (color === "white" ? w : 100 - w);

/**
 * The moves rows line up with the PGN: one row per ply, the same plies in order, the same moves.
 * Anything else (a partial or stale analysis) is shown as no analysis at all (ADR 0049).
 */
function inStep(parsed: ParsedGame, rows: MoveRow[]): boolean {
  if (rows.length !== parsed.plies.length) return false;
  return parsed.plies.every((p, i) => {
    const r = rows[i];
    return r !== undefined && r.ply === p.ply && r.uci === p.uci && r.color === p.color;
  });
}

/**
 * One game for the review page, scoped to the user's linked accounts and read in a READ ONLY
 * transaction. A malformed id, another user's game or an unknown game gives null (the page's
 * not-found), so no id is confirmed to exist. The plies always come from the PGN; analysis is
 * attached only when it is done and in step with the PGN (ADR 0049).
 */
export async function getGameReview(
  db: Db,
  userId: string,
  gameId: string,
): Promise<ReviewGame | null> {
  if (!UUID_RE.test(gameId)) return null;
  return readOnly(db, async (tx) => {
    const ids = await scopedAccountIds(tx, userId);
    if (ids.length === 0) return null;
    const [g] = await tx
      .select({
        id: games.id,
        platform: games.platform,
        pgn: games.pgn,
        playedAt: games.playedAt,
        timeClass: games.timeClass,
        baseSeconds: games.baseSeconds,
        incrementSeconds: games.incrementSeconds,
        rated: games.rated,
        playerColor: games.playerColor,
        whiteName: games.whiteName,
        blackName: games.blackName,
        whiteRating: games.whiteRating,
        blackRating: games.blackRating,
        result: games.result,
        termination: games.termination,
        eco: games.eco,
        openingName: games.openingName,
        analysisStatus: games.analysisStatus,
        analysisProtocol: games.analysisProtocol,
        whiteAccuracy: games.whiteAccuracy,
        blackAccuracy: games.blackAccuracy,
      })
      .from(games)
      .where(and(eq(games.id, gameId), inArray(games.accountId, ids)));
    if (!g) return null;

    const parsed = parseOrEmpty(g.pgn);
    const done = g.analysisStatus === "done";

    let rows: MoveRow[] = [];
    if (done) {
      rows = await tx
        .select({
          ply: moves.ply,
          color: moves.color,
          uci: moves.uci,
          bestUci: moves.bestUci,
          winBefore: moves.winBefore,
          winAfter: moves.winAfter,
          deltaW: moves.deltaW,
          accuracy: moves.accuracy,
          judgement: moves.judgement,
          phase: moves.phase,
          timeSpent: moves.timeSpent,
          decided: moves.decided,
        })
        .from(moves)
        .where(eq(moves.gameId, g.id))
        .orderBy(asc(moves.ply));
    }
    const analysed = done && inStep(parsed, rows);

    // Engine scores for the start and every position after a ply, in one query.
    const evals = new Map<string, Score>();
    if (analysed && g.analysisProtocol) {
      const keys = [
        ...new Set([parsed.startFen, ...parsed.plies.map((p) => p.fenAfter)].map(fenKey)),
      ];
      const found = await tx
        .select({
          fenKey: positionEvals.fenKey,
          scoreCp: positionEvals.scoreCp,
          scoreMate: positionEvals.scoreMate,
        })
        .from(positionEvals)
        .where(
          and(
            eq(positionEvals.protocol, g.analysisProtocol),
            sql`${positionEvals.fenKey} = any(${sql.param(keys)}::text[])`,
          ),
        );
      for (const r of found) {
        const s = storedScore(r);
        if (s) evals.set(r.fenKey, s);
      }
    }

    const last = parsed.plies.length - 1;
    const plies: ReviewPly[] = parsed.plies.map((p, i) => {
      const base = {
        ply: p.ply,
        color: p.color,
        san: p.san,
        uci: p.uci,
        fenBefore: p.fenBefore,
        fenAfter: p.fenAfter,
        clockAfter: p.clockSeconds,
      };
      const r = rows[i];
      if (!analysed || !r) return { ...base, analysis: null };
      const winWhite = toWhite(p.color, r.winAfter);
      const mated = parsed.terminal === "checkmate" && i === last;
      const stored = evals.get(fenKey(p.fenAfter));
      const score: Score = mated ? { mate: 0 } : (stored ?? scoreFromWin(winWhite));
      const classification: Classification =
        r.judgement ?? (r.bestUci !== null && r.bestUci === p.uci ? "best" : null);
      return {
        ...base,
        analysis: {
          winWhite,
          score,
          scoreSource: mated || stored ? "engine" : "derived",
          judgement: r.judgement,
          classification,
          bestUci: r.bestUci,
          accuracy: r.accuracy,
          deltaW: r.deltaW,
          phase: r.phase,
          timeSpent: r.timeSpent,
          decided: r.decided,
        },
      };
    });

    let startScore: ReviewGame["startScore"] = null;
    if (analysed) {
      const stored = evals.get(fenKey(parsed.startFen));
      const first = rows[0];
      if (stored) startScore = { score: stored, source: "engine" };
      else if (first)
        startScore = {
          score: scoreFromWin(toWhite(first.color, first.winBefore)),
          source: "derived",
        };
    }

    const phases: ReviewGame["phases"] = {};
    if (analysed) {
      const mid = rows.find((r) => r.phase === "middlegame")?.ply;
      const end = rows.find((r) => r.phase === "endgame")?.ply;
      if (mid !== undefined) phases.middlegameStartPly = mid;
      if (end !== undefined) phases.endgameStartPly = end;
    }

    const moments = analysed
      ? rows
          .filter((r) => r.color === g.playerColor && r.judgement !== null)
          .sort((a, b) => b.deltaW - a.deltaW || a.ply - b.ply)
          .map((r) => r.ply)
      : [];

    return {
      id: g.id,
      platform: g.platform,
      sourceUrl: sourceUrl(g.platform, parsed.headers),
      playedAt: g.playedAt,
      timeClass: g.timeClass as TimeClass,
      baseSeconds: g.baseSeconds,
      incrementSeconds: g.incrementSeconds,
      rated: g.rated,
      playerColor: g.playerColor,
      white: { name: g.whiteName, rating: g.whiteRating },
      black: { name: g.blackName, rating: g.blackRating },
      result: g.result,
      playerResult: playerResult(g.result, g.playerColor),
      termination: g.termination,
      eco: g.eco,
      openingName: g.openingName,
      analysisStatus: g.analysisStatus,
      accuracy: done
        ? { white: g.whiteAccuracy, black: g.blackAccuracy }
        : { white: null, black: null },
      startFen: parsed.startFen,
      startScore,
      plies,
      analysed,
      hasClocks: plies.some((p) => p.clockAfter !== null),
      phases,
      moments,
    };
  });
}
