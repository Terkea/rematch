import type { StreamEvent } from "@ci/db";
import { toast } from "@ci/ui/overlays";
import type { QueryClient } from "@tanstack/react-query";
import type { Segment } from "../../server/coaching/types";
import { queryKeys } from "../query-keys";

// The R3 events (backend/crates/ai/src/events.rs) on the existing stream (R5, ADR 0070). Report,
// weekly-review and note events are account events and only invalidate keys; `chat.*` events
// are user-owned (`user_events.user_id`), so the stream delivers them to their owner only.

export type ChatStreamChunk = { seq: number; segments: Segment[]; interim: boolean };

/** `queryKeys.chatStream(messageId)`: the chunks so far and, once ended, how it ended. */
export type ChatStreamState = {
  chunks: ChatStreamChunk[];
  done: boolean;
  /** `chat.message.ready`: the validated message differs from what streamed. */
  replace?: boolean;
  /** `chat.message.failed`: the reason (`failureCopy` gives the text). */
  reason?: string;
  /**
   * The turn's AI attempt the chunks belong to (Rust `attempt`, Task 9 review I3). A released and
   * retried turn streams again from seq 0, so a higher attempt discards the lower one's chunks
   * and a lower one's events are ignored. Absent for payloads without it.
   */
  attempt?: number;
};

/** The payload's attempt, or null when it carries none (an older worker). */
function attemptOf(p: Record<string, unknown>): number | null {
  const a = p.attempt;
  return typeof a === "number" && Number.isInteger(a) && a >= 0 ? a : null;
}

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);

const isSegments = (v: unknown): v is Segment[] =>
  Array.isArray(v) && v.every((s) => isRecord(s) && typeof s.kind === "string");

const DEGRADED: Record<string, string> = {
  unavailable: "the coach is unavailable right now",
  budget: "this month's AI spending limit is reached",
  ai_disabled: "AI writing is switched off",
};

const EMPTY: ChatStreamState = { chunks: [], done: false };

function stream(qc: QueryClient, messageId: string): ChatStreamState {
  return qc.getQueryData<ChatStreamState>(queryKeys.chatStream(messageId)) ?? EMPTY;
}

/** Applies one coach event; true when it acted. Unknown types and malformed payloads: false. */
export function applyCoachEvent(qc: QueryClient, e: StreamEvent): boolean {
  const p = e.payload;
  if (!isRecord(p)) return false;
  switch (e.type) {
    case "report.section.ready":
    case "report.ready":
      if (typeof p.reportId !== "string") return false;
      void qc.invalidateQueries({ queryKey: queryKeys.report(e.accountId), exact: true });
      return true;
    case "report.degraded": {
      if (typeof p.reportId !== "string" || typeof p.reason !== "string") return false;
      void qc.invalidateQueries({ queryKey: queryKeys.report(e.accountId), exact: true });
      const why = DEGRADED[p.reason] ?? DEGRADED.unavailable;
      toast(`Your coach's notes are paused: ${why}. The computed report is still here.`);
      return true;
    }
    case "weekly_review.ready":
      if (typeof p.id !== "string") return false;
      void qc.invalidateQueries({ queryKey: queryKeys.weekly(e.accountId), exact: true });
      return true;
    case "weakness.note.ready":
      if (typeof p.changeKey !== "string") return false;
      void qc.invalidateQueries({ queryKey: queryKeys.notes(e.accountId), exact: true });
      return true;
    case "chat.chunk": {
      const { messageId, seq, segments } = p;
      if (typeof messageId !== "string") return false;
      if (typeof seq !== "number" || !Number.isInteger(seq) || seq < 0) return false;
      if (!isSegments(segments)) return false;
      let old = stream(qc, messageId);
      const attempt = attemptOf(p);
      if (attempt !== null && old.attempt !== undefined) {
        // An older attempt's chunk is stale; a newer attempt starts over.
        if (attempt < old.attempt) return true;
        if (attempt > old.attempt) old = { chunks: [], done: false };
      }
      // A replayed chunk (reconnect) or one after the end changes nothing.
      if (old.done || old.chunks.some((c) => c.seq === seq)) return true;
      const chunks = [...old.chunks, { seq, segments, interim: p.interim === true }].sort(
        (a, b) => a.seq - b.seq,
      );
      qc.setQueryData<ChatStreamState>(queryKeys.chatStream(messageId), {
        ...old,
        chunks,
        ...(attempt === null ? {} : { attempt: Math.max(attempt, old.attempt ?? 0) }),
      });
      return true;
    }
    case "chat.message.ready": {
      if (typeof p.messageId !== "string") return false;
      const old = stream(qc, p.messageId);
      const attempt = attemptOf(p);
      if (attempt !== null && old.attempt !== undefined && attempt < old.attempt) return true;
      qc.setQueryData<ChatStreamState>(queryKeys.chatStream(p.messageId), {
        chunks: old.chunks,
        done: true,
        replace: p.replace === true,
        ...(old.attempt === undefined ? {} : { attempt: old.attempt }),
      });
      return true;
    }
    case "chat.message.failed": {
      if (typeof p.messageId !== "string" || typeof p.reason !== "string") return false;
      const old = stream(qc, p.messageId);
      const attempt = attemptOf(p);
      if (attempt !== null && old.attempt !== undefined && attempt < old.attempt) return true;
      qc.setQueryData<ChatStreamState>(queryKeys.chatStream(p.messageId), {
        chunks: old.chunks,
        done: true,
        reason: p.reason,
        ...(old.attempt === undefined ? {} : { attempt: old.attempt }),
      });
      return true;
    }
    default:
      return false;
  }
}
