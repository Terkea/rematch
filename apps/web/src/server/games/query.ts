import { TIME_CLASS_ORDER as TIME_CLASSES } from "../../lib/time-class";
import { OTHER_FORMAT, parseFormatKey } from "../../lib/format-key";
import type { GameFilters, GameListQuery, GameSortKey, TimeClass } from "./types";

type Params = Record<string, string | string[] | undefined>;

/** No filter set: every game of every linked account. */
export const NO_FILTERS: GameFilters = {
  timeClass: null,
  format: null,
  result: null,
  colour: null,
  from: null,
  to: null,
  accountId: null,
};
type Sort = GameListQuery["sort"];

const RESULTS = ["win", "draw", "loss"] as const;
const COLOURS = ["white", "black"] as const;
const DEFAULT_SORT = "played-desc";
/** A Map, so `constructor`, `__proto__` and other prototype keys are never matched. */
const SORTS: ReadonlyMap<string, Sort> = new Map(
  (
    [
      ["played", "desc"],
      ["played", "asc"],
      ["accuracy", "desc"],
      ["accuracy", "asc"],
      ["opponent", "desc"],
      ["blunders", "desc"],
    ] as const satisfies readonly (readonly [GameSortKey, "asc" | "desc"])[]
  ).map(([key, dir]) => [`${key}-${dir}`, { key, dir }]),
);
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const DAY_RE = /^(\d{4})-(\d{2})-(\d{2})$/;
const PAGE_RE = /^[1-9]\d{0,8}$/;
const MIN_YEAR = 2000;
const MAX_YEAR = 2100;

/** A single value; a repeated param is treated as invalid. */
function one(params: Params, key: string): string | null {
  const v = Object.hasOwn(params, key) ? params[key] : undefined;
  return typeof v === "string" ? v : null;
}

function pick<T extends string>(value: unknown, allowed: readonly T[]): T | null {
  return typeof value === "string" && (allowed as readonly string[]).includes(value)
    ? (value as T)
    : null;
}

/** A real calendar day as `YYYY-MM-DD` between 2000 and 2100, or null. */
function day(value: unknown): string | null {
  const m = typeof value === "string" ? DAY_RE.exec(value) : null;
  if (!m) return null;
  const year = Number(m[1]);
  if (year < MIN_YEAR || year > MAX_YEAR) return null;
  const d = new Date(Date.UTC(year, Number(m[2]) - 1, Number(m[3])));
  return d.toISOString().slice(0, 10) === value ? value : null;
}

function uuid(value: unknown): string | null {
  return typeof value === "string" && UUID_RE.test(value) ? value.toLowerCase() : null;
}

/** A format key normalised ("0600+00" is "600+0"), or "other"; null for anything else. */
function format(value: unknown): string | null {
  if (value === OTHER_FORMAT) return OTHER_FORMAT;
  const f = typeof value === "string" ? parseFormatKey(value) : null;
  return f ? `${f.base}+${f.increment}` : null;
}

/** `tc=all`: every time control, when a default time control would otherwise apply (I8). */
const ALL_TIME_CLASSES = "all";

/**
 * The explorer's URL state (ADR 0048). Every invalid param falls back to its default; a `from`
 * later than `to` is swapped. The page is not clamped here: `listGames` clamps it to the last page.
 * `defaultTimeClass` is the time control a URL without `tc` shows (I8); `tc=all` then asks for
 * every time control.
 */
export function parseGameQuery(params: Params, defaultTimeClass?: TimeClass | null): GameListQuery {
  const tc = one(params, "tc");
  let from = day(one(params, "from"));
  let to = day(one(params, "to"));
  if (from !== null && to !== null && from > to) [from, to] = [to, from];
  const timeClass =
    tc === ALL_TIME_CLASSES ? null : (pick(tc, TIME_CLASSES) ?? defaultTimeClass ?? null);
  const filters: GameFilters = {
    timeClass,
    // A format belongs to one timed class (Q2); daily and "All" have none.
    format: timeClass && timeClass !== "daily" ? format(one(params, "fmt")) : null,
    result: pick(one(params, "result"), RESULTS),
    // `color` is the shared filter contract's name (ADR 0083); `colour` is read as its alias.
    colour: pick(one(params, "color"), COLOURS) ?? pick(one(params, "colour"), COLOURS),
    from,
    to,
    accountId: uuid(one(params, "account")),
  };
  const sort = SORTS.get(one(params, "sort") ?? DEFAULT_SORT) ?? SORTS.get(DEFAULT_SORT);
  const page = one(params, "page");
  return {
    filters,
    sort: { key: sort?.key ?? "played", dir: sort?.dir ?? "desc" },
    page: page !== null && PAGE_RE.test(page) ? Number(page) : 1,
    ...(defaultTimeClass ? { defaultTimeClass } : {}),
  };
}

/**
 * The inverse of `parseGameQuery`: only valid values, defaults omitted, in a fixed key order
 * (tc, fmt, result, color, from, to, account, sort, page).
 */
export function serializeGameQuery(q: GameListQuery): URLSearchParams {
  const out = new URLSearchParams();
  const f = q.filters;
  const set = (key: string, value: string | null) => {
    if (value !== null) out.set(key, value);
  };
  const tc = pick(f.timeClass, TIME_CLASSES);
  const fallback = pick(q.defaultTimeClass, TIME_CLASSES);
  if (tc !== fallback) set("tc", tc ?? ALL_TIME_CLASSES);
  if (tc && tc !== "daily") set("fmt", format(f.format));
  set("result", pick(f.result, RESULTS));
  set("color", pick(f.colour, COLOURS));
  set("from", day(f.from));
  set("to", day(f.to));
  set("account", uuid(f.accountId));
  const sort = `${q.sort.key}-${q.sort.dir}`;
  if (sort !== DEFAULT_SORT && SORTS.has(sort)) out.set("sort", sort);
  if (Number.isSafeInteger(q.page) && q.page > 1) out.set("page", String(q.page));
  return out;
}

/**
 * The games page's old `colour` param, moved to `color` (T3): the URL to redirect to, with every
 * other param kept in order, or null when there is no `colour`. An invalid colour is dropped.
 */
export function colourRedirect(path: string, params: Params): string | null {
  if (!Object.hasOwn(params, "colour")) return null;
  const out = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined) continue;
    if (key === "colour") {
      const c = pick(typeof value === "string" ? value : null, COLOURS);
      if (c && !Object.hasOwn(params, "color")) out.set("color", c);
      continue;
    }
    for (const v of [value].flat()) out.append(key, v);
  }
  const q = out.toString();
  return q ? `${path}?${q}` : path;
}
