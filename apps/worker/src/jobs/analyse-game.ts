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
import { and, eq, sql } from "drizzle-orm";
import type { EvalCache } from "../analysis/eval-cache.js";
import { evaluateGame } from "../analysis/evaluate-game.js";
import { type EnginePool, EnginePoolClosedError } from "../engine/engine-pool.js";
import { ANALYSIS_PROTOCOL } from "../engine/protocol.js";
import type { PublishFn } from "../events/publish.js";
import { log as defaultLog, errorMessage, type Logger } from "../log.js";
import type { AnalyseGamePayload } from "../queue/queues.js";
import { ANALYSE_DEADLINE_MS, createDeadline } from "./deadline.js";

export interface AnalyseGameDeps {
  db: Db;
  pool: Pick<EnginePool, "analyse">;
  cache: EvalCache;
  /** Domain events, published after the commit they describe. */
  publish?: PublishFn;
  /** Minimum gap between heartbeat writes (default 10 s, so beats land at least every ~15 s). */
  heartbeatEveryMs?: number;
  /** Hard cap on one attempt (default 10 minutes); overrunning throws JobDeadlineError. */
  deadlineMs?: number;
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

export type AnalyseResult = { status: "done" | "skipped" | "superseded" };

const MAX_ERROR_LENGTH = 2000;
const INSERT_CHUNK = 500;
const DEFAULT_HEARTBEAT_MS = 10_000;

export const isFinalAttempt = (a: JobAttempt | undefined): boolean =>
  a === undefined || a.attemptsMade + 1 >= a.attempts;

/** A newer attempt took the game over; this run's writes were discarded. */
class SupersededError extends Error {}

/**
 * Analyses one game. Every attempt increments `analysis_attempts` and keeps the value it got:
 * the final writes (done, failed, pending, skipped) only apply while the row still carries that
 * value, so a stale duplicate delivery cannot overwrite a newer attempt's result.
 */
export async function handleAnalyseGame(
  deps: AnalyseGameDeps,
  payload: AnalyseGamePayload,
  attempt?: JobAttempt,
): Promise<AnalyseResult> {
  const { db } = deps;
  const log = deps.log ?? defaultLog;
  const [game] = await db.select().from(games).where(eq(games.id, payload.gameId));
  // Retrying cannot make a deleted game appear.
  if (!game) throw new UnrecoverableError(`Game not found: ${payload.gameId}`);
  // A duplicate delivery of finished work: nothing to do.
  if (game.analysisStatus === "done" && game.analysisProtocol === ANALYSIS_PROTOCOL) {
    return { status: "done" };
  }

  // Every attempt (retry, stalled-job pickup, reconciler re-enqueue) starts again from here.
  const startedAt = new Date();
  const [claimed] = await db
    .update(games)
    .set({
      analysisStatus: "analysing",
      analysisError: null,
      analysisStartedAt: startedAt,
      heartbeatAt: startedAt,
      analysisAttempts: sql`${games.analysisAttempts} + 1`,
    })
    .where(eq(games.id, game.id))
    .returning({ attempt: games.analysisAttempts });
  if (!claimed) throw new UnrecoverableError(`Game not found: ${payload.gameId}`);
  const mine = and(eq(games.id, game.id), eq(games.analysisAttempts, claimed.attempt));

  const heartbeatEveryMs = deps.heartbeatEveryMs ?? DEFAULT_HEARTBEAT_MS;
  let lastBeat = startedAt.getTime();
  const heartbeat = async () => {
    const now = Date.now();
    if (now - lastBeat < heartbeatEveryMs) return;
    lastBeat = now;
    await db
      .update(games)
      .set({ heartbeatAt: new Date(now) })
      .where(mine);
  };

  const deadline = createDeadline("analyse-game", deps.deadlineMs ?? ANALYSE_DEADLINE_MS);
  let result: { status: "done" | "skipped"; accuracy?: number | null };
  try {
    result = await analyse(deps, game, { mine, heartbeat, signal: deadline.signal });
  } catch (err) {
    if (err instanceof SupersededError) return { status: "superseded" };
    const message = errorMessage(err).slice(0, MAX_ERROR_LENGTH);
    // A shutdown is not the game's fault: leave it pending for the retry or the reconciler.
    const final = isFinalAttempt(attempt) && !(err instanceof EnginePoolClosedError);
    try {
      const updated = await db
        .update(games)
        .set(
          final
            ? { analysisStatus: "failed", analysisError: message }
            : { analysisStatus: "pending", analysisError: message, pendingSince: new Date() },
        )
        .where(mine)
        .returning({ id: games.id });
      if (final && updated.length > 0) {
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
  } finally {
    deadline.clear();
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

interface AttemptContext {
  /** WHERE clause matching the game only while this attempt still owns it. */
  mine: ReturnType<typeof and>;
  heartbeat: () => Promise<void>;
  signal: AbortSignal;
}

async function analyse(
  deps: AnalyseGameDeps,
  game: typeof games.$inferSelect,
  ctx: AttemptContext,
): Promise<{ status: "done" | "skipped"; accuracy?: number | null }> {
  const { db } = deps;
  const markSkipped = async () => {
    const rows = await db
      .update(games)
      .set({ analysisStatus: "skipped", analysisError: null })
      .where(ctx.mine)
      .returning({ id: games.id });
    if (rows.length === 0) throw new SupersededError();
    return { status: "skipped" as const };
  };

  let parsed: ReturnType<typeof parsePgn>;
  try {
    parsed = parsePgn(game.pgn);
  } catch (err) {
    if (!(err instanceof UnsupportedVariantError)) throw err;
    return markSkipped();
  }
  if (parsed.plies.length < 2) return markSkipped();

  const evals = await evaluateGame(
    {
      pool: deps.pool,
      cache: deps.cache,
      onProgress: ctx.heartbeat,
      signal: ctx.signal,
      ...(deps.concurrency !== undefined ? { concurrency: deps.concurrency } : {}),
    },
    parsed,
    (game.lichessEvals as (EngineScore | null)[] | null) ?? null,
  );
  ctx.signal.throwIfAborted();
  const clock =
    game.baseSeconds !== null && game.incrementSeconds !== null
      ? { baseSeconds: game.baseSeconds, incrementSeconds: game.incrementSeconds }
      : null;
  const records = buildMoveRecords(parsed, evals, clock);
  const winPercents = evals.map((e) => winPercent(e.score));
  const whiteAccuracy = gameAccuracy(winPercents, "white");
  const blackAccuracy = gameAccuracy(winPercents, "black");

  await db.transaction(async (tx) => {
    // Claim first: if a newer attempt owns the row, roll back before touching moves.
    const owned = await tx
      .update(games)
      .set({
        analysisStatus: "done",
        analysisProtocol: ANALYSIS_PROTOCOL,
        analysisError: null,
        whiteAccuracy,
        blackAccuracy,
      })
      .where(ctx.mine)
      .returning({ id: games.id });
    if (owned.length === 0) throw new SupersededError();
    await tx.delete(moves).where(eq(moves.gameId, game.id));
    const rows = records.map((r) => ({ gameId: game.id, ...r }));
    for (let i = 0; i < rows.length; i += INSERT_CHUNK) {
      await tx.insert(moves).values(rows.slice(i, i + INSERT_CHUNK));
    }
  });
  return {
    status: "done",
    accuracy: game.playerColor === "white" ? whiteAccuracy : blackAccuracy,
  };
}
