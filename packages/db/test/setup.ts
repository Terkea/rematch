import { fileURLToPath } from "node:url";
import { sql } from "drizzle-orm";
import { migrate } from "drizzle-orm/postgres-js/migrator";
import { beforeAll, beforeEach } from "vitest";
import { createDb } from "../src/client.js";
import { assertSafeTestDatabase } from "../src/test-guard.js";

const url = assertSafeTestDatabase(process.env.DATABASE_URL_TEST, process.env.DATABASE_URL);

const db = createDb(url);

beforeAll(async () => {
  await migrate(db, {
    migrationsFolder: fileURLToPath(new URL("../drizzle", import.meta.url)),
  });
});

beforeEach(async () => {
  await db.execute(
    sql`truncate table user_events, import_runs, moves, games, position_evals, linked_accounts restart identity cascade`,
  );
});
