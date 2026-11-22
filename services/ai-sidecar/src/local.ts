/**
 * `pnpm --filter @ci/ai-sidecar start:local`: the sidecar on the host with the host's own Claude
 * Code login, no key or token (dev only, ADR 0062 Amendment 2). The script loads the repo `.env`.
 */
import { localEnv, nativeClaudeBinary } from "./local-env.js";

try {
  nativeClaudeBinary();
} catch (e) {
  process.stderr.write(`${(e as Error).message}\n`);
  process.exit(78);
}
const env = localEnv(process.env);
Object.assign(process.env, env);
// The compose URL from .env does not resolve on the host: no store unless localEnv mapped it.
if (env.AI_SESSION_DATABASE_URL === undefined) delete process.env.AI_SESSION_DATABASE_URL;
await import("./index.js");
