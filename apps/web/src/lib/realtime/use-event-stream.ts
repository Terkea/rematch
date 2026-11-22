"use client";

import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { applyEvent, type StreamEvent } from "./reducers";

/** The polling fallback's interval (ADR-WEB-8: `GET /api/events?after=<id>`). */
export const POLL_INTERVAL_MS = 5000;

/** Consecutive `error`s, before the stream ever opened, that give up on SSE for this page. */
export const ERRORS_BEFORE_POLLING = 3;

/** The poll route's page size: a full page means more are waiting, so poll again at once. */
const POLL_PAGE_SIZE = 500;

/** The named SSE events `applyEvent` knows; `hello` only carries the head id. */
const STREAM_TYPES = ["import.progress", "game.analysed", "job.failed", "reset"] as const;

export type StreamMode = "sse" | "polling" | "off";

type PollPage = { events: StreamEvent[]; lastId: number; reset: boolean };

function isPollPage(v: unknown): v is PollPage {
  if (typeof v !== "object" || v === null) return false;
  const page = v as Record<string, unknown>;
  return Array.isArray(page.events) && Number.isSafeInteger(page.lastId);
}

function eventId(raw: string): number | null {
  if (!/^\d{1,15}$/.test(raw)) return null;
  return Number(raw);
}

/**
 * The signed-in user's live events (M4 T23), applied to the query cache with `applyEvent`.
 *
 * - SSE on `/api/events`. `since` is the event id of the server snapshot the page rendered from,
 *   so an event stored between that render and the stream opening is replayed, not lost. Without
 *   it the server answers `hello` with its head id.
 * - Polling `GET /api/events?after=<lastEventId>` every 5000 ms when `EventSource` is missing,
 *   after 3 consecutive errors before any open, or once the browser gives up on the stream.
 * - Every event with an id at or below `lastEventId` is ignored, so replays never apply twice.
 * - A new `reconnectKey` (the joined account ids) reopens the stream from `lastEventId`.
 *
 * The server alone decides whose events these are (the session's user); nothing here names a
 * user or an account, so the client can never widen what it receives.
 */
export function useEventStream(
  opts: { enabled?: boolean; reconnectKey?: string; since?: number | null } = {},
): { mode: StreamMode; lastEventId: number } {
  const { enabled = true, reconnectKey = "" } = opts;
  const qc = useQueryClient();
  const cursor = useRef<number | null>(opts.since ?? null);
  const [lastEventId, setLastEventId] = useState(cursor.current ?? 0);
  const [mode, setMode] = useState<StreamMode>(enabled ? "sse" : "off");

  // biome-ignore lint/correctness/useExhaustiveDependencies: a new reconnectKey must reopen the stream.
  useEffect(() => {
    if (!enabled) {
      setMode("off");
      return;
    }
    let disposed = false;
    let source: EventSource | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;

    const advance = (id: number) => {
      if (cursor.current !== null && id <= cursor.current) return;
      cursor.current = id;
      setLastEventId(id);
    };
    const deliver = (e: StreamEvent) => {
      if (e.type === "reset") {
        // A reset may carry the id we already hold; it must still apply.
        applyEvent(qc, e);
        advance(e.id);
        return;
      }
      if (cursor.current !== null && e.id <= cursor.current) return;
      advance(e.id);
      applyEvent(qc, e);
    };

    const poll = async () => {
      let more = false;
      try {
        const res = await fetch(`/api/events?after=${cursor.current ?? 0}`, {
          cache: "no-store",
          headers: { accept: "application/json" },
        });
        const page: unknown = res.ok ? await res.json() : null;
        if (!disposed && isPollPage(page)) {
          if (page.reset) deliver({ id: page.lastId, accountId: "", type: "reset", payload: null });
          for (const e of page.events) deliver(e);
          advance(page.lastId);
          more = page.events.length >= POLL_PAGE_SIZE;
        }
      } catch {
        // Offline or a failed request: try again on the next tick.
      }
      if (!disposed) timer = setTimeout(poll, more ? 0 : POLL_INTERVAL_MS);
    };
    const startPolling = () => {
      source?.close();
      source = undefined;
      setMode("polling");
      clearTimeout(timer);
      timer = setTimeout(poll, POLL_INTERVAL_MS);
    };

    if (typeof EventSource === "undefined") {
      startPolling();
    } else {
      setMode("sse");
      const es = new EventSource(
        cursor.current === null ? "/api/events" : `/api/events?since=${cursor.current}`,
      );
      source = es;
      let opened = false;
      let errors = 0;
      es.addEventListener("open", () => {
        opened = true;
      });
      es.addEventListener("error", () => {
        if (source !== es) return;
        // CLOSED: the browser will not retry (a 429 or 503 answer, say).
        if (es.readyState === EventSource.CLOSED) startPolling();
        else if (!opened && ++errors >= ERRORS_BEFORE_POLLING) startPolling();
      });
      es.addEventListener("hello", (m) => {
        const id = eventId((m as MessageEvent).lastEventId);
        if (id !== null) advance(id);
      });
      for (const type of STREAM_TYPES) {
        es.addEventListener(type, (m) => {
          const message = m as MessageEvent<string>;
          const id = eventId(message.lastEventId);
          if (id === null) return;
          try {
            const data = JSON.parse(message.data) as { accountId?: unknown; payload?: unknown };
            const accountId = typeof data.accountId === "string" ? data.accountId : "";
            deliver({ id, accountId, type, payload: data.payload ?? null });
          } catch {
            // A malformed frame is skipped; its id is not taken, so a replay can still bring it.
          }
        });
      }
    }

    return () => {
      disposed = true;
      source?.close();
      source = undefined;
      clearTimeout(timer);
    };
  }, [enabled, reconnectKey, qc]);

  return { mode, lastEventId };
}
