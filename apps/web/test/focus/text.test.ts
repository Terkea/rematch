import { describe, expect, it } from "vitest";
import {
  gamesText,
  goalText,
  outcomeText,
  progressText,
  targetWhyText,
  verdictText,
  weekLineText,
} from "../../src/components/focus/focus-text";
import { focusProgress } from "../../src/server/focus/goal";

describe("focus copy", () => {
  it("states the goal with the target, N and the current rate", () => {
    expect(goalText({ targetRate: 0.15, baselineRate: 0.2, gamesN: 20, timeClass: "rapid" })).toBe(
      "Let's get it down to 15.0% of your chances over your next 20 rapid games. It's 20.0% now.",
    );
  });

  it("says where the target comes from, with the n behind the current rate", () => {
    expect(
      targetWhyText({
        target: { rate: 0.12, basis: "peer" },
        baseline: { rate: 0.146, events: 18, chances: 91 },
      }),
    ).toBe(
      "That is 18 of 91 chances in the last 6 months. The target is the rate of the players you're compared with.",
    );
    expect(
      targetWhyText({
        target: { rate: 0.15, basis: "relative" },
        baseline: { rate: 0.2, events: 1, chances: 1 },
      }),
    ).toBe("That is 1 of 1 chance in the last 6 months. The target is a quarter fewer.");
  });

  it("gives progress with k of n and the 80% range, and no rate before any chance", () => {
    const p = focusProgress({ k: 1, n: 12, games: 9, gamesN: 20, target: 0.15 });
    expect(progressText(p)).toMatch(
      /^So far 1 of 12 chances, 8\.3% \(80% range \d+\.\d% to \d+\.\d%\)\.$/,
    );
    const none = focusProgress({ k: 0, n: 0, games: 2, gamesN: 20, target: 0.15 });
    expect(progressText(none)).toBe("No chances yet in these games.");
    expect(gamesText(p)).toBe("9 of 20 games");
    expect(gamesText(focusProgress({ k: 0, n: 0, games: 1, gamesN: 20, target: 0.1 }))).toBe(
      "1 of 20 games",
    );
  });

  it("names the verdict honestly", () => {
    expect(verdictText({ kind: "on_track" })).toBe("On track");
    expect(verdictText({ kind: "not_yet" })).toBe("Not there yet");
    expect(verdictText({ kind: "too_early", moreChances: 4 })).toBe(
      "Too early to tell: 4 more chances",
    );
    expect(verdictText({ kind: "too_early", moreChances: 1 })).toBe(
      "Too early to tell: 1 more chance",
    );
    expect(outcomeText("met")).toBe("Goal met");
    expect(outcomeText("not_met")).toBe("Goal not met");
    expect(outcomeText("inconclusive")).toBe("Too few chances to tell");
  });

  it("writes the weekly review's line", () => {
    expect(
      weekLineText({
        label: "Missed mates",
        events: 1,
        chances: 6,
        games: 2,
        targetRate: 0.15,
        rate: 1 / 6,
        interval: { low: 0.05, high: 0.4, level: 0.8 },
      }),
    ).toBe(
      "Missed mates: 1 of 6 chances in 2 games, 16.7% (80% range 5.0% to 40.0%), against a target of 15.0%.",
    );
    expect(
      weekLineText({
        label: "Missed mates",
        events: 0,
        chances: 0,
        games: 1,
        targetRate: 0.15,
        rate: null,
        interval: null,
      }),
    ).toBe("Missed mates: no chances in 1 game this week. The target is 15.0%.");
    expect(
      weekLineText({
        label: "Missed mates",
        events: 0,
        chances: 0,
        games: 1,
        targetRate: 0.15,
        rate: null,
        interval: null,
        practiceSessions: 2,
      }),
    ).toBe(
      "Practice: 2 sessions. Missed mates: no chances in 1 game this week. The target is 15.0%.",
    );
  });
});
