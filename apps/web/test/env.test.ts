import { describe, expect, it } from "vitest";
import { getEnv, resetEnvCache } from "../src/env";

const base = {
  DATABASE_URL: "postgres://postgres:postgres@localhost:5432/chess",
  REDIS_URL: "redis://localhost:6379",
  BETTER_AUTH_SECRET: "test-secret-test-secret-test-secret-1234",
  CONTACT_EMAIL: "dev@example.invalid",
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

  it("defaults LICHESS_CLIENT_ID to chess-improver", () => {
    expect(getEnv(base).LICHESS_CLIENT_ID).toBe("chess-improver");
  });

  it("caches the parse of process.env", () => {
    resetEnvCache();
    Object.assign(process.env, base);
    expect(getEnv()).toBe(getEnv());
    resetEnvCache();
  });
});
