import { type Db, games, type ImportCursor, importRuns, linkedAccounts, upsertGame } from "@ci/db";
import { UnrecoverableError } from "bullmq";
import { eq, max } from "drizzle-orm";
import type { PublishFn } from "../events/publish.js";
import { type CircuitBreaker, withCircuitBreaker } from "../http/circuit-breaker.js";
import type { PlatformClient } from "../http/platform-client.js";
import type { fetchChesscomGames } from "../importers/chesscom.js";
import type { fetchLichessGames } from "../importers/lichess.js";
import type { NormalizedGame } from "../importers/types.js";
import { log as defaultLog, errorMessage, type Logger } from "../log.js";
import type { ImportAccountPayload } from "../queue/queues.js";
import { isFinalAttempt, type JobAttempt } from "./analyse-game.js";

export type { ImportCursor } from "@ci/db";

type Platform = "lichess" | "chesscom";

export interface ImportAccountDeps {
  db: Db;
  importers: { lichess: typeof fetchLichessGames; chesscom: typeof fetchChesscomGames };
  clients: { lichess: PlatformClient; chesscom: PlatformClient };
  /** Per-platform breakers, shared across jobs; platform calls go through them. */
  breakers?: Partial<Record<Platform, CircuitBreaker>>;
  lichessToken?: string;
  /** Called only after the game row is committed. */
  enqueueAnalyse(gameId: string): Promise<unknown>;
  publish?: PublishFn;
  /** Checkpoint the cursor and run after this many games (default 50)... */
  checkpointEvery?: number;
  /** ...or after this long since the last checkpoint (default 15 s), whichever comes first. */
  checkpointMs?: number;
  log?: Logger;
}

const MAX_ERROR_LENGTH = 2000;
const EMPTY_CURSOR: ImportCursor = { oldestReached: null, newestSeenAt: null };

/**
 * Imports an account's games, newest first, resuming from `linked_accounts.import_cursor`.
 *
 * A pass streams (since: newestSeenAt, until?: oldestReached) and checkpoints `oldestReached`
 * (the oldest game committed so far) every batch. When a pass finishes, `newestSeenAt` becomes the
 * newest game stored and `oldestReached` is cleared. A run first finishes any interrupted pass,
 * then fetches games newer than everything stored. Upserts make overlaps harmless.
 */
