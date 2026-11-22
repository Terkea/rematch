import type { Db } from "@ci/db";
import {
  accountLinks,
  coachConversations,
  coachMessages,
  interactiveEvals,
  userEvents,
} from "@ci/db/generated";
import { and, asc, count, desc, eq, gte, inArray, sql } from "drizzle-orm";
import type { AiDisclosure, Segment } from "../coaching/types.js";
import { readOnly } from "../games/read-only.js";
import type { TimeClass } from "../games/types.js";
import { type ChatAnchor, fallbackTitle, parseAnchor } from "./anchor.js";

// Port of `backend/crates/ai/src/chat/threads.rs` (R3 Task 25) for reads, creates and deletes.
// A thread belongs to the USER who started it (review I3): every query here is scoped by
// `coach_conversations.user_id`, so another user of the same linked account never sees or
// deletes it; their id is simply not found.

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
export const isUuid = (s: string): boolean => UUID.test(s);

/** Rust `MAX_LIST`. */
export const MAX_LIST = 100;

/** New conversations one user may start per window (abuse bound; the quota is the job's). */
export const CHAT_CREATE_LIMIT = { max: 30, windowMs: 60 * 60 * 1000 } as const;

export type MessageStatus = "pending" | "streaming" | "done" | "failed";

export type StreamedChunk = { seq: number; segments: Segment[]; interim: boolean };
export type EngineCheck = { status: "pending" | "done" | "failed"; moves: string[] };

export type ThreadMessage = {
  id: string;
  seq: number;
  role: "user" | "assistant";
  status: MessageStatus;
  anchor: ChatAnchor | null;
  /** Stored segments (model text is only ever rendered from these); a user turn is one text. */
  segments: Segment[] | null;
  failureReason: string | null;
  aiDisclosure: AiDisclosure | null;
  createdAt: Date;
  /** The chunks already streamed for a `pending` or `streaming` answer (R5), in seq order. */
  streamed: StreamedChunk[];
  /** The message's interactive engine checks (`interactive_evals.message_id`), oldest first. */
  engineChecks: EngineCheck[];
};

export type Thread = {
  id: string;
  accountId: string;
  timeClass: TimeClass;
  title: string;
  anchor: ChatAnchor | null;
  messages: ThreadMessage[];
};

export type ConversationSummary = {
  id: string;
  title: string;
  anchor: ChatAnchor | null;
  lastMessageAt: Date;
};

/** Whether the user has any link to the account (verified or not, as `userAccountIds`). */
export async function isLinked(db: Db, userId: string, accountId: string): Promise<boolean> {
  if (!isUuid(accountId)) return false;
  const rows = await db
    .select({ one: sql<number>`1` })
    .from(accountLinks)
    .where(and(eq(accountLinks.userId, userId), eq(accountLinks.linkedAccountId, accountId)))
    .limit(1);
  return rows.length > 0;
}

/** Starts an untitled conversation of `userId` (the first answer sets the title). */
export async function createConversation(
  db: Db,
  userId: string,
  opts: { accountId: string; timeClass: TimeClass; anchor: ChatAnchor | null },
): Promise<
  { kind: "created"; id: string } | { kind: "not_linked" } | { kind: "rate_limited"; retryAt: Date }
> {
  if (!(await isLinked(db, userId, opts.accountId))) return { kind: "not_linked" };
  return db.transaction(async (tx) => {
    // Serialises one user's creates, so the window count below is exact.
    await tx.execute(sql`select pg_advisory_xact_lock(30032, hashtext(${userId}))`);
    const since = new Date(Date.now() - CHAT_CREATE_LIMIT.windowMs);
    const [recent] = await tx
      .select({ n: count(), oldest: sql<string | null>`min(${coachConversations.createdAt})` })
      .from(coachConversations)
      .where(and(eq(coachConversations.userId, userId), gte(coachConversations.createdAt, since)));
    if ((recent?.n ?? 0) >= CHAT_CREATE_LIMIT.max && recent?.oldest) {
      const retryAt = new Date(new Date(recent.oldest).getTime() + CHAT_CREATE_LIMIT.windowMs);
      return { kind: "rate_limited" as const, retryAt };
    }
    const [row] = await tx
      .insert(coachConversations)
      .values({
        accountId: opts.accountId,
        userId,
        timeClass: opts.timeClass,
        title: null,
        anchor: opts.anchor,
      })
      .returning({ id: coachConversations.id });
    if (!row) throw new Error("createConversation: no row");
    return { kind: "created" as const, id: row.id };
  });
}

/** The user's conversations on `accountId`, most recently active first (limit 1..=100). */
export async function listConversations(
  db: Db,
  userId: string,
  accountId: string,
  limit = 50,
): Promise<ConversationSummary[]> {
  if (!isUuid(accountId)) return [];
  const n = Math.min(MAX_LIST, Math.max(1, Math.trunc(limit)));
  return readOnly(db, async (tx) => {
    const rows = await tx
      .select({
        id: coachConversations.id,
        title: coachConversations.title,
        anchor: coachConversations.anchor,
        lastMessageAt: coachConversations.lastMessageAt,
      })
      .from(coachConversations)
      .where(
        and(eq(coachConversations.userId, userId), eq(coachConversations.accountId, accountId)),
      )
      .orderBy(desc(coachConversations.lastMessageAt), desc(coachConversations.createdAt))
      .limit(n);
    return rows.map((r) => {
      const anchor = parseAnchor(r.anchor);
      return {
        id: r.id,
        title: r.title ?? fallbackTitle(anchor),
        anchor,
        lastMessageAt: r.lastMessageAt,
      };
    });
  });
}

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);

