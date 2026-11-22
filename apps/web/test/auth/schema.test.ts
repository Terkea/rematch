import * as tables from "@ci/db";
import { getSchema } from "better-auth/db";
import { getTableColumns, getTableName, type Table } from "drizzle-orm";
import { describe, expect, it } from "vitest";
import { webTestDb } from "../helpers/session";

webTestDb();
const { getAuth } = await import("../../src/server/auth");

// Stands in for `@better-auth/cli generate` (the CLI ends at 1.4.x; ADR-WEB-3): the tables
// Better Auth 1.7.24 expects for our options, compared with the schema generated from db/migrations.
describe("auth schema", () => {
  const expected = getSchema(getAuth().options);
  const byName = new Map<string, Table>(
    [tables.user, tables.session, tables.account, tables.verification].map((t) => [
      getTableName(t),
      t,
    ]),
  );

  it("needs exactly the four core tables (no plugin tables)", () => {
    expect(Object.keys(expected).sort()).toEqual(["account", "session", "user", "verification"]);
    expect(Object.keys(expected.user?.fields ?? {})).toEqual(
      expect.arrayContaining([
        "birthMonth",
        "ageCheckedAt",
        "marketingEmail",
        "publicShareDefault",
      ]),
    );
  });

  for (const [tableName, { fields }] of Object.entries(expected)) {
    it(`${tableName} has every field Better Auth writes, with matching nullability`, () => {
      const table = byName.get(tableName);
      expect(table, tableName).toBeDefined();
      const columns = getTableColumns(table as Table);
      for (const [key, field] of Object.entries(fields)) {
        const name = field.fieldName ?? key;
        const column = columns[name];
        expect(column, `${tableName}.${name}`).toBeDefined();
        if (field.required !== false && field.defaultValue === undefined) {
          expect(column?.notNull, `${tableName}.${name} not null`).toBe(true);
        }
      }
    });
  }
});
