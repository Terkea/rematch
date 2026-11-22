import { createDb, type Db } from "@ci/db";
import { getEnv } from "../env";

/** Most connections the web process opens, in total. */
export const WEB_POOL_MAX = 10;
/** Seconds an idle connection is kept; a dev server then returns to a handful of connections. */
export const WEB_POOL_IDLE_S = 20;

// Next builds every route into its own module graph (and re-evaluates modules on hot reload), so a
// pool kept in a module variable is created once per graph and the pools add up until Postgres
// refuses clients. The one pool lives on globalThis instead: one per process, whoever imports it.
const KEY = Symbol.for("chess-improver.web.db");
type Holder = { [KEY]?: { url: string; db: Db } };

/** The app database for every server module, rebuilt when `DATABASE_URL` changes (tests). */
export function getDb(): Db {
  const url = getEnv().DATABASE_URL;
  const holder = globalThis as Holder;
  const cached = holder[KEY];
  if (cached?.url === url) return cached.db;
  const db = createDb(url, { max: WEB_POOL_MAX, idleTimeoutS: WEB_POOL_IDLE_S });
  holder[KEY] = { url, db };
  return db;
}
