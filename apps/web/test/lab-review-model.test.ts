import { Chess } from "chess.js";
import { describe, expect, it } from "vitest";
import {
  accuracyLine,
  annotationsAt,
  clockAt,
  commentaryAt,
  evalText,
  fenAt,
  graphPoints,
  hasTenths,
  lastMoveAt,
  metaLine,
  resultLine,
  scoreAt,
  sheetMoves,
  spentAt,
} from "../src/app/lab/review/model.js";
import { game } from "./helpers/review-game";

// Pure helpers of the review lab (Task 7), on small hand-built ReviewGames so every number is
// known. Synthetic data is allowed in tests only (B15).

const START = new Chess().fen();

describe("fenAt and lastMoveAt", () => {
  const g = game(["e4", "e5", "Nf3"]);
  it("is the start position at ply 0 and the position after the ply otherwise", () => {
    expect(fenAt(g, 0)).toBe(START);
    expect(fenAt(g, 3)).toBe(g.plies[2]?.fenAfter);
  });
  it("gives the shown move's squares, and none at the start", () => {
    expect(lastMoveAt(g, 0)).toBeUndefined();
    expect(lastMoveAt(g, 3)).toEqual({ from: "g1", to: "f3" });
  });
});

describe("sheetMoves", () => {
  it("marks best moves from the classification and the player's own moves", () => {
    const g = game(
      ["e4", "e5", "Nf3"],
      {
        1: { winWhite: 52, classification: "best" },
        2: { winWhite: 60, judgement: "mistake", classification: "mistake" },
      },
      { playerColor: "black" },
    );
    expect(sheetMoves(g)).toEqual([
      { ply: 1, color: "white", san: "e4", judgement: null, best: true, mine: false },
      { ply: 2, color: "black", san: "e5", judgement: "mistake", best: false, mine: true },
      { ply: 3, color: "white", san: "Nf3", judgement: null, best: false, mine: false },
    ]);
  });
  it("marks nothing for a game without analysis", () => {
    const g = { ...game(["e4"], { 1: { winWhite: 50, classification: "best" } }) };
    const bare = { ...g, analysed: false, plies: g.plies.map((p) => ({ ...p, analysis: null })) };
    expect(sheetMoves(bare)[0]).toMatchObject({ best: false, judgement: null });
  });
});

describe("graphPoints", () => {
  it("is one point per analysed ply, in White's point of view", () => {
    const g = game(["e4", "e5"], {
      1: { winWhite: 55 },
      2: { winWhite: 30, judgement: "blunder" },
    });
    expect(graphPoints(g)).toEqual([
      { ply: 1, winWhite: 55, san: "e4", judgement: null },
      { ply: 2, winWhite: 30, san: "e5", judgement: "blunder" },
    ]);
  });
  it("is empty without analysis", () => {
    const g = game(["e4"]);
    expect(graphPoints({ ...g, analysed: false })).toEqual([]);
  });
});

describe("scoreAt", () => {
  const g = game(["e4", "e5"], { 1: { winWhite: 55, score: { cp: 41 }, scoreSource: "engine" } });
  it("is the start score at ply 0", () => {
    expect(scoreAt(g, 0)).toEqual({ score: { cp: 20 }, source: "derived" });
  });
  it("is the ply's score and its source after a move", () => {
    expect(scoreAt(g, 1)).toEqual({ score: { cp: 41 }, source: "engine" });
  });
  it("is null without analysis", () => {
    expect(scoreAt({ ...g, analysed: false, startScore: null }, 0)).toBeNull();
  });
});

describe("clocks", () => {
  // White 600 → 590 → 571; Black 600 → 597.5.
  const g = game(["e4", "e5", "Nf3"], {}, { clocks: [590, 597.5, 571] });
  it("shows the base time before a side's first move", () => {
    expect(clockAt(g, "black", 1)).toBe(600);
    expect(clockAt(g, "white", 0)).toBe(600);
  });
  it("shows each side's clock after its latest move at the shown ply", () => {
    expect(clockAt(g, "white", 2)).toBe(590);
    expect(clockAt(g, "white", 3)).toBe(571);
    expect(clockAt(g, "black", 3)).toBe(597.5);
  });
  it("is null when the game has no clocks", () => {
    const bare = game(["e4"], {}, { base: null });
    expect(clockAt(bare, "white", 1)).toBeNull();
    expect(clockAt(bare, "black", 1)).toBeNull();
  });
  it("spent is the mover's clock drop on the shown move, plus the increment", () => {
    expect(spentAt(g, 0)).toBeNull();
    expect(spentAt(g, 1)).toEqual({ seconds: 10 });
    expect(spentAt(g, 3)).toEqual({ seconds: 19 });
    expect(spentAt({ ...g, incrementSeconds: 2 }, 3)).toEqual({ seconds: 21 });
  });
  it("spent prefers the stored time spent", () => {
    const stored = game(["e4"], { 1: { winWhite: 50, timeSpent: 7.5 } }, { clocks: [590] });
    expect(spentAt(stored, 1)).toEqual({ seconds: 7.5 });
  });
  it("tenths only when a clock has a fraction", () => {
    expect(hasTenths(g)).toBe(true);
    expect(hasTenths(game(["e4"], {}, { clocks: [590] }))).toBe(false);
  });
});

