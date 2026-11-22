"use client";

import { ArrowCounterClockwiseIcon, ArrowsClockwiseIcon, Icon, WarningIcon } from "@ci/ui/icons";
import { cn } from "@ci/ui/lib";
import { dur, ease, motion, useReducedMotion } from "@ci/ui/motion";
import { toast } from "@ci/ui/overlays";
import { Button, Tooltip, TooltipContent, TooltipTrigger, touchTarget } from "@ci/ui/primitives";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import Link from "next/link";
import { type ReactNode, useId, useState } from "react";
import { queryKeys } from "../../lib/query-keys";
import { syncPanelKey, syncStatusKey } from "../../lib/realtime/sync-events";
import type { AccountSync, SyncStatus } from "../../server/sync";
import { AnalysisStrip } from "./analysis-strip";
import {
  accountLabel,
  analysisLine,
  analysisTotal,
  asideText,
  cooldownText,
  etaText,
  failedText,
  PLATFORM,
  stateLine,
  summaryLine,
  summaryShort,
  syncLine,
  syncReason,
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

/** What a failed "Sync now" says: what happened and how to fix it. */
function failureText(status: number): string {
  if (status === 401) return "You're signed out. Sign in again to sync.";
  if (status === 404) {
    return "This account isn't linked any more. Reload the page to see your accounts.";
  }
  return "Sync didn't start. Check your connection and try again.";
}

/** "Sync now" for one account, or for each of `ids` ("Sync all"), one request per account. */
function useSync() {
  const patch = usePatchAccount();
  const refresh = useRefreshSyncStatus();
  const [busy, setBusy] = useState<string | null>(null);
  /** 0 when it started (or is limited), else the HTTP status (or -1 offline). */
  const one = async (accountId: string): Promise<number> => {
    const res = await fetch(`/api/accounts/${encodeURIComponent(accountId)}/sync`, {
      method: "POST",
    });
    const body = (await res.json().catch(() => ({}))) as SyncAnswer;
    if (res.status === 429 && "error" in body && body.availableAt) {
      patch(accountId, { nextManualAt: body.availableAt });
      return 0;
    }
    if (res.ok) {
      if ("outcome" in body && body.outcome === "queued") patch(accountId, { phase: "checking" });
      return 0;
    }
    return res.status;
  };
  const run = async (ids: string[], key: string) => {
    setBusy(key);
    try {
      const results = await Promise.all(ids.map((id) => one(id).catch(() => -1)));
      const failed = results.find((r) => r !== 0);
      if (failed !== undefined) toast(failureText(failed));
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
      if (!res.ok) {
        toast(
          res.status === 401 || res.status === 404
            ? failureText(res.status)
            : "Retry didn't start. Check your connection and try again.",
        );
        return;
      }
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

/**
 * "Sync now" or "Sync all". While unavailable it stays focusable with `aria-disabled`, reads as
 * unavailable (a dashed, faint edge and faint text, not just a lighter grey), ignores clicks, and
 * names the reason in a tooltip and in its accessible description.
 */
function SyncButton({
  label,
  reason,
  shownBy,
  onClick,
  children,
}: {
  label?: string;
  reason: string | null;
  /** The id of a visible element that already states `reason` word for word. */
  shownBy?: string | undefined;
  onClick: () => void;
  children: ReactNode;
}) {
  const own = useId();
  const id = shownBy ?? own;
  const unavailable = reason !== null;
  const button = (
    <Button
      variant="outline"
      size="sm"
      aria-label={label}
      aria-disabled={unavailable || undefined}
      aria-describedby={unavailable ? id : undefined}
      onClick={unavailable ? undefined : onClick}
      className={cn(
        "shrink-0",
        unavailable &&
          "cursor-not-allowed border-dashed border-rule-strong text-ink-faint hover:bg-transparent aria-disabled:pointer-events-auto active:bg-transparent active:inset-ring-0",
      )}
    >
      <Icon icon={ArrowsClockwiseIcon} />
      {children}
    </Button>
  );
  if (!unavailable) return button;
  return (
    <>
      <Tooltip>
        <TooltipTrigger render={button} />
        <TooltipContent>{reason}</TooltipContent>
      </Tooltip>
      {shownBy ? null : (
        <span id={id} className="sr-only">
          {reason}
        </span>
      )}
    </>
  );
}

/** A short pencil stroke idling along its track while work runs (never a spinner, B18). */
function WorkingStroke() {
  const reduced = useReducedMotion();
  return (
    <span
      aria-hidden="true"
      className="relative inline-block h-0.5 w-10 shrink-0 overflow-hidden rounded-pill bg-well align-middle"
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

/** A line led by the working stroke, so a wrapped line never leaves the stroke on its own. */
function Working({ children }: { children: ReactNode }) {
  return (
    <span className="flex items-baseline gap-2">
      <span className="flex h-5 items-center">
        <WorkingStroke />
      </span>
      <span>{children}</span>
    </span>
  );
}

function AccountRow({
  a,
  now,
  syncing,
  onSync,
  retrying,
  onRetry,
}: {
  a: AccountSync;
  now: Date;
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
  const wait = a.phase === "idle" ? cooldownText(a.nextManualAt, now) : null;
  const failed = failedText(a.analysis);
  const eta = etaText(a.eta);
  const aside = asideText(a.analysis);
  const reason = syncing ? "Starting a sync…" : syncReason(a, now);
  const allDone = analysisTotal(a.analysis) > 0 && a.analysis.done === analysisTotal(a.analysis);
  const line = syncLine(a, now, found);
  const lineId = useId();
  const waitId = useId();
  // The visible line that already says why Sync now is unavailable, if one does.
  const shownBy =
    reason === null ? undefined : reason === line ? lineId : reason === wait ? waitId : undefined;

  return (
    <li data-account={a.accountId} className="flex flex-col gap-3">
      <div className="flex flex-col gap-0.5">
        <div className="flex items-start justify-between gap-4">
          <p className="flex min-w-0 flex-wrap items-baseline gap-x-2 pt-1">
            <span className="text-base font-medium text-ink">{a.name}</span>
            <span className="text-sm text-ink-soft">{PLATFORM[a.platform]}</span>
            {a.verified ? null : (
              <Link
                href="/app/accounts"
                className={cn(
                  "relative rounded-field text-sm text-ink-soft underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
                  touchTarget,
                )}
              >
                Unverified
              </Link>
            )}
          </p>
          <SyncButton
            label={`Sync now ${accountLabel(a)}`}
            reason={reason}
            shownBy={shownBy}
            onClick={onSync}
          >
            Sync now
          </SyncButton>
        </div>
        <p id={lineId} data-part="sync-line" aria-live="polite" className="text-sm text-ink-soft">
          {a.phase === "checking" ? <Working>{line}</Working> : line}
        </p>
        {wait ? (
          <p id={waitId} className="text-sm text-ink-soft">
            {wait}
          </p>
        ) : null}
      </div>

      <div className="flex flex-col gap-1.5">
        {allDone ? null : <AnalysisStrip counts={a.analysis} />}
        <p className="text-sm text-ink">{analysisLine(a.analysis)}</p>
        {eta ? <p className="text-sm text-ink-soft">{eta}</p> : null}
        {aside ? <p className="text-sm text-ink-soft">{aside}</p> : null}
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
        <p className="text-sm text-ink-soft">
          <Working>
            Profile updating… Your weaknesses and skill areas refresh when it's built.
          </Working>
        </p>
      ) : null}
    </li>
  );
}

/**
 * The overview's two lines: every account's totals (all time controls) and the most pressing
 * sync state, with one "Sync now" (one account) or "Sync all", and a link to the full panel on
 * the games page. No tally and no per-account rows, so the coaching headline stays on the first
 * screen.
 */
function CompactPanel({
  accounts,
  now,
  sync,
}: {
  accounts: AccountSync[];
  now: Date;
  sync: ReturnType<typeof useSync>;
}) {
  const ready = accounts.filter((a) => syncReason(a, now) === null).map((a) => a.accountId);
  const one = accounts.length === 1 ? accounts[0] : undefined;
  const reason =
    sync.busy !== null
      ? "Starting a sync…"
      : ready.length > 0
        ? null
        : one
          ? syncReason(one, now)
          : "Every account is syncing or was synced in the last 2 minutes.";
  const unverified = accounts.filter((a) => !a.verified);
  const checking = accounts.some((a) => a.phase === "checking");
  const state = stateLine(accounts, now);
  return (
    <div
      data-slot="sync-panel"
      data-variant="compact"
      className="flex max-w-3xl items-start justify-between gap-4"
    >
      <div className="flex min-w-0 flex-col gap-0.5 text-sm">
        <p className="text-ink">
          <span className="max-sm:hidden">{summaryLine(accounts)}</span>
          <span className="sm:hidden">{summaryShort(accounts)}</span>
        </p>
        <p aria-live="polite" className="text-ink-soft">
          {checking ? <WorkingStroke /> : null}
          {checking ? " " : null}
          <span>{state}</span>{" "}
          <Link
            href="/app/games"
            className="rounded-field text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
          >
            Details
          </Link>
          {unverified.length > 0 ? (
            <>
              <span className="max-sm:hidden"> </span>
              <Link
                href="/app/accounts"
                className="rounded-field underline max-sm:hidden decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
              >
                {`Verify ${unverified.map((a) => a.name).join(" and ")}`}
              </Link>
            </>
          ) : null}
        </p>
      </div>
      {one ? (
        <SyncButton
          label={`Sync now ${accountLabel(one)}`}
          reason={reason}
          onClick={() => void sync.run([one.accountId], one.accountId)}
        >
          Sync now
        </SyncButton>
      ) : (
        <SyncButton reason={reason} onClick={() => void sync.run(ready, "all")}>
          Sync all
        </SyncButton>
      )}
    </div>
  );
}

/**
 * "Sync now" and the analysis progress of each linked account (Task P2): two lines on the
 * overview, the full panel on the games page. It starts from the server's read (`initial`) and
 * stays current through the event stream (`applySyncEvent` invalidates `syncStatusKey`; while it
 * is mounted, `syncPanelKey` lets an analysed game be left to the page's own refresh). Every
 * count is stated, the ETA appears only when the worker measured a pace, and failed games stay
 * visible with a retry. It never says "live".
 */
export function SyncPanel({ initial, variant }: { initial: SyncStatus; variant: Variant }) {
  const status = useSyncStatus(initial) ?? initial;
  const now = useNow(initial.generatedAt);
  const sync = useSync();
  const retry = useRetry();
  useQuery({ queryKey: syncPanelKey(), queryFn: () => null, enabled: false });
  const { accounts } = status;
  if (accounts.length === 0) return null;
  if (variant === "compact") return <CompactPanel accounts={accounts} now={now} sync={sync} />;

  const ready = accounts.filter((a) => syncReason(a, now) === null).map((a) => a.accountId);
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
        {accounts.length > 1 ? (
          <SyncButton
            reason={
              sync.busy !== null
                ? "Starting a sync…"
                : ready.length > 0
                  ? null
                  : "Every account is syncing or was synced in the last 2 minutes."
            }
            onClick={() => void sync.run(ready, "all")}
          >
            Sync all
          </SyncButton>
        ) : null}
      </div>
      <ul aria-label="Sync and analysis" className="m-0 flex list-none flex-col gap-8 p-0">
        {accounts.map((a) => (
          <AccountRow
            key={a.accountId}
            a={a}
            now={now}
            syncing={sync.busy === a.accountId || sync.busy === "all"}
            onSync={() => void sync.run([a.accountId], a.accountId)}
            retrying={retry.busy === a.accountId}
            onRetry={() => void retry.run(a.accountId)}
          />
        ))}
      </ul>
    </section>
  );
}
