"use client";

import { toast } from "@ci/ui/overlays";
import { Button } from "@ci/ui/primitives";
import { dayText } from "@ci/ui/profile";
import { useRouter } from "next/navigation";
import { useState } from "react";
import type { TimeClass } from "../../../../server/games/types";

export type ReportScope = { accountId: string; timeClass: TimeClass };

const pad = (n: number) => String(n).padStart(2, "0");

/** "14:05 UTC". */
export const utcTime = (d: Date) => `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())} UTC`;

/**
 * "at 14:05 UTC" on the same UTC day as `now`, "on 23 Nov at 08:00 UTC" within two days
 * (the beta's daily interval turns on the hour), else "on 22 Dec 2026".
 */
export function whenText(at: Date, now: Date = new Date()): string {
  const day = (d: Date) => d.toISOString().slice(0, 10);
  if (day(at) === day(now)) return `at ${utcTime(at)}`;
  const date = dayText(at.toISOString());
  // Within two days the year is plain and the hour is what matters ("on 23 Nov at 08:00 UTC").
  return at.getTime() - now.getTime() <= 2 * 86_400_000
    ? `on ${date.replace(/ \d{4}$/, "")} at ${utcTime(at)}`
    : `on ${date}`;
}

/** The time a rebuild is refused until, from the 429 body, or null. */
function availableAt(body: unknown): Date | null {
  const raw = (body as { availableAt?: unknown } | null)?.availableAt;
  if (typeof raw !== "string") return null;
  const d = new Date(raw);
  return Number.isNaN(d.getTime()) ? null : d;
}

/**
 * "Build my report" / "Rebuild report": POST /api/coach/report (ruling R4). The worker writes the
 * sections and the stream re-renders the page, so a success only says that it started. A refusal
 * says when the next try is possible: the plan interval or the failure throttle, with its time.
 */
export function useRequestReport(scope: ReportScope, first: boolean) {
  const router = useRouter();
  const [pending, setPending] = useState(false);
  const [blockedUntil, setBlockedUntil] = useState<Date | null>(null);

  const request = async () => {
    if (pending) return;
    setPending(true);
    try {
      const res = await fetch("/api/coach/report", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ accountId: scope.accountId, timeClass: scope.timeClass }),
      });
      const body: unknown = await res.json().catch(() => null);
      if (res.status === 202) {
        toast(
          first
            ? "Building your report. Sections appear as they're written."
            : "Rebuilding your report. Sections appear as they're written.",
        );
        router.refresh();
        return;
      }
      const error = (body as { error?: unknown } | null)?.error;
      const at = availableAt(body);
      if (res.status === 429 && at) {
        setBlockedUntil(at);
        toast.error(
          error === "throttled"
            ? `Your last report attempts didn't finish, so rebuilding is paused. Try again ${whenText(at)}.`
            : `You can rebuild this report ${whenText(at)}.`,
        );
        return;
      }
      if (res.status === 404) {
        toast.error(
          "This account isn't linked to you any more. Reload the page to see your accounts.",
        );
        return;
      }
      toast.error("Your report couldn't be requested. Check your connection and try again.");
    } catch {
      toast.error("Your report couldn't be requested. Check your connection and try again.");
    } finally {
      setPending(false);
    }
  };

  return { request, pending, blockedUntil };
}

/**
 * The report's rebuild control. Disabled while a run is building, and before `nextManualAt` (or a
 * refusal's time) it names the date it becomes available instead of failing later.
 */
export function RebuildButton({
  scope,
  nextManualAt,
  building,
  first = false,
  now,
}: {
  scope: ReportScope;
  nextManualAt: Date | null;
  building: boolean;
  /** The render's clock (the server's), so server and client agree on "at" or "on". */
  now: Date;
  /** No earlier report: a running build is the first one, not a rebuild. */
  first?: boolean;
}) {
  const { request, pending, blockedUntil } = useRequestReport(scope, false);
  const until =
    blockedUntil && (!nextManualAt || blockedUntil > nextManualAt) ? blockedUntil : nextManualAt;
  const label = building
    ? first
      ? "Building your first report"
      : "Rebuilding your report"
    : until
      ? `Rebuild available ${whenText(until, now)}`
      : "Rebuild report";
  return (
    <Button
      variant="outline"
      size="md"
      disabled={building || until !== null || pending}
      onClick={() => void request()}
    >
      {label}
    </Button>
  );
}
