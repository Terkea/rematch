import { randomBytes } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import pg from "pg";

const MIGRATION = fileURLToPath(
  new URL("../../../db/migrations/20261121030000_ai_coaching.sql", import.meta.url),
);

/**
 * The `ai_sdk_session_entries` part of Task 17's real migration, read at test time so the schema
 * can never drift: the table and its indexes, and the `ai_sidecar` role block. The delete trigger
 * between them is left out (it needs `coach_conversations`, which the rest of the migration builds
 * on tables from earlier migrations).
 */
export function sessionStoreDdl(): string {
  const sql = readFileSync(MIGRATION, "utf8");
  const cut = (from: string, to?: string) => {
    const a = sql.indexOf(from);
    const b = to === undefined ? sql.length : sql.indexOf(to, a);
    if (a < 0 || b < 0) throw new Error(`migration section not found: ${from}`);
    return sql.slice(a, b);
  };
  return (
    cut("CREATE TABLE ai_sdk_session_entries", "CREATE FUNCTION coach_conversations_delete") +
    cut("-- The sidecar's restricted role")
  );
}

export type StoreDb = {
  /** Connected as the restricted `ai_sidecar` role. */
  pool: pg.Pool;
  /** Superuser connection to the throwaway database. */
  dbAdmin: pg.Client;
  close(): Promise<void>;
};

/**
 * A throwaway database with `ai_sdk_session_entries` from Task 17's real migration
 * and a pool that connects as the restricted `ai_sidecar` role. The role is created NOLOGIN, as in
 * the real migration; LOGIN and the password are granted here, as dev compose does.
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
  await dbAdmin.query(sessionStoreDdl());
  await dbAdmin.query(`ALTER ROLE ai_sidecar LOGIN PASSWORD '${password}'`);
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
