import { beforeEach, describe, expect, it } from "vitest";
import { changeNotes } from "../../src/server/coaching";
import { NOTE_ITEM, resetCoachingTables, seedNote } from "../helpers/coaching";
import { resetGameTables, seedAccount } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();
let alice: string;
let bob: string;
let accountId: string;

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await resetCoachingTables(db);
  alice = (await createTestSession(db, { email: "alice@example.invalid" })).user.id;
  bob = (await createTestSession(db, { email: "bob@example.invalid" })).user.id;
  ({ accountId } = await seedAccount(db, { userId: alice, username: "alice-notes" }));
});

describe("changeNotes", () => {
  it("only ready notes carry an item", async () => {
    await seedNote(db, accountId, { changeKey: "c1", seriesKey: "W1", onsetDate: "2026-10-02" });
    await seedNote(db, accountId, {
      changeKey: "c2",
      seriesKey: "W3",
      onsetDate: "2026-11-01",
      status: "pending",
    });
    await seedNote(db, accountId, {
      changeKey: "c3",
      seriesKey: "W3",
      onsetDate: "2026-09-01",
      status: "dropped",
    });
    const notes = await changeNotes(db, alice, accountId);
    expect(notes.map((n) => [n.changeKey, n.status])).toEqual([
      ["c2", "pending"],
      ["c1", "ready"],
      ["c3", "dropped"],
    ]);
    expect(notes[1]).toMatchObject({ item: NOTE_ITEM, seriesKey: "W1", onsetDate: "2026-10-02" });
    expect(notes[1]?.aiDisclosure?.label).toBe("Written by AI");
    expect(notes[0]).toMatchObject({ item: null, aiDisclosure: null });
    expect(notes[2]).toMatchObject({ item: null, aiDisclosure: null });
  });

  it("filters by series key", async () => {
    await seedNote(db, accountId, { changeKey: "c1", seriesKey: "W1" });
    await seedNote(db, accountId, { changeKey: "c2", seriesKey: "W3" });
    const notes = await changeNotes(db, alice, accountId, ["W3"]);
    expect(notes.map((n) => n.changeKey)).toEqual(["c2"]);
    expect(await changeNotes(db, alice, accountId, [])).toEqual([]);
  });

  it("foreign account sees nothing", async () => {
    await seedNote(db, accountId, { changeKey: "c1", seriesKey: "W1" });
    expect(await changeNotes(db, bob, accountId)).toEqual([]);
  });
});
