"use client";

import { useRouter } from "next/navigation";
import { useEffect } from "react";
import { TIME_ZONE_COOKIE } from "../lib/time-zone";

/**
 * Sends the browser's time zone as a cookie (R2-8), so the trainer's day turns at the player's
 * midnight. The first load, or a move to another zone, refreshes the page once with it.
 */
export function TimeZoneCookie() {
  const router = useRouter();
  useEffect(() => {
    let zone: string;
    try {
      zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
    } catch {
      return;
    }
    if (!zone) return;
    const value = encodeURIComponent(zone);
    const current = document.cookie
      .split(";")
      .map((c) => c.trim())
      .find((c) => c.startsWith(`${TIME_ZONE_COOKIE}=`));
    if (current === `${TIME_ZONE_COOKIE}=${value}`) return;
    // biome-ignore lint/suspicious/noDocumentCookie: one value, read by the server
    document.cookie = `${TIME_ZONE_COOKIE}=${value}; path=/; max-age=31536000; SameSite=Lax`;
    if (zone !== "UTC" || current !== undefined) router.refresh();
  }, [router]);
  return null;
}
