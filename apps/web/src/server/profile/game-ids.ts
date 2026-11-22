import type { Db } from "@ci/db";
import { games } from "@ci/db/generated";
import { and, eq, inArray } from "drizzle-orm";
import { readOnly, scopedAccountIds } from "../games";

// Coaching data (report inputs, chat segments and anchors) names a game by its platform id
// (ADR-R2-2 R-7); the review page takes `games.id`. Shared by the weaknesses pages (T7) and the
// coach chat (T9).

/**
 * The review page id (`games.id`) of each of the account's games named by its platform id. An
 * id that is not one of that account's games is left out, and an account the user is not linked
 * to maps nothing.
 */
export async function reviewGameIds(
  db: Db,
  userId: string,
  scope: { accountId: string },
  platformIds: readonly string[],
): Promise<Map<string, string>> {
  const ids = [...new Set(platformIds.filter((s) => s.length > 0 && s.length <= 200))];
  if (ids.length === 0) return new Map();
  return readOnly(db, async (tx) => {
    if ((await scopedAccountIds(tx, userId, scope.accountId)).length === 0) return new Map();
    const rows = await tx
      .select({ id: games.id, platformGameId: games.platformGameId })
      .from(games)
      .where(and(eq(games.accountId, scope.accountId), inArray(games.platformGameId, ids)));
    return new Map(rows.map((r) => [r.platformGameId, r.id]));
  });
}
