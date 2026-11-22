import { bandEnds, bound, formatAt, type StatFormat } from "@ci/ui/stats";
import type { Estimate } from "../../../../server/stats/estimate";
import type { Interval } from "../../../../server/stats/wilson";

// The words of one game-level stat (T3): its value with its unit, and its 80% range rounded
// outward at the shown precision (DESIGN.md "KPI stats"). Below the minimum a dash and the
// shortfall, never 0; never NaN. Pure, so a server component may call it.

/** accuracy: 0-100; perGame: a count per game; score: points per game, 0-1. */
export type StatUnit = "accuracy" | "perGame" | "score";

export const DASH = "—";
const MINUS = "−";
const FORMAT: Record<StatUnit, StatFormat> = {
  accuracy: "percent",
  perGame: "decimal",
  score: "percent",
};
/** Accuracy is stored 0-100; the percent formatter wants 0-1. */
const SCALE: Record<StatUnit, number> = { accuracy: 100, perGame: 1, score: 1 };
const fin = (v: number) => Number.isFinite(v);

export const gamesWord = (n: number) => (n === 1 ? "game" : "games");
const count = new Intl.NumberFormat("en-GB");
/** "38 analysed games", "1 rapid game". */
export const gamesText = (n: number, kind = "") =>
  `${count.format(Math.max(0, Math.trunc(n)))} ${kind ? `${kind} ` : ""}${gamesWord(n)}`;

/** "64.2%", "1.23 a game", "58.3%"; a dash when there is no estimate. */
export function statValue(e: Estimate, unit: StatUnit): string {
  if (e.kind !== "estimate" || !fin(e.value)) return DASH;
  const v = e.value / SCALE[unit];
  const text = formatAt(v, FORMAT[unit], unit === "perGame" ? 2 : 1);
  return unit === "perGame" ? `${text} a game` : text;
}

/** "Need 6 more games" below the minimum. */
export const needGames = (needed: number) =>
  `Need ${count.format(needed)} more ${gamesWord(needed)}`;

/** "80% range 61.0% to 67.5%", or why there is none. */
export function statRange(e: Estimate, unit: StatUnit): string {
  if (e.kind !== "estimate") return needGames(e.needed);
  if (!e.interval || !fin(e.interval.low) || !fin(e.interval.high)) {
    return "Every game gave the same value, so there is no range";
  }
  const k = SCALE[unit];
  const f = FORMAT[unit];
  const [low, high] = bandEnds(
    { low: e.interval.low / k, high: e.interval.high / k, level: 0.8 },
    f,
  );
  return `${Math.round(e.interval.level * 100)}% range ${bound(low, f)} to ${bound(high, f)}`;
}

const signed = (text: string, v: number) =>
  v > 0 ? `+${text}` : v < 0 ? `${MINUS}${text.replace(/^-/, "")}` : text;

/** A difference with its unit: "+3.2 points" (accuracy and score), "−0.40 a game". */
export function diffValue(v: number, unit: StatUnit): string {
  if (!fin(v)) return DASH;
  if (unit === "perGame") return `${signed(v.toFixed(2), v)} a game`;
  const points = unit === "score" ? v * 100 : v;
  return `${signed(points.toFixed(1), points)} points`;
}

/** "80% range +1.0 to +5.4", ends rounded outward one place coarser. */
export function diffRange(i: Interval, unit: StatUnit): string {
  const k = unit === "score" ? 100 : 1;
  const low = Math.floor(i.low * k * 10 + 1e-9) / 10;
  const high = Math.ceil(i.high * k * 10 - 1e-9) / 10;
  return `80% range ${signed(low.toFixed(1), low)} to ${signed(high.toFixed(1), high)}`;
}
