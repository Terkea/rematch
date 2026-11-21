import {
  buildMoveRecords,
  type EngineScore,
  gameAccuracy,
  parsePgn,
  UnsupportedVariantError,
  winPercent,
} from "@ci/chess-core";
import { type Db, games, moves } from "@ci/db";
import { UnrecoverableError } from "bullmq";
import { eq, sql } from "drizzle-orm";
import type { EvalCache } from "../analysis/eval-cache.js";
import { evaluateGame } from "../analysis/evaluate-game.js";
import { type EnginePool, EnginePoolClosedError } from "../engine/engine-pool.js";
import { ANALYSIS_PROTOCOL } from "../engine/protocol.js";
import type { PublishFn } from "../events/publish.js";
import { log as defaultLog, errorMessage, type Logger } from "../log.js";
import type { AnalyseGamePayload } from "../queue/queues.js";

export interface AnalyseGameDeps {
  db: Db;
  pool: Pick<EnginePool, "analyse">;
  cache: EvalCache;
  /** Domain events, published after the commit they describe. */
  publish?: PublishFn;
  /** Minimum gap between heartbeat writes (default 10 s, so beats land at least every ~15 s). */
  heartbeatEveryMs?: number;
  /** Parallel engine requests for evaluateGame. */
  concurrency?: number;
  log?: Logger;
}

/** Where this run sits among BullMQ's attempts. Without it, the run is treated as the last one. */
export interface JobAttempt {
  /** Attempts already made before this one (BullMQ `job.attemptsMade`). */
  attemptsMade: number;
  /** Total attempts allowed (BullMQ `job.opts.attempts`). */
  attempts: number;
}

const MAX_ERROR_LENGTH = 2000;
const INSERT_CHUNK = 500;
const DEFAULT_HEARTBEAT_MS = 10_000;

export const isFinalAttempt = (a: JobAttempt | undefined): boolean =>
  a === undefined || a.attemptsMade + 1 >= a.attempts;

export async function handleAnalyseGame(
  deps: AnalyseGameDeps,
  payload: AnalyseGamePayload,
  attempt?: JobAttempt,
): Promise<{ status: "done" | "skipped" }> {
  const { db } = deps;
  const log = deps.log ?? defaultLog;
  const [game] = await db.select().from(games).where(eq(games.id, payload.gameId));
  // Retrying cannot make a deleted game appear.
  if (!game) throw new UnrecoverableError(`Game not found: ${payload.gameId}`);

  // Every attempt (retry, stalled-job pickup, reconciler re-enqueue) starts again from here.
  const startedAt = new Date();
  await db
    .update(games)
    .set({
      analysisStatus: "analysing",
      analysisError: null,
      analysisStartedAt: startedAt,
      heartbeatAt: startedAt,
      analysisAttempts: sql`${games.analysisAttempts} + 1`,
    })
    .where(eq(games.id, game.id));

  const heartbeatEveryMs = deps.heartbeatEveryMs ?? DEFAULT_HEARTBEAT_MS;
  let lastBeat = startedAt.getTime();
  const heartbeat = async () => {
    const now = Date.now();
    if (now - lastBeat < heartbeatEveryMs) return;
    lastBeat = now;
    await db
      .update(games)
      .set({ heartbeatAt: new Date(now) })
      .where(eq(games.id, game.id));
  };

  let result: { status: "done" | "skipped"; accuracy?: number | null };
  try {
    result = await analyse(deps, game, heartbeat);
  } catch (err) {
    const message = errorMessage(err).slice(0, MAX_ERROR_LENGTH);
    // A shutdown is not the game's fault: leave it pending for the retry or the reconciler.
    const final = isFinalAttempt(attempt) && !(err instanceof EnginePoolClosedError);
    try {
      await db
        .update(games)
        .set({ analysisStatus: final ? "failed" : "pending", analysisError: message })
        .where(eq(games.id, game.id));
      if (final) {
        await deps
          .publish?.(game.accountId, "job.failed", {
            kind: "analyse-game",
            gameId: game.id,
            reason: message,
          })
          .catch((e) => log.warn({ gameId: game.id, err: errorMessage(e) }, "publish failed"));
      }
    } catch {
      // Keep the original error; the retry will set the status again.
    }
    throw err;
  }

  if (result.status === "done") {
    await deps
      .publish?.(game.accountId, "game.analysed", {
        gameId: game.id,
        accuracy: result.accuracy ?? null,
      })
      .catch((e) => log.warn({ gameId: game.id, err: errorMessage(e) }, "publish failed"));
  }
  return { status: result.status };
}

async function analyse(
  deps: AnalyseGameDeps,
  game: typeof games.$inferSelect,
  heartbeat: () => Promise<void>,
): Promise<{ status: "done" | "skipped"; accuracy?: number | null }> {
  const { db } = deps;
  let parsed: ReturnType<typeof parsePgn>;
  try {
    parsed = parsePgn(game.pgn);
  } catch (err) {
    if (!(err instanceof UnsupportedVariantError)) throw err;
    await markSkipped(db, game.id);
    return { status: "skipped" };
  }
  if (parsed.plies.length < 2) {
    await markSkipped(db, game.id);
    return { status: "skipped" };
  }

  const evals = await evaluateGame(
    {
      pool: deps.pool,
      cache: deps.cache,
      onProgress: heartbeat,
      ...(deps.concurrency !== undefined ? { concurrency: deps.concurrency } : {}),
    },
    parsed,
    (game.lichessEvals as (EngineScore | null)[] | null) ?? null,
  );
  const clock =
    game.baseSeconds !== null && game.incrementSeconds !== null
      ? { baseSeconds: game.baseSeconds, incrementSeconds: game.incrementSeconds }
      : null;
  const records = buildMoveRecords(parsed, evals, clock);
  const winPercents = evals.map((e) => winPercent(e.score));
  const whiteAccuracy = gameAccuracy(winPercents, "white");
  const blackAccuracy = gameAccuracy(winPercents, "black");

  await db.transaction(async (tx) => {
    await tx.delete(moves).where(eq(moves.gameId, game.id));
    const rows = records.map((r) => ({ gameId: game.id, ...r }));
    for (let i = 0; i < rows.length; i += INSERT_CHUNK) {
      await tx.insert(moves).values(rows.slice(i, i + INSERT_CHUNK));
    }
    await tx
      .update(games)
      .set({
        analysisStatus: "done",
        analysisProtocol: ANALYSIS_PROTOCOL,
        analysisError: null,
        whiteAccuracy,
        blackAccuracy,
      })
      .where(eq(games.id, game.id));
  });
  return {
    status: "done",
    accuracy: game.playerColor === "white" ? whiteAccuracy : blackAccuracy,
  };
}

async function markSkipped(db: Db, gameId: string): Promise<void> {
  await db
    .update(games)
    .set({ analysisStatus: "skipped", analysisError: null })
    .where(eq(games.id, gameId));
}
