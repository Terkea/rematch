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
  });

  it("never returns a zero-width band, even for identical values", () => {
    const flat = Array.from({ length: 12 }, (_, i) => ({ id: `g${i}`, value: 100 }));
    for (const e of rollingMean(flat, "s")) expect(e.high - e.low).toBeGreaterThan(0);
  });
});
