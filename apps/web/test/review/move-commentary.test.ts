import { describe, expect, it } from "vitest";
import {
  ledgerRows,
  moveCommentary,
  moveText,
} from "../../src/app/(app)/app/games/[gameId]/move-commentary";
import { toReviewView } from "../../src/app/(app)/app/games/[gameId]/view-model";
import { game } from "../helpers/review-game";

const MATE_HIGH = 97.54474363414323; // winPercent of a forced mate (the cp clamp)
const SANS = ["e4", "e5", "Nf3", "Nc6", "Bc4", "Nf6", "Ng5", "d5", "exd5", "Na5"];

function build(playerColor: "white" | "black") {
  return toReviewView({
    ...game(SANS, {
      // White's Bc4: 64% to 22% (deltaW 42). The engine wanted d2d4.
      5: { winWhite: 22, judgement: "blunder", deltaW: 42, bestUci: "d2d4" },
      // Black's Nf6: a mistake by 11 points, Black's win 84% after (White 16%).
      6: { winWhite: 16, judgement: "mistake", deltaW: 11, bestUci: "g8e7" },
      // White's Ng5: White had a forced mate (the clamp, 98%) and the move let it go, leaving 90%.
      // Only 7.5 points, but judged a blunder by the lost-mate rule.
      7: { winWhite: 90, judgement: "blunder", deltaW: MATE_HIGH - 90, bestUci: "c4f7" },
      8: { winWhite: 90, classification: "best", bestUci: "d7d5" },
      9: { winWhite: 90 },
    }),
    playerColor,
    moments: playerColor === "white" ? [5, 7] : [6],
  });
}

describe("moveCommentary", () => {
  const white = build("white");

  it("says what the player's blunder cost and what the engine wanted", () => {
    expect(moveCommentary(white, 5)).toBe(
      "3. Bc4?? is a blunder. Win chance 64% to 22%. The engine preferred d4.",
    );
  });

  it("says when a judgement rests on a lost forced mate, not the drop", () => {
    expect(moveCommentary(white, 7)).toBe(
      "4. Ng5?? is a blunder: it let a forced mate go. Win chance 98% to 90%. The engine preferred Bxf7+.",
    );
  });

  it("names the opponent and gives the player's win chance for their mistake", () => {
    // Black's win went 95% to 84%; White's (the player's) 5% to 16%.
    expect(moveCommentary(white, 6)).toBe(
      "3… Nf6? is a mistake by Blacky. Your win chance 5% to 16%. The engine preferred Nge7.",
    );
  });

  it("credits a best move", () => {
    expect(moveCommentary(white, 8)).toBe("4… d5 matches the engine's choice.");
  });

  it("is null for an unjudged move that is not the best, and at the start", () => {
    expect(moveCommentary(white, 9)).toBeNull();
    expect(moveCommentary(white, 0)).toBeNull();
  });

  it("says when the player walked into a forced mate", () => {
    const v = toReviewView({
      ...game(["e4", "e5"], {
        2: { winWhite: MATE_HIGH, judgement: "blunder", deltaW: 6, bestUci: "e7e5" },
      }),
      playerColor: "black",
    });
    expect(moveCommentary(v, 2)).toBe(
      "1… e5?? is a blunder: it allows a forced mate. Win chance 8% to 2%.",
    );
  });
});

describe("moveText", () => {
  it("numbers the move and kerns the glyph", () => {
    const v = build("white");
    expect(moveText(v, 5)).toBe("3. Bc4??");
    expect(moveText(v, 6)).toBe("3… Nf6?");
    expect(moveText(v, 0)).toBeNull();
  });
});

describe("ledgerRows", () => {
  it("lists the player's judged moves in game order, with what each was judged on", () => {
    expect(ledgerRows(build("white"))).toEqual([
      { ply: 5, text: "3. Bc4??", judgement: "blunder", before: 64, after: 22, basis: "win" },
      {
        ply: 7,
        text: "4. Ng5??",
        judgement: "blunder",
        before: 98,
        after: 90,
        basis: "missedMate",
      },
    ]);
  });
});
