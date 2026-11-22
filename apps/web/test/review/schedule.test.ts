import { describe, expect, it } from "vitest";
import { revealSchedule } from "../../src/app/(app)/app/games/[gameId]/reveal/schedule";

// DESIGN.md Choreography §1: the prologue's capped ring stagger, the climax beats and the totals.

const climax = (n: number) => [9, 21, 33].slice(0, n).map((ply) => ({ ply }));

describe("revealSchedule, full", () => {
  it("staggers graph rings by 80ms when they fit", () => {
    expect(revealSchedule({ markedCount: 5, climax: climax(3), mode: "full" }).ringStagger).toBe(
      80,
    );
  });

  it("caps the stagger so the last ring starts by 960 (n = 15)", () => {
    const s = revealSchedule({ markedCount: 15, climax: climax(3), mode: "full" });
    expect(s.ringStagger).toBeCloseTo(25.714, 3);
  });

  it("uses 80ms for a single marked moment", () => {
    expect(revealSchedule({ markedCount: 1, climax: climax(1), mode: "full" }).ringStagger).toBe(
      80,
    );
  });

  it("starts the three beats at 1200, 3260 and 5320, retracting all but the last", () => {
    const s = revealSchedule({ markedCount: 8, climax: climax(3), mode: "full" });
    expect(s.beats.map((b) => b.t0)).toEqual([1200, 3260, 5320]);
    expect(s.beats.map((b) => b.retract)).toEqual([true, true, false]);
    expect(s.beats.map((b) => b.ply)).toEqual([9, 21, 33]);
    expect(s.total).toBe(7220);
  });

  it("totals 3100 with one moment and 600 with none", () => {
    expect(revealSchedule({ markedCount: 1, climax: climax(1), mode: "full" }).total).toBe(3100);
    const none = revealSchedule({ markedCount: 0, climax: [], mode: "full" });
    expect(none.total).toBe(600);
    expect(none.beats).toEqual([]);
  });

  it("totals 5160 with two moments", () => {
    expect(revealSchedule({ markedCount: 2, climax: climax(2), mode: "full" }).total).toBe(5160);
  });
});

describe("revealSchedule, condensed", () => {
  it("caps the stagger at 40ms, has no beats and lasts 600ms", () => {
    const s = revealSchedule({ markedCount: 15, climax: climax(3), mode: "condensed" });
    expect(s.ringStagger).toBeCloseTo(25.714, 3);
    expect(s.beats).toEqual([]);
    expect(s.total).toBe(600);
  });

  it("staggers by 40ms for few marks", () => {
    expect(
      revealSchedule({ markedCount: 5, climax: climax(3), mode: "condensed" }).ringStagger,
    ).toBe(40);
    expect(
      revealSchedule({ markedCount: 1, climax: climax(1), mode: "condensed" }).ringStagger,
    ).toBe(40);
  });
});
