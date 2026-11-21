import { type Db, listEventsPage, type StreamEvent, type UserEventRow } from "@ci/db";
import type { EventHub } from "./hub";

/** Event types are dotted names; anything else could inject SSE fields and is refused. */
export const EVENT_TYPE = /^[a-z][a-z0-9_.-]{0,63}$/;

/** The browser's reconnect delay, sent as `retry:` when a stream opens. */
export const RETRY_MS = 3000;

/**
 * Bounds for every stream (ADR-WEB-8). Mutable only so tests can shrink them.
 * - `heartbeatMs`: a `: ping` comment keeps proxies from timing the stream out.
 * - `maxAgeMs`: a stream is closed after this long; the browser reconnects with `Last-Event-ID`,
 *   which re-checks the session and picks up newly linked accounts.
 * - `maxStreamsPerUser`: concurrent streams per user in this process (tabs); more get 429.
 * - `maxQueuedBytes`: a client that stops reading is cut off once this much is queued for it.
 * - `maxBufferedEvents`: live events held while a replay runs; more closes the stream.
 */
export const eventStreamLimits = {
  heartbeatMs: 15_000,
  maxAgeMs: 30 * 60_000,
  maxStreamsPerUser: 5,
  maxQueuedBytes: 256 * 1024,
  maxBufferedEvents: 2_000,
  replayPageSize: 500,
};

/** `id: <id>\nevent: <type>\ndata: {"accountId":…,"payload":…}\n\n` (JSON keeps data on one line). */
export function formatSseEvent(e: StreamEvent): string {
  if (!Number.isSafeInteger(e.id) || !EVENT_TYPE.test(e.type)) {
    throw new Error(`formatSseEvent: refusing event ${e.id} of type ${JSON.stringify(e.type)}`);
  }
  const data = JSON.stringify({ accountId: e.accountId, payload: e.payload ?? null });
  return `id: ${e.id}\nevent: ${e.type}\ndata: ${data}\n\n`;
}

/** A `Last-Event-ID` / `since` / `after` value: null when absent, "invalid" when malformed. */
export function parseCursor(value: string | null): number | null | "invalid" {
  if (value === null || value === "") return null;
  if (!/^\d{1,15}$/.test(value)) return "invalid";
  return Number(value);
}

const toStreamEvent = (row: UserEventRow): StreamEvent => ({
  id: row.id,
  accountId: row.accountId,
  type: row.type,
  payload: row.payload,
});

let beforeReplay: (() => Promise<void>) | undefined;

/** Tests: runs after the hub subscription is live and before the first replay query. */
export function setBeforeReplayHook(hook: (() => Promise<void>) | undefined): void {
  beforeReplay = hook;
}

const openStreams = new Map<string, number>();

function acquireSlot(userId: string): (() => void) | null {
  const count = openStreams.get(userId) ?? 0;
  if (count >= eventStreamLimits.maxStreamsPerUser) return null;
  openStreams.set(userId, count + 1);
  let held = true;
  return () => {
    if (!held) return;
    held = false;
    const left = (openStreams.get(userId) ?? 1) - 1;
    if (left <= 0) openStreams.delete(userId);
    else openStreams.set(userId, left);
  };
}

export const SSE_HEADERS = {
  "content-type": "text/event-stream; charset=utf-8",
  "cache-control": "no-cache, no-transform",
  "x-accel-buffering": "no",
} as const;

/**
 * The SSE stream for one client (ADR 0034 consumer protocol, ADR-WEB-8):
 * 1. subscribe to the hub and buffer live events;
 * 2. with a cursor, replay `id > cursor` page by page, or send `reset` (and resume from the head)
 *    when the cursor predates pruned events; without one, send `hello` with the head id;
 * 3. flush the buffer, then forward live events, dropping every id at or below the last one sent.
 * A rebuilt LISTEN connection repeats step 2 from the last id sent.
 */
