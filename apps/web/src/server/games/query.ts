import type { GameFilters, GameListQuery, GameSortKey, TimeClass } from "./types";

type Params = Record<string, string | string[] | undefined>;
type Sort = GameListQuery["sort"];

const TIME_CLASSES: readonly TimeClass[] = ["bullet", "blitz", "rapid", "classical", "daily"];
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

/**
 * The explorer's URL state (ADR 0048). Every invalid param falls back to its default; a `from`
 * later than `to` is swapped. The page is not clamped here: `listGames` clamps it to the last page.
 */
export function parseGameQuery(params: Params): GameListQuery {
  let from = day(one(params, "from"));
  let to = day(one(params, "to"));
  if (from !== null && to !== null && from > to) [from, to] = [to, from];
  const filters: GameFilters = {
    timeClass: pick(one(params, "tc"), TIME_CLASSES),
    result: pick(one(params, "result"), RESULTS),
    colour: pick(one(params, "colour"), COLOURS),
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
  };
}

/**
 * The inverse of `parseGameQuery`: only valid values, defaults omitted, in a fixed key order
 * (tc, result, colour, from, to, account, sort, page).
 */
export function serializeGameQuery(q: GameListQuery): URLSearchParams {
  const out = new URLSearchParams();
  const f = q.filters;
  const set = (key: string, value: string | null) => {
    if (value !== null) out.set(key, value);
  };
  set("tc", pick(f.timeClass, TIME_CLASSES));
  set("result", pick(f.result, RESULTS));
  set("colour", pick(f.colour, COLOURS));
  set("from", day(f.from));
  set("to", day(f.to));
  set("account", uuid(f.accountId));
  const sort = `${q.sort.key}-${q.sort.dir}`;
  if (sort !== DEFAULT_SORT && SORTS.has(sort)) out.set("sort", sort);
  if (Number.isSafeInteger(q.page) && q.page > 1) out.set("page", String(q.page));
  return out;
}
