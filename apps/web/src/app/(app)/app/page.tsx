import { cookies } from "next/headers";
import { FocusCard } from "../../../components/focus/focus-card";
import { LiveRefresh } from "../../../components/live-refresh";
import { SyncPanel } from "../../../components/sync/sync-panel";
import { requireUser } from "../../../server/auth/session";
import { getDb } from "../../../server/db";
import { requestEventCursor } from "../../../server/events/request-cursor";
import { getFocusCard } from "../../../server/focus";
import { scopedAccountIds } from "../../../server/games";
import { getOverview } from "../../../server/overview";
import { leadFinding, peerHeadline, resolveCoachScope } from "../../../server/profile";
import { gameStats, scopeOfFilters } from "../../../server/stats/game-stats";
import { loadPeriodComparison } from "../../../server/stats/period-compare";
import { getSyncStatus } from "../../../server/sync";
import { dayOf, isSourceKey } from "../../../server/train/model";
import { loadPractiseToday } from "../../../server/train/store";
import { loadTrajectory } from "../../../server/trajectory/load";
import { compareView } from "./_filters/compare-view";
import { FilterBar, TimeControlSwitch } from "./_filters/filter-bar";
import { loadFilterBar } from "./_filters/load";
import { colourSplitView } from "./_overview/colour-split-view";
import { PractiseCard } from "./_overview/practise-card";
import { practiseView } from "./_overview/practise-view";
import { OverviewSheet } from "./_overview/sheet";
import { chooseTimeClass, knownTimeClass, REVEAL_COOKIE, toOverviewView } from "./_overview/view";
import { TrajectoryCard } from "./_trajectory/card";
import { trajectoryView } from "./_trajectory/view";

export const dynamic = "force-dynamic";
export const metadata = { title: "Overview" };

/**
 * The overview (`/app?tc=<timeClass>`): the signed-in player's analysed games, as a focused flow
 * (ADR 0092): where you are, this week's focus, practise today, how you're doing, then Details. A
 * server component; only the trend is a client island, and it gets a narrow view model. No `tc`,
 * or an unknown or unanalysed one, shows the default class, rapid when there is rapid (I8);
 * `?tc=all` is every class. The time control scopes the whole page; the rest of the shared filter
 * bar (T3, ADR 0083), format, colour, dates and the period comparison, scopes Details: a filtered
 * White | Black row and the comparison, read through `gameStats`. `LiveRefresh` re-renders it as
 * the worker analyses more games.
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
  // The summary's White | Black covers the whole time control, like the headline beside it; the
  // filters beyond the class get their own row in Details.
  const filtered = f.fmt !== null || f.color !== "both" || f.from !== null || f.to !== null;
  const whole = { ...f, fmt: null, color: "both" as const, from: null, to: null };
  const [stats, filteredStats, compared] = ready
    ? await Promise.all([
        gameStats(db, user.id, scopeOfFilters(whole)),
        filtered ? gameStats(db, user.id, scopeOfFilters(f)) : null,
        loadPeriodComparison(db, user.id, f, now),
      ])
    : [null, null, null];
  const basis = tc ? tc : "";
  // The headline's peer comparison (Q1): only for one account's one time class, which is what a
  // peer benchmark covers; "All" or several accounts pool games no benchmark was built for.
  const coach =
    ready && tc && accountIds.length === 1 ? await resolveCoachScope(db, user.id, { tc }) : null;
  const own = coach && coach.timeClass === tc ? coach : null;
  const [peer, trajectory] = own
    ? await Promise.all([peerHeadline(db, user.id, own), loadTrajectory(db, user.id, own)])
    : [null, null];
  // The weekly focus (ADR 0091) of the class shown, on the default account; "All" shows rapid's.
  const focusScope = ready
    ? (coach ?? (await resolveCoachScope(db, user.id, tc ? { tc } : {})))
    : null;
  const focus = focusScope ? await getFocusCard(db, user.id, focusScope) : null;
  // The one practise call (ADR 0092): today's trainer session, kept to the focus's source.
  const focusSource =
    focus?.kind === "active" && isSourceKey(focus.focus.seriesKey) ? focus.focus.seriesKey : null;
  const practise = focusScope
    ? await loadPractiseToday(db, user.id, focusScope, { source: focusSource, now })
    : null;
  return (
    <>
      <LiveRefresh accountIds={accountIds} />
      <OverviewSheet
        view={toOverviewView(overview, tc)}
        timeControl={<TimeControlSwitch model={bar.model} />}
        filterBar={<FilterBar model={bar.model} timeControl={false} />}
        colourSplit={stats ? colourSplitView(stats, "both", basis) : null}
        filteredSplit={filteredStats ? colourSplitView(filteredStats, f.color, basis) : null}
        compare={compared ? compareView(compared) : null}
        filtersNote={filtersNote(f)}
        reveal={reveal}
        peer={peer}
        focus={focus ? <FocusCard card={focus} practise={false} /> : null}
        focusSeries={
          focus?.kind === "active"
            ? focus.focus.seriesKey
            : focus?.kind === "suggest"
              ? focus.suggestion.seriesKey
              : null
        }
        practise={
          practise && focusScope ? (
            <PractiseCard view={practiseView(practise, focusScope, dayOf(now))} />
          ) : null
        }
        trajectory={
          trajectory && own ? (
            <TrajectoryCard
              view={trajectoryView(trajectory, own.timeClass)}
              href={`/app/progress?tc=${own.timeClass}`}
            />
          ) : null
        }
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
  return "Format, colour and dates apply to the colour split and the comparison here. The summary above and the charts below cover every analysed game of the time control.";
}
