import { fileURLToPath } from "node:url";
import { sql } from "drizzle-orm";
import { migrate } from "drizzle-orm/postgres-js/migrator";
import { beforeAll, beforeEach } from "vitest";
import { createDb } from "../src/client.js";

const url = process.env.DATABASE_URL_TEST;
if (!url) throw new Error("DATABASE_URL_TEST is not set (run via docker compose run --rm worker)");

const db = createDb(url);

beforeAll(async () => {
  await migrate(db, {
    migrationsFolder: fileURLToPath(new URL("../drizzle", import.meta.url)),
  });
});

beforeEach(async () => {
  await db.execute(
    sql`truncate table moves, games, position_evals, linked_accounts restart identity cascade`,
  );
});
