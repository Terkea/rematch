import { startWorkers } from "./start-workers.js";

function requireEnv(name: string): string {
  const v = process.env[name];
  if (!v) throw new Error(`${name} is not set`);
  return v;
}

const workers = await startWorkers({
  REDIS_URL: requireEnv("REDIS_URL"),
  DATABASE_URL: requireEnv("DATABASE_URL"),
  STOCKFISH_PATH: requireEnv("STOCKFISH_PATH"),
  CONTACT_EMAIL: requireEnv("CONTACT_EMAIL"),
  LICHESS_TOKEN: process.env.LICHESS_TOKEN,
});
console.log("worker ready");
const stop = () => void workers.close().finally(() => process.exit(0));
process.on("SIGINT", stop);
process.on("SIGTERM", stop);
