import { describe, expect, it } from "vitest";
import { wilson } from "../../src/server/stats";

describe("wilson", () => {
  it("is null with no evidence", () => {
    expect(wilson(0, 0)).toBeNull();
  });

  // Hand check, z = 1.28155, p = 0.5, n = 10:
  // centre 0.5, half width = z * sqrt(0.025 + z^2 / 400) / (1 + z^2 / 10) = 0.18780.
  // (The brief quoted 0.3055 / 0.6945, which no 80% z reproduces; 0.3122 / 0.6878 is the formula.)
  it("gives the 80% interval for 5 of 10", () => {
    const i = wilson(5, 10);
    expect(i?.low).toBeCloseTo(0.3122, 3);
    expect(i?.high).toBeCloseTo(0.6878, 3);
    expect(i?.level).toBe(0.8);
  });

  it("reaches the ends exactly and is never zero-width", () => {
    expect(wilson(10, 10)?.high).toBe(1);
    expect(wilson(0, 10)?.low).toBe(0);
    for (const [s, n] of [
      [10, 10],
      [0, 10],
      [1, 1],
      [0, 1],
    ] as const) {
      const i = wilson(s, n);
      expect(i && i.high - i.low > 0).toBe(true);
    }
  });
});
