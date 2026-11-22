import { describe, expect, it } from "vitest";
import { estimateMean, estimateRate } from "../../src/server/stats";

describe("estimates", () => {
  it("estimate below minimum is insufficient", () => {
    expect(estimateMean([1, 2, 3, 4, 5, 6, 7, 8, 9], "s")).toEqual({
      kind: "insufficient",
      n: 9,
      needed: 1,
    });
  });

  it("estimateRate with no games needs 10", () => {
    expect(estimateRate(0, 0)).toEqual({ kind: "insufficient", n: 0, needed: 10 });
  });

  it("carries n and a non-degenerate interval", () => {
    const e = estimateRate(8, 14);
    expect(e.kind).toBe("estimate");
    if (e.kind === "estimate") {
      expect(e.n).toBe(14);
      expect(e.value).toBeCloseTo(8 / 14, 9);
      expect(e.interval.high).toBeGreaterThan(e.interval.low);
    }
    const flat = estimateMean(Array(12).fill(100), "s");
    if (flat.kind === "estimate") expect(flat.interval.high).toBeGreaterThan(flat.interval.low);
    else throw new Error("expected estimate");
  });

  it("never returns a non-finite number", () => {
    let seed = 12345;
    const rnd = () => {
      seed = (seed * 1664525 + 1013904223) % 4294967296;
      return seed / 4294967296;
    };
    for (let k = 0; k < 200; k++) {
      const n = Math.floor(rnd() * 40);
      const vals = Array.from({ length: n }, () => rnd() * 100);
      if (k % 7 === 0 && n > 0) vals[0] = Number.NaN;
      const m = estimateMean(vals, `k${k}`);
      const r = estimateRate(Math.floor(rnd() * (n + 1)), n);
      for (const e of [m, r]) {
        if (e.kind === "estimate") {
          for (const v of [e.value, e.interval.low, e.interval.high, e.n]) {
            expect(Number.isFinite(v)).toBe(true);
          }
        } else {
          expect(Number.isFinite(e.n) && Number.isFinite(e.needed)).toBe(true);
        }
      }
    }
  });
});
