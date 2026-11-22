import type { Db } from "@ci/db";
import { games, weaknessSeriesPoint } from "@ci/db/generated";
import { and, desc, eq, inArray, sql } from "drizzle-orm";
import { readOnly } from "../../../../server/games";
import type { CoachScope } from "../../../../server/profile";
import { ownsScope } from "../../../../server/profile/scope";

// Two narrow reads the weaknesses pages need beside T1's read model, each behind its `ownsScope`
// check, read-only and limited to the scope's own account and time class.

/** Weeks of raw weekly rate a list row's sparkline shows (the newest weeks with a point). */
export const ROW_WEEKS = 12;

const TIME_CLASS = {
  bullet: "bullet",
  blitz: "blitz",
  rapid: "rapid",
  classical: "classical",
  daily: "daily",
} as const;
const PLATFORM = { lichess: "lichess", chesscom: "chess.com" } as const;
const count = new Intl.NumberFormat("en-GB");

/** "Based on 214 analysed rapid games on lichess." */
export function basisText(scope: CoachScope): string {
  const o = scope.options.find(
    (x) => x.accountId === scope.accountId && x.timeClass === scope.timeClass,
  );
  const n = Math.max(0, Math.trunc(o?.analysedGames ?? 0));
  const games = `${count.format(n)} analysed ${TIME_CLASS[scope.timeClass]} ${n === 1 ? "game" : "games"}`;
  return `Based on ${games} on ${PLATFORM[scope.platform]}.`;
}

/** The scope's URL parameters, so a link opens the same games coached. */
export function scopeQuery(scope: CoachScope): string {
  return new URLSearchParams({ account: scope.accountId, tc: scope.timeClass }).toString();
}

export function weaknessHref(seriesKey: string, scope: CoachScope): string {
  return `/app/weaknesses/${encodeURIComponent(seriesKey)}?${scopeQuery(scope)}`;
}

/**
 * Each series' raw weekly rate (events over chances per ISO week, UTC), oldest first, over the
 * newest `ROW_WEEKS` weeks with a point. A week with no chances is null: a gap, never a zero.
 */
export async function weeklyRates(
  db: Db,
  userId: string,
  scope: CoachScope,
): Promise<Map<string, (number | null)[]>> {
  return readOnly(db, async (tx) => {
    const out = new Map<string, (number | null)[]>();
    if (!(await ownsScope(tx, userId, scope))) return out;
    const week = sql<string>`to_char(${weaknessSeriesPoint.playedAt} at time zone 'UTC', 'IYYY-"W"IW')`;
    const rows = await tx
      .select({
        seriesKey: weaknessSeriesPoint.seriesKey,
        week,
        events: sql<number>`sum(${weaknessSeriesPoint.events})::int`,
        chances: sql<number>`sum(${weaknessSeriesPoint.opportunities})::int`,
      })
      .from(weaknessSeriesPoint)
      .where(
        and(
          eq(weaknessSeriesPoint.accountId, scope.accountId),
          eq(weaknessSeriesPoint.timeClass, scope.timeClass),
        ),
      )
      .groupBy(weaknessSeriesPoint.seriesKey, week)
      .orderBy(weaknessSeriesPoint.seriesKey, desc(week));
    for (const r of rows) {
      const list = out.get(r.seriesKey) ?? [];
      if (list.length >= ROW_WEEKS) continue;
      list.push(r.chances > 0 ? r.events / r.chances : null);
      out.set(r.seriesKey, list);
    }
    for (const list of out.values()) list.reverse();
    return out;
  });
}

/**
 * The review page id (`games.id`) of each of the scope's games named by its platform id, the id
 * report inputs carry (ADR-R2-2 R-7). An id that is not one of the scope's games is left out.
 */
export async function reviewGameIds(
  db: Db,
  userId: string,
  scope: CoachScope,
  platformIds: string[],
): Promise<Map<string, string>> {
  const ids = [...new Set(platformIds.filter((s) => s.length > 0 && s.length <= 200))];
  if (ids.length === 0) return new Map();
  return readOnly(db, async (tx) => {
    if (!(await ownsScope(tx, userId, scope))) return new Map();
    const rows = await tx
      .select({ id: games.id, platformGameId: games.platformGameId })
      .from(games)
      .where(
        and(
          eq(games.accountId, scope.accountId),
          eq(games.platform, scope.platform),
          inArray(games.platformGameId, ids),
        ),
      );
    return new Map(rows.map((r) => [r.platformGameId, r.id]));
  });
}
