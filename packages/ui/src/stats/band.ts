// The KPI copy helpers as plain functions, with no "use client": a server component may call them
// (Task 8: `bandText` from the "use client" kpi-stat module returned a 500 in a server render).

export type StatFormat = "percent" | "decimal" | "integer";

/** Stats render in one locale on server and client, so hydration never disagrees. */
export const DEFAULT_LOCALE = "en-US";

/**
 * The one statistics convention (ADR 0047 R3, ADR-R2-5, ADR-WEB-19): below `KPI_MIN_GAMES` a KPI
 * shows no number, only its basis and the shortfall; every interval is an 80% one.
 */
export const KPI_MIN_GAMES = 10;
export const INTERVAL_LEVEL = 0.8;

export type BandInterval = { low: number; high: number; level: typeof INTERVAL_LEVEL };

/** "Need 4 more games" (one game is singular). */
export const needText = (games: number) => {
  const needed = Math.max(0, KPI_MIN_GAMES - games);
  return `Need ${needed} more ${needed === 1 ? "game" : "games"}`;
};
export type Basis = { games: number; timeClass: string };

/**
 * Interval ends: one step coarser than the value for decimals (0.1 against 0.01), and the value's
 * own step for percent (0.1%) and integer (1). `digits` is the fraction digits shown.
 */
export const STEP: Record<StatFormat, { factor: number; digits: number }> = {
  percent: { factor: 1000, digits: 1 },
  decimal: { factor: 10, digits: 1 },
  integer: { factor: 1, digits: 0 },
};

export const formatAt = (v: number, format: StatFormat, digits: number) =>
  new Intl.NumberFormat(DEFAULT_LOCALE, {
    ...(format === "percent" ? { style: "percent" } : {}),
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(v);

/** A value at the interval ends' precision. */
export const bound = (v: number, format: StatFormat) => formatAt(v, format, STEP[format].digits);

/**
 * The interval's ends rounded outward at the shown precision: low floored, high ceiled, so the
 * stated range is never narrower than the computed one. The epsilon absorbs float noise
 * (0.766 × 1000 is 766.0000000000001, which must not ceil to 767).
 */
export function bandEnds(interval: BandInterval, format: StatFormat): [number, number] {
  const f = STEP[format].factor;
  return [Math.floor(interval.low * f + 1e-9) / f, Math.ceil(interval.high * f - 1e-9) / f];
}

/** "Based on 38 rapid games" (one game is singular). */
export const basisText = ({ games, timeClass }: Basis) =>
  `Based on ${games} ${timeClass} game${games === 1 ? "" : "s"}`;

/** "80% likely between 61.0 and 67.5", with the ends rounded outward. */
export const bandText = (interval: BandInterval, format: StatFormat) => {
  const [low, high] = bandEnds(interval, format);
  return `${Math.round(interval.level * 100)}% likely between ${bound(low, format)} and ${bound(high, format)}`;
};

/** The target at the ends' precision, or one place finer when it would read the same as an end. */
export function targetText(target: number, interval: BandInterval, format: StatFormat): string {
  const ends = bandEnds(interval, format).map((v) => bound(v, format));
  const text = bound(target, format);
  return ends.includes(text) ? formatAt(target, format, STEP[format].digits + 1) : text;
}
