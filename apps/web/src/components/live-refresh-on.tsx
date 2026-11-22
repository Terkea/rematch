"use client";

import { type QueryKey, useQueries, useQueryClient } from "@tanstack/react-query";
import { useRouter } from "next/navigation";
import { useEffect, useLayoutEffect, useRef } from "react";
import { REFRESH_THROTTLE_MS } from "./live-refresh";

function sameKey(a: readonly unknown[], b: readonly unknown[]): boolean {
  return a.length === b.length && a.every((part, i) => part === b[i]);
}

/**
 * M5a's `LiveRefresh` for any query keys (the coach pages watch `report`, `weekly` and `notes`):
 * one never-stale observer per key, so the stream's invalidation (`applyCoachEvent`) lands on a
 * live query and becomes a `router.refresh()`, at most one per `REFRESH_THROTTLE_MS` with a
 * trailing call. Server components stay the only read path. Renders nothing.
 */
export function LiveRefreshOn({ keys }: { keys: QueryKey[] }) {
  const router = useRouter();
  const client = useQueryClient();
  useQueries({
    queries: keys.map((queryKey) => ({
      queryKey,
      queryFn: () => Date.now(),
      staleTime: Number.POSITIVE_INFINITY,
    })),
  });

  const refresh = useRef(router.refresh);
  const watched = useRef(keys);
  useLayoutEffect(() => {
    refresh.current = router.refresh;
    watched.current = keys;
  });

  useEffect(() => {
    let last = Number.NEGATIVE_INFINITY;
    let trailing: ReturnType<typeof setTimeout> | undefined;
    const run = () => {
      trailing = undefined;
      last = Date.now();
      refresh.current();
    };
    const request = () => {
      if (trailing !== undefined) return;
      const wait = last + REFRESH_THROTTLE_MS - Date.now();
      if (wait <= 0) run();
      else trailing = setTimeout(run, wait);
    };
    const unsubscribe = client.getQueryCache().subscribe((event) => {
      if (event.type !== "updated" || event.action.type !== "invalidate") return;
      const key = event.query.queryKey;
      if (watched.current.some((k) => sameKey(k, key))) request();
    });
    return () => {
      unsubscribe();
      if (trailing !== undefined) clearTimeout(trailing);
    };
  }, [client]);

  return null;
}
