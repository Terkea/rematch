import { type JobsOptions, Queue, type WorkerOptions } from "bullmq";
import { Redis } from "ioredis";
import { log as defaultLog, errorMessage, type Logger } from "../log.js";

export const IMPORT_ACCOUNT_QUEUE = "import-account";
export const ANALYSE_GAME_QUEUE = "analyse-game";
export const RECONCILE_QUEUE = "reconcile";
export const RECONCILE_EVERY_MS = 5 * 60_000;

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
  // Keep the last 1000 completed jobs for at most 24 h, and failed jobs for 7 days.
  removeOnComplete: { count: 1000, age: 24 * 3600 },
  removeOnFail: { age: 7 * 24 * 3600 },
};

/**
 * Stalled-job detection: a job whose worker stops renewing its lock for 60 s goes back to waiting,
 * at most twice, then fails.
 */
export const WORKER_OPTIONS = {
  lockDuration: 60_000,
  maxStalledCount: 2,
} satisfies Partial<WorkerOptions>;

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
  reconcile: Queue;
  /**
   * Enqueues analysis unless a live job for the game exists. A retained completed or failed job
   * with the same id is removed first, so re-analysis is never silently swallowed. Returns
   * whether a job was added.
   */
  enqueueAnalyse(gameId: string): Promise<boolean>;
  /**
   * For each game, whether BullMQ holds a job that will still run ("live": waiting, active,
   * delayed), a retained finished one ("finished"), or none. One pipelined round trip, O(1) each.
   */
  analyseLiveness(gameIds: string[]): Promise<JobLiveness[]>;
  /** The exact BullMQ state of the game's analyse job (a round trip each; use for few games). */
  analyseJobState(gameId: string): Promise<string>;
  enqueueImport(payload: ImportAccountPayload, opts?: { jobId?: string }): Promise<void>;
  /** Registers (idempotently) the repeatable reconcile job. */
  scheduleReconcile(): Promise<void>;
  /** Test helper: deletes every job of every queue. */
  obliterate(): Promise<void>;
  close(): Promise<void>;
}

export type JobLiveness = "live" | "finished" | "none";

export interface QueueOptions {
  prefix?: string;
  /** Overrides on top of DEFAULT_JOB_OPTIONS (tests shorten the backoff). */
  jobOptions?: Partial<JobsOptions>;
  log?: Logger;
}

export function createQueues(connection: Redis, opts: QueueOptions = {}): Queues {
  const log = opts.log ?? defaultLog;
  const base = {
    connection,
    defaultJobOptions: { ...DEFAULT_JOB_OPTIONS, ...opts.jobOptions },
    ...(opts.prefix ? { prefix: opts.prefix } : {}),
  };
  const importAccount = new Queue<ImportAccountPayload>(IMPORT_ACCOUNT_QUEUE, base);
  const analyseGame = new Queue<AnalyseGamePayload>(ANALYSE_GAME_QUEUE, base);
  const reconcile = new Queue(RECONCILE_QUEUE, {
    ...base,
    defaultJobOptions: { attempts: 1, removeOnComplete: 100, removeOnFail: { age: 7 * 24 * 3600 } },
  });
  for (const q of [importAccount, analyseGame, reconcile] as Queue[]) {
    q.on("error", (err) => log.error({ queue: q.name, err: errorMessage(err) }, "queue error"));
  }

  // A BullMQ job is the hash <prefix>:<queue>:<jobId>. "timestamp" is written when the job is
  // added and "finishedOn" when it completes or fails for good (BullMQ 6.7.8 layout). A retry
  // that is delayed or waiting has no finishedOn, so it counts as live.
  const analyseLiveness = async (gameIds: string[]): Promise<JobLiveness[]> => {
    if (gameIds.length === 0) return [];
    // The non-blocking connection this module was given; BullMQ keys live on the same server.
    const pipeline = connection.pipeline();
    for (const id of gameIds) {
      pipeline.hmget(analyseGame.toKey(analyseJobId(id)), "timestamp", "finishedOn");
    }
    const replies = (await pipeline.exec()) ?? [];
    return replies.map(([err, value]: [Error | null, unknown]) => {
      if (err) throw err;
      const [timestamp, finishedOn] = value as [string | null, string | null];
      if (timestamp === null) return "none";
      return finishedOn === null ? "live" : "finished";
    });
  };

  return {
    importAccount,
    analyseGame,
    reconcile,
    analyseLiveness,
    analyseJobState: (gameId) => analyseGame.getJobState(analyseJobId(gameId)),
    async enqueueAnalyse(gameId) {
      const jobId = analyseJobId(gameId);
      const [liveness] = await analyseLiveness([gameId]);
      if (liveness === "live") return false;
      if (liveness === "finished") {
        // A concurrent remover may win the race; either way the id is free afterwards.
        await analyseGame.remove(jobId).catch(() => 0);
      }
      await analyseGame.add("analyse", { gameId }, { jobId });
      return true;
    },
    async enqueueImport(payload, jobOpts) {
      await importAccount.add("import", payload, jobOpts?.jobId ? { jobId: jobOpts.jobId } : {});
    },
    async scheduleReconcile() {
      await reconcile.upsertJobScheduler(
        "reconcile-every-5-min",
        { every: RECONCILE_EVERY_MS },
        { name: "reconcile" },
      );
    },
    async obliterate() {
      await Promise.all(
        [importAccount, analyseGame, reconcile].map((q) =>
          (q as Queue).obliterate({ force: true }),
        ),
      );
    },
    async close() {
      await Promise.all([importAccount.close(), analyseGame.close(), reconcile.close()]);
    },
  };
}
