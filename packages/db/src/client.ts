import { drizzle } from "drizzle-orm/postgres-js";
import postgres from "postgres";
import * as schema from "./schema.js";

export type Db = ReturnType<typeof createDb>;

export interface DbOptions {
  /** Server-side `statement_timeout` for every connection, so no query can hang a job forever. */
  statementTimeoutMs?: number;
  /** Most connections the pool opens (postgres.js defaults to 10). */
  max?: number;
  /** Seconds an idle connection stays open before it is closed (postgres.js default: never). */
  idleTimeoutS?: number;
}

export function createDb(url: string, opts: DbOptions = {}) {
  const sql = postgres(url, {
    onnotice: () => {},
    ...(opts.max !== undefined ? { max: opts.max } : {}),
    ...(opts.idleTimeoutS !== undefined ? { idle_timeout: opts.idleTimeoutS } : {}),
    ...(opts.statementTimeoutMs !== undefined
      ? { connection: { statement_timeout: opts.statementTimeoutMs } }
      : {}),
  });
  return drizzle(sql, { schema });
}
