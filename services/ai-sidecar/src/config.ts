import { homedir } from "node:os";
import { join } from "node:path";
import { ConfigError } from "./errors.js";

export { ConfigError };

/** `local`: the host's own Claude Code login (dev only, ADR 0062 Amendment 2). */
export type AuthKind = "api_key" | "cloud" | "oauth" | "local" | "none";

export type SidecarConfig = {
  port: number;
  /** Listen address: all interfaces in a container, loopback for the host run (`start:local`). */
  host: string;
  token: string;
  maxConcurrency: number;
  maxQueue: number;
  queueWaitMs: number;
  workDir: string;
  /** The child's Claude config dir: a private one, or in local mode the host's own. */
  configDir: string;
  production: boolean;
  auth: AuthKind;
  /** Session-store connection (the `ai_sidecar` role). Required in production. */
  databaseUrl: string | undefined;
  /** The credential and PATH variables the child may see; everything else is dropped. */
  childBaseEnv: Record<string, string>;
};

export const GUARD_MESSAGE =
  "refusing to start: production needs ANTHROPIC_API_KEY or a cloud provider; a subscription token is not allowed (ADR 0062)";

const CLOUD_FLAGS = [
  "CLAUDE_CODE_USE_BEDROCK",
  "CLAUDE_CODE_USE_VERTEX",
  "CLAUDE_CODE_USE_FOUNDRY",
];
export const LOCAL_GUARD_MESSAGE =
  "refusing to start: AI_SIDECAR_LOCAL_CLAUDE uses a personal Claude Code login and is dev only; production needs ANTHROPIC_API_KEY or a cloud provider (ADR 0062)";
const LOCAL_MIXED_MESSAGE =
  "AI_SIDECAR_LOCAL_CLAUDE=1 cannot be combined with ANTHROPIC_API_KEY, CLAUDE_CODE_OAUTH_TOKEN or a cloud provider";

/**
 * What the CLI needs to find the host's login (and to run) on POSIX and Windows. Names only, no
 * credential: the login is read from the config dir these point at.
 */
const LOCAL_NAMES = [
  "HOME",
  "USER",
  "LOGNAME",
  "USERPROFILE",
  "APPDATA",
  "LOCALAPPDATA",
  "HOMEDRIVE",
  "HOMEPATH",
  "USERNAME",
  "SystemRoot",
  "TEMP",
  "TMP",
  "TMPDIR",
  "CLAUDE_CONFIG_DIR",
];

/** Provider variables a cloud child needs (region, project, credentials of that provider). */
const CLOUD_PREFIXES = ["AWS_", "ANTHROPIC_BEDROCK_", "ANTHROPIC_VERTEX_", "ANTHROPIC_FOUNDRY_"];
const CLOUD_NAMES = [
  "CLOUD_ML_REGION",
  "GOOGLE_APPLICATION_CREDENTIALS",
  "GOOGLE_CLOUD_PROJECT",
  "CLAUDE_CODE_SKIP_BEDROCK_AUTH",
  "CLAUDE_CODE_SKIP_VERTEX_AUTH",
  "CLAUDE_CODE_SKIP_FOUNDRY_AUTH",
];

function intVar(env: NodeJS.ProcessEnv, name: string, fallback: number, min: number): number {
  const raw = env[name];
  if (raw === undefined || raw === "") return fallback;
  if (!/^\d+$/.test(raw) || Number(raw) < min) {
    throw new ConfigError(`${name} must be an integer >= ${min}`);
  }
  return Number(raw);
}

const set = (v: string | undefined): v is string => v !== undefined && v !== "";

function credentialKind(env: NodeJS.ProcessEnv): AuthKind {
  if (set(env.ANTHROPIC_API_KEY)) return "api_key";
  if (CLOUD_FLAGS.some((f) => env[f] === "1")) return "cloud";
  if (set(env.CLAUDE_CODE_OAUTH_TOKEN)) return "oauth";
  return "none";
}

