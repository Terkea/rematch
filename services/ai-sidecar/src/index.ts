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
  // An empty cwd and a private config dir for every child (ADR-R3-6). Both are scratch (tmpfs in
  // compose): durable chat transcripts live in Postgres through the session store.
  mkdirSync(cfg.workDir, { recursive: true, mode: 0o700 });
  mkdirSync(cfg.configDir, { recursive: true, mode: 0o700 });

  const pool = cfg.databaseUrl
    ? new pg.Pool({ connectionString: cfg.databaseUrl, max: cfg.maxConcurrency + 2 })
    : undefined;
  pool?.on("error", () => stdoutLogger.warn({ msg: "session store pool error" }));
  const metrics = new Metrics();
  const shutdown = new AbortController();
  const store = pool
    ? new PgSessionStore(pool, { onNulStripped: (n) => metrics.nulStrippedEntries(n) })
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
  server.listen(cfg.port, "0.0.0.0", () => {
    stdoutLogger.info({ msg: `listening on ${cfg.port} (auth ${cfg.auth})` });
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
