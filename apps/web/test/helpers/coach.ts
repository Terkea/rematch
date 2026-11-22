import { type Db, insertUserEvent } from "@ci/db";
import { coachConversations, coachMessages } from "@ci/db/generated";
import { sql } from "drizzle-orm";
import type { Segment } from "../../src/server/coaching/types";
import type { TimeClass } from "../../src/server/games/types";
import { assertSeedable } from "./games";

// Test-only writers for the coach chat. Synthetic rows only; each first runs `assertSeedable`.

/** The disclosure an AI-written answer carries (shape of Rust `make_disclosure`). */
export const TEST_DISCLOSURE = {
  label: "Written by AI",
  note: "Test note.",
  model: "test-model",
  promptVersion: "chat@1",
  generatedAt: "2026-11-20T10:00:00.000Z",
};

export type SeedMessage = {
  role: "user" | "assistant";
  status: "pending" | "streaming" | "done" | "failed";
  text?: string;
  segments?: Segment[];
  failureReason?: string;
  /** Default: set on a `done` assistant message, else null. Pass null for fixed no-data copy. */
  aiDisclosure?: object | null;
  createdAt?: Date;
};

/** A conversation of `userId` on `accountId` with `messages` at seq 1, 2, … */
export async function seedThread(
  db: Db,
  opts: {
    userId: string;
    accountId: string;
    timeClass: TimeClass;
    title?: string | null;
    anchor?: object | null;
    lastMessageAt?: Date;
    messages?: SeedMessage[];
  },
): Promise<{ threadId: string; messageIds: string[] }> {
  await assertSeedable(db);
  const [conv] = await db
    .insert(coachConversations)
    .values({
      userId: opts.userId,
      accountId: opts.accountId,
      timeClass: opts.timeClass,
      title: opts.title ?? null,
      anchor: opts.anchor ?? null,
      ...(opts.lastMessageAt ? { lastMessageAt: opts.lastMessageAt } : {}),
    })
    .returning({ id: coachConversations.id });
  if (!conv) throw new Error("seedThread: no conversation row");
  const messageIds: string[] = [];
  let seq = 0;
  for (const m of opts.messages ?? []) {
    seq += 1;
    const disclosure =
      m.aiDisclosure !== undefined
        ? m.aiDisclosure
        : m.role === "assistant" && m.status === "done"
          ? TEST_DISCLOSURE
          : null;
    const [row] = await db
      .insert(coachMessages)
      .values({
        conversationId: conv.id,
        seq,
        role: m.role,
        status: m.status,
        text: m.text ?? null,
        segments: m.segments ?? null,
        failureReason: m.failureReason ?? null,
        aiDisclosure: disclosure,
        ...(m.createdAt ? { createdAt: m.createdAt } : {}),
      })
      .returning({ id: coachMessages.id });
    if (!row) throw new Error("seedThread: no message row");
    messageIds.push(row.id);
  }
  return { threadId: conv.id, messageIds };
}

/** A `chat.chunk` user event as the worker emits it (`emit_for_user`, Rust `ChatChunk`). */
export async function seedChunk(
  db: Db,
  opts: {
    userId: string;
    accountId: string;
    messageId: string;
    seq: number;
    segments: Segment[];
    interim?: boolean;
    /** The turn's AI attempt (Rust `ChatChunk.attempt`); omitted, the payload has none. */
    attempt?: number;
  },
): Promise<number> {
  await assertSeedable(db);
  return insertUserEvent(db, {
    accountId: opts.accountId,
    userId: opts.userId,
    type: "chat.chunk",
    payload: {
      messageId: opts.messageId,
      ...(opts.attempt === undefined ? {} : { attempt: opts.attempt }),
      seq: opts.seq,
      text: opts.segments.map((s) => ("text" in s ? s.text : "")).join(""),
      segments: opts.segments,
      citations: [],
      interim: opts.interim ?? false,
    },
  });
}

/** Empties the chat, job and event tables. */
export async function resetCoachTables(db: Db): Promise<void> {
  await assertSeedable(db);
  await db.execute(
    sql`truncate table coach_conversations, coach_messages, interactive_evals, jobs, user_events cascade`,
  );
}
