import { accountLinks } from "@ci/db";
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

  it("never counts another user's own event on a shared account", async () => {
    const { user } = await createTestSession(db, { birthMonth: "1990-01" });
    const { user: other } = await createTestSession(db, { birthMonth: "1990-01" });
    const shared = await linkAccount(db, user.id);
    await db.insert(accountLinks).values({
      userId: other.id,
      linkedAccountId: shared,
      method: "location_code",
    });
    const e1 = await insertAndPublish(db, shared, "t");
    await insertAndPublish(db, shared, "chat.chunk", {}, user.id);
    expect(await eventCursor(db, other.id)).toBe(e1);
  });
});
