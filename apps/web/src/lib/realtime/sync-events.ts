import type { StreamEvent } from "@ci/db";
import type { QueryClient, QueryKey } from "@tanstack/react-query";
import type { SyncStatus } from "../../server/sync";
import { queryKeys } from "../query-keys";

/** The user's sync and analysis progress (`GET /api/sync`, `SyncStatus`), all accounts at once. */
export const syncStatusKey = () => ["sync-status"] as const;

/**
 * Observed (never fetched) while a `SyncPanel` is mounted. The panel's page re-renders on the
 * `games` refresh (`LiveRefresh`) and brings a newer status with it, so the stream need not
 * refetch the status for the same event.
 */
export const syncPanelKey = () => ["sync-panel"] as const;

/** At most one status refetch per window, with a trailing one, however fast games are analysed. */
export const SYNC_INVALIDATE_MS = 3000;

const throttles = new WeakMap<
  QueryClient,
  { last: number; trailing: ReturnType<typeof setTimeout> | undefined }
>();

function invalidateStatus(qc: QueryClient): void {
  const t = throttles.get(qc) ?? { last: Number.NEGATIVE_INFINITY, trailing: undefined };
  throttles.set(qc, t);
  if (t.trailing !== undefined) return;
  const run = () => {
    t.trailing = undefined;
    t.last = Date.now();
    void qc.invalidateQueries({ queryKey: syncStatusKey(), exact: true });
  };
  const wait = t.last + SYNC_INVALIDATE_MS - Date.now();
  if (wait <= 0) run();
  else t.trailing = setTimeout(run, wait);
}

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);

const observed = (qc: QueryClient, queryKey: QueryKey) =>
  (qc.getQueryCache().find({ queryKey, exact: true })?.getObserversCount() ?? 0) > 0;

/** A page with the panel and a live `games` refresh will bring a newer status by itself. */
const pageRefreshes = (qc: QueryClient, accountId: string) =>
  observed(qc, syncPanelKey()) && observed(qc, queryKeys.games(accountId));

/**
 * Keeps the sync and analysis status current from the stream (Task P2):
 * - a finished import refreshes the status, and with new games the account's `games` key too
 *   (which `LiveRefresh` turns into a page refresh); with no new games the page is left alone;
 * - the first progress of a run the cached status does not show as checking (a sync the worker
 *   started) refreshes it once; later progress is read from `importProgress` directly;
 * - an analysed game, a failed job and each profile build refresh it, except an analysed game on
 *   a page that refreshes itself (the panel plus `LiveRefresh`), so the status is read once.
 */
export function applySyncEvent(qc: QueryClient, e: StreamEvent): void {
  switch (e.type) {
    case "import.progress": {
      if (!isRecord(e.payload)) return;
      if (e.payload.done === true) {
        const found = typeof e.payload.imported === "number" && e.payload.imported > 0;
        if (found) {
          void qc.invalidateQueries({ queryKey: queryKeys.games(e.accountId), exact: true });
        }
        if (!(found && pageRefreshes(qc, e.accountId))) invalidateStatus(qc);
        return;
      }
      const status = qc.getQueryData<SyncStatus>(syncStatusKey());
      const shown = status?.accounts?.find((a) => a.accountId === e.accountId);
      if (shown && shown.phase !== "checking") invalidateStatus(qc);
      return;
    }
    case "game.analysed":
      if (!pageRefreshes(qc, e.accountId)) invalidateStatus(qc);
      return;
    case "job.failed":
    case "profile.updated":
    case "weakness.changed":
      invalidateStatus(qc);
      return;
    default:
      return;
  }
}
