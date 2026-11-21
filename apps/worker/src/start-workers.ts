import os from "node:os";
import { createDb, type Db } from "@ci/db";
import { type Job, Worker, type WorkerOptions } from "bullmq";
import type { Redis } from "ioredis";
import { createDbEvalCache } from "./analysis/eval-cache.js";
import { EnginePool } from "./engine/engine-pool.js";
import { publishEvent } from "./events/publish.js";
import { createCircuitBreaker } from "./http/circuit-breaker.js";
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
  CONTACT_EMAIL: string;
  LICHESS_TOKEN?: string | undefined;
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

function createWorker<T>(
  rt: WorkerRuntime,
  queue: string,
  processor: (job: Job<T>) => Promise<unknown>,
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

export function createAnalyseWorker(
  rt: WorkerRuntime,
  deps: AnalyseGameDeps,
  concurrency: number,
): Worker<AnalyseGamePayload> {
  return createWorker<AnalyseGamePayload>(
    rt,
    ANALYSE_GAME_QUEUE,
    (job) => handleAnalyseGame({ log: rt.log, ...deps }, job.data, attemptOf(job)),
    concurrency,
    (d) => ({ gameId: d.gameId }),
  );
}

export function createImportWorker(
  rt: WorkerRuntime,
  deps: ImportAccountDeps,
  concurrency: number,
): Worker<ImportAccountPayload> {
  return createWorker<ImportAccountPayload>(
    rt,
    IMPORT_ACCOUNT_QUEUE,
    (job) => handleImportAccount({ log: rt.log, ...deps }, job.data, attemptOf(job)),
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
  const db = createDb(env.DATABASE_URL);
  const size = opts.poolSize ?? Math.max(1, os.cpus().length - 1);
  let pool: EnginePool;
  try {
    pool = await EnginePool.create({ size, path: env.STOCKFISH_PATH });
  } catch (err) {
    await db.$client.end({ timeout: 5 }).catch(() => {});
    throw err;
  }
  const connection = createRedisConnection(env.REDIS_URL);
  connection.on("error", (err) => log.error({ err: errorMessage(err) }, "redis error"));
  const queues = createQueues(connection, { ...(opts.prefix ? { prefix: opts.prefix } : {}), log });
  const workers: Worker[] = [];
  try {
    const cache = createDbEvalCache(db);
    const publish: AnalyseGameDeps["publish"] = (accountId, type, payload) =>
      publishEvent({ db, redis: connection }, accountId, type, payload);
    const clients = {
      lichess: createPlatformClient({ name: "lichess", contactEmail: env.CONTACT_EMAIL }),
      chesscom: createPlatformClient({ name: "chesscom", contactEmail: env.CONTACT_EMAIL }),
    };
    const isOutage = (err: unknown) => !(err instanceof PlatformNotFoundError);
    const breakers = {
      lichess: createCircuitBreaker({
        failureThreshold: 5,
        cooldownMs: 60_000,
        isFailure: isOutage,
      }),
      chesscom: createCircuitBreaker({
        failureThreshold: 5,
        cooldownMs: 60_000,
        isFailure: isOutage,
      }),
    };
    const rt: WorkerRuntime = {
      connection,
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
          enqueueAnalyse: queues.enqueueAnalyse,
          publish,
        },
        2,
      ),
      createAnalyseWorker(rt, { db, pool, cache, publish }, size),
      createReconcileWorker(rt, { db, queues }),
    );
  } catch (err) {
    await Promise.allSettled(workers.map((w) => w.close(true)));
    await queues.close().catch(() => {});
    connection.disconnect();
    await pool.close();
    await db.$client.end({ timeout: 5 }).catch(() => {});
    throw err;
  }

  return {
    workers,
    pool,
    db,
    redis: connection,
    queues,
    async close() {
      await Promise.all(workers.map((w) => w.close()));
      await queues.close();
      connection.disconnect();
      await pool.close();
      await db.$client.end({ timeout: 5 });
    },
  };
}
