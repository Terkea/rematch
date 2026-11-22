import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  betaQuantile,
  lgamma,
  lnBeta,
  P_IMP_DELTA_REL,
  P_IMP_DRAWS,
  P_IMP_SEED,
  pImprovement,
  posterior,
  pRegression,
  probAbove,
  REGRESSION_FACTOR,
  regIncBeta,
  SHAPE_FLOOR,
} from "../../src/server/stats/posterior";

// `profile-core`'s `posterior_fixture` test writes this grid from the Rust implementation; every
// case must match the port to 1e-9.
type Side = { k: number; n: number; mu: number; n0: number };
type Cases = {
  posterior: (Side & { alpha: number; beta: number; mean: number; low: number; high: number })[];
  probAbove: (Side & { threshold: number; p: number })[];
  quantile: { p: number; a: number; b: number; q: number }[];
  regIncBeta: { x: number; a: number; b: number; y: number }[];
  lnBeta: { a: number; b: number; y: number }[];
  lgamma: { x: number; y: number }[];
  pImprovement: {
    current: Side;
    baseline: Side;
    deltaRel: number;
    draws: number;
    seed: number;
    p: number;
  }[];
  pRegression: {
    current: Side;
    baseline: Side;
    factor: number;
    draws: number;
    seed: number;
    p: number;
  }[];
};

const cases = JSON.parse(
  readFileSync(
    new URL(
      "../../../../backend/crates/profile-core/tests/fixtures/posterior_cases.json",
      import.meta.url,
    ),
    "utf8",
  ),
) as Cases;

const TOL = 1e-9;
function close(got: number, want: number, what: string) {
  expect(Number.isFinite(got), `${what}: ${got} is not finite`).toBe(true);
  expect(Math.abs(got - want), `${what}: got ${got}, want ${want}`).toBeLessThanOrEqual(TOL);
}
const post = (s: Side) => posterior(s.k, s.n, { mu: s.mu, n0: s.n0 });

describe("the Beta posterior port matches profile-core", () => {
  it("has a non-trivial grid", () => {
    expect(cases.posterior.length).toBeGreaterThan(100);
    expect(cases.pImprovement.length).toBeGreaterThan(10);
  });

  it("special functions", () => {
    for (const c of cases.lgamma) close(lgamma(c.x), c.y, `lgamma(${c.x})`);
    for (const c of cases.lnBeta) close(lnBeta(c.a, c.b), c.y, `lnBeta(${c.a}, ${c.b})`);
    for (const c of cases.regIncBeta)
      close(regIncBeta(c.x, c.a, c.b), c.y, `I_${c.x}(${c.a}, ${c.b})`);
  });

  it("Beta quantiles", () => {
    for (const c of cases.quantile)
      close(betaQuantile(c.p, c.a, c.b), c.q, `q(${c.p}; ${c.a}, ${c.b})`);
  });

  it("posteriors and their 80% intervals", () => {
    for (const c of cases.posterior) {
      const p = post(c);
      const what = `posterior(${c.k}, ${c.n}, ${c.mu}, ${c.n0})`;
      close(p.alpha, c.alpha, `${what}.alpha`);
      close(p.beta, c.beta, `${what}.beta`);
      close(p.mean, c.mean, `${what}.mean`);
      close(p.interval.low, c.low, `${what}.low`);
      close(p.interval.high, c.high, `${what}.high`);
    }
  });

  it("P(theta > threshold)", () => {
    for (const c of cases.probAbove) close(probAbove(post(c), c.threshold), c.p, "probAbove");
  });

  it("P_imp and P_reg draw the same seeded stream", () => {
    for (const c of cases.pImprovement)
      close(
        pImprovement(post(c.current), post(c.baseline), {
          deltaRel: c.deltaRel,
          draws: c.draws,
          seed: c.seed,
        }),
        c.p,
        `pImprovement ${JSON.stringify(c)}`,
      );
    for (const c of cases.pRegression)
      close(
        pRegression(post(c.current), post(c.baseline), {
          factor: c.factor,
          draws: c.draws,
          seed: c.seed,
        }),
        c.p,
        `pRegression ${JSON.stringify(c)}`,
      );
  });
});

describe("posterior edge cases", () => {
  it("uses the Rust defaults", () => {
    expect([P_IMP_DELTA_REL, REGRESSION_FACTOR, P_IMP_DRAWS, P_IMP_SEED, SHAPE_FLOOR]).toEqual([
      0.15, 1.15, 10_000, 1, 1e-3,
    ]);
  });

  it("floors a zero shape and keeps the mean inside the interval", () => {
    const p = posterior(0, 0, { mu: 0, n0: 0 });
    expect(p.alpha).toBe(SHAPE_FLOOR);
    expect(p.beta).toBe(SHAPE_FLOOR);
    expect(p.interval.low).toBeLessThanOrEqual(p.mean);
    expect(p.interval.high).toBeGreaterThanOrEqual(p.mean);
  });

  it("clamps invalid input to a finite proper posterior", () => {
    for (const [k, n, mu, n0] of [
      [Number.NaN, 10, 0.2, 5],
      [12, 10, 0.2, 5],
      [-1, -5, Number.NaN, -3],
      [1, Number.POSITIVE_INFINITY, 2, 5],
    ] as const) {
      const p = posterior(k, n, { mu, n0 });
      for (const v of [p.alpha, p.beta, p.mean, p.interval.low, p.interval.high])
        expect(Number.isFinite(v)).toBe(true);
    }
  });

  it("gives 0 for zero draws", () => {
    const a = posterior(1, 10, { mu: 0.1, n0: 5 });
    expect(pImprovement(a, a, { draws: 0 })).toBe(0);
  });
});
