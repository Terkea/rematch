import { accountLinks, coachMessages } from "@ci/db/generated";
import { games } from "@ci/ui/fixtures";
import { eq, inArray, sql } from "drizzle-orm";
import { beforeEach, describe, expect, it } from "vitest";
import {
  CHAT_CREATE_LIMIT,
  createConversation,
  deleteConversation,
  getThread,
  listConversations,
} from "../../src/server/coach/index.js";
import { resetCoachTables, seedChunk, seedThread } from "../helpers/coach";
import { resetGameTables, seedAccount } from "../helpers/games";
import { createTestSession, resetAuthTables, webTestDb } from "../helpers/session";

const db = webTestDb();

let a: string;
let b: string;
let accountId: string;

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
});

const text = (t: string) => [{ kind: "text" as const, text: t }];

describe("threads", () => {
  it("creates a conversation only on a linked account", async () => {
    const made = await createConversation(db, a, {
      accountId,
      timeClass: "rapid",
      anchor: { kind: "weakness", weaknessId: "W3" },
    });
    expect(made.kind).toBe("created");
    const other = (await seedAccount(db, { username: "nobody" })).accountId;
    expect(
      await createConversation(db, a, { accountId: other, timeClass: "rapid", anchor: null }),
    ).toEqual({ kind: "not_linked" });
    if (made.kind !== "created") throw new Error("not created");
    const thread = await getThread(db, a, made.id);
    expect(thread).toMatchObject({
      id: made.id,
      accountId,
      timeClass: "rapid",
      title: "Chat about a weakness",
      anchor: { kind: "weakness", weaknessId: "W3" },
      messages: [],
    });
  });

  it("creating needs the user's own link, not another user's", async () => {
    const theirs = (await seedAccount(db, { userId: b, username: "theirs" })).accountId;
    expect(
      await createConversation(db, a, { accountId: theirs, timeClass: "rapid", anchor: null }),
    ).toEqual({ kind: "not_linked" });
    expect(
      (await createConversation(db, b, { accountId: theirs, timeClass: "rapid", anchor: null }))
        .kind,
    ).toBe("created");
  });

  it("creating is rate limited per user", async () => {
    for (let i = 0; i < CHAT_CREATE_LIMIT.max; i++) {
      await seedThread(db, { userId: a, accountId, timeClass: "rapid" });
    }
    // A new start (blitz, not a repeat of the seeded rapid ones) is refused.
    const r = await createConversation(db, a, { accountId, timeClass: "blitz", anchor: null });
    expect(r.kind).toBe("rate_limited");
    expect(
      (await createConversation(db, b, { accountId, timeClass: "rapid", anchor: null })).kind,
    ).toBe("created");
  });

  it("thread of another user is not found", async () => {
    const { threadId } = await seedThread(db, {
      userId: a,
      accountId,
      timeClass: "rapid",
      messages: [{ role: "user", status: "done", text: "Hi" }],
    });
    expect(await getThread(db, b, threadId)).toBeNull();
    expect(await getThread(db, a, threadId)).not.toBeNull();
    expect(await getThread(db, a, "not-a-uuid")).toBeNull();
  });

  it("list shows only the user's threads, newest first", async () => {
    const old = await seedThread(db, {
      userId: a,
      accountId,
      timeClass: "rapid",
      title: "Old",
      lastMessageAt: new Date("2026-11-01T00:00:00Z"),
    });
    const fresh = await seedThread(db, {
      userId: a,
      accountId,
      timeClass: "blitz",
      lastMessageAt: new Date("2026-11-20T00:00:00Z"),
    });
    await seedThread(db, { userId: b, accountId, timeClass: "rapid", title: "Theirs" });
    const list = await listConversations(db, a, accountId);
    expect(list.map((t) => t.id)).toEqual([fresh.threadId, old.threadId]);
    expect(list[0]).toMatchObject({ title: "Chat with your coach", anchor: null });
    expect(list[1]?.title).toBe("Old");
    expect(list[0]?.lastMessageAt).toBeInstanceOf(Date);
    expect(await listConversations(db, b, accountId)).toHaveLength(1);
  });

  it("delete of another user's thread deletes nothing", async () => {
    const { threadId, messageIds } = await seedThread(db, {
      userId: a,
      accountId,
      timeClass: "rapid",
      messages: [
        { role: "user", status: "done", text: "Q" },
        { role: "assistant", status: "done", segments: text("A") },
      ],
    });
    expect(await deleteConversation(db, b, threadId)).toBe(false);
    expect(await getThread(db, a, threadId)).not.toBeNull();
    expect(await deleteConversation(db, a, threadId)).toBe(true);
    expect(await getThread(db, a, threadId)).toBeNull();
    expect(
      await db.select().from(coachMessages).where(inArray(coachMessages.id, messageIds)),
    ).toEqual([]);
    expect(await deleteConversation(db, a, threadId)).toBe(false);
  });

  it("thread replays stored chunks", async () => {
    const { threadId, messageIds } = await seedThread(db, {
      userId: a,
      accountId,
      timeClass: "rapid",
      messages: [
        { role: "user", status: "done", text: "Q" },
        { role: "assistant", status: "streaming" },
      ],
    });
    const mid = messageIds[1] as string;
    await seedChunk(db, { userId: a, accountId, messageId: mid, seq: 2, segments: text("second") });
    await seedChunk(db, {
      userId: a,
      accountId,
      messageId: mid,
      seq: 1,
      segments: text("first"),
      interim: true,
    });
    const thread = await getThread(db, a, threadId);
    const answer = thread?.messages[1];
    expect(answer?.status).toBe("streaming");
    expect(answer?.streamed).toEqual([
      { seq: 1, segments: text("first"), interim: true },
      { seq: 2, segments: text("second"), interim: false },
    ]);
    expect(thread?.messages[0]?.streamed).toEqual([]);
  });

  it("streamed chunks are the owner's only", async () => {
    const { threadId, messageIds } = await seedThread(db, {
      userId: a,
      accountId,
      timeClass: "rapid",
      messages: [
        { role: "user", status: "done", text: "Q" },
        { role: "assistant", status: "pending" },
      ],
    });
    const mid = messageIds[1] as string;
    await seedChunk(db, { userId: b, accountId, messageId: mid, seq: 1, segments: text("forged") });
    await seedChunk(db, { userId: a, accountId, messageId: mid, seq: 2, segments: text("mine") });
    const thread = await getThread(db, a, threadId);
    expect(thread?.messages[1]?.streamed).toEqual([
      { seq: 2, segments: text("mine"), interim: false },
    ]);
  });

  it("done messages carry segments and disclosure, never raw text", async () => {
    const { threadId } = await seedThread(db, {
      userId: a,
      accountId,
      timeClass: "rapid",
      messages: [
        { role: "user", status: "done", text: "My question" },
        { role: "assistant", status: "done", text: "{{move:x}}", segments: text("Answer") },
        { role: "assistant", status: "failed", failureReason: "budget" },
      ],
    });
    const thread = await getThread(db, a, threadId);
    const [q, ans, failed] = thread?.messages ?? [];
    expect(q).toMatchObject({ seq: 1, role: "user", segments: text("My question") });
    expect(ans).toMatchObject({ seq: 2, role: "assistant", segments: text("Answer") });
    expect(ans?.aiDisclosure?.label).toBe("Written by AI");
    expect(ans).not.toHaveProperty("text");
    expect(failed).toMatchObject({ status: "failed", failureReason: "budget", segments: null });
  });

  it("engine checks are listed per message, in SAN from their real ply", async () => {
    const { threadId, messageIds } = await seedThread(db, {
      userId: a,
      accountId,
      timeClass: "rapid",
      messages: [
        { role: "user", status: "done", text: "Q" },
        { role: "assistant", status: "done", segments: text("A") },
      ],
    });
    const mid = messageIds[1] as string;
    const start = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    // A real position from the public fixture: after White's 28th move, Black to move.
    const plies = games.middlegameBlunder.plies;
    const after55 = plies[54]?.fenAfter as string;
    const [b56, w57] = [plies[55], plies[56]];
    expect(after55.split(" ")[1]).toBe("b");
    expect(after55.split(" ")[5]).toBe("28");
    await db.execute(sql`
      insert into interactive_evals (account_id, message_id, protocol, base_fen, moves, fen, status, created_at)
      values (${accountId}, ${mid}, 'p1', ${start}, array['e2e4','e7e5'], ${start}, 'done', now() - interval '3 minute'),
             (${accountId}, ${mid}, 'p1', ${after55}, array[${b56?.uci}, ${w57?.uci}], ${after55}, 'pending', now() - interval '2 minute'),
             (${accountId}, ${mid}, 'p1', ${start}, array['e2e4','e2e4'], ${start}, 'failed', now() - interval '1 minute'),
             (${accountId}, ${mid}, 'p1', 'not a fen', array['e2e4'], 'not a fen', 'failed', now()),
             (${accountId}, null, 'p1', ${start}, array['c2c4'], ${start}, 'done', now())`);
    const thread = await getThread(db, a, threadId);
    expect(thread?.messages[1]?.engineChecks).toEqual([
      { status: "done", moves: ["e4", "e5"], startPly: 1 },
      { status: "pending", moves: [b56?.san, w57?.san], startPly: 56 },
      // Like Rust pv_to_san: the line stops at the first illegal move.
      { status: "failed", moves: ["e4"], startPly: 1 },
      { status: "failed", moves: [], startPly: null },
    ]);
    expect(thread?.messages[0]?.engineChecks).toEqual([]);
  });

  it("read and delete still work after unlinking", async () => {
    const { threadId } = await seedThread(db, { userId: a, accountId, timeClass: "rapid" });
    await db.delete(accountLinks).where(eq(accountLinks.userId, a));
    expect(await getThread(db, a, threadId)).not.toBeNull();
    expect(await deleteConversation(db, a, threadId)).toBe(true);
  });
});
