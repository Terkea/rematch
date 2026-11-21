import { drizzle } from "drizzle-orm/postgres-js";
import postgres from "postgres";
import * as schema from "./schema.js";

export type Db = ReturnType<typeof createDb>;

export interface DbOptions {
  /** Server-side `statement_timeout` for every connection, so no query can hang a job forever. */
  statementTimeoutMs?: number;
}

export function createDb(url: string, opts: DbOptions = {}) {
  const sql = postgres(url, {
    onnotice: () => {},
    ...(opts.statementTimeoutMs !== undefined
      ? { connection: { statement_timeout: opts.statementTimeoutMs } }
      : {}),
  });
  return drizzle(sql, { schema });
}
