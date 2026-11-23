import { z } from "zod";
import { defaultTimeClass, TIME_CLASS_ORDER } from "../../lib/time-class";
import type { TimeClass } from "../games/types";
import { OTHER_FORMAT, parseFormatKey } from "./formats";
import { DEFAULT_PRESET, presetRange } from "./presets";

// The one URL search-param contract of every filtered page (T2a):
//   tc      time class; rapid first and the default (`defaultTimeClass`), omitted when default
//   fmt     a format of that class, "600+0" (shown "10+0") or "other"; never for daily
//   from,to a UTC day range, `YYYY-MM-DD`, inclusive (a reversed pair is swapped). Without
//           either, the last 90 days (`DEFAULT_PRESET`, ADR 0083 Amendment 2); `from=all` is
//           all time, an open start. One bound alone leaves the other end open.
//   color   both | white | black (default both); the games page's `colour` is read as an alias
//   cmp     month | 30d | 50g | custom: compare period A (earlier) with B (later)
//   a_from, a_to, b_from, b_to   the custom periods (only with cmp=custom)
// Every invalid or repeated value falls back to its default. `serializeFilters` is the inverse:
// only valid values, defaults omitted, keys in the order above.

export const COLORS = ["both", "white", "black"] as const;
export type Color = (typeof COLORS)[number];
export const COMPARE_MODES = ["month", "30d", "50g", "custom"] as const;
export type CompareMode = (typeof COMPARE_MODES)[number];

/** A UTC day range; a null end is open. */
export type DayRange = { from: string | null; to: string | null };

export type StatsFilters = {
  /** Null only when the player has no games in any class. */
  tc: TimeClass | null;
  fmt: string | null;
  from: string | null;
  to: string | null;
  color: Color;
  cmp: CompareMode | null;
  /** The custom periods; both set exactly when `cmp` is "custom". */
  a: DayRange | null;
  b: DayRange | null;
};

export type FilterContext = {
  /** The time classes the player has games in (the default is the first by `TIME_CLASS_ORDER`). */
  available: readonly TimeClass[];
  /** The class's format keys, when known; a `fmt` outside them is dropped. */
  formats?: readonly string[];
  /** Today, for the default range; the current time when absent. */
  now?: Date;
  /**
   * What a URL without dates shows: the last 90 days (the default), or every game (the games
   * list, which is a list rather than a statistic).
   */
  defaultPeriod?: "90d" | "all";
};

/** `from=all`: all time, where the page would otherwise show its default range. */
export const ALL_TIME = "all";

/** The range a URL without dates shows under `ctx`. */
export function defaultRange(ctx: Pick<FilterContext, "now" | "defaultPeriod">): DayRange {
  return ctx.defaultPeriod === "all"
    ? { from: null, to: null }
    : presetRange(DEFAULT_PRESET, ctx.now ?? new Date());
}

const sameRange = (a: DayRange, b: DayRange) => a.from === b.from && a.to === b.to;

/**
 * Writes `range` into `q` so the page reads it back: its days, nothing for the default (so the
 * link keeps rolling), or `from=all` for all time where the default is the last 90 days. Links
 * between pages use it, so "All time" travels.
 */
export function setRange(
  q: URLSearchParams,
  range: DayRange,
  ctx: Pick<FilterContext, "defaultPeriod" | "now"> = {},
): void {
  if (sameRange(range, defaultRange(ctx))) return;
  if (range.from === null && range.to === null) {
    if (ctx.defaultPeriod !== "all") q.set("from", ALL_TIME);
    return;
  }
  if (range.from !== null) q.set("from", range.from);
  if (range.to !== null) q.set("to", range.to);
}

export type Params = Record<string, string | string[] | undefined> | URLSearchParams;

const DAY_RE = /^(\d{4})-(\d{2})-(\d{2})$/;
const MIN_YEAR = 2000;
const MAX_YEAR = 2100;

const Day = z
  .string()
  .regex(DAY_RE)
  .refine((v) => {
    const year = Number(v.slice(0, 4));
    if (year < MIN_YEAR || year > MAX_YEAR) return false;
    const d = new Date(`${v}T00:00:00Z`);
    return !Number.isNaN(d.getTime()) && d.toISOString().slice(0, 10) === v;
  });
const Tc = z.enum(TIME_CLASS_ORDER as [TimeClass, ...TimeClass[]]);
// A `+` typed into a query string decodes as a space, so "600 0" is read as "600+0".
const Fmt = z.union([
  z.literal(OTHER_FORMAT),
  z
    .string()
    .regex(/^\d{1,6}[+ ]\d{1,4}$/)
    .transform((raw) => {
      const key = raw.replace(" ", "+");
      const f = parseFormatKey(key);
      return f ? `${f.base}+${f.increment}` : key;
    }),
]);
const ColorParam = z.enum(COLORS);
const GamesColour = z.enum(["white", "black"]);
const Cmp = z.enum(COMPARE_MODES);

