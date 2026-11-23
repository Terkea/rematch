import type { DayRange, StatsFilters } from "./params";

// Date presets and comparison periods. A preset is a shortcut for a `from`/`to` pair (the URL
// keeps the days, so a link always shows the same games); `presetOf` names the pair again. All
// days are UTC, and ranges include both ends.

export const PRESETS = ["30d", "90d", "6m", "12m", "all", "custom"] as const;
export type Preset = (typeof PRESETS)[number];
export type FixedPreset = Exclude<Preset, "custom">;

/** The range a URL without dates shows on the statistics pages (ADR 0083 Amendment 2). */
export const DEFAULT_PRESET = "90d" satisfies FixedPreset;

/** Games of a comparison side: a day range (open ends allowed) or the newest games by count. */
export type Period =
  | { kind: "days"; from: string | null; to: string | null }
  | { kind: "games"; skip: number; take: number };

/** Games per side of `cmp=50g`. */
export const COMPARE_GAMES = 50;
/** Days per side of `cmp=30d`. */
export const COMPARE_DAYS = 30;

const DAY_MS = 86_400_000;

/** The UTC day of `now`, `YYYY-MM-DD`. */
export function utcDay(now: Date): string {
  return now.toISOString().slice(0, 10);
}

function addDays(day: string, days: number): string {
  return utcDay(new Date(Date.parse(`${day}T00:00:00Z`) + days * DAY_MS));
}

/** `day` moved by `months` calendar months, clamped to the month's last day (Mar 31 - 1m = Feb 28). */
function addMonths(day: string, months: number): string {
  const [y, m, d] = day.split("-").map(Number) as [number, number, number];
  const first = new Date(Date.UTC(y, m - 1 + months, 1));
  const last = new Date(Date.UTC(first.getUTCFullYear(), first.getUTCMonth() + 1, 0)).getUTCDate();
  first.setUTCDate(Math.min(d, last));
  return utcDay(first);
}

/**
 * The days of a preset ending today: "30d" is today and the 29 days before it, "90d" today and
 * the 89 before; "6m" and "12m" start the day after the same date N months ago; "all" is
 * unbounded.
 */
export function presetRange(preset: FixedPreset, now: Date): DayRange {
  const today = utcDay(now);
  switch (preset) {
    case "all":
      return { from: null, to: null };
    case "30d":
      return { from: addDays(today, -29), to: today };
    case "90d":
      return { from: addDays(today, -89), to: today };
    case "6m":
      return { from: addDays(addMonths(today, -6), 1), to: today };
    case "12m":
      return { from: addDays(addMonths(today, -12), 1), to: today };
  }
}

/** The preset a range is, or "custom". */
export function presetOf(range: DayRange, now: Date): Preset {
  for (const p of PRESETS) {
    if (p === "custom") continue;
    const r = presetRange(p, now);
    if (r.from === range.from && r.to === range.to) return p;
  }
  return "custom";
}

/** `filters` with the preset's range (a "custom" preset keeps the current range). */
export function applyPreset(filters: StatsFilters, preset: Preset, now: Date): StatsFilters {
  if (preset === "custom") return filters;
  return { ...filters, ...presetRange(preset, now) };
}

/**
 * The two sides of `filters.cmp`, A the earlier and B the later, or null without a comparison.
 * - month: B is this calendar month up to today, A the whole month before;
 * - 30d: B is the last 30 days, A the 30 before;
 * - 50g: B is the newest 50 games, A the 50 before them;
 * - custom: the `a_*` and `b_*` ranges.
 * The periods replace `from`/`to`; the other filters (time class, format, colour) still apply.
 */
export function comparePeriods(filters: StatsFilters, now: Date): { a: Period; b: Period } | null {
  const today = utcDay(now);
  switch (filters.cmp) {
    case null:
      return null;
    case "month": {
      const first = `${today.slice(0, 7)}-01`;
      return {
        a: { kind: "days", from: addMonths(first, -1), to: addDays(first, -1) },
        b: { kind: "days", from: first, to: today },
      };
    }
    case "30d": {
      const bFrom = addDays(today, -(COMPARE_DAYS - 1));
      return {
        a: { kind: "days", from: addDays(bFrom, -COMPARE_DAYS), to: addDays(bFrom, -1) },
        b: { kind: "days", from: bFrom, to: today },
      };
    }
    case "50g":
      return {
        a: { kind: "games", skip: COMPARE_GAMES, take: COMPARE_GAMES },
        b: { kind: "games", skip: 0, take: COMPARE_GAMES },
      };
    case "custom":
      if (!filters.a || !filters.b) return null;
      return { a: { kind: "days", ...filters.a }, b: { kind: "days", ...filters.b } };
  }
}
