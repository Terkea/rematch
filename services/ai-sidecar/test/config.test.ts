import { homedir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { ConfigError, LOCAL_GUARD_MESSAGE, loadConfig } from "../src/config.js";

const TOKEN = "x".repeat(32);
const GUARD =
  "refusing to start: production needs ANTHROPIC_API_KEY or a cloud provider; a subscription token is not allowed (ADR 0062)";
const DEV = { AI_SIDECAR_ENV: "development" };
const DB = { AI_SESSION_DATABASE_URL: "postgres://ai_sidecar@db/chess" };

describe("loadConfig", () => {
  it("applies_the_defaults", () => {
    const c = loadConfig({ ...DEV, AI_SIDECAR_TOKEN: TOKEN });
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
        loadConfig({
          ...DEV,
          AI_SIDECAR_TOKEN: TOKEN,
          CLAUDE_CODE_OAUTH_TOKEN: "oat-1",
          ...DB,
          ...flag,
        }),
      ).toThrow(new ConfigError(GUARD));
    }
  });

  it("production_refuses_no_credentials", () => {
    expect(() =>
      loadConfig({ ...DEV, AI_SIDECAR_TOKEN: TOKEN, AI_SIDECAR_ENV: "production", ...DB }),
    ).toThrow(GUARD);
    // The auth guard is checked first, so a bare production start reports it.
    expect(() =>
      loadConfig({ ...DEV, AI_SIDECAR_TOKEN: TOKEN, AI_SIDECAR_ENV: "production" }),
    ).toThrow(GUARD);
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
      loadConfig({
        ...DEV,
        AI_SIDECAR_TOKEN: TOKEN,
        AI_SIDECAR_ENV: "production",
        ANTHROPIC_API_KEY: "k",
      }),
    ).toThrow(ConfigError);
  });

  it("anything_but_development_or_test_is_production", () => {
    const oauth = { AI_SIDECAR_TOKEN: TOKEN, CLAUDE_CODE_OAUTH_TOKEN: "oat-1", ...DB };
    for (const env of [{}, { NODE_ENV: "staging" }, { AI_SIDECAR_ENV: "prod" }, { NODE_ENV: "" }]) {
      expect(() => loadConfig({ ...oauth, ...env })).toThrow(GUARD);
    }
    for (const env of [
      { AI_SIDECAR_ENV: "development" },
      { AI_SIDECAR_ENV: "test" },
      { NODE_ENV: "development" },
      { NODE_ENV: "test" },
    ]) {
      expect(loadConfig({ ...oauth, ...env }).production).toBe(false);
    }
    // NODE_ENV=production wins over a development AI_SIDECAR_ENV.
    expect(() =>
      loadConfig({ ...oauth, AI_SIDECAR_ENV: "development", NODE_ENV: "production" }),
    ).toThrow(GUARD);
  });

  it("development_allows_oauth", () => {
    const c = loadConfig({ ...DEV, AI_SIDECAR_TOKEN: TOKEN, CLAUDE_CODE_OAUTH_TOKEN: "oat-1" });
    expect(c.auth).toBe("oauth");
    expect(c.production).toBe(false);
    expect(loadConfig({ ...DEV, AI_SIDECAR_TOKEN: TOKEN, ANTHROPIC_API_KEY: "" }).auth).toBe(
      "none",
    );
  });

  it("short_sidecar_token_is_rejected", () => {
    expect(() => loadConfig({ ...DEV, AI_SIDECAR_TOKEN: "x".repeat(31) })).toThrow(ConfigError);
    expect(() => loadConfig({})).toThrow(ConfigError);
  });

  it("reads_the_overrides", () => {
    const c = loadConfig({
      ...DEV,
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
    expect(() => loadConfig({ ...DEV, AI_SIDECAR_TOKEN: TOKEN, AI_SIDECAR_PORT: "abc" })).toThrow(
      ConfigError,
    );
  });

  it("listens_on_all_interfaces_unless_told_otherwise", () => {
    expect(loadConfig({ ...DEV, AI_SIDECAR_TOKEN: TOKEN }).host).toBe("0.0.0.0");
    expect(loadConfig({ ...DEV, AI_SIDECAR_TOKEN: TOKEN, AI_SIDECAR_HOST: "127.0.0.1" }).host).toBe(
      "127.0.0.1",
    );
  });
});

describe("local mode (the host's Claude Code login)", () => {
  const LOCAL = { ...DEV, AI_SIDECAR_TOKEN: TOKEN, AI_SIDECAR_LOCAL_CLAUDE: "1" };

  it("needs_the_flag_set_to_exactly_1", () => {
    expect(loadConfig(LOCAL).auth).toBe("local");
    for (const v of ["", "0", "true", "yes"]) {
      expect(loadConfig({ ...LOCAL, AI_SIDECAR_LOCAL_CLAUDE: v }).auth).toBe("none");
    }
  });

  it("refuses_another_credential_beside_it", () => {
    for (const extra of [
      { ANTHROPIC_API_KEY: "k" },
      { CLAUDE_CODE_OAUTH_TOKEN: "oat-1" },
      { CLAUDE_CODE_USE_BEDROCK: "1" },
    ]) {
      expect(() => loadConfig({ ...LOCAL, ...extra })).toThrow(ConfigError);
    }
    // Empty values are not credentials.
    expect(loadConfig({ ...LOCAL, ANTHROPIC_API_KEY: "", CLAUDE_CODE_OAUTH_TOKEN: "" }).auth).toBe(
      "local",
    );
  });

  it("refuses_to_start_in_production", () => {
    for (const env of [
      { NODE_ENV: "production" },
      { AI_SIDECAR_ENV: "production" },
      { AI_SIDECAR_ENV: "development", NODE_ENV: "production" },
    ]) {
      expect(() => loadConfig({ ...LOCAL, ...DB, ...env })).toThrow(
        new ConfigError(LOCAL_GUARD_MESSAGE),
      );
    }
  });

  it("uses_the_host_claude_config_dir_not_a_private_one", () => {
    // AI_SIDECAR_CONFIG_DIR is the container's private dir; it does not apply here.
    const c = loadConfig({ ...LOCAL, AI_SIDECAR_CONFIG_DIR: "/c" });
    expect(c.configDir).toBe(join(homedir(), ".claude"));
    expect(loadConfig({ ...LOCAL, CLAUDE_CONFIG_DIR: "/my/claude" }).configDir).toBe("/my/claude");
  });
});
