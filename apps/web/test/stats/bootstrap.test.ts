import { describe, expect, it } from "vitest";
import {
  bootstrapMean,
  compareMeans,
  dataSeed,
  estimateMean,
  mulberry32,
} from "../../src/server/stats";

const values = Array.from({ length: 50 }, (_, i) => 60 + ((i * 37) % 23));

describe("bootstrapMean", () => {
  it("is deterministic for the same values and seed", () => {
    expect(bootstrapMean(values, "s")).toEqual(bootstrapMean(values, "s"));
  });

  it("is null for no values", () => {
    expect(bootstrapMean([], "s")).toBeNull();
  });

  it("gives a degenerate interval for one value", () => {
    const r = bootstrapMean([70], "s");
    expect(r?.mean).toBe(70);
    expect(r?.interval.low).toBe(70);
    expect(r?.interval.high).toBe(70);
  });

  it("brackets the mean of 50 values tightly", () => {
    const r = bootstrapMean(values, "s");
    if (!r) throw new Error("null");
    expect(r.interval.low).toBeLessThanOrEqual(r.mean);
    expect(r.interval.high).toBeGreaterThanOrEqual(r.mean);
    expect(r.interval.high - r.interval.low).toBeLessThan(10);
  });
});

describe("stable intervals", () => {
  const reversed = [...values].reverse();
  const shuffled = values.map((_, i) => values[(i * 7) % values.length] as number);

  it("gives one interval for the same data in any row order", () => {
    expect(bootstrapMean(reversed, "s")).toEqual(bootstrapMean(values, "s"));
    expect(bootstrapMean(shuffled, "s")?.interval).toEqual(bootstrapMean(values, "s")?.interval);
  });

  it("gives the same estimate and comparison for the same rows in any order", () => {
    expect(estimateMean(reversed, "m")).toEqual(estimateMean(values, "m"));
    const other = values.map((v) => v + 3);
    const x = compareMeans(values, other, { higherIsBetter: true, seed: "c" });
    const y = compareMeans([...values].reverse(), [...other].reverse(), {
      higherIsBetter: true,
      seed: "c",
    });
    expect(y).toEqual(x);
  });

  it("uses a different stream for different data", () => {
    expect(dataSeed("s", values)).not.toBe(
      dataSeed(
        "s",
        values.map((v) => v + 1),
      ),
    );
    expect(dataSeed("s", values, [1])).not.toBe(dataSeed("s", [1], values));
  });

  it("keeps the 80% range honest: about 80% coverage on a fixed synthetic population", () => {
    const rnd = mulberry32(12345);
    const normal = () => {
      const u = Math.max(rnd(), 1e-12);
      return Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * rnd());
    };
    let hit = 0;
    const trials = 200;
    for (let t = 0; t < trials; t++) {
      const sample = Array.from({ length: 40 }, () => 50 + 10 * normal());
      const r = bootstrapMean(sample, "coverage");
      if (r && r.interval.low <= 50 && 50 <= r.interval.high) hit++;
    }
    expect(hit / trials).toBeGreaterThan(0.7);
    expect(hit / trials).toBeLessThan(0.9);
  });

  // The randomness and clock ban over every maths module: test/determinism.test.ts.
});
