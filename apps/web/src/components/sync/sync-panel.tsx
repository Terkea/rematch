"use client";

import { ArrowCounterClockwiseIcon, ArrowsClockwiseIcon, Icon, WarningIcon } from "@ci/ui/icons";
import { cn } from "@ci/ui/lib";
import { dur, ease, motion, useReducedMotion } from "@ci/ui/motion";
import { toast } from "@ci/ui/overlays";
import { Button } from "@ci/ui/primitives";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import Link from "next/link";
import { useState } from "react";
import { queryKeys } from "../../lib/query-keys";
import { syncStatusKey } from "../../lib/realtime/sync-events";
import type { AccountSync, SyncStatus } from "../../server/sync";
import { AnalysisStrip } from "./analysis-strip";
import {
  accountLabel,
  analysisLine,
  asideText,
  cooldownText,
  etaText,
  failedText,
  PLATFORM,
  syncLine,
} from "./copy";
import { useNow, useRefreshSyncStatus, useSyncStatus } from "./use-sync-status";

type Variant = "compact" | "full";

/** Changes one account in the cached status (an answer arrives before the refetch). */
function usePatchAccount() {
  const qc = useQueryClient();
  return (accountId: string, patch: Partial<AccountSync>) =>
    qc.setQueryData<SyncStatus>(syncStatusKey(), (old) =>
      old
        ? {
            ...old,
            accounts: old.accounts.map((a) => (a.accountId === accountId ? { ...a, ...patch } : a)),
          }
        : old,
    );
}

type SyncAnswer =
  | { accountId: string; outcome: "queued" | "already" }
  | { error: string; availableAt?: string };

/** "Sync now" for one account, or for each of `ids` ("Sync all"), one request per account. */
function useSync() {
  const patch = usePatchAccount();
  const refresh = useRefreshSyncStatus();
  const [busy, setBusy] = useState<string | null>(null);
  const one = async (accountId: string): Promise<boolean> => {
    const res = await fetch(`/api/accounts/${encodeURIComponent(accountId)}/sync`, {
      method: "POST",
    });
    const body = (await res.json().catch(() => ({}))) as SyncAnswer;
    if (res.status === 429 && "error" in body && body.availableAt) {
      patch(accountId, { nextManualAt: body.availableAt });
      return true;
    }
    if (res.ok) {
      patch(accountId, { phase: "checking" });
      return true;
    }
    return false;
  };
  const run = async (ids: string[], key: string) => {
    setBusy(key);
    try {
      const results = await Promise.all(ids.map((id) => one(id).catch(() => false)));
      if (results.includes(false)) {
        toast("Sync didn't start. Check your connection and try again.");
      }
    } finally {
      setBusy(null);
      void refresh();
    }
  };
  return { run, busy };
}

/** "Retry" for an account's failed games. */
function useRetry() {
  const refresh = useRefreshSyncStatus();
  const [busy, setBusy] = useState<string | null>(null);
  const run = async (accountId: string) => {
    setBusy(accountId);
    try {
      const res = await fetch("/api/sync/retry", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ accountId }),
      });
      if (!res.ok) throw new Error(String(res.status));
      const { queued } = (await res.json()) as { queued: number };
      toast(
        queued > 0
          ? `Retrying ${queued} ${queued === 1 ? "game" : "games"}.`
          : "Those games were retried a moment ago. Try again in 2 minutes.",
      );
    } catch {
      toast("Retry didn't start. Check your connection and try again.");
    } finally {
      setBusy(null);
      void refresh();
    }
  };
  return { run, busy };
}

/** A short pencil stroke idling along its track while an import runs (never a spinner, B18). */
function CheckingStroke() {
  const reduced = useReducedMotion();
  return (
    <span
      aria-hidden="true"
      className="relative block h-0.5 w-16 overflow-hidden rounded-pill bg-well"
    >
      {reduced ? (
        <span className="absolute inset-y-0 left-0 w-1/3 rounded-pill bg-pencil/70" />
      ) : (
        <motion.span
          className="absolute inset-y-0 left-0 w-1/3 rounded-pill bg-pencil/70"
          animate={{ x: ["-100%", "300%"] }}
          transition={{
            duration: dur.soak * 2.5,
            ease: ease.soak,
            repeat: Number.POSITIVE_INFINITY,
          }}
        />
      )}
    </span>
  );
}

