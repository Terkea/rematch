import { and, asc, eq, gt, inArray, isNull, max, min, or, sql } from "drizzle-orm";
import type { Db } from "./client.js";
// `user_events` as db/migrations define it (with `user_id`, migration 20261121030400). The
// hand-written table in ./schema.js mirrors the legacy TS-worker schema, which has no `user_id`.
import { userEvents as ownedUserEvents } from "./generated/schema.js";
import { userEvents } from "./schema.js";

/** Advisory lock key that serialises user-event inserts (same value as the Rust `events` crate). */
export const USER_EVENTS_LOCK_KEY = 7_262_011;

/** The Postgres LISTEN/NOTIFY channel every user event is announced on (ADR 0034). */
export const USER_EVENTS_CHANNEL = "user_events";

/** NOTIFY bodies above this many bytes are sent without `payload` (Postgres's limit is 8000). */
export const MAX_NOTIFY_BYTES = 7900;

/** How long events are kept for replay (ADR 0034 `EVENT_RETENTION`). */
export const EVENT_RETENTION_MS = 7 * 24 * 3600 * 1000;

/** The TS worker's per-account Redis pub/sub channel (M1; Redis goes away with the Rust worker). */
export const userEventsChannel = (accountId: string): string => `user-events:${accountId}`;

/**
 * One user event as clients see it, live or replayed. `userId` is set on a user-owned event
 * (`user_events.user_id`): it is for that user only, never for other users of the account.
 */
export type StreamEvent = {
  id: number;
  accountId: string;
  userId?: string | null;
  type: string;
  payload: unknown;
};

export type UserEventRow = typeof userEvents.$inferSelect;

/** A `user_events` row with its owner (`userId` null: an account event). */
export type OwnedUserEventRow = typeof ownedUserEvents.$inferSelect;

/** True when `viewerUserId` may receive the event: account events, or the viewer's own. */
export function eventVisibleTo(
  event: { userId?: string | null },
  viewerUserId: string | null,
): boolean {
  if (event.userId == null) return true;
  return viewerUserId !== null && event.userId.toLowerCase() === viewerUserId.toLowerCase();
}

/** UTC with exactly six fractional digits, like Rust's `common::js_time::iso_micros`. */
const ISO_MICROS = sql<string>`to_char(${userEvents.createdAt} at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.US"Z"')`;

/**
 * The NOTIFY body, with the same fields and truncation rule as the Rust `events` crate (key order
 * may differ: serde_json sorts keys, consumers parse the JSON): camelCase
 * `{id, accountId, userId?, type, createdAt, payload}` (`userId` only on a user-owned event), or
 * `truncated: true` without `payload` when it would not fit in `MAX_NOTIFY_BYTES`.
 */
export function notificationJson(
  stored: { id: number; createdAt: string },
  accountId: string,
  type: string,
  payload: unknown,
  userId: string | null = null,
): string {
  const head = {
    id: stored.id,
    accountId,
    ...(userId === null ? {} : { userId }),
    type,
    createdAt: stored.createdAt,
  };
  const full = JSON.stringify({ ...head, payload });
  if (Buffer.byteLength(full, "utf8") <= MAX_NOTIFY_BYTES) return full;
  return JSON.stringify({ ...head, truncated: true });
}

/**
 * Inserts one user event, announces it with `NOTIFY user_events` (ADR 0034) and returns its id.
 * The id is taken under a transaction-scoped advisory lock, which is released only after the
 * enclosing transaction commits, so ids become visible, and NOTIFYs are delivered, in id order.
 * Pass a transaction to make the event part of it; a rolled-back insert notifies nobody.
 * A `userId` makes it a user-owned event (Rust `insert_owned_event`); that needs the
 * db/migrations schema.
 */
