import { beforeEach, describe, expect, it } from "vitest";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";
import { insertAndPublish, linkAccount, resetEventTables } from "./helpers";

const db = webTestDb();
const { eventCursor } = await import("../../src/server/events/cursor");

beforeEach(async () => {
  await resetAuthTables(db);
  await resetEventTables(db);
});

describe("eventCursor", () => {
  it("is the newest event id of the user's own accounts, 0 when there is none", async () => {
    const { user } = await createTestSession(db, { birthMonth: "1990-01" });
    const { user: other } = await createTestSession(db, { birthMonth: "1990-01" });
    expect(await eventCursor(db, user.id)).toBe(0);
    const mine = await linkAccount(db, user.id);
    const theirs = await linkAccount(db, other.id);
    const e1 = await insertAndPublish(db, mine, "t");
    await insertAndPublish(db, theirs, "t");
    expect(await eventCursor(db, user.id)).toBe(e1);
  });
});
