import type { Db } from "@ci/db";
import { coachConversations, coachMessages } from "@ci/db/generated";
import { eq, sql } from "drizzle-orm";
import { enqueueJob, JOB } from "../jobs/index.js";
import type { ChatAnchor } from "./anchor.js";
import { isLinked, isUuid } from "./threads.js";

/** Rust `CHAT_LIMITS.max_question_chars` (Unicode scalar values, Rust `chars().count()`). */
export const MAX_QUESTION_CHARS = 2000;

/**
 * Questions one user may post per window, over all their threads. An abuse bound only: the
 * plan's quota is enforced by the `coach-chat` job (R6), never here.
 */
export const CHAT_POST_LIMIT = { max: 10, windowMs: 10 * 60 * 1000 } as const;

/** Advisory lock class for one user's posts (R4 uses 30030; R3 30001–30004 and 30020). */
const POST_LOCK_CLASS = 30031;

export type PostResult =
  | { kind: "posted"; userMessageId: string; assistantMessageId: string }
  | { kind: "not_found" }
  | { kind: "not_linked" }
  | { kind: "busy" }
  | { kind: "empty" }
  | { kind: "too_long" }
  | { kind: "rate_limited"; retryAt: Date };

/** The question checks of Rust `post_user_message`, before any database work. */
export function checkQuestion(question: string): "empty" | "too_long" | null {
  if (question.trim().length === 0) return "empty";
  if ([...question].length > MAX_QUESTION_CHARS) return "too_long";
  return null;
}

/**
 * Port of Rust `chat::threads::post_user_message`: the user turn (`done`) at `max(seq) + 1` and a
 * `pending` assistant turn after it, `last_message_at = now()`, and the `coach-chat` job for the
 * assistant turn, all in ONE transaction (stronger than Rust, which enqueues after the commit; the
 * reconciler covers either way). The conversation row is locked `FOR UPDATE`, and a conversation
 * with an answer `pending` or `streaming` is `busy`, so two turns are never queued together.
 */
export async function postUserMessage(
  db: Db,
  userId: string,
  threadId: string,
  question: string,
  anchor: ChatAnchor | null,
): Promise<PostResult> {
  const invalid = checkQuestion(question);
  if (invalid) return { kind: invalid };
  if (!isUuid(threadId)) return { kind: "not_found" };

  return db.transaction(async (tx): Promise<PostResult> => {
    await tx.execute(sql`select pg_advisory_xact_lock(${POST_LOCK_CLASS}, hashtext(${userId}))`);
    // Only the owner's conversation is found (another user's id is not found). The row lock
    // serialises posts to one conversation; the checks below are new statements, so under READ
    // COMMITTED they see a post that committed while this one waited.
    const [owned] = (await tx.execute(sql`
      select account_id from coach_conversations
      where id = ${threadId} and user_id = ${userId} for update`)) as unknown as {
      account_id: string;
    }[];
    if (!owned) return { kind: "not_found" };
    const accountId = owned.account_id;
    // The poster must still be linked to the account (Rust `require_link`).
    if (!(await isLinked(tx as unknown as Db, userId, accountId))) return { kind: "not_linked" };

    const [busy] = (await tx.execute(sql`
      select exists (select 1 from coach_messages where conversation_id = ${threadId}
        and role = 'assistant' and status in ('pending', 'streaming')) as busy`)) as unknown as {
      busy: boolean;
    }[];
    if (busy?.busy) return { kind: "busy" };

    const [recent] = (await tx.execute(sql`
      select count(*)::int as n, min(m.created_at) as oldest
      from coach_messages m join coach_conversations c on c.id = m.conversation_id
      where c.user_id = ${userId} and m.role = 'user'
        and m.created_at > now() - make_interval(secs => ${CHAT_POST_LIMIT.windowMs / 1000})`)) as unknown as {
      n: number;
      oldest: Date | string | null;
    }[];
    if (recent && recent.n >= CHAT_POST_LIMIT.max && recent.oldest !== null) {
      const retryAt = new Date(new Date(recent.oldest).getTime() + CHAT_POST_LIMIT.windowMs);
      return { kind: "rate_limited", retryAt };
    }

    const [next] = (await tx.execute(sql`
      select coalesce(max(seq), 0) + 1 as seq from coach_messages
      where conversation_id = ${threadId}`)) as unknown as { seq: number }[];
    const seq = Number(next?.seq ?? 1);
    const [user, assistant] = await tx
      .insert(coachMessages)
      .values([
        {
          conversationId: threadId,
          seq,
          role: "user",
          status: "done",
          anchor,
          text: question,
        },
        {
          conversationId: threadId,
          seq: seq + 1,
          role: "assistant",
          status: "pending",
          anchor: null,
          text: null,
        },
      ])
      .returning({ id: coachMessages.id, seq: coachMessages.seq });
    if (!user || !assistant) throw new Error("postUserMessage: insert returned no rows");
    const [u, a] = user.seq < assistant.seq ? [user, assistant] : [assistant, user];
    await tx
      .update(coachConversations)
      .set({ lastMessageAt: sql`now()` })
      .where(eq(coachConversations.id, threadId));
    await enqueueJob(tx as unknown as Db, {
      kind: JOB.chat.kind,
      payload: JOB.chat.payload(a.id),
      dedupeKey: JOB.chat.key(a.id),
      userKey: accountId,
      priority: JOB.chat.priority,
      maxAttempts: JOB.chat.maxAttempts,
    });
    return { kind: "posted", userMessageId: u.id, assistantMessageId: a.id };
  });
}
