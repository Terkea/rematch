import { describe, expect, it } from "vitest";
import { assertSafeTestDatabase } from "../src/test-guard.js";

describe("assertSafeTestDatabase", () => {
  const test = "postgres://u:p@h:5432/chess_test";

  it("accepts a *_test database that differs from DATABASE_URL", () => {
    expect(assertSafeTestDatabase(test, "postgres://u:p@h:5432/chess")).toBe(test);
    expect(assertSafeTestDatabase(test, undefined)).toBe(test);
  });

  it("rejects a database name not ending in _test", () => {
    expect(() => assertSafeTestDatabase("postgres://u:p@h:5432/chess")).toThrow(/_test/);
  });

  it("rejects when DATABASE_URL_TEST equals DATABASE_URL", () => {
    expect(() => assertSafeTestDatabase(test, test)).toThrow(/equals DATABASE_URL/);
  });

  it("rejects a missing URL", () => {
    expect(() => assertSafeTestDatabase(undefined)).toThrow(/not set/);
  });
});
