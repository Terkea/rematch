import os from "node:os";
import { createDb, type Db, games } from "@ci/db";
import { DelayedError, type Job, Worker, type WorkerOptions } from "bullmq";
import { and, eq, notInArray } from "drizzle-orm";
import type { Redis } from "ioredis";
import { createDbEvalCache } from "./analysis/eval-cache.js";
import { EnginePool } from "./engine/engine-pool.js";
import { publishEvent } from "./events/publish.js";
import { CircuitOpenError, createCircuitBreaker } from "./http/circuit-breaker.js";
import { createPlatformClient, PlatformNotFoundError } from "./http/platform-client.js";
import { fetchChesscomGames } from "./importers/chesscom.js";
import { fetchLichessGames } from "./importers/lichess.js";
import { type AnalyseGameDeps, handleAnalyseGame, type JobAttempt } from "./jobs/analyse-game.js";
import { handleImportAccount, type ImportAccountDeps } from "./jobs/import-account.js";
import { type ReconcileDeps, reconcile } from "./jobs/reconcile.js";
import { log as defaultLog, errorMessage, type Logger } from "./log.js";
import {
  ANALYSE_GAME_QUEUE,
  type AnalyseGamePayload,
  createQueues,
  createRedisConnection,
  IMPORT_ACCOUNT_QUEUE,
  type ImportAccountPayload,
  type Queues,
  RECONCILE_QUEUE,
  WORKER_OPTIONS,
} from "./queue/queues.js";

export interface WorkerEnv {
  REDIS_URL: string;
  DATABASE_URL: string;
  STOCKFISH_PATH: string;
  LICHESS_TOKEN?: string | undefined;
  /** Days the incremental Lichess pass re-fetches below the watermark (default 30). */
  LICHESS_SINCE_LOOKBACK_DAYS?: string | undefined;
}

export interface WorkerRuntime {
  connection: Redis;
  prefix?: string;
  /** Overrides on top of WORKER_OPTIONS (tests shorten the lock and stalled interval). */
  workerOptions?: Partial<WorkerOptions>;
  log?: Logger;
}

const attemptOf = (job: Job): JobAttempt => ({
  attemptsMade: job.attemptsMade,
  attempts: job.opts.attempts ?? 1,
});

/** Server-side cap on any single query from the worker. */
export const WORKER_STATEMENT_TIMEOUT_MS = 30_000;
/** Lower bound on how long an import waits for an open breaker before re-probing. */
const MIN_BREAKER_DELAY_MS = 100;

function createWorker<T>(
  rt: WorkerRuntime,
  queue: string,
  processor: (job: Job<T>, token?: string) => Promise<unknown>,
  concurrency: number,
  context: (data: T) => Record<string, unknown>,
): Worker<T> {
  const log = rt.log ?? defaultLog;
  const worker = new Worker<T>(queue, processor, {
    connection: rt.connection,
    ...(rt.prefix ? { prefix: rt.prefix } : {}),
    ...WORKER_OPTIONS,
    ...rt.workerOptions,
    concurrency,
  });
  worker.on("error", (err) => log.error({ queue, err: errorMessage(err) }, "worker error"));
  worker.on("failed", (job, err) =>
    log.error(
      {
        queue,
        jobId: job?.id,
        ...(job ? context(job.data) : {}),
        attempt: job ? job.attemptsMade : undefined,
        maxAttempts: job?.opts.attempts,
        err: errorMessage(err),
      },
      "job failed",
    ),
  );
  worker.on("stalled", (jobId) => log.warn({ queue, jobId }, "job stalled; re-queued"));
  return worker;
}

/**
 * When BullMQ gives up on an analyse job (attempts used up, or stalled more than
 * maxStalledCount, where the handler never even ran), make the DB say so. Guarded so a game
 * already done, failed or skipped is left as it is, and `job.failed` is published once.
 */
async function recordFinalFailure(
  deps: AnalyseGameDeps,
  log: Logger,
  job: Job<AnalyseGamePayload>,
  err: Error,
): Promise<void> {
  if ((await job.getState()) !== "failed") return;
  const reason = errorMessage(err).slice(0, 2000);
  const [row] = await deps.db
    .update(games)
    .set({ analysisStatus: "failed", analysisError: reason })
    .where(
      and(
        eq(games.id, job.data.gameId),
        notInArray(games.analysisStatus, ["done", "failed", "skipped"]),
      ),
    )
    .returning({ accountId: games.accountId });
  if (!row) return;
  log.warn({ gameId: job.data.gameId, jobId: job.id, reason }, "analysis given up by BullMQ");
  await deps
    .publish?.(row.accountId, "job.failed", {
      kind: "analyse-game",
      gameId: job.data.gameId,
      reason,
    })
    .catch((e) => log.warn({ gameId: job.data.gameId, err: errorMessage(e) }, "publish failed"));
}

