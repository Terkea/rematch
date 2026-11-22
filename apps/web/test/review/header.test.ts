import { describe, expect, it } from "vitest";
import {
  accuracyLine,
  metaLine,
  pageTitle,
  resultLine,
} from "../../src/app/(app)/app/games/[gameId]/header";
import { game } from "../helpers/review-game";

const g = { ...game(["e4"]), playedAt: new Date("2026-11-12T23:30:00Z") };

describe("review header", () => {
  it("titles the page with both players and the day, in UTC", () => {
    expect(pageTitle(g)).toBe("Whitey v Blacky, 12 Nov 2026");
  });

  it("reads the time control, rated and the day", () => {
    expect(metaLine(g)).toBe("Rapid 10+0, rated, 12 Nov 2026");
    expect(
      metaLine({ ...g, timeClass: "blitz", baseSeconds: 180, incrementSeconds: 2, rated: false }),
    ).toBe("Blitz 3+2, casual, 12 Nov 2026");
    expect(metaLine({ ...g, timeClass: "daily", baseSeconds: null })).toBe(
      "Daily, rated, 12 Nov 2026",
    );
  });

  it("says who won", () => {
    expect(resultLine({ ...g, playerResult: "win" })).toBe("You won");
    expect(resultLine({ ...g, playerResult: "loss" })).toBe("You lost");
    expect(resultLine({ ...g, playerResult: "draw" })).toBe("Draw");
    expect(resultLine({ ...g, playerResult: "unfinished" })).toBe("Unfinished");
  });

  it("gives both accuracies, leaving out a missing one", () => {
    expect(accuracyLine(g)).toEqual(["Your accuracy 70.0%", "Opponent 80.0%"]);
    expect(accuracyLine({ ...g, playerColor: "black" })).toEqual([
      "Your accuracy 80.0%",
      "Opponent 70.0%",
    ]);
    expect(accuracyLine({ ...g, accuracy: { white: null, black: 64 } })).toEqual([
      "Opponent 64.0%",
    ]);
    expect(accuracyLine({ ...g, accuracy: { white: null, black: null } })).toEqual([]);
  });
});
