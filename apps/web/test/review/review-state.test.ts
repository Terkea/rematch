import { describe, expect, it } from "vitest";
import {
  initialState,
  type ReviewState,
  reviewReducer,
} from "../../src/app/(app)/app/games/[gameId]/review-state";
import { toReviewView } from "../../src/app/(app)/app/games/[gameId]/view-model";
import { game } from "../helpers/review-game";

const SANS = ["e4", "e5", "Nf3", "Nc6", "Bc4", "Nf6", "Ng5", "d5"];
// Moments are worst first (T3): ply 7 is the worst, then 3.
const view = toReviewView({
  ...game(SANS, {
    3: { winWhite: 40, judgement: "mistake", deltaW: 12 },
    7: { winWhite: 20, judgement: "blunder", deltaW: 20 },
  }),
  moments: [7, 3],
  playerColor: "black",
});
const at = (ply: number, more: Partial<ReviewState> = {}): ReviewState => ({
  ply,
  playing: false,
  orientation: "white",
  showEngine: false,
  ...more,
});

describe("initialState", () => {
  it("clamps the ply param", () => {
    expect(initialState(view, "9999").ply).toBe(SANS.length);
    expect(initialState(view, "-4").ply).toBe(0);
    expect(initialState(view, "3").ply).toBe(3);
  });

  it("opens on the worst moment without a usable param", () => {
    expect(initialState(view, "abc").ply).toBe(7);
    expect(initialState(view, undefined).ply).toBe(7);
    expect(initialState(view, "2.5").ply).toBe(7);
  });

  it("opens on the last ply when there are no moments", () => {
    expect(initialState({ ...view, moments: [] }, undefined).ply).toBe(SANS.length);
  });

  it("faces the player's side, engine off, not playing", () => {
    expect(initialState(view, undefined)).toMatchObject({
      orientation: "black",
      showEngine: false,
      playing: false,
    });
  });
});

describe("reviewReducer", () => {
  it("never steps below the start or past the end", () => {
    expect(reviewReducer(view, at(0), { type: "step", by: -1 }).ply).toBe(0);
    expect(reviewReducer(view, at(8), { type: "step", by: 1 }).ply).toBe(8);
    expect(reviewReducer(view, at(4), { type: "go", ply: 99 }).ply).toBe(8);
  });

  it("jumps between moments in game order", () => {
    expect(reviewReducer(view, at(0), { type: "moment", dir: 1 }).ply).toBe(3);
    expect(reviewReducer(view, at(3), { type: "moment", dir: 1 }).ply).toBe(7);
    expect(reviewReducer(view, at(7), { type: "moment", dir: 1 }).ply).toBe(7);
    expect(reviewReducer(view, at(8), { type: "moment", dir: -1 }).ply).toBe(7);
    expect(reviewReducer(view, at(3), { type: "moment", dir: -1 }).ply).toBe(3);
  });

  it("stops at the end of play", () => {
    const s = reviewReducer(view, at(7, { playing: true }), { type: "tick" });
    expect(s).toMatchObject({ ply: 8, playing: false });
    expect(reviewReducer(view, at(2, { playing: true }), { type: "tick" })).toMatchObject({
      ply: 3,
      playing: true,
    });
  });

  it("stops play on any navigation", () => {
    for (const a of [
      { type: "go", ply: 2 },
      { type: "step", by: 1 },
      { type: "first" },
      { type: "last" },
      { type: "moment", dir: 1 },
    ] as const)
      expect(reviewReducer(view, at(4, { playing: true }), a).playing).toBe(false);
  });

  it("plays from the start when play is pressed at the end", () => {
    expect(reviewReducer(view, at(8), { type: "togglePlay" })).toMatchObject({
      ply: 0,
      playing: true,
    });
    expect(reviewReducer(view, at(4, { playing: true }), { type: "togglePlay" }).playing).toBe(
      false,
    );
  });

  it("flips the board and toggles the engine move", () => {
    expect(reviewReducer(view, at(4), { type: "flip" }).orientation).toBe("black");
    expect(reviewReducer(view, at(4, { orientation: "black" }), { type: "flip" }).orientation).toBe(
      "white",
    );
    expect(reviewReducer(view, at(4), { type: "toggleEngine" }).showEngine).toBe(true);
  });

  it("goes to the first and last ply", () => {
    expect(reviewReducer(view, at(4), { type: "first" }).ply).toBe(0);
    expect(reviewReducer(view, at(4), { type: "last" }).ply).toBe(8);
  });
});
