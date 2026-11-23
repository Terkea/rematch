"use client";

import { useRouter } from "next/navigation";
import { type ReactNode, useEffect, useRef, useState, useTransition } from "react";

/** Asks the server to make the scope's new trainer positions (a POST: pages never write). */
export async function requestFreshItems(accountId: string, timeClass: string): Promise<boolean> {
  try {
    const res = await fetch("/api/train/refresh", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ accountId, timeClass }),
    });
    return res.ok;
  } catch {
    return false;
  }
}

/**
 * The trainer's new positions (ADR 0090 Amendment 1): while `stale`, it makes the positions and
 * reloads the page with them. On the page's first render it shows `pending` meanwhile; a game that
 * lands mid-session never hides the session. If the request fails, the page shows what it has.
 */
export function FreshItems({
  stale,
  accountId,
  timeClass,
  pending,
  children,
}: {
  stale: boolean;
  accountId: string;
  timeClass: string;
  pending: ReactNode;
  children: ReactNode;
}) {
  const router = useRouter();
  const [settled, setSettled] = useState(false);
  const [first] = useState(stale);
  const [reloading, startReload] = useTransition();
  // One request per stale scope: a re-render while it runs never asks again.
  const asked = useRef<string | null>(null);
  const routerRef = useRef(router);
  routerRef.current = router;
  useEffect(() => {
    const key = `${accountId}:${timeClass}`;
    if (!stale) {
      asked.current = null;
      return;
    }
    if (asked.current === key) return;
    asked.current = key;
    void requestFreshItems(accountId, timeClass).then((ok) => {
      setSettled(true);
      if (ok) startReload(() => routerRef.current.refresh());
    });
  }, [stale, accountId, timeClass]);
  if (first && stale && (!settled || reloading)) return pending;
  return children;
}

/** The overview's quiet ask: its count already includes the waiting positions. */
export function FreshItemsRequest({
  accountId,
  timeClass,
}: {
  accountId: string;
  timeClass: string;
}) {
  useEffect(() => {
    void requestFreshItems(accountId, timeClass);
  }, [accountId, timeClass]);
  return null;
}
