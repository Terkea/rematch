import { describe, expect, it } from "vitest";
import { judgementBasis, toReviewView } from "../../src/app/(app)/app/games/[gameId]/view-model";
import { game } from "../helpers/review-game";

// Task 10: the client gets a narrow view model, and every printed judgement says what it rests on.

describe("judgementBasis", () => {
  // The stored rule (packages/chess-core judgement.ts): a mover's win% drop of 15, 10 or 5 points
  // is a blunder, mistake or inaccuracy, except where a forced mate is lost or walked into.
  it("is the win% drop when the judgement matches the thresholds", () => {
    expect(judgementBasis("blunder", 64, 22)).toBe("win");
    expect(judgementBasis("mistake", 95, 84)).toBe("win");
    expect(judgementBasis("inaccuracy", 91, 85)).toBe("win");
    expect(judgementBasis("blunder", 50, 35)).toBe("win");
  });

  it("is a lost forced mate when the move started at the mate clamp", () => {
    // 46…h4?? 98% to 90%: an 8-point drop judged a blunder, because the mate was lost.
    expect(judgementBasis("blunder", 97.54474363414323, 90.1)).toBe("missedMate");
    expect(judgementBasis("inaccuracy", 97.54474363414323, 97.54474363414323)).toBe("missedMate");
  });

  it("is a forced mate allowed when the move ended at the mate clamp", () => {
    expect(judgementBasis("blunder", 9, 2.4552563658567763)).toBe("allowedMate");
    expect(judgementBasis("mistake", 7, 2.4552563658567763)).toBe("allowedMate");
  });

  it("never calls a threshold judgement a mate rule, even at the clamp", () => {
    expect(judgementBasis("blunder", 97.54474363414323, 20)).toBe("win");
    expect(judgementBasis("blunder", 40, 2.4552563658567763)).toBe("win");
  });
});

describe("toReviewView", () => {
  const full = {
    ...game(["e4", "e5", "Qh5", "Nc6", "Bc4", "Nf6", "Qxf7#"], {
      6: {
        winWhite: 97.54474363414323,
        judgement: "blunder",
        classification: "blunder",
        deltaW: 6,
        bestUci: "g7g6",
      },
      1: { winWhite: 52, classification: "best", bestUci: "e2e4" },
    }),
    sourceUrl: "https://www.chess.com/game/live/1",
    eco: "C20",
    openingName: "King's Pawn Game",
    termination: "Whitey won by checkmate",
  };

  it("keeps only what the client draws", () => {
    const v = toReviewView({ ...full, playerColor: "black", moments: [6] });
    expect(Object.keys(v).sort()).toEqual(
      [
        "title",
        "white",
        "black",
        "playerColor",
        "startFen",
        "startScore",
        "status",
        "hasClocks",
        "baseSeconds",
        "incrementSeconds",
        "phases",
        "moments",
        "plies",
      ].sort(),
    );
    const json = JSON.stringify(v);
    for (const leak of ["chess.com/game", "C20", "King's Pawn", "won by checkmate", "00000000"])
      expect(json).not.toContain(leak);
    expect(Object.keys(v.plies[0] ?? {}).sort()).toEqual(
      ["ply", "color", "san", "uci", "fenAfter", "clockAfter", "analysis"].sort(),
    );
  });

  it("prints the win% drop only on judged plies, with its basis", () => {
    const v = toReviewView({ ...full, playerColor: "black", moments: [6] });
    expect(v.plies[0]?.analysis?.drop).toBeUndefined();
    expect(v.plies[0]?.analysis?.best).toBe(true);
    const drop = v.plies[5]?.analysis?.drop;
    // Black's win after is 100 − 97.54…; before is that plus the stored drop.
    expect(drop?.after).toBeCloseTo(2.4552563658567763, 9);
    expect(drop?.before).toBeCloseTo(8.4552563658567763, 9);
    expect(drop?.basis).toBe("allowedMate");
  });

  it("names the analysis status for a game without analysis", () => {
    const pending = { ...full, analysed: false, analysisStatus: "pending" as const };
    expect(toReviewView(pending).status).toBe("waiting");
    expect(toReviewView({ ...pending, analysisStatus: "analysing" }).status).toBe("waiting");
    expect(toReviewView({ ...pending, analysisStatus: "failed" }).status).toBe("failed");
    expect(toReviewView({ ...pending, analysisStatus: "skipped" }).status).toBe("skipped");
    // Done but out of step with the PGN (ADR 0049): shown as waiting, never as analysed.
    expect(toReviewView({ ...pending, analysisStatus: "done" }).status).toBe("waiting");
    expect(toReviewView(full).status).toBe("analysed");
  });

  it("drops every ply's analysis when the game is not analysed", () => {
    const v = toReviewView({ ...full, analysed: false, analysisStatus: "failed" });
    expect(v.plies.every((p) => p.analysis === null)).toBe(true);
    expect(v.startScore).toBeNull();
  });
});
