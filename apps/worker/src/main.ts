import { installGracefulShutdown } from "./lifecycle/shutdown.js";
import { log } from "./log.js";
import { startWorkers } from "./start-workers.js";

function requireEnv(name: string): string {
  const v = process.env[name];
  if (!v) throw new Error(`${name} is not set`);
  return v;
}

const running = await startWorkers({
  REDIS_URL: requireEnv("REDIS_URL"),
  DATABASE_URL: requireEnv("DATABASE_URL"),
  STOCKFISH_PATH: requireEnv("STOCKFISH_PATH"),
  CONTACT_EMAIL: requireEnv("CONTACT_EMAIL"),
  LICHESS_TOKEN: process.env.LICHESS_TOKEN,
});
installGracefulShutdown(running, { graceMs: 25_000 });
await running.queues.scheduleReconcile();
log.info({ workers: running.workers.length }, "worker ready");
