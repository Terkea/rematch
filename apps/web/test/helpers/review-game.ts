import { Chess } from "chess.js";
import type { PlyAnalysis, ReviewGame, ReviewPly } from "../../src/server/games/review";

// A hand-built ReviewGame for the review lab's tests (Task 7). Synthetic data, tests only (B15).

type Mark = Partial<PlyAnalysis> & { winWhite: number };

/** A ReviewGame from SAN, with the given analysis per ply (1-based) and clocks after each move. */
export function game(
  sans: string[],
  marks: Record<number, Mark> = {},
  opts: { playerColor?: "white" | "black"; clocks?: (number | null)[]; base?: number | null } = {},
): ReviewGame {
  const chess = new Chess();
  let win = 50;
  const plies: ReviewPly[] = sans.map((san, i) => {
    const m = chess.move(san);
    const mark = marks[i + 1];
    win = mark?.winWhite ?? win;
    const analysis: PlyAnalysis = {
      score: { cp: 0 },
      scoreSource: "derived",
      judgement: null,
      classification: null,
      bestUci: null,
      accuracy: 90,
      deltaW: 0,
      phase: "opening",
      timeSpent: null,
      decided: false,
      ...mark,
      winWhite: win,
    };
    return {
      ply: i + 1,
      color: i % 2 === 0 ? "white" : "black",
      san: m.san,
      uci: `${m.from}${m.to}${m.promotion ?? ""}`,
      fenBefore: m.before,
      fenAfter: m.after,
      clockAfter: opts.clocks?.[i] ?? null,
      analysis,
    };
  });
  return {
    id: "00000000-0000-4000-8000-000000000001",
    platform: "chesscom",
    sourceUrl: null,
    playedAt: new Date("2026-11-07T10:00:00Z"),
    timeClass: "rapid",
    baseSeconds: opts.base === undefined ? 600 : opts.base,
    incrementSeconds: 0,
    rated: true,
    playerColor: opts.playerColor ?? "white",
    white: { name: "Whitey", rating: 1500 },
    black: { name: "Blacky", rating: 1510 },
    result: "0-1",
    playerResult: "loss",
    termination: null,
    eco: null,
    openingName: null,
    analysisStatus: "done",
    accuracy: { white: 70, black: 80 },
    startFen: new Chess().fen(),
    startScore: { score: { cp: 20 }, source: "derived" },
    plies,
    analysed: true,
    hasClocks: (opts.clocks ?? []).some((c) => c !== null),
    phases: {},
    moments: [],
  };
}
