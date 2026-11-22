import { describe, expect, it } from "vitest";
import { rollingMean } from "../../src/server/stats";

const pts = (n: number) =>
  Array.from({ length: n }, (_, i) => ({ id: `g${i + 1}`, value: 50 + (i % 7) * 3 }));

describe("rollingMean", () => {
  it("gives nothing for 9 points", () => {
    expect(rollingMean(pts(9), "s")).toEqual([]);
  });

  it("starts on the 10th point and uses a 20-point window", () => {
    const p = pts(25);
    const r = rollingMean(p, "s");
    expect(r).toHaveLength(16);
    expect(r[0]?.id).toBe("g10");
    const last = r[15];
    const window = p.slice(5, 25).map((x) => x.value);
    expect(last?.id).toBe("g25");
    expect(last?.mean).toBeCloseTo(window.reduce((a, b) => a + b, 0) / 20, 9);
    expect(last?.low).not.toBeNull();
    expect((last?.low ?? 0) < (last?.mean ?? 0)).toBe(true);
    expect((last?.high ?? 0) > (last?.mean ?? 0)).toBe(true);
  });

  it("gives a null band, not a widened one, for identical values", () => {
    const flat = Array.from({ length: 12 }, (_, i) => ({ id: `g${i}`, value: 100 }));
    const r = rollingMean(flat, "s");
    expect(r.length).toBeGreaterThan(0);
    for (const e of r) {
      expect(e.mean).toBe(100);
      expect(e.low).toBeNull();
      expect(e.high).toBeNull();
    }
  });

  it("keeps the band inside 0 to 100", () => {
    const near = Array.from({ length: 30 }, (_, i) => ({ id: `g${i}`, value: i % 5 ? 100 : 98 }));
    for (const e of rollingMean(near, "s")) {
      expect(e.high ?? 0).toBeLessThanOrEqual(100);
      expect(e.low ?? 0).toBeGreaterThanOrEqual(0);
    }
  });
});