/** A single value; a missing or repeated key is null. */
function one(params: Params, key: string): string | null {
  if (params instanceof URLSearchParams) {
    const all = params.getAll(key);
    return all.length === 1 ? (all[0] as string) : null;
  }
  const v = Object.hasOwn(params, key) ? params[key] : undefined;
  return typeof v === "string" ? v : null;
}

function parsed<T>(schema: z.ZodType<T>, value: string | null): T | null {
  if (value === null) return null;
  const r = schema.safeParse(value);
  return r.success ? r.data : null;
}

function range(from: string | null, to: string | null): DayRange {
  return from !== null && to !== null && from > to ? { from: to, to: from } : { from, to };
}

const hasBound = (r: DayRange) => r.from !== null || r.to !== null;

/** The filters a URL asks for, every invalid value replaced by its default. */
export function parseFilters(params: Params, ctx: FilterContext): StatsFilters {
  const fallback = defaultTimeClass(ctx.available);
  const asked = parsed(Tc, one(params, "tc"));
  const tc = asked !== null && ctx.available.includes(asked) ? asked : fallback;

  let fmt = tc === null || tc === "daily" ? null : parsed(Fmt, one(params, "fmt"));
  if (fmt !== null && ctx.formats && !ctx.formats.includes(fmt)) fmt = null;

  const rawFrom = one(params, "from");
  const days = range(parsed(Day, rawFrom), parsed(Day, one(params, "to")));
  const { from, to } =
    rawFrom === ALL_TIME || hasBound(days) ? days : defaultRange(ctx);
  const color =
    parsed(ColorParam, one(params, "color")) ??
    parsed(GamesColour, one(params, "colour")) ??
    "both";

  let cmp = parsed(Cmp, one(params, "cmp"));
  let a: DayRange | null = null;
  let b: DayRange | null = null;
  if (cmp === "custom") {
    a = range(parsed(Day, one(params, "a_from")), parsed(Day, one(params, "a_to")));
    b = range(parsed(Day, one(params, "b_from")), parsed(Day, one(params, "b_to")));
    if (!hasBound(a) || !hasBound(b)) [cmp, a, b] = [null, null, null];
  }
  return { tc, fmt, from, to, color, cmp, a, b };
}

/** The URL of `filters`: only valid values, defaults omitted, in the contract's key order. */
export function serializeFilters(f: StatsFilters, ctx: FilterContext): URLSearchParams {
  const out = new URLSearchParams();
  const set = (key: string, value: string | null) => {
    if (value !== null) out.set(key, value);
  };
  const tc = parsed(Tc, f.tc);
  const fallback = defaultTimeClass(ctx.available);
  if (tc !== null && tc !== fallback && ctx.available.includes(tc)) set("tc", tc);
  const effective = tc !== null && ctx.available.includes(tc) ? tc : fallback;
  if (effective !== null && effective !== "daily") {
    const fmt = parsed(Fmt, f.fmt);
    if (fmt !== null && (!ctx.formats || ctx.formats.includes(fmt))) set("fmt", fmt);
  }
  setRange(out, range(parsed(Day, f.from), parsed(Day, f.to)), ctx);
  const color = parsed(ColorParam, f.color);
  if (color !== null && color !== "both") set("color", color);
  const cmp = parsed(Cmp, f.cmp);
  if (cmp === "custom") {
    const a = f.a ? range(parsed(Day, f.a.from), parsed(Day, f.a.to)) : null;
    const b = f.b ? range(parsed(Day, f.b.from), parsed(Day, f.b.to)) : null;
    if (a && b && hasBound(a) && hasBound(b)) {
      set("cmp", cmp);
      set("a_from", a.from);
      set("a_to", a.to);
      set("b_from", b.from);
      set("b_to", b.to);
    }
  } else {
    set("cmp", cmp);
  }
  return out;
}

/**
 * The games page's URL for the same scope (its own contract, `parseGameQuery`): `tc`, `colour`
 * (the alias of `color`; "both" is no colour), `from` and `to`. The games page has no formats.
 */
export function gamesPageParams(f: StatsFilters, ctx: FilterContext): URLSearchParams {
  const out = new URLSearchParams();
  // The games list opens on every game, so its URL carries the days and all time is no dates.
  const canonical = serializeFilters(
    { ...f, fmt: null, cmp: null, a: null, b: null },
    { ...ctx, defaultPeriod: "all" },
  );
  const tc = canonical.get("tc");
  if (tc !== null) out.set("tc", tc);
  const colour = canonical.get("color");
  if (colour !== null) out.set("colour", colour);
  for (const key of ["from", "to"] as const) {
    const v = canonical.get(key);
    if (v !== null) out.set(key, v);
  }
  return out;
}
