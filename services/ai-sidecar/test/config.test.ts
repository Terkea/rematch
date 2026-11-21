import { describe, expect, it } from "vitest";
import { ConfigError, loadConfig } from "../src/config.js";

const TOKEN = "x".repeat(32);
const GUARD =
  "refusing to start: production needs ANTHROPIC_API_KEY or a cloud provider; a subscription token is not allowed (ADR-R3-6)";
const DB = { AI_SESSION_DATABASE_URL: "postgres://ai_sidecar@db/chess" };

describe("loadConfig", () => {
  it("applies_the_defaults", () => {
    const c = loadConfig({ AI_SIDECAR_TOKEN: TOKEN });
    expect(c).toMatchObject({
      port: 7310,
      token: TOKEN,
      maxConcurrency: 4,
      maxQueue: 32,
      queueWaitMs: 30000,
      workDir: "/var/lib/ai-sidecar/work",
      configDir: "/var/lib/ai-sidecar/claude",
      production: false,
      auth: "none",
    });
  });

  it("production_refuses_subscription_only_token", () => {
    for (const flag of [{ AI_SIDECAR_ENV: "production" }, { NODE_ENV: "production" }]) {
      expect(() =>
        loadConfig({ AI_SIDECAR_TOKEN: TOKEN, CLAUDE_CODE_OAUTH_TOKEN: "oat-1", ...DB, ...flag }),
      ).toThrow(new ConfigError(GUARD));
    }
  });

  it("production_refuses_no_credentials", () => {
    expect(() =>
      loadConfig({ AI_SIDECAR_TOKEN: TOKEN, AI_SIDECAR_ENV: "production", ...DB }),
    ).toThrow(GUARD);
    // The auth guard is checked first, so a bare production start reports it.
    expect(() => loadConfig({ AI_SIDECAR_TOKEN: TOKEN, AI_SIDECAR_ENV: "production" })).toThrow(
      GUARD,
    );
  });

  it("production_accepts_api_key_and_cloud", () => {
    const base = { AI_SIDECAR_TOKEN: TOKEN, AI_SIDECAR_ENV: "production", ...DB };
    expect(loadConfig({ ...base, ANTHROPIC_API_KEY: "k" }).auth).toBe("api_key");
    for (const v of [
      "CLAUDE_CODE_USE_BEDROCK",
      "CLAUDE_CODE_USE_VERTEX",
      "CLAUDE_CODE_USE_FOUNDRY",
    ]) {
      expect(loadConfig({ ...base, [v]: "1" }).auth).toBe("cloud");
    }
    expect(loadConfig({ ...base, ANTHROPIC_API_KEY: "k", CLAUDE_CODE_OAUTH_TOKEN: "o" }).auth).toBe(
      "api_key",
    );
  });

  it("production_needs_the_session_store_database", () => {
    expect(() =>
      loadConfig({ AI_SIDECAR_TOKEN: TOKEN, AI_SIDECAR_ENV: "production", ANTHROPIC_API_KEY: "k" }),
    ).toThrow(ConfigError);
  });

  it("development_allows_oauth", () => {
    const c = loadConfig({ AI_SIDECAR_TOKEN: TOKEN, CLAUDE_CODE_OAUTH_TOKEN: "oat-1" });
    expect(c.auth).toBe("oauth");
    expect(c.production).toBe(false);
    expect(loadConfig({ AI_SIDECAR_TOKEN: TOKEN, ANTHROPIC_API_KEY: "" }).auth).toBe("none");
  });

  it("short_sidecar_token_is_rejected", () => {
    expect(() => loadConfig({ AI_SIDECAR_TOKEN: "x".repeat(31) })).toThrow(ConfigError);
    expect(() => loadConfig({})).toThrow(ConfigError);
  });

  it("reads_the_overrides", () => {
    const c = loadConfig({
      AI_SIDECAR_TOKEN: TOKEN,
      AI_SIDECAR_PORT: "9000",
      AI_SIDECAR_MAX_CONCURRENCY: "2",
      AI_SIDECAR_MAX_QUEUE: "5",
      AI_SIDECAR_QUEUE_WAIT_MS: "100",
      AI_SIDECAR_WORK_DIR: "/w",
      AI_SIDECAR_CONFIG_DIR: "/c",
    });
    expect(c).toMatchObject({ port: 9000, maxConcurrency: 2, maxQueue: 5, queueWaitMs: 100 });
    expect([c.workDir, c.configDir]).toEqual(["/w", "/c"]);
    expect(() => loadConfig({ AI_SIDECAR_TOKEN: TOKEN, AI_SIDECAR_PORT: "abc" })).toThrow(
      ConfigError,
    );
  });
});