/** Throws when local mode is asked for beside another credential: which one runs is ambiguous. */
export function authKind(env: NodeJS.ProcessEnv): AuthKind {
  const kind = credentialKind(env);
  if (env.AI_SIDECAR_LOCAL_CLAUDE !== "1") return kind;
  if (kind !== "none") throw new ConfigError(LOCAL_MIXED_MESSAGE);
  return "local";
}

function childBaseEnv(env: NodeJS.ProcessEnv, auth: AuthKind): Record<string, string> {
  const out: Record<string, string> = {};
  if (set(env.PATH)) out.PATH = env.PATH;
  if (auth === "local") {
    // Looked up by name: on Windows process.env is case-insensitive (Path, SYSTEMROOT).
    for (const name of LOCAL_NAMES) {
      const v = env[name];
      if (set(v)) out[name] = v;
    }
  } else if (auth === "api_key" && set(env.ANTHROPIC_API_KEY)) {
    out.ANTHROPIC_API_KEY = env.ANTHROPIC_API_KEY;
  } else if (auth === "oauth" && set(env.CLAUDE_CODE_OAUTH_TOKEN)) {
    out.CLAUDE_CODE_OAUTH_TOKEN = env.CLAUDE_CODE_OAUTH_TOKEN;
  } else if (auth === "cloud") {
    for (const [k, v] of Object.entries(env)) {
      if (!set(v)) continue;
      if (
        CLOUD_FLAGS.includes(k) ||
        CLOUD_NAMES.includes(k) ||
        CLOUD_PREFIXES.some((p) => k.startsWith(p))
      ) {
        out[k] = v;
      }
    }
  }
  return out;
}

function configDir(env: NodeJS.ProcessEnv, auth: AuthKind): string {
  if (auth === "local") {
    // Where the CLI itself looks: CLAUDE_CONFIG_DIR, else ~/.claude.
    return set(env.CLAUDE_CONFIG_DIR) ? env.CLAUDE_CONFIG_DIR : join(homedir(), ".claude");
  }
  return set(env.AI_SIDECAR_CONFIG_DIR) ? env.AI_SIDECAR_CONFIG_DIR : "/var/lib/ai-sidecar/claude";
}

export function loadConfig(env: NodeJS.ProcessEnv): SidecarConfig {
  const token = env.AI_SIDECAR_TOKEN ?? "";
  if (token.length < 32) {
    throw new ConfigError("AI_SIDECAR_TOKEN is required and must be at least 32 characters");
  }
  // Fail safe: only an explicit development or test mode relaxes the guard. AI_SIDECAR_ENV wins
  // over NODE_ENV, except that NODE_ENV=production always means production.
  const mode = set(env.AI_SIDECAR_ENV) ? env.AI_SIDECAR_ENV : env.NODE_ENV;
  const production = env.NODE_ENV === "production" || !(mode === "development" || mode === "test");
  const auth = authKind(env);
  if (production && auth === "local") throw new ConfigError(LOCAL_GUARD_MESSAGE);
  if (production && (auth === "oauth" || auth === "none")) {
    throw new ConfigError(GUARD_MESSAGE);
  }
  const databaseUrl = set(env.AI_SESSION_DATABASE_URL) ? env.AI_SESSION_DATABASE_URL : undefined;
  if (production && !databaseUrl) {
    throw new ConfigError(
      "refusing to start: production needs AI_SESSION_DATABASE_URL for the chat session store",
    );
  }
  return {
    port: intVar(env, "AI_SIDECAR_PORT", 7310, 1),
    host: set(env.AI_SIDECAR_HOST) ? env.AI_SIDECAR_HOST : "0.0.0.0",
    token,
    maxConcurrency: intVar(env, "AI_SIDECAR_MAX_CONCURRENCY", 4, 1),
    maxQueue: intVar(env, "AI_SIDECAR_MAX_QUEUE", 32, 0),
    queueWaitMs: intVar(env, "AI_SIDECAR_QUEUE_WAIT_MS", 30000, 1),
    workDir: set(env.AI_SIDECAR_WORK_DIR) ? env.AI_SIDECAR_WORK_DIR : "/var/lib/ai-sidecar/work",
    configDir: configDir(env, auth),
    production,
    auth,
    databaseUrl,
    childBaseEnv: childBaseEnv(env, auth),
  };
}
