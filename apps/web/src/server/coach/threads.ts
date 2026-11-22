import type { Db } from "@ci/db";
import {
  accountLinks,
  coachConversations,
  coachMessages,
  interactiveEvals,
  userEvents,
} from "@ci/db/generated";
import { Chess } from "chess.js";
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
/**
 * One interactive engine check of a message: the tried moves in SAN from the checked position,
 * and `startPly`, the game ply of the first of them (null when the stored FEN cannot be read).
 */
export type EngineCheck = {
  status: "pending" | "done" | "failed";
  moves: string[];
  startPly: number | null;
};

/**
 * The stored UCI line in SAN from `baseFen`, stopping at the first illegal move (Rust
 * `validator::pv::pv_to_san`), and the ply of its first move: a FEN's full-move number and side
 * to move give the plies already played.
 */
export function engineLine(baseFen: string, uci: readonly string[]): Omit<EngineCheck, "status"> {
  let board: Chess;
  try {
    board = new Chess(baseFen);
  } catch {
    return { moves: [], startPly: null };
  }
  const fields = baseFen.trim().split(/\s+/);
  const fullMove = Number(fields[5]);
  const startPly =
    Number.isInteger(fullMove) && fullMove >= 1
      ? 2 * (fullMove - 1) + (fields[1] === "b" ? 1 : 0) + 1
      : null;
  const moves: string[] = [];
  for (const u of uci) {
    const m = /^([a-h][1-8])([a-h][1-8])([qrbn])?$/.exec(u);
    if (!m) break;
    try {
      const played = board.move({ from: m[1] as string, to: m[2] as string, promotion: m[3] });
      moves.push(played.san);
    } catch {
      break;
    }
  }
  return { moves, startPly };
}

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

/** A repeat of the same start inside this window answers the first one (double submit, retry). */
export const START_DEDUPE_SECONDS = 60;

/** Advisory lock class for one user's creates (posts use 30031, R4 30030). */
const CREATE_LOCK_CLASS = 30032;

export type CreateResult =
  | { kind: "created"; id: string }
  | { kind: "not_linked" }
  | { kind: "rate_limited"; retryAt: Date };

export type NewConversation = {
  accountId: string;
  timeClass: TimeClass;
  anchor: ChatAnchor | null;
};

/** Takes the user's create lock: creates, and their dedupe lookups, are serialised per user. */
export async function lockCreates(tx: Db, userId: string): Promise<void> {
  await tx.execute(sql`select pg_advisory_xact_lock(${CREATE_LOCK_CLASS}, hashtext(${userId}))`);
}

/** The same conversation key (`account`, `tc`, anchor) as SQL; jsonb equality is semantic. */
export function sameStart(opts: NewConversation) {
  return sql`c.account_id = ${opts.accountId}::uuid
    and c.time_class = ${opts.timeClass}::time_class
    and ${opts.anchor === null ? sql`c.anchor is null` : sql`c.anchor = ${JSON.stringify(opts.anchor)}::jsonb`}
    and c.created_at > now() - make_interval(secs => ${START_DEDUPE_SECONDS})`;
}

/**
 * Starts an untitled conversation (the first answer sets the title). A repeat of an empty start
 * with the same account, time class and anchor within `START_DEDUPE_SECONDS` answers the first.
 */
export async function createConversation(
  db: Db,
  userId: string,
  opts: NewConversation,
): Promise<CreateResult> {
  if (!isUuid(opts.accountId)) return { kind: "not_linked" };
  return db.transaction(async (raw) => {
    const tx = raw as unknown as Db;
    await lockCreates(tx, userId);
    if (!(await isLinked(tx, userId, opts.accountId))) return { kind: "not_linked" };
    const [again] = (await tx.execute(sql`
      select c.id from coach_conversations c
      where c.user_id = ${userId} and ${sameStart(opts)}
        and not exists (select 1 from coach_messages m where m.conversation_id = c.id)
      order by c.created_at desc limit 1`)) as unknown as { id: string }[];
    if (again) return { kind: "created", id: again.id };
    return createInTx(tx, userId, opts);
  });
}

/** The create, on the caller's transaction (which holds `lockCreates`): link, bound, insert. */
export async function createInTx(
  tx: Db,
  userId: string,
  opts: NewConversation,
): Promise<CreateResult> {
  if (!(await isLinked(tx, userId, opts.accountId))) return { kind: "not_linked" };
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
          baseFen: interactiveEvals.baseFen,
          moves: interactiveEvals.moves,
        })
        .from(interactiveEvals)
        .where(inArray(interactiveEvals.messageId, ids))
        .orderBy(asc(interactiveEvals.createdAt), asc(interactiveEvals.id));
      for (const e of evals) {
        if (!e.messageId) continue;
        const list = checks.get(e.messageId) ?? [];
        list.push({ status: e.status, ...engineLine(e.baseFen, e.moves) });
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