export async function handleImportAccount(
  deps: ImportAccountDeps,
  payload: ImportAccountPayload,
  attempt?: JobAttempt,
): Promise<{ imported: number; skipped: number }> {
  const { db } = deps;
  const log = deps.log ?? defaultLog;
  const checkpointEvery = deps.checkpointEvery ?? 50;
  const checkpointMs = deps.checkpointMs ?? 15_000;
  const [account] = await db
    .select()
    .from(linkedAccounts)
    .where(eq(linkedAccounts.id, payload.accountId));
  if (!account) throw new UnrecoverableError(`Linked account not found: ${payload.accountId}`);

  const [run] = await db
    .insert(importRuns)
    .values({ accountId: account.id, status: "running" })
    .returning({ id: importRuns.id });
  if (!run) throw new Error("import run insert returned no row");

  const publish = async (type: "import.progress" | "job.failed", body: object) => {
    try {
      await deps.publish?.(account.id, type, body);
    } catch (err) {
      log.warn({ accountId: account.id, err: errorMessage(err) }, "publish failed");
    }
  };

  const username = account.username.toLowerCase();
  const platform = account.platform;
  const breaker = deps.breakers?.[platform];
  const client = breaker
    ? withCircuitBreaker(deps.clients[platform], breaker)
    : deps.clients[platform];
  const fetchGames = (range: { since?: Date; until?: Date }): AsyncIterable<NormalizedGame> => {
    const opts = {
      ...range,
      ...(payload.max !== undefined ? { max: payload.max } : {}),
    };
    return platform === "lichess"
      ? deps.importers.lichess(client, account.username, {
          ...opts,
          ...(deps.lichessToken ? { token: deps.lichessToken } : {}),
        })
      : deps.importers.chesscom(client, account.username, opts);
  };

  let cursor: ImportCursor = { ...EMPTY_CURSOR, ...account.importCursor };
  let imported = 0;
  let skipped = 0;
  let frontier: Date | null = cursor.oldestReached ? new Date(cursor.oldestReached) : null;
  let sinceCheckpoint = 0;
  let lastCheckpoint = Date.now();

  const checkpoint = async () => {
    cursor = { ...cursor, oldestReached: frontier ? frontier.toISOString() : null };
    await db
      .update(linkedAccounts)
      .set({ importCursor: cursor })
      .where(eq(linkedAccounts.id, account.id));
    await db
      .update(importRuns)
      .set({ imported, heartbeatAt: new Date() })
      .where(eq(importRuns.id, run.id));
    sinceCheckpoint = 0;
    lastCheckpoint = Date.now();
    await publish("import.progress", { runId: run.id, imported });
  };

  const runPass = async (range: { since?: Date; until?: Date }) => {
    for await (const g of fetchGames(range)) {
      const playerColor =
        g.white.name.toLowerCase() === username
          ? "white"
          : g.black.name.toLowerCase() === username
            ? "black"
            : null;
      if (playerColor === null) {
        skipped++;
      } else {
        const res = await upsertGame(db, {
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
        if (res.inserted) imported++;
        // Also re-enqueue existing pending games: a previous run may have died between the
        // insert and the enqueue. The deterministic job id dedupes.
        if (res.analysisStatus === "pending") await deps.enqueueAnalyse(res.id);
      }
      if (frontier === null || g.playedAt < frontier) frontier = g.playedAt;
      sinceCheckpoint++;
      if (sinceCheckpoint >= checkpointEvery || Date.now() - lastCheckpoint >= checkpointMs) {
        await checkpoint();
      }
    }
    // The pass reached its lower bound: everything up to the newest stored game is imported.
    const [row] = await db
      .select({ newest: max(games.playedAt) })
      .from(games)
      .where(eq(games.accountId, account.id));
    const newest = [row?.newest, cursor.newestSeenAt ? new Date(cursor.newestSeenAt) : null]
      .filter((d): d is Date => d instanceof Date)
      .sort((a, b) => b.getTime() - a.getTime())[0];
    frontier = null;
    cursor = { oldestReached: null, newestSeenAt: newest ? newest.toISOString() : null };
    await checkpoint();
  };

  try {
    if (cursor.oldestReached) {
      await runPass({
        until: new Date(cursor.oldestReached),
        ...(cursor.newestSeenAt ? { since: new Date(cursor.newestSeenAt) } : {}),
      });
    }
    await runPass(cursor.newestSeenAt ? { since: new Date(cursor.newestSeenAt) } : {});
  } catch (err) {
    const message = errorMessage(err).slice(0, MAX_ERROR_LENGTH);
    try {
      // Keep the progress made: the retry resumes below the last committed game.
      if (frontier !== null) {
        cursor = { ...cursor, oldestReached: frontier.toISOString() };
        await db
          .update(linkedAccounts)
          .set({ importCursor: cursor })
          .where(eq(linkedAccounts.id, account.id));
      }
      await db
        .update(importRuns)
        .set({ status: "failed", imported, lastError: message, finishedAt: new Date() })
        .where(eq(importRuns.id, run.id));
    } catch {
      // Keep the original error; the reconciler handles a run left "running".
    }
    if (isFinalAttempt(attempt) && !(err instanceof UnrecoverableError)) {
      await publish("job.failed", { kind: "import-account", runId: run.id, reason: message });
    }
    throw err;
  }

  const finishedAt = new Date();
  await db
    .update(importRuns)
    .set({ status: "done", imported, finishedAt, heartbeatAt: finishedAt })
    .where(eq(importRuns.id, run.id));
  await db
    .update(linkedAccounts)
    .set({ lastSyncedAt: finishedAt })
    .where(eq(linkedAccounts.id, account.id));
  await publish("import.progress", { runId: run.id, imported, done: true });
  return { imported, skipped };
}