export function createAnalyseWorker(
  rt: WorkerRuntime,
  deps: AnalyseGameDeps,
  concurrency: number,
): Worker<AnalyseGamePayload> {
  const log = rt.log ?? defaultLog;
  const worker = createWorker<AnalyseGamePayload>(
    rt,
    ANALYSE_GAME_QUEUE,
    (job) => handleAnalyseGame({ log, ...deps }, job.data, attemptOf(job)),
    concurrency,
    (d) => ({ gameId: d.gameId }),
  );
  worker.on("failed", (job, err) => {
    if (!job) return;
    recordFinalFailure(deps, log, job, err).catch((e) =>
      log.error(
        { gameId: job.data.gameId, err: errorMessage(e) },
        "recording final failure failed",
      ),
    );
  });
  return worker;
}

export function createImportWorker(
  rt: WorkerRuntime,
  deps: ImportAccountDeps,
  concurrency: number,
): Worker<ImportAccountPayload> {
  const log = rt.log ?? defaultLog;
  return createWorker<ImportAccountPayload>(
    rt,
    IMPORT_ACCOUNT_QUEUE,
    async (job, token) => {
      try {
        return await handleImportAccount({ log, ...deps }, job.data, attemptOf(job));
      } catch (err) {
        if (!(err instanceof CircuitOpenError) || !token) throw err;
        // The platform is down, not this job: wait out the breaker without using an attempt.
        const delayMs = Math.max(MIN_BREAKER_DELAY_MS, err.retryInMs);
        log.warn(
          { jobId: job.id, accountId: job.data.accountId, delayMs },
          "circuit open; delayed",
        );
        await job.moveToDelayed(Date.now() + delayMs, token);
        throw new DelayedError();
      }
    },
    concurrency,
    (d) => ({ accountId: d.accountId }),
  );
}

export function createReconcileWorker(rt: WorkerRuntime, deps: ReconcileDeps): Worker {
  return createWorker(
    rt,
    RECONCILE_QUEUE,
    () => reconcile({ log: rt.log, ...deps }),
    1,
    () => ({}),
  );
}

export interface RunningWorkers {
  workers: Worker[];
  pool: EnginePool;
  db: Db;
  redis: Redis;
  queues: Queues;
  close(): Promise<void>;
}

export async function startWorkers(
  env: WorkerEnv,
  opts: { prefix?: string; poolSize?: number; log?: Logger } = {},
): Promise<RunningWorkers> {
  const log = opts.log ?? defaultLog;
  const db = createDb(env.DATABASE_URL, { statementTimeoutMs: WORKER_STATEMENT_TIMEOUT_MS });
  const size = opts.poolSize ?? Math.max(1, os.cpus().length - 1);
  let pool: EnginePool | undefined;
  let connection: Redis | undefined;
  let queues: Queues | undefined;
  const workers: Worker[] = [];
  try {
    pool = await EnginePool.create({ size, path: env.STOCKFISH_PATH });
    const redis = createRedisConnection(env.REDIS_URL);
    connection = redis;
    redis.on("error", (err) => log.error({ err: errorMessage(err) }, "redis error"));
    const q = createQueues(redis, { ...(opts.prefix ? { prefix: opts.prefix } : {}), log });
    queues = q;
    const cache = createDbEvalCache(db);
    const publish: AnalyseGameDeps["publish"] = (accountId, type, payload) =>
      publishEvent({ db, redis }, accountId, type, payload);
    const clients = {
      lichess: createPlatformClient({ name: "lichess" }),
      chesscom: createPlatformClient({ name: "chesscom" }),
    };
    const isOutage = (err: unknown) => !(err instanceof PlatformNotFoundError);
    const breaker = () =>
      createCircuitBreaker({ failureThreshold: 5, cooldownMs: 60_000, isFailure: isOutage });
    const breakers = { lichess: breaker(), chesscom: breaker() };
    const lookback = Number(env.LICHESS_SINCE_LOOKBACK_DAYS);
    const rt: WorkerRuntime = {
      connection: redis,
      ...(opts.prefix ? { prefix: opts.prefix } : {}),
      log,
    };
    workers.push(
      createImportWorker(
        rt,
        {
          db,
          importers: { lichess: fetchLichessGames, chesscom: fetchChesscomGames },
          clients,
          breakers,
          ...(env.LICHESS_TOKEN ? { lichessToken: env.LICHESS_TOKEN } : {}),
          ...(Number.isFinite(lookback) && env.LICHESS_SINCE_LOOKBACK_DAYS
            ? { lichessSinceLookbackDays: lookback }
            : {}),
          enqueueAnalyse: q.enqueueAnalyse,
          publish,
        },
        2,
      ),
      createAnalyseWorker(rt, { db, pool, cache, publish }, size),
      createReconcileWorker(rt, { db, queues: q, publish }),
    );
  } catch (err) {
    await Promise.allSettled(workers.map((w) => w.close(true)));
    await queues?.close().catch(() => {});
    connection?.disconnect();
    await pool?.close().catch(() => {});
    await db.$client.end({ timeout: 5 }).catch(() => {});
    throw err;
  }

  if (!pool || !queues || !connection) throw new Error("startWorkers: setup incomplete");
  const readyPool = pool;
  const readyQueues = queues;
  const redis = connection;
  return {
    workers,
    pool: readyPool,
    db,
    redis,
    queues: readyQueues,
    async close() {
      await Promise.all(workers.map((w) => w.close()));
      await readyQueues.close();
      redis.disconnect();
      await readyPool.close();
      await db.$client.end({ timeout: 5 });
    },
  };
}
