import { type Db, linkedAccounts, upsertGame } from "@ci/db";
import { eq } from "drizzle-orm";
import type { PlatformClient } from "../http/platform-client.js";
import type { fetchChesscomGames } from "../importers/chesscom.js";
import type { fetchLichessGames } from "../importers/lichess.js";
import type { ImportAccountPayload } from "../queue/queues.js";

export interface ImportAccountDeps {
  db: Db;
  importers: { lichess: typeof fetchLichessGames; chesscom: typeof fetchChesscomGames };
  clients: { lichess: PlatformClient; chesscom: PlatformClient };
  lichessToken?: string;
  /** Called only after the game row is committed. */
  enqueueAnalyse(gameId: string): Promise<void>;
}

export async function handleImportAccount(
  deps: ImportAccountDeps,
  payload: ImportAccountPayload,
): Promise<{ imported: number; skipped: number }> {
  const { db } = deps;
  const [account] = await db
    .select()
    .from(linkedAccounts)
    .where(eq(linkedAccounts.id, payload.accountId));
  if (!account) throw new Error(`Linked account not found: ${payload.accountId}`);

  const username = account.username.toLowerCase();
  const max = payload.max;
  const games =
    account.platform === "lichess"
      ? deps.importers.lichess(deps.clients.lichess, account.username, {
          ...(max !== undefined ? { max } : {}),
          ...(deps.lichessToken ? { token: deps.lichessToken } : {}),
        })
      : deps.importers.chesscom(deps.clients.chesscom, account.username, {
          ...(max !== undefined ? { max } : {}),
        });

  let imported = 0;
  let skipped = 0;
  for await (const g of games) {
    const playerColor =
      g.white.name.toLowerCase() === username
        ? "white"
        : g.black.name.toLowerCase() === username
          ? "black"
          : null;
    if (playerColor === null) {
      skipped++;
      continue;
    }
    const { id, inserted } = await upsertGame(db, {
      accountId: account.id,
      platform: g.platform,
      platformGameId: g.platformGameId,
      pgn: g.pgn,
      playedAt: g.playedAt,
      timeClass: g.timeClass,
      baseSeconds: g.baseSeconds,
      incrementSeconds: g.incrementSeconds,
      rated: g.rated,
      playerColor,
      whiteName: g.white.name,
      blackName: g.black.name,
      whiteRating: g.white.rating,
      blackRating: g.black.rating,
      result: g.result,
      termination: g.termination,
      eco: g.eco,
      openingName: g.openingName,
      lichessEvals: g.lichessEvals,
      analysisStatus: "pending",
    });
    if (inserted) {
      await deps.enqueueAnalyse(id);
      imported++;
    }
  }

  await db
    .update(linkedAccounts)
    .set({ lastSyncedAt: new Date() })
    .where(eq(linkedAccounts.id, account.id));
  return { imported, skipped };
}
