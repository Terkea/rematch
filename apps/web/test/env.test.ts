import { describe, expect, it } from "vitest";
import { getEnv, resetEnvCache } from "../src/env";

const base = {
  DATABASE_URL: "postgres://postgres:postgres@localhost:5432/chess",
  REDIS_URL: "redis://localhost:6379",
  BETTER_AUTH_SECRET: "test-secret-test-secret-test-secret-1234",
};

describe("getEnv", () => {
  it("throws, naming BETTER_AUTH_SECRET, when it is missing", () => {
    const { BETTER_AUTH_SECRET: _omitted, ...rest } = base;
    expect(() => getEnv(rest)).toThrow(/BETTER_AUTH_SECRET/);
  });

  it("rejects a BETTER_AUTH_SECRET shorter than 32 characters", () => {
    expect(() => getEnv({ ...base, BETTER_AUTH_SECRET: "short" })).toThrow(/BETTER_AUTH_SECRET/);
  });

  it("parses CHESSCOM_ENABLED=1 to true and absent to false", () => {
    expect(getEnv({ ...base, CHESSCOM_ENABLED: "1" }).CHESSCOM_ENABLED).toBe(true);
    expect(getEnv(base).CHESSCOM_ENABLED).toBe(false);
  });

  it("defaults AI_BETA_MANUAL_REPORTS_PER_DAY to 1 and rejects nonsense", () => {
    expect(getEnv(base).AI_BETA_MANUAL_REPORTS_PER_DAY).toBe(1);
    expect(
      getEnv({ ...base, AI_BETA_MANUAL_REPORTS_PER_DAY: "3" }).AI_BETA_MANUAL_REPORTS_PER_DAY,
    ).toBe(3);
    expect(() => getEnv({ ...base, AI_BETA_MANUAL_REPORTS_PER_DAY: "0" })).toThrow(
      /AI_BETA_MANUAL_REPORTS_PER_DAY/,
    );
    expect(() => getEnv({ ...base, AI_BETA_MANUAL_REPORTS_PER_DAY: "x" })).toThrow(
      /AI_BETA_MANUAL_REPORTS_PER_DAY/,
    );
  });

  it("defaults LICHESS_CLIENT_ID to chess-improver", () => {
    expect(getEnv(base).LICHESS_CLIENT_ID).toBe("chess-improver");
  });

  it("turns INVITE_ONLY on by default and off only for exactly 0", () => {
    expect(getEnv(base).INVITE_ONLY).toBe(true);
    expect(getEnv({ ...base, INVITE_ONLY: "1" }).INVITE_ONLY).toBe(true);
    expect(getEnv({ ...base, INVITE_ONLY: "0" }).INVITE_ONLY).toBe(false);
  });

  it("parses INVITE_ALLOWLIST into trimmed lower-case emails", () => {
    expect(getEnv(base).INVITE_ALLOWLIST).toEqual([]);
    expect(
      getEnv({ ...base, INVITE_ALLOWLIST: " Dev@Example.invalid, ,b@example.invalid" })
        .INVITE_ALLOWLIST,
    ).toEqual(["dev@example.invalid", "b@example.invalid"]);
  });

  it("caches the parse of process.env", () => {
    resetEnvCache();
    Object.assign(process.env, base);
    expect(getEnv()).toBe(getEnv());
    resetEnvCache();
  });
});
