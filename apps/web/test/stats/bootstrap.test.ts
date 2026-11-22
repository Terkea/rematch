import { describe, expect, it } from "vitest";
import { bootstrapMean } from "../../src/server/stats";

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
