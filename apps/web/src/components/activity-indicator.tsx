"use client";

import { Icon, WarningCircleIcon } from "@ci/ui/icons";
import { dur, ease, motion, useReducedMotion } from "@ci/ui/motion";
import {
  Button,
  Popover,
  PopoverClose,
  PopoverContent,
  PopoverTitle,
  PopoverTrigger,
} from "@ci/ui/primitives";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { queryKeys } from "../lib/query-keys";
import {
  type Activity,
  EMPTY_ACTIVITY,
  importSummary,
  kindLabel,
  updateActivity,
} from "../lib/realtime/activity";
import type { SyncStatus } from "../server/sync";
import { analysisTotal } from "./sync/copy";
import { useSyncStatus } from "./sync/use-sync-status";

/** The visible import line, worded like the progress placeholder's (DESIGN.md, Progress). */
export function importText(s: { done: number; total?: number }): string {
  if (s.total === undefined) return `Importing: ${s.done} games so far`;
  return `Importing ${s.done} / ${s.total} games`;
}

/**
 * What the sync status adds when no import is streaming: a due or running sync, then analysis
 * still to do (done of total over every account), then a profile build. Null when idle.
 */
export function syncSummary(
  status: SyncStatus | undefined,
): { text: string; short: string; fraction: number | null } | null {
  const accounts = status?.accounts ?? [];
  if (accounts.some((a) => a.phase === "checking")) {
    return { text: "Checking for new games", short: "Checking", fraction: null };
  }
  const outstanding = accounts.filter(
    (a) => a.analysis.waiting + a.analysis.analysing + a.analysis.retrying > 0,
  );
  if (outstanding.length > 0) {
    const done = accounts.reduce((s, a) => s + a.analysis.done, 0);
    const total = accounts.reduce((s, a) => s + analysisTotal(a.analysis), 0);
    return {
      text: `Analysing ${done} / ${total} games`,
      short: `${done} / ${total}`,
      fraction: total > 0 ? Math.min(1, done / total) : null,
    };
  }
  if (accounts.some((a) => a.profileUpdating)) {
    return { text: "Updating your profile", short: "Profile", fraction: null };
  }
  return null;
}

const failedText = (n: number) => `${n} ${n === 1 ? "job" : "jobs"} failed`;

const capitalise = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

const clock = new Intl.DateTimeFormat("en-GB", { hour: "2-digit", minute: "2-digit" });

/** Reads `activity` from the cache; the stream is its only writer, so it never refetches. */
function useActivity(): Activity {
  const qc = useQueryClient();
  const { data } = useQuery({
    queryKey: queryKeys.activity(),
    queryFn: () => qc.getQueryData<Activity>(queryKeys.activity()) ?? EMPTY_ACTIVITY,
    initialData: EMPTY_ACTIVITY,
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: Number.POSITIVE_INFINITY,
  });
  return data;
}

/**
 * Background work, quietly, in the header (M4 T23, Task P2): unfinished imports as a count and a pencil
 * stroke that soaks forward with each update, and failed jobs as a button that lists them.
 * Nothing when idle. It reports counts, never a connection state, so it can't claim "live"
 * while updates arrive by polling. Nothing moves unless a count changed (M1); under reduced
 * motion the stroke sets its value at once (M10).
 */
export function ActivityIndicator() {
  const activity = useActivity();
  const qc = useQueryClient();
  const reduced = useReducedMotion();
  const imports = importSummary(activity);
  // The sync status (Task P2): a running sync, analysis left to do, a profile build.
  const status = useSyncStatus();
  const work = imports ? null : syncSummary(status);
  const failures = activity.failures;
  if (!imports && !work && failures.length === 0) return null;

  const fraction =
    imports?.total !== undefined && imports.total > 0
      ? Math.min(1, imports.done / imports.total)
      : null;

  return (
    <motion.div
      data-slot="activity-indicator"
      className="flex items-center gap-2"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      transition={{ duration: dur.quick, ease: ease.out }}
    >
      {imports ? (
        <span className="flex h-10 items-center gap-2 px-1 text-sm whitespace-nowrap text-ink-soft tabular-nums">
          {fraction !== null ? (
            <span
              aria-hidden="true"
              className="relative block h-0.75 w-8 overflow-hidden rounded-pill bg-rule sm:w-12"
            >
              <motion.span
                data-part="stroke"
                className="absolute inset-0 origin-left rounded-pill bg-pencil"
                initial={false}
                animate={{ scaleX: fraction }}
                transition={reduced ? { duration: 0 } : { duration: dur.soak, ease: ease.soak }}
              />
            </span>
          ) : null}
          {/* The words in full from 640px; below, the short count, with the words for assistive tech. */}
          <span className="sr-only sm:not-sr-only">{importText(imports)}</span>
          <span aria-hidden="true" className="sm:hidden">
            {imports.total === undefined
              ? `${imports.done} games`
              : `${imports.done} / ${imports.total}`}
          </span>
        </span>
      ) : null}
      {work ? (
        <span className="flex h-10 items-center gap-2 px-1 text-sm whitespace-nowrap text-ink-soft tabular-nums">
          {work.fraction !== null ? (
            <span
              aria-hidden="true"
              className="relative block h-0.75 w-8 overflow-hidden rounded-pill bg-rule sm:w-12"
            >
              <motion.span
                data-part="stroke"
                className="absolute inset-0 origin-left rounded-pill bg-pencil"
                initial={false}
                animate={{ scaleX: work.fraction }}
                transition={reduced ? { duration: 0 } : { duration: dur.soak, ease: ease.soak }}
              />
            </span>
          ) : null}
          <span className="sr-only sm:not-sr-only">{work.text}</span>
          <span aria-hidden="true" className="sm:hidden">
            {work.short}
          </span>
        </span>
      ) : null}
      {failures.length > 0 ? (
        <Popover>
          <PopoverTrigger
            render={
              <Button variant="quiet" size="sm" className="text-pen-text hover:text-pen-text" />
            }
          >
            <Icon icon={WarningCircleIcon} />
            {/* Below 640px the bare count beside the warning icon; the name stays in words. */}
            <span aria-hidden="true" className="sm:hidden">
              {failures.length}
            </span>
            <span className="max-sm:sr-only">{failedText(failures.length)}</span>
          </PopoverTrigger>
          <PopoverContent align="end" className="w-80 gap-3" data-surface="sheet">
            <PopoverTitle className="text-sm">Background jobs that failed</PopoverTitle>
            <ul className="m-0 flex max-h-72 list-none flex-col gap-3 overflow-y-auto p-0">
              {failures.map((f, i) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: append-only and never reordered; two failures can match in every field.
                <li key={`${f.at}-${i}`} className="flex flex-col gap-0.5">
                  <span className="flex items-baseline justify-between gap-3">
                    <span className="font-medium text-ink">{capitalise(kindLabel(f.kind))}</span>
                    <time
                      dateTime={new Date(f.at).toISOString()}
                      className="text-xs text-ink-faint"
                    >
                      {clock.format(f.at)}
                    </time>
                  </span>
                  <span className="text-ink-soft">{f.reason}</span>
                </li>
              ))}
            </ul>
            <PopoverClose
              render={<Button variant="quiet" size="sm" className="-mx-2 self-start" />}
              onClick={() => updateActivity(qc, (a) => ({ ...a, failures: [] }))}
            >
              Clear
            </PopoverClose>
          </PopoverContent>
        </Popover>
      ) : null}
    </motion.div>
  );
}