describe("annotationsAt (R8)", () => {
  // 1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6?? 4. Qxf7#: Black (the player) blunders with 3...Nf6.
  const sans = ["e4", "e5", "Qh5", "Nc6", "Bc4", "Nf6", "Qxf7#"];
  const g = game(
    sans,
    {
      5: { winWhite: 60, judgement: "inaccuracy", classification: "inaccuracy", bestUci: "f1b5" },
      6: { winWhite: 99, judgement: "blunder", classification: "blunder", bestUci: "g7g6" },
      3: { winWhite: 52, judgement: "mistake", classification: "mistake", bestUci: "g1f3" },
    },
    { playerColor: "black" },
  );
  it("draws nothing at the start", () => {
    expect(annotationsAt(g, 0)).toEqual([]);
  });
  it("a player's blunder: pen ring, pen arrow, pen ?? badge and the good best-move arrow", () => {
    expect(annotationsAt(g, 6)).toEqual([
      { kind: "circle", square: "f6", tone: "pen" },
      { kind: "arrow", from: "g8", to: "f6", tone: "pen" },
      { kind: "arrow", from: "g7", to: "g6", tone: "good" },
      { kind: "mark", square: "f6", glyph: "??", tone: "pen" },
    ]);
  });
  it("an opponent's mistake: a pencil badge and the best move, no pen", () => {
    expect(annotationsAt(g, 3)).toEqual([
      { kind: "arrow", from: "g1", to: "f3", tone: "good" },
      { kind: "mark", square: "h5", glyph: "?", tone: "pencil" },
    ]);
  });
  it("an inaccuracy: a badge and no ring", () => {
    const marks = annotationsAt({ ...g, playerColor: "white" }, 5);
    expect(marks.filter((m) => m.kind === "circle")).toEqual([]);
    expect(marks).toContainEqual({ kind: "mark", square: "c4", glyph: "?!", tone: "pen" });
  });
  it("no good arrow for a judged move whose engine move is the move played", () => {
    const same = game(["e4"], {
      1: { winWhite: 40, judgement: "inaccuracy", classification: "inaccuracy", bestUci: "e2e4" },
    });
    expect(annotationsAt(same, 1)).toEqual([
      { kind: "mark", square: "e4", glyph: "?!", tone: "pen" },
    ]);
  });
  it("no best-move arrow when the move played was best", () => {
    const best = game(["e4"], { 1: { winWhite: 50, classification: "best", bestUci: "e2e4" } });
    expect(annotationsAt(best, 1)).toEqual([]);
  });
});

describe("commentaryAt", () => {
  const sans = ["e4", "e5", "Qh5", "Nc6", "Bc4", "Nf6", "Qxf7#"];
  const g = game(
    sans,
    {
      2: { winWhite: 48, classification: "best", bestUci: "e7e5" },
      3: { winWhite: 46, judgement: "inaccuracy", classification: "inaccuracy", deltaW: 2 },
      // Black's win chance 64% → 22%: White's win% after is 78, the drop 42.
      6: {
        winWhite: 78,
        judgement: "blunder",
        classification: "blunder",
        bestUci: "g7g6",
        deltaW: 42,
      },
    },
    { playerColor: "black" },
  );
  it("a player's blunder reads like the coach's line", () => {
    expect(commentaryAt(g, 6)).toBe(
      "3… Nf6?? is a blunder. Win chance 64% to 22%. The engine preferred g6.",
    );
  });
  it("an opponent's mistake names whose it was and the player's win chance", () => {
    expect(commentaryAt(g, 3)).toBe(
      "2. Qh5?! is an inaccuracy by Whitey. Your win chance 52% to 54%.",
    );
  });
  it("a best move matches the engine", () => {
    expect(commentaryAt(g, 2)).toBe("1… e5 matches the engine's choice.");
  });
  it("is null at the start, for an unremarkable move and without analysis", () => {
    expect(commentaryAt(g, 0)).toBeNull();
    expect(commentaryAt(g, 1)).toBeNull();
    expect(commentaryAt({ ...g, analysed: false }, 6)).toBeNull();
  });
});

describe("header lines", () => {
  const g = game(["e4"]);
  it("meta line: time control, rated and the UTC day", () => {
    expect(metaLine(g)).toBe("Rapid 10+0, rated, 7 Nov 2026");
    expect(
      metaLine({ ...g, timeClass: "blitz", baseSeconds: 180, incrementSeconds: 2, rated: false }),
    ).toBe("Blitz 3+2, casual, 7 Nov 2026");
    expect(
      metaLine({ ...g, baseSeconds: null, playedAt: new Date("2026-11-07T23:30:00-02:00") }),
    ).toBe("Rapid, rated, 8 Nov 2026");
  });
  it("result line from the player's side", () => {
    expect(resultLine(g)).toBe("You lost");
    expect(resultLine({ ...g, playerResult: "win" })).toBe("You won");
    expect(resultLine({ ...g, playerResult: "draw" })).toBe("Draw");
  });
  it("accuracy line, left out when null", () => {
    expect(accuracyLine(g)).toBe("Your accuracy 70.0%, opponent 80.0%");
    expect(accuracyLine({ ...g, playerColor: "black" })).toBe(
      "Your accuracy 80.0%, opponent 70.0%",
    );
    expect(accuracyLine({ ...g, accuracy: { white: 70, black: null } })).toBe(
      "Your accuracy 70.0%",
    );
    expect(accuracyLine({ ...g, accuracy: { white: null, black: null } })).toBeNull();
  });
  it("eval text marks a derived score with ≈ and uses the real minus", () => {
    expect(evalText({ score: { cp: -123 }, source: "derived" })).toBe("≈ −1.2");
    expect(evalText({ score: { cp: 57 }, source: "engine" })).toBe("+0.6");
    expect(evalText({ score: { mate: 3 }, source: "engine" })).toBe("#3");
    expect(evalText(null)).toBeNull();
  });
});
