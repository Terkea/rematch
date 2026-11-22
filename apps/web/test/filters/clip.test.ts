import { describe, expect, it } from "vitest";
import {
  clipWeekly,
  clipWeeklyRates,
  weekOverlaps,
} from "../../src/app/(app)/app/_filters/clip.js";

// Date filters on the stored weekly series (T3): a week stays when any of its days is in range.

describe("weekOverlaps", () => {
  it("keeps a week that touches the range at either end", () => {
    // ISO week 2026-W40 runs Mon 28 Sep to Sun 4 Oct.
    expect(weekOverlaps("2026-W40", { from: "2026-10-04", to: null })).toBe(true);
    expect(weekOverlaps("2026-W40", { from: "2026-10-05", to: null })).toBe(false);
    expect(weekOverlaps("2026-W40", { from: null, to: "2026-09-28" })).toBe(true);
    expect(weekOverlaps("2026-W40", { from: null, to: "2026-09-27" })).toBe(false);
    expect(weekOverlaps("2026-09-28", { from: "2026-10-01", to: "2026-10-01" })).toBe(true);
  });
});

describe("clipWeekly", () => {
  const pts = ["2026-W38", "2026-W40", "2026-W44"].map((week) => ({ week, value: 1 }));

  it("drops the weeks outside the range, and keeps all without one", () => {
    expect(clipWeekly(pts, { from: "2026-09-28", to: "2026-10-30" }).map((p) => p.week)).toEqual([
      "2026-W40",
      "2026-W44",
    ]);
    expect(clipWeekly(pts, { from: null, to: null })).toBe(pts);
  });

  it("blanks out-of-range weeks of a dense series to gaps, never zeros", () => {
    // Three Mondays: 14 Sep, 21 Sep, 28 Sep.
    expect(
      clipWeeklyRates([0.1, 0.2, 0.3], "2026-09-14", { from: "2026-09-21", to: null }),
    ).toEqual([null, 0.2, 0.3]);
  });
});
