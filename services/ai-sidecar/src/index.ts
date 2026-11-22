import { mkdirSync } from "node:fs";
import pg from "pg";
import { ConfigError, loadConfig } from "./config.js";
import { Limiter } from "./limiter.js";
import { Metrics, stdoutLogger } from "./log.js";
import { createSidecarServer } from "./server.js";
import { PgSessionStore } from "./session-store.js";

/** EX_CONFIG from sysexits.h: the configuration is unusable. */
const EXIT_CONFIG = 78;

function main(): void {
  let cfg: ReturnType<typeof loadConfig>;
  try {
    cfg = loadConfig(process.env);
  } catch (e) {
    if (e instanceof ConfigError) {
      process.stderr.write(`${e.message}\n`);
      process.exit(EXIT_CONFIG);
    }
    throw e;
  }
  // An empty cwd and a private config dir for every child (ADR 0062). Both are scratch (tmpfs in
  // compose): durable chat transcripts live in Postgres through the session store.
  mkdirSync(cfg.workDir, { recursive: true, mode: 0o700 });
  // Local mode uses the host's own Claude config dir as it is; it is never created here.
  if (cfg.auth !== "local") mkdirSync(cfg.configDir, { recursive: true, mode: 0o700 });

  const pool = cfg.databaseUrl
    ? new pg.Pool({
        connectionString: cfg.databaseUrl,
        // Each running chat turn holds one connection for its lock, and the store needs more for
        // loads and appends meanwhile.
        max: 2 * cfg.maxConcurrency + 2,
        // Fail fast when the database is unreachable, so a turn does not sit on a limiter slot.
        connectionTimeoutMillis: 5000,
        keepAlive: true,
      })
    : undefined;
  pool?.on("error", () => stdoutLogger.warn({ msg: "session store pool error" }));
  const metrics = new Metrics();
  const shutdown = new AbortController();
  const store = pool
    ? new PgSessionStore(pool, {
        onNulStripped: (n) => metrics.nulStrippedEntries(n),
        onLockLost: () => {
          metrics.lockLostOnce();
          stdoutLogger.warn({
            msg: "turn lock connection lost; exclusion gone for this turn",
            event: "lock_lost",
          });
        },
      })
    : undefined;
  const server = createSidecarServer(cfg, {
    limiter: new Limiter(cfg.maxConcurrency, cfg.maxQueue, cfg.queueWaitMs),
    logger: stdoutLogger,
    metrics,
    shutdown: shutdown.signal,
    ...(store ? { store } : {}),
  });
  // Chat turns stream for minutes; the default request timeout would cut them off.
  server.requestTimeout = 0;
  server.listen(cfg.port, cfg.host, () => {
    stdoutLogger.info({ msg: `listening on ${cfg.host}:${cfg.port} (auth ${cfg.auth})` });
  });
  const stop = () => {
    // Abort in-flight calls first (each ends with a 503 or an error event and kills its claude
    // subprocess), so close() does not wait out a long chat turn.
    shutdown.abort();
    server.close(() => {
      void pool?.end().finally(() => process.exit(0));
      if (!pool) process.exit(0);
    });
  };
  process.on("SIGTERM", stop);
  process.on("SIGINT", stop);
}

main();
