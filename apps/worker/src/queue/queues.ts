import { type JobsOptions, Queue } from "bullmq";
import { Redis } from "ioredis";

export const IMPORT_ACCOUNT_QUEUE = "import-account";
export const ANALYSE_GAME_QUEUE = "analyse-game";

export interface ImportAccountPayload {
  accountId: string;
  max?: number;
}

export interface AnalyseGamePayload {
  gameId: string;
}

export const DEFAULT_JOB_OPTIONS: JobsOptions = {
  attempts: 3,
  backoff: { type: "exponential", delay: 5000 },
};

/** BullMQ workers need `maxRetriesPerRequest: null` on their blocking connections. */
export function createRedisConnection(url: string): Redis {
  return new Redis(url, { maxRetriesPerRequest: null });
}

/**
 * Deterministic job id, so enqueueing the same game twice is a no-op. BullMQ rejects custom ids
 * containing ":" (see ADR 0028), hence the dash.
 */
export const analyseJobId = (gameId: string): string => `analyse-${gameId}`;

export interface Queues {
  importAccount: Queue<ImportAccountPayload>;
  analyseGame: Queue<AnalyseGamePayload>;
  enqueueAnalyse(gameId: string): Promise<void>;
  enqueueImport(payload: ImportAccountPayload): Promise<void>;
  close(): Promise<void>;
}

export function createQueues(connection: Redis, prefix?: string): Queues {
  const base = {
    connection,
    defaultJobOptions: DEFAULT_JOB_OPTIONS,
    ...(prefix ? { prefix } : {}),
  };
  const importAccount = new Queue<ImportAccountPayload>(IMPORT_ACCOUNT_QUEUE, base);
  const analyseGame = new Queue<AnalyseGamePayload>(ANALYSE_GAME_QUEUE, base);
  return {
    importAccount,
    analyseGame,
    async enqueueAnalyse(gameId) {
      await analyseGame.add("analyse", { gameId }, { jobId: analyseJobId(gameId) });
    },
    async enqueueImport(payload) {
      await importAccount.add("import", payload);
    },
    async close() {
      await Promise.all([importAccount.close(), analyseGame.close()]);
    },
  };
}
