"use client";

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { syncStatusKey } from "../../lib/realtime/sync-events";
import type { SyncStatus } from "../../server/sync";

/** `GET /api/sync`; throws on a non-2xx answer or a body that is not a status, so the cache keeps the last good one. */
export async function fetchSyncStatus(): Promise<SyncStatus> {
  const res = await fetch("/api/sync", { cache: "no-store" });
  if (!res.ok) throw new Error(`sync status ${res.status}`);
  const body = (await res.json()) as Partial<SyncStatus>;
  // Anything but a status (a proxy page, a newer shape) keeps the last good one.
  if (!Array.isArray(body.accounts) || typeof body.generatedAt !== "string") {
    throw new Error("sync status: unexpected body");
  }
  return body as SyncStatus;
}

/** How long a fetched status counts as fresh; the stream invalidates it sooner when work moves. */
export const SYNC_STALE_MS = 30_000;

/**
 * The sync and analysis status from the shared query cache. `initial` (the server's read) seeds
 * it on a page; without it (the header) the first mount fetches, and a failed fetch is simply no
 * status (`undefined`), never an error shown in the header.
 */
export function useSyncStatus(initial?: SyncStatus): SyncStatus | undefined {
  const qc = useQueryClient();
  const { data, dataUpdatedAt } = useQuery({
    queryKey: syncStatusKey(),
    queryFn: fetchSyncStatus,
    staleTime: SYNC_STALE_MS,
    retry: false,
    ...(initial
      ? { initialData: initial, initialDataUpdatedAt: Date.parse(initial.generatedAt) }
      : {}),
  });
  // A newer server read (a router refresh, another page) wins over an older cached status.
  const newer =
    initial !== undefined && Date.parse(initial.generatedAt) > dataUpdatedAt ? initial : undefined;
  useEffect(() => {
    if (newer) {
      qc.setQueryData(syncStatusKey(), newer, { updatedAt: Date.parse(newer.generatedAt) });
    }
  }, [qc, newer]);
  return newer ?? data;
}

/** Refetches the status now (after an action). */
export function useRefreshSyncStatus(): () => Promise<void> {
  const qc = useQueryClient();
  return () => qc.invalidateQueries({ queryKey: syncStatusKey(), exact: true });
}

/**
 * A clock for "3 min ago" that starts at the server's `generatedAt` (so the first render matches
 * the server's HTML) and then ticks every `everyMs`.
 */
export function useNow(start: string, everyMs = 15_000): Date {
  const [now, setNow] = useState(() => new Date(start));
  useEffect(() => {
    setNow(new Date());
    const id = setInterval(() => setNow(new Date()), everyMs);
    return () => clearInterval(id);
  }, [everyMs]);
  return now;
}
