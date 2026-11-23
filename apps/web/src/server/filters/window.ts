import { type Db, games } from "@ci/db";
import { and, eq, gte, inArray, lt, type SQL, sql } from "drizzle-orm";
import type { TimeClass } from "../games/types";
import type { DayRange } from "./params";

// How many games a date range holds, for the filter bar's thin-window note (ADR 0083
// Amendment 2): the default 90 days with fewer than `KPI_MIN_GAMES` games say so and offer all
// time. Every game played counts, analysed or not, so the note's number is the plain count.

const startOf = (day: string) => new Date(`${day}T00:00:00Z`);
const dayAfter = (day: string) => new Date(startOf(day).getTime() + 86_400_000);

/** The accounts' games of `timeClass` (every class when null) played inside `range`. */
export async function gamesInRange(
  db: Db,
  accountIds: readonly string[],
  timeClass: TimeClass | null,
  range: DayRange,
): Promise<number> {
  if (accountIds.length === 0) return 0;
  const conds: SQL[] = [inArray(games.accountId, [...accountIds])];
  if (timeClass) conds.push(eq(games.timeClass, timeClass));
  if (range.from) conds.push(gte(games.playedAt, startOf(range.from)));
  if (range.to) conds.push(lt(games.playedAt, dayAfter(range.to)));
  const [row] = await db
    .select({ n: sql<number>`count(*)::int` })
    .from(games)
    .where(and(...conds));
  return row?.n ?? 0;
}
