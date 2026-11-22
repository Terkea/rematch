import { cookies } from "next/headers";
import { LiveRefresh } from "../../../components/live-refresh";
import { SyncPanel } from "../../../components/sync/sync-panel";
import { requireUser } from "../../../server/auth/session";
import { getDb } from "../../../server/db";
import { requestEventCursor } from "../../../server/events/request-cursor";
import { scopedAccountIds } from "../../../server/games";
import { getOverview } from "../../../server/overview";
import { leadFinding } from "../../../server/profile";
import { getSyncStatus } from "../../../server/sync";
import { OverviewSheet } from "./_overview/sheet";
import { knownTimeClass, parseTimeClass, REVEAL_COOKIE, toOverviewView } from "./_overview/view";

export const dynamic = "force-dynamic";
export const metadata = { title: "Overview" };

/**
 * The overview (`/app?tc=<timeClass>`): the signed-in player's analysed games. A server component;
 * only the trend is a client island, and it gets a narrow view model. An unknown or unanalysed
 * `tc` falls back to All. `LiveRefresh` re-renders it as the worker analyses more games.
 */
export default async function OverviewPage({
  searchParams,
}: {
  searchParams: Promise<{ tc?: string | string[] }>;
}) {
  const user = await requireUser();
  // Before any page data, so the event stream replays whatever lands after this read (T27).
  await requestEventCursor(user.id);
  const db = getDb();
  const asked = knownTimeClass((await searchParams).tc);
  let overview = await getOverview(db, user.id, { timeClass: asked });
  const tc = parseTimeClass(
    asked ?? undefined,
    overview.timeClasses.map((c) => c.timeClass),
  );
  if (asked !== null && tc === null) overview = await getOverview(db, user.id, { timeClass: null });
  // The report reveal runs on the first view in a browser session only (DESIGN.md choreography 2).
  const reveal = !(await cookies()).has(REVEAL_COOKIE);
  const [accountIds, lead, sync] = await Promise.all([
    scopedAccountIds(db, user.id),
    leadFinding(db, user.id),
    getSyncStatus(db, user.id),
  ]);
  return (
    <>
      <LiveRefresh accountIds={accountIds} />
      <OverviewSheet
        view={toOverviewView(overview, tc)}
        reveal={reveal}
        lead={lead}
        sync={sync.accounts.length > 0 ? <SyncPanel initial={sync} variant="compact" /> : undefined}
      />
    </>
  );
}
