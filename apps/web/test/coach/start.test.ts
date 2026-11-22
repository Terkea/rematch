import { coachConversations, coachMessages, jobs } from "@ci/db/generated";
import { eq, sql } from "drizzle-orm";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  CHAT_POST_LIMIT,
  checkQuestion,
  createConversation,
  postUserMessage,
  startConversation,
} from "../../src/server/coach/index.js";
import { resetCoachTables, seedThread } from "../helpers/coach";
import { resetGameTables, seedAccount } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();

let a: string;
let accountId: string;

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await resetCoachTables(db);
  a = (await createTestSession(db)).user.id;
  accountId = (await seedAccount(db, { userId: a, username: "starter" })).accountId;
});

afterEach(async () => {
  await db.execute(sql`drop trigger if exists test_fail_jobs on jobs`);
  await db.execute(sql`drop function if exists test_fail_jobs()`);
});

/** Makes every insert into `jobs` fail, so an enqueue throws inside its transaction. */
async function breakEnqueue() {
  await db.execute(sql`
    create function test_fail_jobs() returns trigger language plpgsql as $$
    begin raise exception 'enqueue refused (test)'; end $$`);
  await db.execute(sql`
    create trigger test_fail_jobs before insert on jobs
    for each row execute function test_fail_jobs()`);
}

const conversations = () => db.select().from(coachConversations);
const start = (question: string, anchor: object | null = null) =>
  startConversation(db, a, {
    accountId,
    timeClass: "rapid",
    anchor: anchor as never,
    question,
  });

describe("startConversation (create and first post in one transaction)", () => {
  it("creates the thread, both turns and the job", async () => {
    const r = await start("How do I stop hanging pieces?");
    expect(r.kind).toBe("started");
    if (r.kind !== "started") throw new Error("not started");
    const rows = await db
      .select()
      .from(coachMessages)
      .where(eq(coachMessages.conversationId, r.id));
    expect(rows.map((m) => [m.seq, m.role, m.status]).sort()).toEqual([
      [1, "user", "done"],
      [2, "assistant", "pending"],
    ]);
    const [job] = await db.select().from(jobs);
    expect(job?.payload).toEqual({ messageId: r.assistantMessageId });
  });

  it("a refused first post leaves no thread behind", async () => {
    for (let i = 0; i < CHAT_POST_LIMIT.max; i++) {
      const t = await seedThread(db, { userId: a, accountId, timeClass: "rapid" });
      expect((await postUserMessage(db, a, t.threadId, `Q${i}`, null)).kind).toBe("posted");
    }
    const before = (await conversations()).length;
    const r = await start("One more");
    expect(r.kind).toBe("rate_limited");
    expect(await conversations()).toHaveLength(before);
  });

  it("a failed enqueue rolls back the thread and both turns", async () => {
    await breakEnqueue();
    await expect(start("Will this stick?")).rejects.toThrow();
    expect(await conversations()).toHaveLength(0);
    expect(await db.select().from(coachMessages)).toHaveLength(0);
  });

  it("a retried or double-submitted start gives the same thread, one job", async () => {
    const anchor = { kind: "weakness", weaknessId: "W3" };
    const [x, y] = await Promise.all([start("Same", anchor), start("Same", anchor)]);
    const z = await start("Same", anchor);
    expect([x.kind, y.kind, z.kind]).toEqual(["started", "started", "started"]);
    if (x.kind !== "started" || y.kind !== "started" || z.kind !== "started") throw new Error();
    expect(y.id).toBe(x.id);
    expect(z.id).toBe(x.id);
    expect(z.assistantMessageId).toBe(x.assistantMessageId);
    expect(await conversations()).toHaveLength(1);
    expect(await db.select().from(jobs)).toHaveLength(1);
    // A different question, or another anchor, is a new chat.
    const other = await start("Different", anchor);
    const bare = await start("Same", null);
    if (other.kind !== "started" || bare.kind !== "started") throw new Error();
    expect(new Set([x.id, other.id, bare.id]).size).toBe(3);
  });

  it("a double-submitted empty create gives the same thread", async () => {
    const opts = { accountId, timeClass: "blitz" as const, anchor: null };
    const [x, y] = await Promise.all([
      createConversation(db, a, opts),
      createConversation(db, a, opts),
    ]);
    if (x.kind !== "created" || y.kind !== "created") throw new Error("not created");
    expect(y.id).toBe(x.id);
    expect(await conversations()).toHaveLength(1);
  });
});

describe("postUserMessage atomicity", () => {
  it("a failed enqueue leaves neither turn", async () => {
    const t = await seedThread(db, { userId: a, accountId, timeClass: "rapid" });
    const before = (await conversations())[0]?.lastMessageAt.getTime();
    await breakEnqueue();
    await expect(postUserMessage(db, a, t.threadId, "Hello?", null)).rejects.toThrow();
    expect(await db.select().from(coachMessages)).toHaveLength(0);
    expect((await conversations())[0]?.lastMessageAt.getTime()).toBe(before);
  });
});

describe("checkQuestion trims like Rust str::trim (Unicode White_Space)", () => {
  it("whitespace-only is empty, a BOM is not whitespace", () => {
    expect(checkQuestion(" \t\n  　")).toBe("empty");
    expect(checkQuestion("﻿")).toBeNull();
    expect(checkQuestion("​")).toBeNull();
  });
});