export async function openEventStream(opts: {
  db: Db;
  hub: EventHub;
  userId: string;
  accountIds: string[];
  cursor: number | null;
  signal: AbortSignal;
}): Promise<Response> {
  const { db, hub, accountIds, signal } = opts;
  const limits = { ...eventStreamLimits };
  const release = acquireSlot(opts.userId);
  if (!release) {
    return Response.json(
      { error: "too_many_streams" },
      { status: 429, headers: { "retry-after": "30" } },
    );
  }

  const encoder = new TextEncoder();
  let controller: ReadableStreamDefaultController<Uint8Array> | undefined;
  let closed = false;
  let lastId = opts.cursor ?? 0;
  let replaying = true;
  const buffer: StreamEvent[] = [];
  let heartbeat: ReturnType<typeof setInterval> | undefined;
  let maxAge: ReturnType<typeof setTimeout> | undefined;

  const close = () => {
    if (closed) return;
    closed = true;
    clearInterval(heartbeat);
    clearTimeout(maxAge);
    unsubscribe();
    release();
    signal.removeEventListener("abort", close);
    try {
      controller?.close();
    } catch {
      // Already cancelled by the client.
    }
  };

  const write = (text: string) => {
    if (closed || !controller) return;
    controller.enqueue(encoder.encode(text));
    if ((controller.desiredSize ?? 0) < -limits.maxQueuedBytes) close();
  };

  const send = (e: StreamEvent) => {
    if (closed || e.id <= lastId) return;
    write(formatSseEvent(e));
    lastId = e.id;
  };

  const onLive = (e: StreamEvent) => {
    if (!replaying) return send(e);
    buffer.push(e);
    if (buffer.length > limits.maxBufferedEvents) close();
  };

  async function sync(hello: boolean): Promise<void> {
    if (closed) return;
    replaying = true;
    try {
      if (beforeReplay) await beforeReplay();
      if (hello) {
        const page = await listEventsPage(db, accountIds, Number.MAX_SAFE_INTEGER, 1);
        lastId = page.headId;
        write(`id: ${lastId}\nevent: hello\ndata: {}\n\n`);
      } else {
        for (;;) {
          if (closed) return;
          const page = await listEventsPage(db, accountIds, lastId, limits.replayPageSize);
          if (page.reset) {
            lastId = Math.max(lastId, page.headId);
            write(`id: ${lastId}\nevent: reset\ndata: {}\n\n`);
            break;
          }
          for (const row of page.events) send(toStreamEvent(row));
          if (page.events.length < limits.replayPageSize) break;
        }
      }
      for (const e of buffer.splice(0)) send(e);
      replaying = false;
    } catch (error) {
      console.error("[events] replay failed", error);
      close();
    }
  }

  let syncing: Promise<void> = Promise.resolve();
  const unsubscribe = hub.subscribe(accountIds, onLive, () => {
    // Buffer from now on, so nothing sent live can jump over events lost in the gap.
    replaying = true;
    syncing = syncing.then(() => sync(false));
  });

  try {
    await hub.ready();
  } catch {
    unsubscribe();
    release();
    return Response.json({ error: "events_unavailable" }, { status: 503 });
  }
  if (signal.aborted) {
    unsubscribe();
    release();
    return new Response(null, { status: 499 });
  }

  const body = new ReadableStream<Uint8Array>(
    {
      start(c) {
        controller = c;
        write(`retry: ${RETRY_MS}\n\n`);
        heartbeat = setInterval(() => write(": ping\n\n"), limits.heartbeatMs);
        maxAge = setTimeout(close, limits.maxAgeMs);
        signal.addEventListener("abort", close, { once: true });
        syncing = syncing.then(() => sync(opts.cursor === null));
      },
      cancel: close,
    },
    new ByteLengthQueuingStrategy({ highWaterMark: 0 }),
  );
  return new Response(body, { status: 200, headers: SSE_HEADERS });
}
