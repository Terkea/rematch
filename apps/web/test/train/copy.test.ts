import { describe, expect, it } from "vitest";
import {
  patternText,
  rateText,
  SOURCE_LABEL,
  sourceRowText,
} from "../../src/app/(app)/app/train/copy";
import { WEAKNESSES } from "../../src/server/profile/catalogue";
import { rateOf } from "../../src/server/train/model";

// The train page's words: one name per weakness (the catalogue's), n with every rate, and no
// claim of a change unless the 80% ranges separate.

describe("SOURCE_LABEL", () => {
  it("uses the catalogue's plain names", () => {
    for (const key of ["W4.missed", "W3.shallow", "W3.deep", "W7.traps"] as const) {
      expect(SOURCE_LABEL[key]).toBe(WEAKNESSES.find((w) => w.key === key)?.label);
    }
    expect(SOURCE_LABEL.blunder).toBe("Blunders");
  });
});

describe("rateText", () => {
  it("states the rate, its 80% range and n", () => {
    expect(rateText(rateOf(18, 91))).toMatch(/^19\.8% of 91 chances \(80% range \d+–\d+%\)$/);
  });

  it("is a dash with no chances, never 0%", () => {
    expect(rateText(rateOf(0, 0))).toBe("– (no chances yet)");
  });
});

describe("sourceRowText", () => {
  const drill = new Date("2026-11-01T00:00:00Z");

  it("says too early below 30 chances, with what is still needed", () => {
    expect(
      sourceRowText({
        firstDrill: drill,
        before: rateOf(18, 91),
        after: rateOf(1, 12),
        verdict: "too_early",
      }),
    ).toBe("Too early to tell: 12 chances in games since your first drill, and 30 are needed.");
  });

  it("says not drilled yet before any attempt", () => {
    expect(
      sourceRowText({
        firstDrill: null,
        before: rateOf(18, 91),
        after: rateOf(0, 0),
        verdict: "too_early",
      }),
    ).toBe("Not drilled yet.");
  });

  it("claims lower only on separated ranges, and says when they overlap", () => {
    const lower = sourceRowText({
      firstDrill: drill,
      before: rateOf(40, 100),
      after: rateOf(5, 100),
      verdict: "lower",
    });
    expect(lower).toMatch(/^Lower since your first drill: 5\.0% of 100 chances/);
    expect(
      sourceRowText({
        firstDrill: drill,
        before: rateOf(20, 100),
        after: rateOf(17, 100),
        verdict: "no_clear_change",
      }),
    ).toMatch(/^No clear change yet: the ranges overlap\./);
  });
});

describe("patternText", () => {
  it("compares drilled and held-back patterns, or says too early", () => {
    expect(
      patternText({
        since: null,
        drilled: rateOf(0, 0),
        heldOut: rateOf(0, 0),
        verdict: "too_early",
      }),
    ).toBe("Not drilled yet.");
    expect(
      patternText({
        since: new Date("2026-11-01"),
        drilled: rateOf(3, 4),
        heldOut: rateOf(1, 2),
        verdict: "too_early",
      }),
    ).toBe(
      "Too early to tell: 4 chances at the patterns you drilled and 2 at those held back, and each needs 30.",
    );
  });
});
