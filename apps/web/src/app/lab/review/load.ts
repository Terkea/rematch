import { type Db, games, moves } from "@ci/db";
import { and, desc, eq, inArray, isNotNull, sql } from "drizzle-orm";
import { notFound } from "next/navigation";
import { requireUser } from "../../../server/auth/session";
import { getDb } from "../../../server/db";
import { readOnly } from "../../../server/games/read-only";
import { getGameReview, type ReviewGame } from "../../../server/games/review";
import { scopedAccountIds } from "../../../server/games/scope";
import { labEnabled } from "../lab-enabled";

/** Candidates tried in order when no game is given: the first that passes the analysis guard wins. */
const CANDIDATES = 25;

/**
 * The review lab's game: the given one, or else the user's latest analysed game with the most
 * moments (the player's judged moves). Scoped exactly like `getGameReview`; null when nothing fits.
 */
export async function findLabReview(
  db: Db,
  userId: string,
  gameId?: string,
): Promise<ReviewGame | null> {
  if (gameId) return getGameReview(db, userId, gameId);
  const ids = await readOnly(db, async (tx) => {
    const accounts = await scopedAccountIds(tx, userId);
    if (accounts.length === 0) return [];
    const judged = sql<number>`count(${moves.ply})`;
    const rows = await tx
      .select({ id: games.id })
      .from(games)
      .leftJoin(
        moves,
        and(
          eq(moves.gameId, games.id),
          eq(moves.color, games.playerColor),
          isNotNull(moves.judgement),
        ),
      )
      .where(and(inArray(games.accountId, accounts), eq(games.analysisStatus, "done")))
      .groupBy(games.id)
      .orderBy(desc(judged), desc(games.playedAt), games.id)
      .limit(CANDIDATES);
    return rows.map((r) => r.id);
  });
  for (const id of ids) {
    const review = await getGameReview(db, userId, id);
    if (review?.analysed) return review;
  }
  return null;
}

/** For the lab pages: the signed-in user's game, or the not-found page. */
export async function loadLabReview(gameId?: string): Promise<ReviewGame> {
  // Not only the layout: Next renders layout and page together, so no auth or read runs in production.
  if (!labEnabled(process.env)) notFound();
  const user = await requireUser();
  const review = await findLabReview(getDb(), user.id, gameId);
  if (!review) notFound();
  return review;
}
