"use client";

import { useRouter } from "next/navigation";
import { type ReactNode, useEffect, useState, useTransition } from "react";

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
 * The trainer's new positions (ADR 0090 Amendment 1): while `stale`, it shows `pending`, makes the
 * positions and reloads the page with them. If that fails, the page shows what it has.
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
  const [reloading, startReload] = useTransition();
  useEffect(() => {
    if (!stale) return;
    let live = true;
    void requestFreshItems(accountId, timeClass).then((ok) => {
      if (!live) return;
      setSettled(true);
      if (ok) startReload(() => router.refresh());
    });
    return () => {
      live = false;
    };
  }, [stale, accountId, timeClass, router]);
  if (stale && (!settled || reloading)) return pending;
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
