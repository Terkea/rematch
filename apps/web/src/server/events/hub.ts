import { type StreamEvent, USER_EVENTS_CHANNEL, userEvents } from "@ci/db";
import { eq } from "drizzle-orm";
import { getEnv } from "../../env";
import { getDb } from "../db";
import { EVENT_TYPE } from "./sse";

/**
 * postgres.js `sql.listen`: `onListen` runs after every successful LISTEN, the first one and each
 * one after the connection was rebuilt (postgres.js relistens on its own).
 */
export type ListenFn = (
  channel: string,
  onNotify: (payload: string) => void,
  onListen: () => void,
) => Promise<{ unlisten(): Promise<void> }>;

export type EventHubDeps = {
  listen: ListenFn;
  /** The stored event, for a NOTIFY that came without its payload (`truncated`, ADR 0034). */
  fetchEvent(id: number): Promise<StreamEvent | null>;
  log?: (message: string, error?: unknown) => void;
};

export interface EventHub {
  /**
   * Calls `listener` for every live event of these accounts, in id order. `onReconnect` runs
   * when the LISTEN connection was rebuilt: notifications in the gap are gone, so replay from
   * the cursor. Returns the unsubscribe function (idempotent).
   */
  subscribe(
    accountIds: string[],
    listener: (e: StreamEvent) => void,
    onReconnect?: () => void,
  ): () => void;
  /** Resolves once LISTEN is active; rejects (and retries on the next call) when it fails. */
  ready(): Promise<void>;
  /** Accounts with at least one subscriber. */
  channelCount(): number;
  close(): Promise<void>;
}

type Subscription = { listener: (e: StreamEvent) => void; onReconnect?: () => void };

type Notification = {
  id: number;
  accountId: string;
  type: string;
  payload?: unknown;
  truncated?: boolean;
};

function parseNotification(raw: string): Notification | null {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return null;
  }
  if (typeof value !== "object" || value === null) return null;
  const note = value as Record<string, unknown>;
  if (!Number.isSafeInteger(note.id) || (note.id as number) < 1) return null;
  if (typeof note.accountId !== "string" || typeof note.type !== "string") return null;
  if (!EVENT_TYPE.test(note.type)) return null;
  if (note.truncated !== true && !("payload" in note)) return null;
  return {
    id: note.id as number,
    accountId: note.accountId.toLowerCase(),
    type: note.type,
    payload: note.payload,
    truncated: note.truncated === true,
  };
}

/**
 * One `LISTEN user_events` per process for every SSE client (ADR 0034, ADR-WEB-8). All accounts
 * share the channel, so each notification is routed only to the subscribers of its `accountId`.
 * Notifications are handled one at a time, so a truncated one (payload fetched by id) never
 * overtakes the next.
 */
export function createEventHub(deps: EventHubDeps): EventHub {
  const log = deps.log ?? ((message, error) => console.error(`[events] ${message}`, error ?? ""));
  const byAccount = new Map<string, Set<Subscription>>();
  let listening: Promise<void> | undefined;
  let unlisten: (() => Promise<void>) | undefined;
  let listenedOnce = false;
  let closed = false;
  let queue: Promise<void> = Promise.resolve();

  const enqueue = (work: () => Promise<void> | void) => {
    queue = queue.then(work).catch((error) => log("dispatch failed", error));
  };

  async function handle(raw: string): Promise<void> {
    const note = parseNotification(raw);
    if (!note) {
      log("skipped a malformed notification");
      return;
    }
    const subs = byAccount.get(note.accountId);
    if (!subs || subs.size === 0) return;
    let event: StreamEvent | null = {
      id: note.id,
      accountId: note.accountId,
      type: note.type,
      payload: note.payload,
    };
    if (note.truncated) {
      // Pruned meanwhile: nothing to send (a replay would not find it either).
      event = await deps.fetchEvent(note.id);
      if (!event || event.accountId.toLowerCase() !== note.accountId) return;
    }
    for (const sub of [...(byAccount.get(note.accountId) ?? [])]) {
      try {
        sub.listener(event);
      } catch (error) {
        log("listener failed", error);
      }
    }
  }

  function onListen() {
    if (!listenedOnce) {
      listenedOnce = true;
      return;
    }
    enqueue(() => {
      const all = new Set<Subscription>();
      for (const subs of byAccount.values()) for (const sub of subs) all.add(sub);
      for (const sub of all) sub.onReconnect?.();
    });
  }

  function ready(): Promise<void> {
    if (closed) return Promise.reject(new Error("event hub closed"));
    listening ??= deps
      .listen(USER_EVENTS_CHANNEL, (raw) => enqueue(() => handle(raw)), onListen)
      .then(
        (meta) => {
          unlisten = meta.unlisten;
        },
        (error) => {
          listening = undefined;
          log("LISTEN failed", error);
          throw error;
        },
      );
    return listening;
  }

  return {
    subscribe(accountIds, listener, onReconnect) {
      const sub: Subscription = { listener, ...(onReconnect ? { onReconnect } : {}) };
      const ids = [...new Set(accountIds.map((id) => id.toLowerCase()))];
      for (const id of ids) {
        const subs = byAccount.get(id) ?? new Set<Subscription>();
        subs.add(sub);
        byAccount.set(id, subs);
      }
      ready().catch(() => {});
      let active = true;
      return () => {
        if (!active) return;
        active = false;
        for (const id of ids) {
          const subs = byAccount.get(id);
          subs?.delete(sub);
          if (subs?.size === 0) byAccount.delete(id);
        }
      };
    },
    ready,
    channelCount: () => byAccount.size,
    async close() {
      closed = true;
      byAccount.clear();
      await listening?.catch(() => {});
      await unlisten?.();
    },
  };
}

let cached: { url: string; hub: EventHub } | undefined;

/** The process's hub, on the app database's postgres.js client (rebuilt when the URL changes). */
export function getEventHub(): EventHub {
  const url = getEnv().DATABASE_URL;
  if (cached?.url !== url) {
    const db = getDb();
    cached = {
      url,
      hub: createEventHub({
        listen: (channel, onNotify, onListen) => db.$client.listen(channel, onNotify, onListen),
        async fetchEvent(id) {
          const [row] = await db.select().from(userEvents).where(eq(userEvents.id, id));
          return row
            ? { id: row.id, accountId: row.accountId, type: row.type, payload: row.payload }
            : null;
        },
      }),
    };
  }
  return cached.hub;
}

/** Tests: stops the process hub's LISTEN. */
export async function resetEventHub(): Promise<void> {
  const current = cached;
  cached = undefined;
  await current?.hub.close();
}
