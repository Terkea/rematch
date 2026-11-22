import { mulberry32 } from "./bootstrap";
import { CONFIDENCE } from "./constants";

// A line-by-line port of `profile-core`'s Beta posterior (`stats/posterior.rs`), its Beta
// quantiles and sampling (`stats/beta.rs`) and the special functions (`stats/special.rs`), so the
// web's period comparisons use the engine's numbers. `test/stats/posterior.test.ts` checks every
// case of the Rust-written `posterior_cases.json` to 1e-9. Change both sides together.

/** Replacement for a posterior shape that is exactly 0 (ADR-R2-5 section 7). */
export const SHAPE_FLOOR = 1e-3;
/** Largest `alpha + beta`; larger counts are scaled down together, keeping the mean. */
export const MAX_SHAPE = 1e7;
/** Relative improvement P_imp asks for: `theta_c < theta_b (1 - 0.15)`. */
export const P_IMP_DELTA_REL = 0.15;
/** Regression factor: `theta_c > theta_b * 1.15`. */
export const REGRESSION_FACTOR = 1.15;
export const P_IMP_DRAWS = 10_000;
export const P_IMP_SEED = 1;
/** Relative bisection tolerance of quantiles on `ln x`. */
export const QUANTILE_TOL = 1e-10;

/** Rust's `f64::MIN_POSITIVE`, the smallest normal double (not `Number.MIN_VALUE`). */
const MIN_POSITIVE = 2.2250738585072014e-308;

export type Prior = { mu: number; n0: number };
export type BetaPosterior = {
  alpha: number;
  beta: number;
  mean: number;
  interval: { low: number; high: number };
};

// --- special.rs ---------------------------------------------------------------------------

const LANCZOS_G = 7;
const LANCZOS = [
  0.9999999999998099, 676.5203681218851, -1259.1392167224028, 771.3234287776531, -176.6150291621406,
  12.507343278686905, -0.13857109526572012, 9.984369578019572e-6, 1.5056327351493116e-7,
] as const;
const HALF_LN_2PI = 0.9189385332046728;
const STIRLING_MIN = 10;
const LENTZ_TINY = 1e-300;
const LENTZ_EPS = 1e-15;
const LENTZ_MAX_TERMS = 20_000;

/** `ln |Gamma(x)|`: `+Infinity` at the poles and at `+-Infinity`, NaN for NaN. */
export function lgamma(x: number): number {
  if (Number.isNaN(x)) return Number.NaN;
  if (!Number.isFinite(x) || (x <= 0 && x === Math.floor(x))) return Number.POSITIVE_INFINITY;
  if (x < 0.5) return Math.log(Math.PI / Math.abs(Math.sin(Math.PI * x))) - lgamma(1 - x);
  const z = x - 1;
  let series = LANCZOS[0];
  for (let i = 1; i < LANCZOS.length; i++) series += (LANCZOS[i] as number) / (z + i);
  const t = z + LANCZOS_G + 0.5;
  return 0.5 * Math.log(2 * Math.PI) + (z + 0.5) * Math.log(t) - t + Math.log(series);
}

function stirlingRemainder(x: number): number {
  const r = 1 / x;
  const r2 = r * r;
  return (
    r *
    (1 / 12 -
      r2 *
        (1 / 360 -
          r2 * (1 / 1260 - r2 * (1 / 1680 - r2 * (1 / 1188 - r2 * (691 / 360_360 - r2 / 156))))))
  );
}

/** `ln B(a, b)` for `a, b > 0`, with Stirling differences for large shapes. */
export function lnBeta(a: number, b: number): number {
  const [lo, hi] = a <= b ? [a, b] : [b, a];
  const s = lo + hi;
  if (lo >= STIRLING_MIN) {
    return (
      HALF_LN_2PI -
      0.5 * Math.log(s) -
      (lo - 0.5) * Math.log1p(hi / lo) -
      (hi - 0.5) * Math.log1p(lo / hi) +
      stirlingRemainder(lo) +
      stirlingRemainder(hi) -
      stirlingRemainder(s)
    );
  }
  if (hi >= STIRLING_MIN) {
    return (
      lgamma(lo) -
      (hi - 0.5) * Math.log1p(lo / hi) -
      lo * Math.log(s) +
      lo +
      stirlingRemainder(hi) -
      stirlingRemainder(s)
    );
  }
  return lgamma(lo) + lgamma(hi) - lgamma(s);
}

function validShapes(a: number, b: number): boolean {
  return a > 0 && b > 0 && Number.isFinite(a) && Number.isFinite(b);
}

