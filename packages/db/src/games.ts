import { and, eq } from "drizzle-orm";
import type { Db } from "./client.js";
import { games } from "./schema.js";

export type NewGame = typeof games.$inferInsert;
export type AnalysisStatus = (typeof games.$inferSelect)["analysisStatus"];

/**
 * Inserts a game, or returns the existing one for the same (platform, platformGameId).
 * `inserted` is true only when this call created the row; `analysisStatus` is the row's current
 * status, so a re-import can re-enqueue games that are still pending.
 */
export async function upsertGame(
  db: Db,
  row: NewGame,
): Promise<{ id: string; inserted: boolean; analysisStatus: AnalysisStatus }> {
  const [created] = await db
    .insert(games)
    .values(row)
    .onConflictDoNothing({ target: [games.platform, games.platformGameId] })
    .returning({ id: games.id, analysisStatus: games.analysisStatus });
  if (created) return { ...created, inserted: true };

  const [existing] = await db
    .select({ id: games.id, analysisStatus: games.analysisStatus })
    .from(games)
    .where(and(eq(games.platform, row.platform), eq(games.platformGameId, row.platformGameId)));
  if (!existing) throw new Error("upsertGame: conflicting row vanished");
  return { ...existing, inserted: false };
}
