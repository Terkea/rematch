import { and, eq } from "drizzle-orm";
import type { Db } from "./client.js";
import { games } from "./schema.js";

export type NewGame = typeof games.$inferInsert;

/**
 * Inserts a game, or returns the existing one for the same (platform, platformGameId).
 * `inserted` is true only when this call created the row.
 */
export async function upsertGame(db: Db, row: NewGame): Promise<{ id: string; inserted: boolean }> {
  const [created] = await db
    .insert(games)
    .values(row)
    .onConflictDoNothing({ target: [games.platform, games.platformGameId] })
    .returning({ id: games.id });
  if (created) return { id: created.id, inserted: true };

  const [existing] = await db
    .select({ id: games.id })
    .from(games)
    .where(and(eq(games.platform, row.platform), eq(games.platformGameId, row.platformGameId)));
  if (!existing) throw new Error("upsertGame: conflicting row vanished");
  return { id: existing.id, inserted: false };
}