/** The regularised incomplete beta `I_x(a, b)`, the Beta(a, b) CDF. NaN for invalid input. */
export function regIncBeta(x: number, a: number, b: number): number {
  if (Number.isNaN(x) || !validShapes(a, b)) return Number.NaN;
  return regIncBetaWithLnBeta(x, a, b, lnBeta(a, b));
}

function regIncBetaWithLnBeta(x: number, a: number, b: number, lnB: number): number {
  if (Number.isNaN(x)) return Number.NaN;
  if (x <= 0) return 0;
  if (x >= 1) return 1;
  const lnX = Math.log(x);
  const ln1mx = Math.log1p(-x);
  const i =
    x < (a + 1) / (a + b + 2)
      ? cfBranch(x, lnX, ln1mx, a, b, lnB)
      : 1 - cfBranch(1 - x, ln1mx, lnX, b, a, lnB);
  return Math.min(1, Math.max(0, i));
}

function cfBranch(x: number, lnX: number, ln1mx: number, a: number, b: number, lnB: number) {
  const lnFront = a * lnX + b * ln1mx - lnB - Math.log(a);
  return Math.exp(lnFront) / continuedFraction(x, a, b);
}

function continuedFraction(x: number, a: number, b: number): number {
  let f = 1;
  let c = 1;
  let d = 0;
  for (let j = 1; j <= LENTZ_MAX_TERMS; j++) {
    const m = Math.floor(j / 2);
    const dj =
      j % 2 === 1
        ? (-(a + m) * (a + b + m) * x) / ((a + 2 * m) * (a + 2 * m + 1))
        : (m * (b - m) * x) / ((a + 2 * m - 1) * (a + 2 * m));
    d = 1 + dj * d;
    if (Math.abs(d) < LENTZ_TINY) d = LENTZ_TINY;
    d = 1 / d;
    c = 1 + dj / c;
    if (Math.abs(c) < LENTZ_TINY) c = LENTZ_TINY;
    const delta = c * d;
    f *= delta;
    if (Math.abs(delta - 1) < LENTZ_EPS) break;
  }
  return f;
}

// --- beta.rs ------------------------------------------------------------------------------

/** A proper Beta(a, b) or a point mass (the limit as one shape goes to 0). */
type Dist = { kind: "beta"; a: number; b: number; lnB: number } | { kind: "point"; at: number };

function betaDist(a: number, b: number): Dist | null {
  return validShapes(a, b) ? { kind: "beta", a, b, lnB: lnBeta(a, b) } : null;
}

function fromShapes(a: number, b: number): Dist | null {
  if (a === 0 && b > 0 && Number.isFinite(b)) return { kind: "point", at: 0 };
  if (b === 0 && a > 0 && Number.isFinite(a)) return { kind: "point", at: 1 };
  return betaDist(a, b);
}

function cdf(d: Dist, x: number): number {
  if (Number.isNaN(x)) return Number.NaN;
  if (d.kind === "point") return x >= d.at ? 1 : 0;
  return regIncBetaWithLnBeta(x, d.a, d.b, d.lnB);
}

function quantileOf(d: Dist, p: number): number {
  if (!(p >= 0 && p <= 1)) return Number.NaN;
  if (d.kind === "point") return d.at;
  if (p === 0) return 0;
  if (p === 1) return 1;
  if (cdf(d, MIN_POSITIVE) >= p) return 0;
  let lo = Math.log(MIN_POSITIVE);
  let hi = 0;
  while (hi - lo > QUANTILE_TOL) {
    const mid = 0.5 * (lo + hi);
    if (cdf(d, Math.exp(mid)) < p) lo = mid;
    else hi = mid;
  }
  return Math.exp(0.5 * (lo + hi));
}

/** The `p` quantile of Beta(a, b); NaN for invalid shapes or a `p` outside `[0, 1]`. */
export function betaQuantile(p: number, a: number, b: number): number {
  const d = betaDist(a, b);
  return d ? quantileOf(d, p) : Number.NaN;
}

type Rnd = () => number;
const openUnit = (rng: Rnd) => 1 - rng();

function standardNormal(rng: Rnd): number {
  const u1 = openUnit(rng);
  const u2 = rng();
  return Math.sqrt(-2 * Math.log(u1)) * Math.cos(2 * Math.PI * u2);
}

/** `ln` of one Gamma(shape, 1) draw (Marsaglia and Tsang). */
function lnGammaVariate(rng: Rnd, shape: number): number {
  if (shape < 1) {
    const u = openUnit(rng);
    return lnGammaVariate(rng, shape + 1) + Math.log(u) / shape;
  }
  const d = shape - 1 / 3;
  const c = 1 / Math.sqrt(9 * d);
  for (;;) {
    const x = standardNormal(rng);
    let v = 1 + c * x;
    if (v <= 0) continue;
    v = v * v * v;
    const u = openUnit(rng);
    const x2 = x * x;
    if (u < 1 - 0.0331 * x2 * x2 || Math.log(u) < 0.5 * x2 + d * (1 - v + Math.log(v))) {
      return Math.log(d) + Math.log(v);
    }
  }
}

