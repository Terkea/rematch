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

  it("carries n and a real interval", () => {
    const e = estimateRate(8, 14);
    expect(e.kind).toBe("estimate");
    if (e.kind === "estimate") {
      expect(e.n).toBe(14);
      expect(e.value).toBeCloseTo(8 / 14, 9);
      expect(e.interval && e.interval.high > e.interval.low).toBe(true);
    }
  });

  it("n counts only the finite values", () => {
    const vals = [...Array(12).fill(50), Number.NaN, Number.POSITIVE_INFINITY];
    const e = estimateMean(vals, "s");
    expect(e.kind === "estimate" && e.n).toBe(12);
    const short = estimateMean([...Array(9).fill(50), Number.NaN], "s");
    expect(short).toEqual({ kind: "insufficient", n: 9, needed: 1 });
  });

  it("zero spread gives a null interval, never a widened one", () => {
    const flat = estimateMean(Array(12).fill(100), "s", { min: 0, max: 100 });
    expect(flat).toMatchObject({ kind: "estimate", value: 100, n: 12, interval: null });
    const clean = estimateMean(Array(15).fill(0), "s");
    expect(clean).toMatchObject({ kind: "estimate", value: 0, n: 15, interval: null });
  });

  it("an interval never leaves the metric's domain", () => {
    const counts = [...Array(29).fill(0), 1];
    const c = estimateMean(counts, "c");
    if (c.kind !== "estimate" || !c.interval) throw new Error("expected interval");
    expect(c.interval.low).toBeGreaterThanOrEqual(0);
    const acc = estimateMean([...Array(29).fill(100), 99], "a", { min: 0, max: 100 });
    if (acc.kind !== "estimate" || !acc.interval) throw new Error("expected interval");
    expect(acc.interval.high).toBeLessThanOrEqual(100);
  });

  it("never returns a non-finite number", () => {
    let seed = 12345;
    const rnd = () => {
      seed = (seed * 1664525 + 1013904223) % 4294967296;
      return seed / 4294967296;
    };
    for (let k = 0; k < 200; k++) {
      const n = Math.floor(rnd() * 40);
      const vals = Array.from({ length: n }, () => (k % 5 === 0 ? 0 : rnd() * 100));
      if (k % 7 === 0 && n > 0) vals[0] = Number.NaN;
      const m = estimateMean(vals, `k${k}`);
      const r = estimateRate(Math.floor(rnd() * (n + 1)), n);
      for (const e of [m, r]) {
        if (e.kind === "estimate") {
          const nums = [e.value, e.n, ...(e.interval ? [e.interval.low, e.interval.high] : [])];
          for (const v of nums) expect(Number.isFinite(v)).toBe(true);
          if (e.interval) expect(e.interval.high).toBeGreaterThan(e.interval.low);
        } else {
          expect(Number.isFinite(e.n) && Number.isFinite(e.needed)).toBe(true);
        }
      }
    }
  });
});
