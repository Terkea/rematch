/**
 * Parity tooling (R1 Task 24; deleted with the TS worker in Task 27): enqueues a TS analyse job
 * for every pending game, so `src/main.ts` analyses a seeded database at once instead of waiting
 * for the reconciler.
 *
 * Usage: DATABASE_URL=... REDIS_URL=... tsx --conditions=development scripts/enqueue-pending.ts
 * It uses BullMQ's default prefix, the one `src/main.ts` reads: the parity run gives each TS
 * worker its own compose project, so its Redis holds nothing else.
 */
import { pathToFileURL } from "node:url";
import { createDb, type Db, games } from "@ci/db";
import { eq } from "drizzle-orm";
import { createQueues, createRedisConnection, type Queues } from "../src/queue/queues.js";

/** Enqueues an analyse job for every pending game; returns how many were added. */
export async function enqueuePending(db: Db, queues: Queues): Promise<number> {
  const pending = await db
    .select({ id: games.id })
    .from(games)
    .where(eq(games.analysisStatus, "pending"))
    .orderBy(games.id);
  let added = 0;
  for (const g of pending) {
    if (await queues.enqueueAnalyse(g.id)) added += 1;
  }
  return added;
}

function requireEnv(name: string): string {
  const v = process.env[name];
  if (!v) throw new Error(`${name} is not set`);
  return v;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const db = createDb(requireEnv("DATABASE_URL"));
  const connection = createRedisConnection(requireEnv("REDIS_URL"));
  const queues = createQueues(connection);
  try {
    const n = await enqueuePending(db, queues);
    console.log(`enqueue-pending: enqueued ${n} analyse jobs`);
  } finally {
    await queues.close();
    connection.disconnect();
    await db.$client.end();
  }
}