export async function insertUserEvent(
  db: Pick<Db, "transaction">,
  event: { accountId: string; type: string; payload: object; userId?: string | null },
): Promise<number> {
  const { userId = null, ...accountEvent } = event;
  return db.transaction(async (tx) => {
    await tx.execute(sql`select pg_advisory_xact_lock(${USER_EVENTS_LOCK_KEY})`);
    const [row] =
      userId === null
        ? await tx
            .insert(userEvents)
            .values(accountEvent)
            .returning({ id: userEvents.id, createdAt: ISO_MICROS })
        : await tx
            .insert(ownedUserEvents)
            .values({ ...accountEvent, userId })
            .returning({ id: ownedUserEvents.id, createdAt: ISO_MICROS });
    if (!row) throw new Error("insertUserEvent: insert returned no row");
    const body = notificationJson(row, event.accountId, event.type, event.payload, userId);
    await tx.execute(sql`select pg_notify(${USER_EVENTS_CHANNEL}, ${body})`);
    return row.id;
  });
}

/**
 * Events for the given accounts with `id > afterId`, oldest first: the replay query behind SSE
 * `Last-Event-ID` and the polling fallback. Exactly-once in order relies on every writer using
 * `insertUserEvent` (or the Rust `insert_user_event`). It does not filter by `user_id`: never use
 * it to deliver events to a user (use `listEventsPage`).
 */
export async function listEventsAfter(
  db: Db,
  accountIds: string[],
  afterId: number,
  limit = 500,
): Promise<UserEventRow[]> {
  if (accountIds.length === 0) return [];
  return db
    .select()
    .from(userEvents)
    .where(and(inArray(userEvents.accountId, accountIds), gt(userEvents.id, afterId)))
    .orderBy(asc(userEvents.id))
    .limit(limit);
}

/** The smallest retained event id for the accounts, or null when they have none. */
export async function oldestEventId(db: Db, accountIds: string[]): Promise<number | null> {
  if (accountIds.length === 0) return null;
  const [row] = await db
    .select({ id: min(userEvents.id) })
    .from(userEvents)
    .where(inArray(userEvents.accountId, accountIds));
  return row?.id ?? null;
}

/** One replay page plus what the client needs to resume (ADR 0034 `list_events_after`). */
export type EventsPage = {
  /** `afterId < pruned_through_id`: the client may have missed pruned events and must refetch. */
  reset: boolean;
  events: OwnedUserEventRow[];
  /**
   * Where a client without a usable cursor resumes: the accounts' newest id, raised to the prune
   * watermark so that cursor is never stale. Never another account's id.
   */
  headId: number;
};

/**
 * Rust `list_events_after` plus the resume point, for one viewer: the watermark, the page and the
 * head are read from one `REPEATABLE READ READ ONLY` snapshot. Only events the viewer may see are
 * read: `user_id IS NULL OR user_id = viewerUserId` (migration 20261121030400, ADR-R3-8). A null
 * viewer sees account events only. The head ignores other users' events too.
 */
export async function listEventsPage(
  db: Db,
  accountIds: string[],
  afterId: number,
  limit: number,
  viewerUserId: string | null,
): Promise<EventsPage> {
  const userEvents = ownedUserEvents;
  const visible =
    viewerUserId === null
      ? isNull(userEvents.userId)
      : or(isNull(userEvents.userId), eq(userEvents.userId, viewerUserId));
  return db.transaction(
    async (tx) => {
      const [mark] = await tx.execute<{ pruned: string }>(
        sql`select pruned_through_id::text as pruned from user_events_watermark`,
      );
      const pruned = Number(mark?.pruned ?? 0);
      if (accountIds.length === 0) return { reset: afterId < pruned, events: [], headId: pruned };
      const mine = and(inArray(userEvents.accountId, accountIds), visible);
      const events = await tx
        .select()
        .from(userEvents)
        .where(and(mine, gt(userEvents.id, afterId)))
        .orderBy(asc(userEvents.id))
        .limit(limit);
      const [top] = await tx
        .select({ id: max(userEvents.id) })
        .from(userEvents)
        .where(mine);
      return { reset: afterId < pruned, events, headId: Math.max(top?.id ?? 0, pruned) };
    },
    { isolationLevel: "repeatable read", accessMode: "read only" },
  );
}
