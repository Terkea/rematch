import type { Db } from "@ci/db";
import { games } from "@ci/db/generated";
import { and, eq, inArray } from "drizzle-orm";
import { readOnly } from "../games";
import { ownsScope } from "./scope";
import type { CoachScope } from "./types";

/**
 * The review page id (`games.id`) of each of the scope's games named by its platform id, the id
 * report inputs and model segments carry (ADR-R2-2 R-7). Behind `ownsScope`; an id that is not
 * one of the scope's games is left out, so it never becomes a link. (The same read as Task 7's
 * `reviewGameIds`; the coordinator dedupes them at merge.)
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
