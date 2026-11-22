"use client";

import type { AiDisclosureData, EngineCheck, Segment, StreamedChunk } from "@ci/ui/coach";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { queryKeys } from "../../../../../lib/query-keys";
import type { ChatStreamState } from "../../../../../lib/realtime/coach-events";
import { useRealtime } from "../../../../../lib/realtime/realtime-provider";
import { POLL_INTERVAL_MS } from "../../../../../lib/realtime/use-event-stream";
import type { CapReason } from "../../../../../server/coach/copy";
import { CHAT_COPY, failureCopy } from "../../../../../server/coach/copy";
import type { Thread, ThreadMessage } from "../../../../../server/coach/threads";
import { ANSWERING, canRetry, NOT_LIVE, postErrorCopy, SENDING } from "../copy";

// One coach thread in the browser (Task 9, R5). The page reads the thread on the server; this
// hook keeps it, shows the unfinished answer's chunks as `chat.chunk` events land in the query
// cache (`applyCoachEvent`), and when the stream ends it reads the stored thread again: the
// stored answer is authoritative. A `replace` hides the streamed text at once (it was retracted
// or repaired); a re-read that keeps failing is retried with backoff and, after a bounded wait,
// shown as an honest error so the composer unlocks. Chunks are keyed by (attempt, seq): a retried
// turn's newer attempt replaces the older one's text, live and on replay. While the event stream
// is not open it reads the thread every few seconds instead.

/** The thread as the page or `GET /api/coach/threads/[id]` gives it (dates may be strings). */
export type ThreadData = Omit<Thread, "messages"> & {
  messages: (Omit<ThreadMessage, "createdAt"> & { createdAt: Date | string })[];
};

/** One message as the view renders it. */
export type MessageView = {
  id: string;
  role: "user" | "assistant";
  status: "pending" | "streaming" | "done" | "failed";
  segments: Segment[] | null;
  streamed: StreamedChunk[];
  engineChecks: EngineCheck[];
  /** The player's text for a failed answer. */
  failure: string | null;
  failureReason: string | null;
  disclosure: AiDisclosureData | null;
  /** The question to ask again for a failed answer worth retrying; else null. */
  retry: string | null;
  /** A question still on its way to the server (the optimistic post). */
  sending: boolean;
  /** An answer that finished while this page was open: it settles with the `pen` spring. */
  settled: boolean;
};

export type ComposerState = { disabled: boolean; reason: string | null };

const isLive = (m: { role: string; status: string }) =>
  m.role === "assistant" && (m.status === "pending" || m.status === "streaming");

/**
 * The stored chunks (replayed by `getThread`, of attempt `storedAttempt`) and the live ones (of
 * `liveAttempt`, absent for payloads without one): only the newer attempt's, one per seq, in order.
 */
export function mergeChunks(
  stored: readonly StreamedChunk[],
  storedAttempt: number,
  live: readonly StreamedChunk[],
  liveAttempt: number | undefined,
): StreamedChunk[] {
  const sources =
    liveAttempt === undefined || liveAttempt === storedAttempt
      ? [...stored, ...live]
      : liveAttempt > storedAttempt
        ? [...live]
        : [...stored];
  const bySeq = new Map<number, StreamedChunk>();
  for (const c of sources) if (!bySeq.has(c.seq)) bySeq.set(c.seq, c);
  return [...bySeq.values()].sort((x, y) => x.seq - y.seq);
}

const questionText = (m: { segments: Segment[] | null } | undefined): string | null => {
  const t = (m?.segments ?? [])
    .map((s) => (s.kind === "text" ? s.text : ""))
    .join("")
    .trim();
  return t === "" ? null : t;
};

/** Re-reads after the stream ended: the first waits this long, each next twice as long. */
const SETTLE_FIRST_MS = 1000;
/** Re-reads never wait longer than this. */
const SETTLE_MAX_MS = 30_000;
/** After this long without the stored answer, the turn is shown as not loaded (composer unlocks). */
export const SETTLE_GIVE_UP_MS = 20_000;
/** The honest error for an answer that finished but could not be read. */
export const UNLOADED_COPY =
  "Your coach finished this answer, but it didn't load. Reload the page to read it.";

