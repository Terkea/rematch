import os from "node:os";
import { createDb } from "@ci/db";
import { Worker } from "bullmq";
import { createDbEvalCache } from "./analysis/eval-cache.js";
import { EnginePool } from "./engine/engine-pool.js";
import { createPlatformClient } from "./http/platform-client.js";
import { fetchChesscomGames } from "./importers/chesscom.js";
import { fetchLichessGames } from "./importers/lichess.js";
import { handleAnalyseGame } from "./jobs/analyse-game.js";
import { handleImportAccount } from "./jobs/import-account.js";
import {
  ANALYSE_GAME_QUEUE,
  type AnalyseGamePayload,
  createQueues,
  createRedisConnection,
  IMPORT_ACCOUNT_QUEUE,
  type ImportAccountPayload,
} from "./queue/queues.js";

export interface WorkerEnv {
  REDIS_URL: string;
  DATABASE_URL: string;
  STOCKFISH_PATH: string;
  CONTACT_EMAIL: string;
  LICHESS_TOKEN?: string | undefined;
}

export async function startWorkers(
  env: WorkerEnv,
  opts: { prefix?: string; poolSize?: number } = {},
): Promise<{ close(): Promise<void> }> {
  const db = createDb(env.DATABASE_URL);
  const size = opts.poolSize ?? Math.max(1, os.cpus().length - 1);
  const pool = await EnginePool.create({ size, path: env.STOCKFISH_PATH });
  const cache = createDbEvalCache(db);
  const connection = createRedisConnection(env.REDIS_URL);
  const queues = createQueues(connection, opts.prefix);
  const clients = {
    lichess: createPlatformClient({ name: "lichess", contactEmail: env.CONTACT_EMAIL }),
    chesscom: createPlatformClient({ name: "chesscom", contactEmail: env.CONTACT_EMAIL }),
  };
  const common = { connection, ...(opts.prefix ? { prefix: opts.prefix } : {}) };

  const importWorker = new Worker<ImportAccountPayload>(
    IMPORT_ACCOUNT_QUEUE,
    (job) =>
      handleImportAccount(
        {
          db,
          importers: { lichess: fetchLichessGames, chesscom: fetchChesscomGames },
          clients,
          ...(env.LICHESS_TOKEN ? { lichessToken: env.LICHESS_TOKEN } : {}),
          enqueueAnalyse: queues.enqueueAnalyse,
        },
        job.data,
      ),
    { ...common, concurrency: 2 },
  );
  const analyseWorker = new Worker<AnalyseGamePayload>(
    ANALYSE_GAME_QUEUE,
    (job) => handleAnalyseGame({ db, pool, cache }, job.data),
    { ...common, concurrency: size },
  );

  return {
    async close() {
      await Promise.all([importWorker.close(), analyseWorker.close()]);
      await queues.close();
      connection.disconnect();
      await pool.close();
    },
  };
}
