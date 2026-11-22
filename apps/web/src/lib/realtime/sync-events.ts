import type { StreamEvent } from "@ci/db";
import type { QueryClient } from "@tanstack/react-query";
import { queryKeys } from "../query-keys";

/** The user's sync and analysis progress (`GET /api/sync`, `SyncStatus`), all accounts at once. */
export const syncStatusKey = () => ["sync-status"] as const;

/** At most one status refetch per window, with a trailing one, however fast games are analysed. */
export const SYNC_INVALIDATE_MS = 3000;

const throttles = new WeakMap<
  QueryClient,
  { last: number; trailing: ReturnType<typeof setTimeout> | undefined }
>();

function invalidateStatus(qc: QueryClient): void {
  const t = throttles.get(qc) ?? { last: Number.NEGATIVE_INFINITY, trailing: undefined };
  throttles.set(qc, t);
  if (t.trailing !== undefined) return;
  const run = () => {
    t.trailing = undefined;
    t.last = Date.now();
    void qc.invalidateQueries({ queryKey: syncStatusKey(), exact: true });
  };
  const wait = t.last + SYNC_INVALIDATE_MS - Date.now();
  if (wait <= 0) run();
  else t.trailing = setTimeout(run, wait);
}

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);

/**
 * Keeps the sync and analysis status current from the stream (Task P2): a finished import (and
 * its new games, through the account's `games` key, which `LiveRefresh` turns into a page
 * refresh), each analysed game, a failed job and each profile build. Progress inside a run does
 * not refetch: the panel reads that count from `importProgress` directly.
 */
export function applySyncEvent(qc: QueryClient, e: StreamEvent): void {
  switch (e.type) {
    case "import.progress":
      if (!isRecord(e.payload) || e.payload.done !== true) return;
      invalidateStatus(qc);
      void qc.invalidateQueries({ queryKey: queryKeys.games(e.accountId), exact: true });
      return;
    case "game.analysed":
    case "job.failed":
    case "profile.updated":
    case "weakness.changed":
      invalidateStatus(qc);
      return;
    default:
      return;
  }
}
