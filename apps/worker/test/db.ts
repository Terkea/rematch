import { fileURLToPath } from "node:url";
import { assertSafeTestDatabase, createDb, type Db } from "@ci/db";
import { sql } from "drizzle-orm";
import { migrate } from "drizzle-orm/postgres-js/migrator";

/**
 * The worker's own test database (`WORKER_DATABASE_URL_TEST`), separate from the db package's so
 * parallel package tests cannot truncate each other's rows. Returns null when it is unset outside
 * Docker (tests skip); inside Docker (`IN_DOCKER=1`) an unset URL is an error.
 */
export async function getWorkerTestDb(): Promise<Db | null> {
  const url = process.env.WORKER_DATABASE_URL_TEST;
  if (!url) {
    if (process.env.IN_DOCKER === "1") {
      throw new Error("WORKER_DATABASE_URL_TEST must be set when IN_DOCKER=1");
    }
    return null;
  }
  assertSafeTestDatabase(url, process.env.DATABASE_URL);
  const db = createDb(url);
  await migrate(db, {
    migrationsFolder: fileURLToPath(new URL("../../../packages/db/drizzle", import.meta.url)),
  });
  await db.execute(
    sql`truncate table user_events, import_runs, moves, games, position_evals, linked_accounts restart identity cascade`,
  );
  return db;
}

export const workerTestDbConfigured = (): boolean =>
  Boolean(process.env.WORKER_DATABASE_URL_TEST) || process.env.IN_DOCKER === "1";
