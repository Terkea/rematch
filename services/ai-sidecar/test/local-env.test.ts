import { existsSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { LOCAL_GUARD_MESSAGE, loadConfig } from "../src/config.js";
import { DEV_TOKEN, localEnv, nativeClaudeBinary } from "../src/local-env.js";

describe("localEnv (pnpm start:local)", () => {
  it("turns_on_local_mode_on_loopback_7310", () => {
    const env = localEnv({ PATH: "/usr/bin" });
    expect(env).toMatchObject({
      AI_SIDECAR_LOCAL_CLAUDE: "1",
      AI_SIDECAR_ENV: "development",
      AI_SIDECAR_HOST: "127.0.0.1",
      AI_SIDECAR_PORT: "7310",
      // The compose default, so the Docker worker and the host sidecar agree without a .env.
      AI_SIDECAR_TOKEN: DEV_TOKEN,
    });
    expect(DEV_TOKEN).toBe("dev-only-sidecar-token-change-me-0000000");
    const cfg = loadConfig(env);
    expect([cfg.auth, cfg.host, cfg.port, cfg.databaseUrl]).toEqual([
      "local",
      "127.0.0.1",
      7310,
      undefined,
    ]);
  });

  it("keeps_the_token_from_the_env_file", () => {
    expect(localEnv({ AI_SIDECAR_TOKEN: "t".repeat(40) }).AI_SIDECAR_TOKEN).toBe("t".repeat(40));
  });

  it("points_the_session_store_at_the_host_published_postgres", () => {
    const inCompose = "postgres://ai_sidecar:pw@postgres:5432/chess";
    expect(localEnv({ AI_SESSION_DATABASE_URL: inCompose }).AI_SESSION_DATABASE_URL).toBe(
      "postgres://ai_sidecar:pw@localhost:55432/chess",
    );
    // An explicit host URL wins.
    expect(
      localEnv({
        AI_SESSION_DATABASE_URL: inCompose,
        AI_SIDECAR_LOCAL_DATABASE_URL: "postgres://ai_sidecar:x@127.0.0.1:6000/chess",
      }).AI_SESSION_DATABASE_URL,
    ).toBe("postgres://ai_sidecar:x@127.0.0.1:6000/chess");
    // A URL that is not the compose host is kept; none means no store.
    expect(
      localEnv({ AI_SESSION_DATABASE_URL: "postgres://a:b@localhost:55432/chess" })
        .AI_SESSION_DATABASE_URL,
    ).toBe("postgres://a:b@localhost:55432/chess");
    expect(localEnv({ AI_SESSION_DATABASE_URL: "" })).not.toHaveProperty("AI_SESSION_DATABASE_URL");
  });

  it("never_overrides_production", () => {
    expect(() => loadConfig(localEnv({ NODE_ENV: "production" }))).toThrow(LOCAL_GUARD_MESSAGE);
  });
});

describe("nativeClaudeBinary", () => {
  it("finds_the_sdk_binary_of_this_platform", () => {
    expect(existsSync(nativeClaudeBinary())).toBe(true);
  });

  it("names_the_missing_package", () => {
    expect(() => nativeClaudeBinary("aix", "ppc64")).toThrow(
      /@anthropic-ai\/claude-agent-sdk-aix-ppc64/,
    );
  });
});
