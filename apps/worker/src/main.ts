import { installGracefulShutdown } from "./lifecycle/shutdown.js";
import { errorMessage, log } from "./log.js";
import { startWorkers } from "./start-workers.js";

function requireEnv(name: string): string {
  const v = process.env[name];
  if (!v) throw new Error(`${name} is not set`);
  return v;
}

// startWorkers closes whatever it created if it fails partway.
const running = await startWorkers({
  REDIS_URL: requireEnv("REDIS_URL"),
  DATABASE_URL: requireEnv("DATABASE_URL"),
  STOCKFISH_PATH: requireEnv("STOCKFISH_PATH"),
  CONTACT_EMAIL: requireEnv("CONTACT_EMAIL"),
  LICHESS_TOKEN: process.env.LICHESS_TOKEN,
  LICHESS_SINCE_LOOKBACK_DAYS: process.env.LICHESS_SINCE_LOOKBACK_DAYS,
});
const shutdownNow = installGracefulShutdown(running, { graceMs: 25_000 });
try {
  await running.queues.scheduleReconcile();
  log.info({ workers: running.workers.length }, "worker ready");
} catch (err) {
  log.error({ err: errorMessage(err) }, "could not schedule the reconciler");
  shutdownNow("startup failed", 1);
}
