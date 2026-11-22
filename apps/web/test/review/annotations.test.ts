import { describe, expect, it } from "vitest";
import { annotationsFor } from "../../src/app/(app)/app/games/[gameId]/annotations";
import { toReviewView } from "../../src/app/(app)/app/games/[gameId]/view-model";
import { game } from "../helpers/review-game";

// R8 as redesigned in Task 10 (critic, T7 change 4): the player's mistake is a red-pen stroke from
// the square it left to the edge of the square it reached, so no ink covers the piece; the glyph
// badge sits in a corner of that square. No ring: on a phone the ring, the arrowhead and the badge
// made one red blob over the piece.

const SANS = ["e4", "e5", "Nf3", "Nc6", "Bc4", "Nf6", "Ng5", "d5", "exd5"];
const view = toReviewView({
  ...game(SANS, {
    2: { winWhite: 54, judgement: "inaccuracy", deltaW: 6, bestUci: "c7c5" },
    3: { winWhite: 42, judgement: "mistake", deltaW: 12, bestUci: "b1c3" },
    5: { winWhite: 40, judgement: "blunder", deltaW: 18, bestUci: "d2d4" },
    6: { winWhite: 55, judgement: "mistake", deltaW: 12, bestUci: "g8e7" },
    7: { winWhite: 50, judgement: "inaccuracy", deltaW: 5, bestUci: "d2d3" },
    8: { winWhite: 50, bestUci: "d7d6" },
    9: { winWhite: 50, bestUci: "e4d5" },
  }),
  playerColor: "white",
});

describe("annotationsFor", () => {
  it("marks the player's blunder in pen, off the piece, with the engine's move in green", () => {
    expect(annotationsFor(view, 5, false)).toEqual([
      { kind: "arrow", from: "f1", to: "c4", tone: "pen", toEdge: true },
      { kind: "arrow", from: "d2", to: "d4", tone: "good" },
      { kind: "mark", square: "c4", glyph: "??", tone: "pen", place: "edge" },
    ]);
  });

  it("never rings the destination", () => {
    for (let ply = 0; ply <= SANS.length; ply++)
      expect(annotationsFor(view, ply, true).some((a) => a.kind === "circle")).toBe(false);
  });

  it("marks the opponent's mistake in pencil, with no pen", () => {
    const marks = annotationsFor(view, 6, false);
    expect(marks).toContainEqual({
      kind: "mark",
      square: "f6",
      glyph: "?",
      tone: "pencil",
      place: "edge",
    });
    expect(marks.some((a) => a.tone === "pen")).toBe(false);
  });

  it("gives an inaccuracy its badge only, in muted pencil", () => {
    expect(annotationsFor(view, 7, false)).toEqual([
      { kind: "arrow", from: "d2", to: "d3", tone: "good" },
      { kind: "mark", square: "g5", glyph: "?!", tone: "pencil", place: "edge" },
    ]);
  });

  // Fix round 1 (review-t10 I1): only the player's ?? is full red on the board.
  it("grades the player's badges: ?? full pen, ? light pen, ?! pencil", () => {
    const badge = (ply: number) => annotationsFor(view, ply, false).find((a) => a.kind === "mark");
    expect(badge(5)).toMatchObject({ glyph: "??", tone: "pen" });
    expect(badge(5)).not.toHaveProperty("weight");
    expect(badge(3)).toMatchObject({ glyph: "?", tone: "pen", weight: "light" });
    expect(badge(7)).toMatchObject({ glyph: "?!", tone: "pencil" });
  });

  it("adds exactly one pencil arrow for the engine's move in the shown position", () => {
    const off = annotationsFor(view, 7, false);
    const on = annotationsFor(view, 7, true);
    expect(on.length - off.length).toBe(1);
    // The position after ply 7: the engine's choice for ply 8 was d7d6.
    expect(on).toContainEqual({ kind: "arrow", from: "d7", to: "d6", tone: "pencil" });
  });

  it("shows the engine's move at the start as well", () => {
    const v = toReviewView(game(["e4"], { 1: { winWhite: 52, bestUci: "d2d4" } }));
    expect(annotationsFor(v, 0, true)).toEqual([
      { kind: "arrow", from: "d2", to: "d4", tone: "pencil" },
    ]);
  });

  it("is empty at ply 0 without the engine, and without analysis", () => {
    expect(annotationsFor(view, 0, false)).toEqual([]);
    const pending = toReviewView({
      ...game(SANS, { 5: { winWhite: 40, judgement: "blunder", deltaW: 18 } }),
      analysed: false,
      analysisStatus: "pending",
    });
    expect(annotationsFor(pending, 5, true)).toEqual([]);
  });
});
