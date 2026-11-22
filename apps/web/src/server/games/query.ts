import type { GameFilters, GameListQuery, GameSortKey, TimeClass } from "./types";

type Params = Record<string, string | string[] | undefined>;

const TIME_CLASSES: readonly TimeClass[] = ["bullet", "blitz", "rapid", "classical", "daily"];
const RESULTS = ["win", "draw", "loss"] as const;
const COLOURS = ["white", "black"] as const;
const SORTS: Record<string, { key: GameSortKey; dir: "asc" | "desc" }> = {
  "played-desc": { key: "played", dir: "desc" },
  "played-asc": { key: "played", dir: "asc" },
  "accuracy-desc": { key: "accuracy", dir: "desc" },
  "accuracy-asc": { key: "accuracy", dir: "asc" },
  "opponent-desc": { key: "opponent", dir: "desc" },
  "blunders-desc": { key: "blunders", dir: "desc" },
};
const DEFAULT_SORT = "played-desc";
const DEFAULT_SORT_VALUE = { key: "played", dir: "desc" } as const;
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const DAY_RE = /^(\d{4})-(\d{2})-(\d{2})$/;
const PAGE_RE = /^[1-9]\d{0,8}$/;

/** A single value; a repeated param is treated as invalid. */
function one(params: Params, key: string): string | null {
  const v = params[key];
  return typeof v === "string" ? v : null;
}

function pick<T extends string>(value: string | null, allowed: readonly T[]): T | null {
  return value !== null && (allowed as readonly string[]).includes(value) ? (value as T) : null;
}

/** A real calendar day as `YYYY-MM-DD`, or null. */
function day(value: string | null): string | null {
  const m = value === null ? null : DAY_RE.exec(value);
  if (!m) return null;
  const d = new Date(Date.UTC(Number(m[1]), Number(m[2]) - 1, Number(m[3])));
  return d.toISOString().slice(0, 10) === value ? value : null;
}

/**
 * The explorer's URL state (ADR 0048). Every invalid param falls back to its default; a `from`
 * later than `to` is swapped. The page is not clamped here: `listGames` clamps it to the last page.
 */
export function parseGameQuery(params: Params): GameListQuery {
  let from = day(one(params, "from"));
  let to = day(one(params, "to"));
  if (from !== null && to !== null && from > to) [from, to] = [to, from];
  const account = one(params, "account");
  const filters: GameFilters = {
    timeClass: pick(one(params, "tc"), TIME_CLASSES),
    result: pick(one(params, "result"), RESULTS),
    colour: pick(one(params, "colour"), COLOURS),
    from,
    to,
    accountId: account !== null && UUID_RE.test(account) ? account.toLowerCase() : null,
  };
  const sortParam = one(params, "sort");
  const sort = (sortParam !== null && SORTS[sortParam]) || DEFAULT_SORT_VALUE;
  const page = one(params, "page");
  return {
    filters,
    sort: { key: sort.key, dir: sort.dir },
    page: page !== null && PAGE_RE.test(page) ? Number(page) : 1,
  };
}

/** The inverse of `parseGameQuery`, omitting defaults, in a fixed key order. */
export function serializeGameQuery(q: GameListQuery): URLSearchParams {
  const out = new URLSearchParams();
  const f = q.filters;
  if (f.timeClass) out.set("tc", f.timeClass);
  if (f.result) out.set("result", f.result);
  if (f.colour) out.set("colour", f.colour);
  if (f.from) out.set("from", f.from);
  if (f.to) out.set("to", f.to);
  if (f.accountId) out.set("account", f.accountId);
  const sort = `${q.sort.key}-${q.sort.dir}`;
  if (sort !== DEFAULT_SORT && sort in SORTS) out.set("sort", sort);
  if (q.page > 1) out.set("page", String(q.page));
  return out;
}
