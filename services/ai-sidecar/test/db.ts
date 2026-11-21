import { randomBytes } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import pg from "pg";

const FIXTURE = fileURLToPath(new URL("./fixtures/ai_sdk_session_entries.sql", import.meta.url));

export type StoreDb = {
  /** Connected as the restricted `ai_sidecar` role. */
  pool: pg.Pool;
  /** Superuser connection to the throwaway database. */
  dbAdmin: pg.Client;
  close(): Promise<void>;
};

/**
 * A throwaway database with the `ai_sdk_session_entries` fixture (the real migration is Task 17's)
 * and a pool that connects as the restricted `ai_sidecar` role.
 */
export async function createStoreDb(adminUrl: string): Promise<StoreDb> {
  const dbName = `ai_sidecar_store_${randomBytes(4).toString("hex")}`;
  const password = randomBytes(12).toString("hex");
  const admin = new pg.Client({ connectionString: adminUrl });
  await admin.connect();
  await admin.query(`CREATE DATABASE ${dbName}`);
  const url = new URL(adminUrl);
  url.pathname = `/${dbName}`;
  const dbAdmin = new pg.Client({ connectionString: url.toString() });
  await dbAdmin.connect();
  await dbAdmin.query(readFileSync(FIXTURE, "utf8"));
  await dbAdmin.query(`ALTER ROLE ai_sidecar PASSWORD '${password}'`);
  url.username = "ai_sidecar";
  url.password = password;
  const pool = new pg.Pool({ connectionString: url.toString(), max: 4 });
  return {
    pool,
    dbAdmin,
    close: async () => {
      await pool.end();
      await dbAdmin.end();
      await admin.query(`DROP DATABASE IF EXISTS ${dbName} WITH (FORCE)`);
      await admin.end();
    },
  };
}
