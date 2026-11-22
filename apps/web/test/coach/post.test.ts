import { accountLinks, coachConversations, coachMessages, jobs } from "@ci/db/generated";
import { asc, eq } from "drizzle-orm";
import { beforeEach, describe, expect, it } from "vitest";
import {
  CHAT_POST_LIMIT,
  createConversation,
  deleteConversation,
  getThread,
  postUserMessage,
} from "../../src/server/coach/index.js";
import { resetCoachTables, seedThread } from "../helpers/coach";
import { resetGameTables, seedAccount } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();

let a: string;
let b: string;
let accountId: string;
let threadId: string;

beforeEach(async () => {
  await resetAuthTables(db);
  await resetGameTables(db);
  await resetCoachTables(db);
  a = (await createTestSession(db)).user.id;
  b = (await createTestSession(db)).user.id;
  accountId = (await seedAccount(db, { userId: a, username: "shared" })).accountId;
  await db
    .insert(accountLinks)
    .values({ userId: b, linkedAccountId: accountId, method: "username" });
  const made = await createConversation(db, a, { accountId, timeClass: "rapid", anchor: null });
  if (made.kind !== "created") throw new Error("not created");
  threadId = made.id;
});

const messagesOf = (id: string) =>
  db
    .select()
    .from(coachMessages)
    .where(eq(coachMessages.conversationId, id))
    .orderBy(asc(coachMessages.seq));

