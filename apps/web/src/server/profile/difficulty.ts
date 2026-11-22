import type { Db } from "@ci/db";
import { games, moveFeatures } from "@ci/db/generated";
import { and, eq, isNotNull, sql } from "drizzle-orm";
import { readOnly } from "../games";
import { ownsScope } from "./scope";
import type { CoachScope, DifficultyStatus } from "./types";

/**
 * How many of the scope's assessed positions have a Maia difficulty (ADR-R2-13). The web never
 * estimates difficulty: `pending_maia` positions are only counted, and stated as waiting.
 */
export async function difficultyStatus(
  db: Db,
  userId: string,
  scope: CoachScope,
): Promise<DifficultyStatus> {
  return readOnly(db, async (tx) => {
    const out: DifficultyStatus = { positions: 0, pendingMaia: 0, ok: 0, notApplicable: 0 };
    if (!(await ownsScope(tx, userId, scope))) return out;
    const rows = await tx
      .select({ status: moveFeatures.difficultyStatus, n: sql<number>`count(*)::int` })
      .from(moveFeatures)
      .innerJoin(games, eq(games.id, moveFeatures.gameId))
      .where(
        and(
          eq(games.accountId, scope.accountId),
          eq(games.timeClass, scope.timeClass),
          isNotNull(moveFeatures.difficultyStatus),
        ),
      )
      .groupBy(moveFeatures.difficultyStatus);
    for (const r of rows) {
      if (r.status === "ok") out.ok += r.n;
      else if (r.status === "pending_maia") out.pendingMaia += r.n;
      else if (r.status === "not_applicable") out.notApplicable += r.n;
      else continue;
      out.positions += r.n;
    }
    return out;
  });
}
