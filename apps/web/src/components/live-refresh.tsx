"use client";

import { useQueries, useQueryClient } from "@tanstack/react-query";
import { useRouter } from "next/navigation";
import { useEffect, useLayoutEffect, useRef } from "react";
import { queryKeys } from "../lib/query-keys";

/** At most one `router.refresh()` per window, with a trailing call for events inside it. */
export const REFRESH_THROTTLE_MS = 3000;

function sameKey(a: readonly unknown[], b: readonly unknown[]): boolean {
  return a.length === b.length && a.every((part, i) => part === b[i]);
}

/**
 * Re-renders the current server page when one of `accountIds` gets a newly analysed game
 * (ADR 0056). It keeps one never-stale observer per `games(accountId)`, so the stream's
 * `invalidateQueries` (T23 `applyEvent`, on `game.analysed`) lands on a live query; the
 * invalidation then becomes a throttled `router.refresh()`. Server components stay the only read
 * path: nothing here holds game data. Renders nothing.
 */
export function LiveRefresh({ accountIds }: { accountIds: string[] }) {
  const router = useRouter();
  const client = useQueryClient();
  useQueries({
    queries: accountIds.map((id) => ({
      queryKey: queryKeys.games(id),
      queryFn: () => Date.now(),
      staleTime: Number.POSITIVE_INFINITY,
    })),
  });

  // The refresh and the watched keys are read through refs, so a re-render never resets the
  // throttle window. They are updated after render, never during it.
  const refresh = useRef(router.refresh);
  const keys = useRef(accountIds.map((id) => queryKeys.games(id)));
  useLayoutEffect(() => {
    refresh.current = router.refresh;
    keys.current = accountIds.map((id) => queryKeys.games(id));
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
      if (keys.current.some((watched) => sameKey(watched, key))) request();
    });
    return () => {
      unsubscribe();
      if (trailing !== undefined) clearTimeout(trailing);
    };
  }, [client]);

  return null;
}