describe("postUserMessage", () => {
  it("posts the user turn and a pending answer", async () => {
    const before = (
      await db.select().from(coachConversations).where(eq(coachConversations.id, threadId))
    )[0]?.lastMessageAt;
    const anchor = { kind: "move" as const, gameId: "g-1", ply: 14 };
    const r = await postUserMessage(db, a, threadId, "Why was 14. Nf5 bad?", anchor);
    expect(r.kind).toBe("posted");
    if (r.kind !== "posted") throw new Error("not posted");
    const rows = await messagesOf(threadId);
    expect(rows.map((m) => [m.seq, m.role, m.status])).toEqual([
      [1, "user", "done"],
      [2, "assistant", "pending"],
    ]);
    expect(rows[0]).toMatchObject({ id: r.userMessageId, text: "Why was 14. Nf5 bad?", anchor });
    expect(rows[1]).toMatchObject({ id: r.assistantMessageId, text: null, anchor: null });
    const all = await db.select().from(jobs);
    expect(all).toHaveLength(1);
    expect(all[0]).toMatchObject({
      kind: "coach-chat",
      payload: { messageId: r.assistantMessageId },
      dedupeKey: `chat:${r.assistantMessageId}`,
      userKey: accountId,
      priority: 10,
      maxAttempts: 2,
      state: "queued",
    });
    const after = (
      await db.select().from(coachConversations).where(eq(coachConversations.id, threadId))
    )[0]?.lastMessageAt;
    expect(after?.getTime()).toBeGreaterThan(before?.getTime() ?? 0);
  });

  it("seq continues after existing messages", async () => {
    const seeded = await seedThread(db, {
      userId: a,
      accountId,
      timeClass: "rapid",
      messages: [
        { role: "user", status: "done", text: "Q1" },
        { role: "assistant", status: "done", segments: [{ kind: "text", text: "A1" }] },
        { role: "user", status: "done", text: "Q2" },
        { role: "assistant", status: "failed", failureReason: "budget" },
      ],
    });
    const r = await postUserMessage(db, a, seeded.threadId, "Q3", null);
    expect(r.kind).toBe("posted");
    const rows = await messagesOf(seeded.threadId);
    expect(rows.slice(-2).map((m) => [m.seq, m.role, m.status])).toEqual([
      [5, "user", "done"],
      [6, "assistant", "pending"],
    ]);
  });

  it("second post while answering is busy", async () => {
    expect((await postUserMessage(db, a, threadId, "First", null)).kind).toBe("posted");
    expect(await postUserMessage(db, a, threadId, "Second", null)).toEqual({ kind: "busy" });
    await db.update(coachMessages).set({ status: "streaming" }).where(eq(coachMessages.seq, 2));
    expect(await postUserMessage(db, a, threadId, "Third", null)).toEqual({ kind: "busy" });
    await db.update(coachMessages).set({ status: "done" }).where(eq(coachMessages.seq, 2));
    expect((await postUserMessage(db, a, threadId, "Fourth", null)).kind).toBe("posted");
    expect(await db.select().from(jobs)).toHaveLength(2);
  });

  it("two concurrent posts give one posted and one busy", async () => {
    const results = await Promise.all([
      postUserMessage(db, a, threadId, "One", null),
      postUserMessage(db, a, threadId, "Two", null),
    ]);
    expect(results.map((r) => r.kind).sort()).toEqual(["busy", "posted"]);
    expect(await messagesOf(threadId)).toHaveLength(2);
    expect(await db.select().from(jobs)).toHaveLength(1);
  });

  it("blank question is empty", async () => {
    expect(await postUserMessage(db, a, threadId, "  \n\t ", null)).toEqual({ kind: "empty" });
    expect(await postUserMessage(db, a, threadId, "", null)).toEqual({ kind: "empty" });
    expect(await messagesOf(threadId)).toHaveLength(0);
  });

  it("2001 code points is too long", async () => {
    expect(await postUserMessage(db, a, threadId, "a".repeat(2001), null)).toEqual({
      kind: "too_long",
    });
    // 2000 code points, 2001 UTF-16 units: counted like Rust's chars().
    const emoji = `${"a".repeat(1999)}\u{1F600}`;
    expect(emoji.length).toBe(2001);
    expect((await postUserMessage(db, a, threadId, emoji, null)).kind).toBe("posted");
    expect(await db.select().from(jobs)).toHaveLength(1);
  });

  it("post to another user's thread is not found", async () => {
    expect(await postUserMessage(db, b, threadId, "Sneaky", null)).toEqual({
      kind: "not_found",
    });
    expect(await postUserMessage(db, a, "not-a-uuid", "Hi", null)).toEqual({
      kind: "not_found",
    });
    expect(await messagesOf(threadId)).toHaveLength(0);
    expect(await db.select().from(jobs)).toHaveLength(0);
  });

  it("post after unlinking is not_linked; read and delete still work", async () => {
    await db.delete(accountLinks).where(eq(accountLinks.userId, a));
    expect(await postUserMessage(db, a, threadId, "Still there?", null)).toEqual({
      kind: "not_linked",
    });
    expect(await messagesOf(threadId)).toHaveLength(0);
    expect(await getThread(db, a, threadId)).not.toBeNull();
    expect(await deleteConversation(db, a, threadId)).toBe(true);
  });

  it("posting is rate limited per user", async () => {
    const ids: string[] = [];
    for (let i = 0; i < CHAT_POST_LIMIT.max; i++) {
      const t = await seedThread(db, { userId: a, accountId, timeClass: "rapid" });
      ids.push(t.threadId);
      expect((await postUserMessage(db, a, t.threadId, `Q${i}`, null)).kind).toBe("posted");
    }
    const r = await postUserMessage(db, a, threadId, "One too many", null);
    expect(r.kind).toBe("rate_limited");
    if (r.kind !== "rate_limited") throw new Error("not limited");
    expect(r.retryAt.getTime()).toBeGreaterThan(Date.now());
    expect(r.retryAt.getTime()).toBeLessThanOrEqual(Date.now() + CHAT_POST_LIMIT.windowMs + 1000);
    // Another user's posts are not counted against them.
    const theirs = await seedThread(db, { userId: b, accountId, timeClass: "rapid" });
    expect((await postUserMessage(db, b, theirs.threadId, "Mine", null)).kind).toBe("posted");
  });
});