function AccountRow({
  a,
  now,
  variant,
  syncing,
  onSync,
  retrying,
  onRetry,
}: {
  a: AccountSync;
  now: Date;
  variant: Variant;
  syncing: boolean;
  onSync: () => void;
  retrying: boolean;
  onRetry: () => void;
}) {
  // The running import's own count, as the stream writes it (`import.progress`); an observer
  // only, it never fetches.
  const { data: progress } = useQuery<{ imported?: number; done?: boolean } | null>({
    queryKey: queryKeys.importProgress(a.accountId),
    queryFn: () => null,
    enabled: false,
  });
  const found = a.phase === "checking" && progress?.done !== true ? progress?.imported : undefined;
  const wait = cooldownText(a.nextManualAt, now);
  const failed = failedText(a.analysis);
  const eta = etaText(a.eta);
  const aside = variant === "full" ? asideText(a.analysis) : null;
  const full = variant === "full";
  const label = accountLabel(a);
  const blocked = a.phase !== "idle" || wait !== null || syncing;

  return (
    <li data-account={a.accountId} className={cn("flex flex-col", full ? "gap-3" : "gap-2")}>
      <div className="flex flex-col gap-0.5">
        <div className="flex items-start justify-between gap-4">
          <p className="flex min-w-0 flex-wrap items-baseline gap-x-2 pt-1">
            <span className={cn("font-medium text-ink", full ? "text-base" : "text-sm")}>
              {a.name}
            </span>
            <span className="text-sm text-ink-faint">{PLATFORM[a.platform]}</span>
            {a.verified ? null : (
              <Link
                href="/app/accounts"
                className="rounded-field text-sm text-ink-soft underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
              >
                Unverified
              </Link>
            )}
          </p>
          <Button
            variant={full ? "outline" : "quiet"}
            size="sm"
            disabled={blocked}
            onClick={onSync}
            aria-label={`Sync now ${label}`}
            className={cn("shrink-0", full ? undefined : "-mr-3")}
          >
            <Icon icon={ArrowsClockwiseIcon} />
            Sync now
          </Button>
        </div>
        <p
          data-part="sync-line"
          aria-live="polite"
          className="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm text-ink-soft"
        >
          <span>{syncLine(a, now, found)}</span>
          {a.phase === "checking" ? <CheckingStroke /> : null}
        </p>
        {wait ? <p className="text-sm text-ink-faint">{wait}</p> : null}
      </div>

      <div className="flex flex-col gap-1.5">
        <AnalysisStrip counts={a.analysis} max={full ? 72 : 48} />
        <p className="text-sm text-ink">{analysisLine(a.analysis)}</p>
        {eta ? <p className="text-sm text-ink-soft">{eta}</p> : null}
        {aside ? <p className="text-xs text-ink-faint">{aside}</p> : null}
      </div>

      {failed ? (
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
          <p className="flex items-center gap-2 text-sm text-ink">
            <Icon icon={WarningIcon} size={16} className="shrink-0" />
            {failed}
          </p>
          {a.analysis.failed > 0 ? (
            <Button
              variant="quiet"
              size="sm"
              disabled={retrying}
              onClick={onRetry}
              className="-ml-3 sm:ml-0"
            >
              <Icon icon={ArrowCounterClockwiseIcon} />
              {`Retry ${a.analysis.failed} ${a.analysis.failed === 1 ? "game" : "games"}`}
            </Button>
          ) : null}
        </div>
      ) : null}

      {a.profileUpdating ? (
        <p className="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm text-ink-soft">
          <span>
            {full
              ? "Profile updating… Your weaknesses and skill areas refresh when it's built."
              : "Profile updating…"}
          </span>
          <CheckingStroke />
        </p>
      ) : null}
    </li>
  );
}

/**
 * "Sync now" and the analysis progress of each linked account (Task P2): compact on the
 * overview, fuller on the games page. It starts from the server's read (`initial`) and stays
 * current through the event stream (`applySyncEvent` invalidates `syncStatusKey`). Every count is
 * stated, the ETA appears only when the worker measured a pace, and failed games stay visible
 * with a retry. It never says "live".
 */
export function SyncPanel({ initial, variant }: { initial: SyncStatus; variant: Variant }) {
  const status = useSyncStatus(initial) ?? initial;
  const now = useNow(initial.generatedAt);
  const sync = useSync();
  const retry = useRetry();
  const { accounts } = status;
  if (accounts.length === 0) return null;
  const full = variant === "full";
  const many = accounts.length > 1;
  const ready = accounts
    .filter((a) => a.phase === "idle" && cooldownText(a.nextManualAt, now) === null)
    .map((a) => a.accountId);

  const list = (
    <ul
      aria-label="Sync and analysis"
      className={cn("m-0 flex list-none flex-col p-0", full ? "gap-8" : "gap-5")}
    >
      {accounts.map((a) => (
        <AccountRow
          key={a.accountId}
          a={a}
          now={now}
          variant={variant}
          syncing={sync.busy === a.accountId || sync.busy === "all"}
          onSync={() => void sync.run([a.accountId], a.accountId)}
          retrying={retry.busy === a.accountId}
          onRetry={() => void retry.run(a.accountId)}
        />
      ))}
    </ul>
  );

  const syncAll = many ? (
    <Button
      variant="outline"
      size="sm"
      disabled={ready.length === 0 || sync.busy !== null}
      onClick={() => void sync.run(ready, "all")}
    >
      <Icon icon={ArrowsClockwiseIcon} />
      Sync all
    </Button>
  ) : null;

  if (!full) {
    return (
      <div data-slot="sync-panel" data-variant="compact" className="flex max-w-2xl flex-col gap-3">
        {list}
        {syncAll ? <div>{syncAll}</div> : null}
      </div>
    );
  }
  return (
    <section
      aria-labelledby="sync-heading"
      data-slot="sync-panel"
      data-variant="full"
      data-surface="sheet"
      className="flex flex-col gap-6 rounded-sheet bg-sheet px-(--space-panel-pad) py-6"
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-col gap-1">
          <h2 id="sync-heading" className="font-serif text-xl font-semibold text-ink">
            Sync and analysis
          </h2>
          <p className="max-w-prose text-sm text-ink-soft">
            New games are fetched from each account, then analysed move by move.
          </p>
        </div>
        {syncAll}
      </div>
      {list}
    </section>
  );
}
