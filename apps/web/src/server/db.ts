import { createDb, type Db } from "@ci/db";
import { getEnv } from "../env";

let cached: { url: string; db: Db } | undefined;

/** The app database for route handlers, rebuilt when `DATABASE_URL` changes (tests). */
export function getDb(): Db {
  const url = getEnv().DATABASE_URL;
  if (cached?.url !== url) cached = { url, db: createDb(url) };
  return cached.db;
}
