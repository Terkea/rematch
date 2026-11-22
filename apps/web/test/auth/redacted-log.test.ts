import { inspect } from "node:util";
import { afterEach, describe, expect, it, vi } from "vitest";
import { getEnv } from "../../src/env.js";
import { flushBackgroundTasks, runAfterResponse } from "../../src/server/auth/background.js";
import { redactedLog } from "../../src/server/auth/redacted-log.js";
import { createAuth } from "../../src/server/auth.js";

// T21 review Minor 7 (T27): Better Auth logs a failed DB write with the row it tried to write,
// and a magic-link verification row holds the address (`{"type":"magic-link","email":…}`). No
// email may reach the server log: not from Better Auth, and not from a failed background task
// (review-t27 I1: Drizzle query errors carry the bound address, nodemailer its recipients).

afterEach(() => vi.restoreAllMocks());

// What the console would print: util.inspect walks messages, stacks, causes and own fields.
const printed = (spy: { mock: { calls: unknown[][] } }) =>
  spy.mock.calls
    .map((call) =>
      call.map((a) => (typeof a === "string" ? a : inspect(a, { depth: 10 }))).join(" "),
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

  it("redacts an error's cause and own fields, and keeps them", () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    const pg = Object.assign(new Error("duplicate key value"), {
      code: "23505",
      detail: "Key (email)=(adult@example.invalid) already exists.",
    });
    const drizzle = new Error(
      'Failed query: insert into "magic_link_sends" values ($1)\nparams: adult@example.invalid',
      { cause: pg },
    );
    redactedLog("error", "magic link failed", drizzle);
    const out = printed(error);
    expect(out).not.toContain("@example.invalid");
    expect(out).toContain("23505");
    expect(out).toContain("Key (email)=([email]) already exists.");
    expect(out).toContain("params: [email]");
  });

  it("catches unusual addresses", () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    redactedLog("error", "o'brien!#x@example.invalid, ana@exämple.invalid, root@localhost");
    const out = printed(error);
    expect(out).not.toMatch(/brien|ana@|root@/);
  });
});

describe("background task failures", () => {
  it("log through the redaction, Drizzle and nodemailer errors included", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    const smtp = Object.assign(new Error("Can't send mail - all recipients were rejected"), {
      response: "550 5.1.1 <kid@example.invalid>: Recipient address rejected",
      rejected: ["kid@example.invalid"],
      envelope: { from: "no-reply@chess-improver.local", to: ["kid@example.invalid"] },
    });
    runAfterResponse("magic link", async () => {
      throw new Error("Failed query: select 1\nparams: adult@example.invalid", { cause: smtp });
    });
    await expect(flushBackgroundTasks()).rejects.toThrow("Failed query");
    const out = printed(error);
    expect(out).toContain("magic link failed");
    expect(out).toContain("Recipient address rejected");
    expect(out).not.toMatch(/@example\.invalid|@chess-improver\.local/);
  });
});
