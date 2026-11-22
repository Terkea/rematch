import { type Db, games } from "@ci/db";
import { and, eq, inArray, type SQL, sql } from "drizzle-orm";
import type { TimeClass } from "../games/types";

// Time-control formats ("10+0", "3+2") inside a time class, built from `games.base_seconds` and
// `games.increment_seconds`. The URL key is the raw seconds ("600+0"); the label is minutes. A
// class lists its 5 most played formats, then "Other" for the rest (including games whose base
// is unknown). Daily has no formats.

/** The `fmt` value of every game outside a class's top formats. */
export const OTHER_FORMAT = "other";
/** Formats listed by name per time class; the rest are "Other". */
export const TOP_FORMATS = 5;

const KEY_RE = /^(\d{1,6})\+(\d{1,4})$/;

/** `600+0` for 600 s and no increment (a missing increment is 0); null without a base. */
export function formatKey(base: number | null, increment: number | null): string | null {
  if (base == null || !Number.isSafeInteger(base) || base < 0) return null;
  const inc = increment != null && Number.isSafeInteger(increment) && increment > 0 ? increment : 0;
  return `${base}+${inc}`;
}

/** `{ base, increment }` of a valid key, normalised (`0600+00` is `600+0`); null otherwise. */
export function parseFormatKey(key: string): { base: number; increment: number } | null {
  const m = KEY_RE.exec(key);
  if (!m) return null;
  return { base: Number(m[1]), increment: Number(m[2]) };
}

/** A base in seconds as players say it: 600 is "10", 90 is "1.5", 30 is "30s". */
function baseLabel(seconds: number): string {
  if (seconds % 60 === 0) return String(seconds / 60);
  if (seconds < 60) return `${seconds}s`;
  return String(Math.round((seconds / 60) * 10) / 10);
}

/** "10+0" for `600+0`, "Other" for `other`; an invalid key is returned as is. */
export function formatLabel(key: string): string {
  if (key === OTHER_FORMAT) return "Other";
  const f = parseFormatKey(key);
  return f ? `${baseLabel(f.base)}+${f.increment}` : key;
}

export type FormatCount = { base: number | null; increment: number | null; games: number };
export type FormatOption = { key: string; label: string; games: number };

/**
 * The class's format options: the `TOP_FORMATS` most played (ties by key), then "Other" holding
 * every other game when there is any. Daily, or a class with no known format, has none.
 */
export function topFormats(counts: readonly FormatCount[], timeClass: TimeClass): FormatOption[] {
  if (timeClass === "daily") return [];
  const byKey = new Map<string, number>();
  let unknown = 0;
  for (const c of counts) {
    const games = Number.isFinite(c.games) && c.games > 0 ? c.games : 0;
    const key = formatKey(c.base, c.increment);
    if (key === null) unknown += games;
    else byKey.set(key, (byKey.get(key) ?? 0) + games);
  }
  const ranked = [...byKey.entries()]
    .filter(([, games]) => games > 0)
    .sort(([ka, a], [kb, b]) => b - a || (ka < kb ? -1 : ka > kb ? 1 : 0));
  if (ranked.length === 0) return [];
  const top = ranked.slice(0, TOP_FORMATS);
  const rest = ranked.slice(TOP_FORMATS).reduce((sum, [, games]) => sum + games, unknown);
  const out = top.map(([key, games]) => ({ key, label: formatLabel(key), games }));
  if (rest > 0) out.push({ key: OTHER_FORMAT, label: formatLabel(OTHER_FORMAT), games: rest });
  return out;
}

/** The format options of `accountIds`' games in `timeClass` (one grouped read). */
export async function listFormats(
  db: Db,
  accountIds: readonly string[],
  timeClass: TimeClass,
): Promise<FormatOption[]> {
  if (timeClass === "daily" || accountIds.length === 0) return [];
  const rows = await db
    .select({
      base: games.baseSeconds,
      increment: games.incrementSeconds,
      games: sql<number>`count(*)::int`,
    })
    .from(games)
    .where(and(inArray(games.accountId, [...accountIds]), eq(games.timeClass, timeClass)))
    .groupBy(games.baseSeconds, games.incrementSeconds);
  return topFormats(rows, timeClass);
}

const sameFormat = (f: { base: number; increment: number }): SQL =>
  sql`(${games.baseSeconds} = ${f.base} and coalesce(${games.incrementSeconds}, 0) = ${f.increment})`;

/**
 * The SQL condition for `fmt` given the class's options: one format, or (for "Other") every game
 * not in a named option. Null when `fmt` is null; an invalid key matches nothing.
 */
export function formatCondition(fmt: string | null, options: readonly FormatOption[]): SQL | null {
  if (fmt === null) return null;
  if (fmt === OTHER_FORMAT) {
    // No "Other" option: every game has a named format, so Other is empty.
    if (!options.some((o) => o.key === OTHER_FORMAT)) return sql`false`;
    const named = options
      .map((o) => parseFormatKey(o.key))
      .filter((f): f is { base: number; increment: number } => f !== null);
    if (named.length === 0) return null;
    return sql`not coalesce(${sql.join(named.map(sameFormat), sql` or `)}, false)`;
  }
  const f = parseFormatKey(fmt);
  return f ? sameFormat(f) : sql`false`;
}