const segmentsOf = (v: unknown): Segment[] | null =>
  Array.isArray(v) && v.every((s) => isRecord(s) && typeof s.kind === "string")
    ? (v as Segment[])
    : null;

function disclosureOf(v: unknown): AiDisclosure | null {
  if (!isRecord(v)) return null;
  const { label, note, model, promptVersion, generatedAt } = v;
  return [label, note, model, promptVersion, generatedAt].every((x) => typeof x === "string")
    ? (v as AiDisclosure)
    : null;
}

/** One of the user's conversations with its messages in seq order, or null. */
export async function getThread(db: Db, userId: string, id: string): Promise<Thread | null> {
  if (!isUuid(id)) return null;
  return readOnly(db, async (tx) => {
    const [conv] = await tx
      .select()
      .from(coachConversations)
      .where(and(eq(coachConversations.id, id), eq(coachConversations.userId, userId)));
    if (!conv) return null;
    const rows = await tx
      .select()
      .from(coachMessages)
      .where(eq(coachMessages.conversationId, id))
      .orderBy(asc(coachMessages.seq));
    const ids = rows.map((m) => m.id);
    const live = rows
      .filter((m) => m.role === "assistant" && (m.status === "pending" || m.status === "streaming"))
      .map((m) => m.id);

    const checks = new Map<string, EngineCheck[]>();
    if (ids.length > 0) {
      const evals = await tx
        .select({
          messageId: interactiveEvals.messageId,
          status: interactiveEvals.status,
          moves: interactiveEvals.moves,
        })
        .from(interactiveEvals)
        .where(inArray(interactiveEvals.messageId, ids))
        .orderBy(asc(interactiveEvals.createdAt), asc(interactiveEvals.id));
      for (const e of evals) {
        if (!e.messageId) continue;
        const list = checks.get(e.messageId) ?? [];
        list.push({ status: e.status, moves: e.moves });
        checks.set(e.messageId, list);
      }
    }

    // R5: what the worker already streamed, from the owner's own `chat.chunk` events only.
    const streamed = new Map<string, StreamedChunk[]>();
    if (live.length > 0) {
      const chunks = await tx
        .select({ payload: userEvents.payload })
        .from(userEvents)
        .where(
          and(
            eq(userEvents.userId, userId),
            eq(userEvents.accountId, conv.accountId),
            eq(userEvents.type, "chat.chunk"),
            inArray(sql<string>`${userEvents.payload}->>'messageId'`, live),
          ),
        )
        .orderBy(asc(userEvents.id));
      for (const { payload: p } of chunks) {
        if (!isRecord(p) || typeof p.messageId !== "string") continue;
        const seq = p.seq;
        const segments = segmentsOf(p.segments);
        if (typeof seq !== "number" || !Number.isInteger(seq) || seq < 0 || !segments) continue;
        const list = streamed.get(p.messageId) ?? [];
        if (list.some((c) => c.seq === seq)) continue;
        list.push({ seq, segments, interim: p.interim === true });
        streamed.set(p.messageId, list);
      }
      for (const list of streamed.values()) list.sort((x, y) => x.seq - y.seq);
    }

    const anchor = parseAnchor(conv.anchor);
    return {
      id: conv.id,
      accountId: conv.accountId,
      timeClass: conv.timeClass,
      title: conv.title ?? fallbackTitle(anchor),
      anchor,
      messages: rows.map((m) => ({
        id: m.id,
        seq: m.seq,
        role: m.role,
        status: m.status,
        anchor: parseAnchor(m.anchor),
        // The player's own question is not model text: it is shown as one text segment.
        segments:
          m.role === "user"
            ? m.text === null
              ? null
              : [{ kind: "text" as const, text: m.text }]
            : segmentsOf(m.segments),
        failureReason: m.failureReason,
        aiDisclosure: disclosureOf(m.aiDisclosure),
        createdAt: m.createdAt,
        streamed: streamed.get(m.id) ?? [],
        engineChecks: checks.get(m.id) ?? [],
      })),
    };
  });
}

/**
 * Hard-deletes one of the user's conversations (messages by cascade, SDK session entries by the
 * trigger). False when it does not exist or is another user's. Works after unlinking.
 */
export async function deleteConversation(db: Db, userId: string, id: string): Promise<boolean> {
  if (!isUuid(id)) return false;
  const deleted = await db
    .delete(coachConversations)
    .where(and(eq(coachConversations.id, id), eq(coachConversations.userId, userId)))
    .returning({ id: coachConversations.id });
  return deleted.length > 0;
}
