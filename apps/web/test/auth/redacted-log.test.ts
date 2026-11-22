import { afterEach, describe, expect, it, vi } from "vitest";
import { getEnv } from "../../src/env.js";
import { redactedLog } from "../../src/server/auth/redacted-log.js";
import { createAuth } from "../../src/server/auth.js";

// T21 review Minor 7 (T27): Better Auth logs a failed DB write with the row it tried to write,
// and a magic-link verification row holds the address (`{"type":"magic-link","email":…}`). No
// email may reach the server log.

afterEach(() => vi.restoreAllMocks());

const printed = (spy: { mock: { calls: unknown[][] } }) =>
  spy.mock.calls
    .map((call) =>
      call
        .map((a) =>
          a instanceof Error ? `${a.message} ${a.stack}` : (JSON.stringify(a) ?? String(a)),
        )
        .join(" "),
    )
    .join("\n");

describe("the auth logger", () => {
  it("strips every email address from the message and its arguments", () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    redactedLog(
      "error",
      "Failed to create verification for adult@example.invalid",
      { identifier: "tok", value: '{"type":"magic-link","email":"adult@example.invalid"}' },
      new Error('insert failed: values ("kid@example.invalid")'),
      ["ana.b+1@sub.example.invalid"],
    );
    const out = printed(error);
    expect(out).not.toMatch(/@example\.invalid/);
    expect(out).toContain("Failed to create verification for [email]");
    expect(out).toContain("magic-link");
    expect(out).toContain("insert failed");
  });

  it("logs each level to its console method", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    redactedLog("warn", "careful");
    expect(warn).toHaveBeenCalledWith("[auth] careful");
  });

  it("is the logger Better Auth is configured with", () => {
    const env = getEnv({
      ...process.env,
      DATABASE_URL: "postgres://x/y_test",
      BETTER_AUTH_SECRET: "x".repeat(32),
      BETTER_AUTH_URL: "http://localhost:3100",
      REDIS_URL: "redis://r:6379",
      SMTP_URL: "smtp://m:1025",
    });
    const auth = createAuth(env, {} as never);
    expect(auth.options.logger?.log).toBe(redactedLog);
  });
});