export function useThread(
  threadId: string,
  initial: ThreadData,
  opts: { exhausted?: CapReason | null; onSettled?: () => void } = {},
) {
  const qc = useQueryClient();
  const { mode } = useRealtime();
  const [thread, setThread] = useState<ThreadData>(initial);
  const [sending, setSending] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [settled, setSettled] = useState<ReadonlySet<string>>(() => new Set());
  /** Answers whose stream ended but whose stored row could not be read in time. */
  const [unloaded, setUnloaded] = useState<ReadonlySet<string>>(() => new Set());
  const onSettled = useRef(opts.onSettled);
  /** The thread as last set, for comparing a fresh read against (no side effects in updaters). */
  const current = useRef(thread);
  useEffect(() => {
    onSettled.current = opts.onSettled;
    current.current = thread;
  });

  const live = useMemo(() => thread.messages.findLast(isLive) ?? null, [thread]);
  const liveId = live?.id ?? null;
  const streamKey = queryKeys.chatStream(liveId ?? "none");

  // I1: an observer keeps the stream's cache entry alive while the thread is open (a query
  // without one is collected after `gcTime`, mid-turn). It never fetches.
  useQuery<ChatStreamState | null>({
    queryKey: streamKey,
    queryFn: () => qc.getQueryData<ChatStreamState>(streamKey) ?? null,
    enabled: false,
    gcTime: Number.POSITIVE_INFINITY,
  });
  // The entry is dropped when the answer settles or the thread closes.
  useEffect(() => {
    if (liveId === null) return;
    const key = queryKeys.chatStream(liveId);
    return () => qc.removeQueries({ queryKey: key, exact: true });
  }, [qc, liveId]);

  // Watches what the event reducer writes for the unfinished answer.
  const subscribe = useCallback((notify: () => void) => qc.getQueryCache().subscribe(notify), [qc]);
  const stream = useSyncExternalStore(
    subscribe,
    () =>
      liveId === null ? undefined : qc.getQueryData<ChatStreamState>(queryKeys.chatStream(liveId)),
    () => undefined,
  );

  const refetch = useCallback(async (): Promise<ThreadData | null> => {
    try {
      const res = await fetch(`/api/coach/threads/${encodeURIComponent(threadId)}`, {
        credentials: "same-origin",
        headers: { accept: "application/json" },
        cache: "no-store",
      });
      if (!res.ok) return null;
      const next = (await res.json()) as ThreadData;
      if (!next || !Array.isArray(next.messages)) return null;
      const finished = current.current.messages
        .filter((m) => isLive(m) && next.messages.some((n) => n.id === m.id && !isLive(n)))
        .map((m) => m.id);
      current.current = next;
      setThread(next);
      if (finished.length > 0) {
        setSettled((s) => new Set([...s, ...finished]));
        setUnloaded((s) => new Set([...s].filter((id) => !finished.includes(id))));
        onSettled.current?.();
      }
      return next;
    } catch {
      return null;
    }
  }, [threadId]);

  // I2: the stream ended. Read the stored answer, retrying with backoff until it is final; after
  // SETTLE_GIVE_UP_MS the turn is shown as not loaded, and the retries go on in the background.
  const ended = stream?.done === true;
  useEffect(() => {
    if (!ended || liveId === null) return;
    const id = liveId;
    let wait = SETTLE_FIRST_MS;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let cancelled = false;
    // Still unfinished here after the bounded wait: say so, and unlock the composer.
    const giveUp = setTimeout(() => {
      if (current.current.messages.some((m) => m.id === id && isLive(m))) {
        setUnloaded((s) => (s.has(id) ? s : new Set([...s, id])));
      }
    }, SETTLE_GIVE_UP_MS);
    const run = async () => {
      const next = await refetch();
      if (cancelled) return;
      const still = next?.messages.some((m) => m.id === id && isLive(m)) ?? true;
      if (!still) return;
      timer = setTimeout(run, wait);
      wait = Math.min(wait * 2, SETTLE_MAX_MS);
    };
    void run();
    return () => {
      cancelled = true;
      clearTimeout(giveUp);
      if (timer) clearTimeout(timer);
    };
  }, [ended, liveId, refetch]);

  // No open stream (polling, connecting, off): read the thread every few seconds while it runs.
  const streaming = mode === "sse";
  useEffect(() => {
    if (streaming || liveId === null) return;
    const timer = setInterval(() => void refetch(), POLL_INTERVAL_MS);
    return () => clearInterval(timer);
  }, [streaming, liveId, refetch]);

  const post = useCallback(
    async (question: string): Promise<boolean> => {
      setError(null);
      setSending(question);
      try {
        const res = await fetch(`/api/coach/threads/${encodeURIComponent(threadId)}/messages`, {
          method: "POST",
          credentials: "same-origin",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ question }),
        });
        const data = (await res.json().catch(() => null)) as {
          error?: string;
          retryAt?: string;
          userMessageId?: string;
          assistantMessageId?: string;
        } | null;
        if (!res.ok || !data?.userMessageId || !data.assistantMessageId) {
          setError(postErrorCopy(data?.error ?? "unknown", data?.retryAt));
          // Busy: an answer this page has not seen yet is running; show it.
          if (data?.error === "busy") void refetch();
          return false;
        }
        const { userMessageId, assistantMessageId } = data;
        setThread((t) => {
          const seq = t.messages.reduce((n, m) => Math.max(n, m.seq), 0) + 1;
          const base = {
            anchor: null,
            failureReason: null,
            aiDisclosure: null,
            createdAt: new Date().toISOString(),
            streamed: [],
            streamedAttempt: 0,
            engineChecks: [],
          };
          return {
            ...t,
            messages: [
              ...t.messages,
              {
                ...base,
                id: userMessageId,
                seq,
                role: "user",
                status: "done",
                segments: [{ kind: "text", text: question }],
              },
              {
                ...base,
                id: assistantMessageId,
                seq: seq + 1,
                role: "assistant",
                status: "pending",
                segments: null,
              },
            ],
          };
        });
        return true;
      } catch {
        setError(postErrorCopy("network"));
        return false;
      } finally {
        setSending(null);
      }
    },
    [threadId, refetch],
  );

  const messages = useMemo<MessageView[]>(() => {
    const lastAssistant = thread.messages.findLast((m) => m.role === "assistant")?.id ?? null;
    const out = thread.messages.map((m, i): MessageView => {
      const isUnfinished = m.id === liveId;
      const lost = isUnfinished && unloaded.has(m.id);
      // `replace`: the streamed text was retracted or repaired, so it is hidden at once.
      const hidden = isUnfinished && stream?.done === true && stream.replace === true;
      const chunks =
        isUnfinished && !lost && !hidden
          ? mergeChunks(m.streamed, m.streamedAttempt ?? 0, stream?.chunks ?? [], stream?.attempt)
          : [];
      // The stream said it failed and the stored row has not caught up yet.
      const streamFailed = isUnfinished && stream?.done === true && stream.reason !== undefined;
      const status: MessageView["status"] =
        streamFailed || lost
          ? "failed"
          : isUnfinished
            ? chunks.length > 0
              ? "streaming"
              : "pending"
            : m.status;
      const reason = lost ? "unloaded" : streamFailed ? (stream?.reason ?? null) : m.failureReason;
      const failed = status === "failed" && m.role === "assistant";
      const question = failed ? questionText(thread.messages[i - 1]) : null;
      return {
        id: m.id,
        role: m.role,
        status,
        segments: m.segments,
        streamed: chunks,
        engineChecks: m.engineChecks,
        failure: lost ? UNLOADED_COPY : failed ? failureCopy(reason ?? "") : null,
        failureReason: failed ? reason : null,
        disclosure: m.aiDisclosure,
        // m2: only the latest answer can be asked again.
        retry: failed && !lost && m.id === lastAssistant && canRetry(reason) ? question : null,
        sending: false,
        settled: settled.has(m.id),
      };
    });
    if (sending !== null) {
      out.push({
        id: "sending",
        role: "user",
        status: "pending",
        segments: [{ kind: "text", text: sending }],
        streamed: [],
        engineChecks: [],
        failure: null,
        failureReason: null,
        disclosure: null,
        retry: null,
        sending: true,
        settled: false,
      });
    }
    return out;
  }, [thread, liveId, stream, sending, settled, unloaded]);

  const answering = messages.some(
    (m) => m.role === "assistant" && (m.status === "pending" || m.status === "streaming"),
  );
  const exhausted = opts.exhausted ?? null;
  const composer: ComposerState = exhausted
    ? { disabled: true, reason: CHAT_COPY.capText(exhausted) }
    : sending !== null
      ? { disabled: true, reason: SENDING }
      : answering
        ? { disabled: true, reason: ANSWERING }
        : { disabled: false, reason: null };

  return {
    thread,
    messages,
    answering,
    composer,
    error,
    clearError: () => setError(null),
    post,
    /** While an answer runs and the event stream is not open. */
    pace: answering && !streaming ? NOT_LIVE : null,
  };
}
