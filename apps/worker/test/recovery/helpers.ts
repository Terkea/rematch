import { randomUUID } from "node:crypto";
import { type Db, games, upsertGame } from "@ci/db";
import { eq } from "drizzle-orm";
import type { Logger } from "../../src/log.js";
import { workerTestDbConfigured } from "../db.js";
import { insertAccount, makeGame } from "../jobs/helpers.js";

export const redisUrl = process.env.REDIS_URL ?? "";

/** Postgres and Redis are both needed; inside Docker a missing one is an error, not a skip. */
export const stackConfigured = (): boolean =>
  workerTestDbConfigured() && (Boolean(redisUrl) || process.env.IN_DOCKER === "1");

export const uniquePrefix = (): string => `test-${randomUUID()}`;

/** Short locks so stalled-job detection kicks in within about a second. */
export const FAST_WORKER = { lockDuration: 1000, stalledInterval: 500, maxStalledCount: 2 };

/** Retries without the production 5 s exponential backoff. */
export const FAST_RETRY = { backoff: { type: "fixed", delay: 20 } } as const;

export const silentLog: Logger = { info() {}, warn() {}, error() {} };

/** Polls `fn` until it returns a truthy value, or fails after `timeoutMs`. */
export async function waitFor<T>(
  fn: () => Promise<T | null | undefined | false> | T | null | undefined | false,
  { timeoutMs = 10_000, intervalMs = 50, what = "condition" } = {},
): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const v = await fn();
    if (v) return v;
    if (Date.now() > deadline)
      throw new Error(`Timed out after ${timeoutMs} ms waiting for ${what}`);
    await new Promise((r) => setTimeout(r, intervalMs));
  }
}

export async function addGame(db: Db, pgn: string, username: string) {
  const acct = await insertAccount(db, username);
  const g = makeGame({ pgn, white: { name: username, rating: 1 } });
  const { id } = await upsertGame(db, {
    accountId: acct.id,
    platform: g.platform,
    platformGameId: g.platformGameId,
    pgn: g.pgn,
    playedAt: g.playedAt,
    timeClass: g.timeClass,
    baseSeconds: g.baseSeconds,
    incrementSeconds: g.incrementSeconds,
    rated: g.rated,
    playerColor: "white",
    whiteName: username,
    blackName: "x",
    result: g.result,
  });
  return { id, accountId: acct.id };
}

export async function getGame(db: Db, id: string) {
  const [g] = await db.select().from(games).where(eq(games.id, id));
  return g;
}

export const fakeLine = [{ multipv: 1, score: { cp: 20 }, pv: ["e2e4"], depth: 1, nodes: 1 }];

export const isDead = (pid: number): boolean => {
  try {
    process.kill(pid, 0);
    return false;
  } catch {
    return true;
  }
};
