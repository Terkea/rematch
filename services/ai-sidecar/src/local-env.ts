import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

/** The sidecar token compose uses when `.env` sets none (docker-compose.yml). */
export const DEV_TOKEN = "dev-only-sidecar-token-change-me-0000000";

const set = (v: string | undefined): v is string => v !== undefined && v !== "";

/**
 * The session store from the host: the compose URL (`@postgres:5432`) becomes the host-published
 * port (`localhost:55432`, docker-compose.ports.yml). `AI_SIDECAR_LOCAL_DATABASE_URL` wins.
 */
function hostDatabaseUrl(env: NodeJS.ProcessEnv): string | undefined {
  if (set(env.AI_SIDECAR_LOCAL_DATABASE_URL)) return env.AI_SIDECAR_LOCAL_DATABASE_URL;
  if (!set(env.AI_SESSION_DATABASE_URL)) return undefined;
  const url = new URL(env.AI_SESSION_DATABASE_URL);
  if (url.hostname !== "postgres") return env.AI_SESSION_DATABASE_URL;
  url.hostname = "localhost";
  url.port = "55432";
  return url.toString();
}

/**
 * The environment of `pnpm --filter @ci/ai-sidecar start:local`: the sidecar on the host, on
 * loopback port 7310, with the host's own Claude Code login (ADR 0062 Amendment 2). NODE_ENV is
 * left alone, so `NODE_ENV=production` still refuses to start.
 */
export function localEnv(env: NodeJS.ProcessEnv): NodeJS.ProcessEnv {
  const out: NodeJS.ProcessEnv = {
    ...env,
    AI_SIDECAR_LOCAL_CLAUDE: "1",
    AI_SIDECAR_ENV: set(env.AI_SIDECAR_ENV) ? env.AI_SIDECAR_ENV : "development",
    AI_SIDECAR_HOST: "127.0.0.1",
    AI_SIDECAR_PORT: set(env.AI_SIDECAR_PORT) ? env.AI_SIDECAR_PORT : "7310",
    AI_SIDECAR_TOKEN: set(env.AI_SIDECAR_TOKEN) ? env.AI_SIDECAR_TOKEN : DEV_TOKEN,
    // An empty cwd of its own, outside every project, so no CLAUDE.md or .claude is near it.
    AI_SIDECAR_WORK_DIR: join(tmpdir(), "chess-improver-ai-sidecar", "work"),
  };
  const db = hostDatabaseUrl(env);
  if (db) out.AI_SESSION_DATABASE_URL = db;
  else delete out.AI_SESSION_DATABASE_URL;
  return out;
}

/**
 * The SDK's native `claude` for this platform (an optional dependency of the SDK). Throws with the
 * package name when the install skipped it.
 */
export function nativeClaudeBinary(
  platform: string = process.platform,
  arch: string = process.arch,
): string {
  const pkg = `@anthropic-ai/claude-agent-sdk-${platform}-${arch}`;
  const fromSdk = createRequire(import.meta.resolve("@anthropic-ai/claude-agent-sdk"));
  const candidates = platform === "linux" ? [pkg, `${pkg}-musl`] : [pkg];
  for (const name of candidates) {
    try {
      const dir = dirname(fromSdk.resolve(`${name}/package.json`));
      const bin = join(dir, platform === "win32" ? "claude.exe" : "claude");
      if (existsSync(bin)) return bin;
    } catch {
      // not installed; try the next candidate
    }
  }
  throw new Error(
    `the Claude Agent SDK's native binary is missing: install ${pkg} (run pnpm install without --no-optional)`,
  );
}
