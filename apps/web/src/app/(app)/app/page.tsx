import { cookies } from "next/headers";
import { LiveRefresh } from "../../../components/live-refresh";
import { SyncPanel } from "../../../components/sync/sync-panel";
import { requireUser } from "../../../server/auth/session";
import { getDb } from "../../../server/db";
import { requestEventCursor } from "../../../server/events/request-cursor";
import { scopedAccountIds } from "../../../server/games";
import { getOverview } from "../../../server/overview";
import { leadFinding, peerHeadline, resolveCoachScope } from "../../../server/profile";
import { gameStats, scopeOfFilters } from "../../../server/stats/game-stats";
import { loadPeriodComparison } from "../../../server/stats/period-compare";
import { getSyncStatus } from "../../../server/sync";
import { compareView } from "./_filters/compare-view";
import { FilterBar } from "./_filters/filter-bar";
import { loadFilterBar } from "./_filters/load";
import { colourSplitView } from "./_overview/colour-split-view";
import { OverviewSheet } from "./_overview/sheet";
import { chooseTimeClass, knownTimeClass, REVEAL_COOKIE, toOverviewView } from "./_overview/view";

export const dynamic = "force-dynamic";
export const metadata = { title: "Overview" };

/**
 * The overview (`/app?tc=<timeClass>`): the signed-in player's analysed games. A server component;
 * only the trend is a client island, and it gets a narrow view model. No `tc`, or an unknown or
 * unanalysed one, shows the default class, rapid when there is rapid (I8); `?tc=all` is every
 * class. The shared filter bar (T3, ADR 0083) adds format, colour, dates and the period
 * comparison: they drive the White | Black row and the comparison, read through `gameStats`.
 * `LiveRefresh` re-renders it as the worker analyses more games.
 */
export default async function OverviewPage({
  searchParams,
}: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const user = await requireUser();
  // Before any page data, so the event stream replays whatever lands after this read (T27).
  await requestEventCursor(user.id);
  const db = getDb();
  const params = await searchParams;
  const raw = params.tc;
  const asked = knownTimeClass(raw);
  let overview = await getOverview(db, user.id, { timeClass: asked });
  const tc = chooseTimeClass(
    raw,
    overview.timeClasses.map((c) => c.timeClass),
  );
  if (tc !== asked) overview = await getOverview(db, user.id, { timeClass: tc });
  // The report reveal runs on the first view in a browser session only (DESIGN.md choreography 2).
  const reveal = !(await cookies()).has(REVEAL_COOKIE);
  const [accountIds, lead, sync] = await Promise.all([
    scopedAccountIds(db, user.id),
    leadFinding(db, user.id),
    getSyncStatus(db, user.id),
  ]);
  const now = new Date();
  const bar = await loadFilterBar(db, params, {
    basePath: "/app",
    accountIds,
    timeClasses: overview.timeClasses.map((c) => ({ tc: c.timeClass, games: c.analysed })),
    allTimeClasses: true,
    color: "on",
    dates: true,
    compare: true,
    now,
  });
  const f = { ...bar.filters, tc };
  // With no analysed game the page is an empty state: nothing to split or compare.
  const ready = overview.timeClasses.length > 0;
  const [stats, compared] = ready
    ? await Promise.all([
        gameStats(db, user.id, scopeOfFilters(f)),
        loadPeriodComparison(db, user.id, f, now),
      ])
    : [null, null];
  const basis = tc ? tc : "";
  // The headline's peer comparison (Q1): only for one account's one time class, which is what a
  // peer benchmark covers; "All" or several accounts pool games no benchmark was built for.
  const coach =
    ready && tc && accountIds.length === 1 ? await resolveCoachScope(db, user.id, { tc }) : null;
  const peer = coach && coach.timeClass === tc ? await peerHeadline(db, user.id, coach) : null;
  return (
    <>
      <LiveRefresh accountIds={accountIds} />
      <OverviewSheet
        view={toOverviewView(overview, tc)}
        filterBar={<FilterBar model={bar.model} />}
        colourSplit={stats ? colourSplitView(stats, f.color, basis) : null}
        compare={compared ? compareView(compared) : null}
        filtersNote={filtersNote(f)}
        reveal={reveal}
        peer={peer}
        lead={lead}
        sync={sync.accounts.length > 0 ? <SyncPanel initial={sync} variant="compact" /> : undefined}
      />
    </>
  );
}

/** What the filters beyond the class change on this page, said once (null with none set). */
function filtersNote(f: {
  fmt: string | null;
  color: string;
  from: string | null;
  to: string | null;
}) {
  if (f.fmt === null && f.color === "both" && f.from === null && f.to === null) return null;
  return "Format, colour and dates apply to the colour split and the comparison. The headline and charts below cover every analysed game of the time control.";
}