function sample(d: Dist, rng: Rnd): number {
  if (d.kind === "point") return d.at;
  const lnX = lnGammaVariate(rng, d.a);
  const lnY = lnGammaVariate(rng, d.b);
  return 1 / (1 + Math.exp(lnY - lnX));
}

// --- posterior.rs -------------------------------------------------------------------------

const nonneg = (x: number) => (Number.isFinite(x) && x > 0 ? x : 0);

/**
 * The posterior of `k` events in `n` opportunities under `Beta(n0 mu, n0 (1 - mu))`, with its 80%
 * equal-tailed interval widened to include the mean. Invalid input is clamped (NaN or negative
 * counts become 0, `k` to `[0, n]`, `mu` to `[0, 1]`, NaN `mu` to 1/2), so the result is always a
 * finite proper posterior. A closed window passes unweighted counts (weight 1, no decay).
 */
export function posterior(k: number, n: number, prior: Prior): BetaPosterior {
  let nn = nonneg(n);
  let kk = Math.min(nonneg(k), nn);
  let n0 = nonneg(prior.n0);
  const mu = Number.isNaN(prior.mu) ? 0.5 : Math.min(1, Math.max(0, prior.mu));

  const big = Math.max(n0, nn);
  if (big > 0) {
    const relTotal = n0 / big + nn / big;
    if (relTotal > MAX_SHAPE / big) {
      const f = MAX_SHAPE / big / relTotal;
      n0 *= f;
      nn *= f;
      kk = Math.min(kk * f, nn);
    }
  }

  const floorZero = (x: number) => (x > 0 ? x : SHAPE_FLOOR);
  const alpha = floorZero(n0 * mu + kk);
  const beta = floorZero(n0 * (1 - mu) + (nn - kk));
  const dist = betaDist(alpha, beta);
  if (!dist) return { alpha: 1, beta: 1, mean: 0.5, interval: { low: 0.1, high: 0.9 } };
  const mean = alpha / (alpha + beta);
  const tail = (1 - CONFIDENCE) / 2;
  const qLow = quantileOf(dist, tail);
  const qHigh = quantileOf(dist, 1 - tail);
  const low = Math.min(Number.isNaN(qLow) ? mean : qLow, mean);
  const high = Math.max(Number.isNaN(qHigh) ? mean : qHigh, mean);
  return { alpha, beta, mean, interval: { low, high } };
}

/** `P(theta > threshold)`, in `[0, 1]`; 0 for a NaN threshold or invalid shapes. */
export function probAbove(p: BetaPosterior, threshold: number): number {
  const d = fromShapes(p.alpha, p.beta);
  if (!d || Number.isNaN(threshold)) return 0;
  return Math.min(1, Math.max(0, 1 - cdf(d, threshold)));
}

type McOpts = { draws?: number; seed?: number };

function monteCarlo(
  current: BetaPosterior,
  baseline: BetaPosterior,
  draws: number,
  seed: number,
  hit: (c: number, b: number) => boolean,
): number {
  const c = fromShapes(current.alpha, current.beta);
  const b = fromShapes(baseline.alpha, baseline.beta);
  if (!c || !b || !(draws > 0)) return 0;
  const rng = mulberry32(seed);
  let hits = 0;
  for (let i = 0; i < draws; i++) {
    const xc = sample(c, rng);
    const xb = sample(b, rng);
    if (hit(xc, xb)) hits++;
  }
  return hits / draws;
}

/** `P(theta_c < theta_b (1 - deltaRel))` by seeded Monte Carlo pairs (current drawn first). */
export function pImprovement(
  current: BetaPosterior,
  baseline: BetaPosterior,
  opts: McOpts & { deltaRel?: number } = {},
): number {
  const factor = 1 - (opts.deltaRel ?? P_IMP_DELTA_REL);
  return monteCarlo(
    current,
    baseline,
    opts.draws ?? P_IMP_DRAWS,
    opts.seed ?? P_IMP_SEED,
    (c, b) => c < b * factor,
  );
}

/** `P(theta_c > theta_b factor)` by the same seeded stream as `pImprovement`. */
export function pRegression(
  current: BetaPosterior,
  baseline: BetaPosterior,
  opts: McOpts & { factor?: number } = {},
): number {
  const factor = opts.factor ?? REGRESSION_FACTOR;
  return monteCarlo(
    current,
    baseline,
    opts.draws ?? P_IMP_DRAWS,
    opts.seed ?? P_IMP_SEED,
    (c, b) => c > b * factor,
  );
}
