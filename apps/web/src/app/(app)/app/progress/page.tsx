import { Button } from "@ci/ui/primitives";
import Link from "next/link";
import { CoachScopeSwitch } from "../../../../components/coach-scope-switch";
import { FocusCard } from "../../../../components/focus/focus-card";
import { LiveRefreshOn } from "../../../../components/live-refresh-on";
import { queryKeys } from "../../../../lib/query-keys";
import { requireUser } from "../../../../server/auth/session";
import { requestNow } from "../../../../server/clock";
import { getDb } from "../../../../server/db";
import { requestEventCursor } from "../../../../server/events/request-cursor";
import { getFocusCard } from "../../../../server/focus";
import { resolveCoachScope } from "../../../../server/profile";
import { loadPeriodComparison } from "../../../../server/stats/period-compare";
import { loadTrajectory } from "../../../../server/trajectory/load";
import { EmptySheet, PageHeading } from "../../_shell/page-heading";
import { CompareBlock } from "../_filters/compare-block";
import { compareView } from "../_filters/compare-view";
import { FilterBar } from "../_filters/filter-bar";
import { loadCoachBar, manyAccounts } from "../_filters/load";
import { storedSide } from "../_filters/side";
import { TrajectorySection } from "../_trajectory/section";
import { trajectoryView } from "../_trajectory/view";
import { loadProgress } from "./load";
import { ProgressView, toProgressView } from "./progress-view";

export const dynamic = "force-dynamic";
export const metadata = { title: "Progress" };

/**
 * The progress page (`/app/progress?account=&tc=`): before and after, the rating and KPI trends,
 * the weakness timeline and the weekly reviews of one coach scope (ruling R1). Every read goes
 * through the read models, which check the scope is the user's. A new weekly review or report
 * refreshes the page through the event stream.
 */
export default async function ProgressPage({
  searchParams,
}: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const user = await requireUser();
  // Before any page data, so the event stream replays whatever lands after this read (T27).
  await requestEventCursor(user.id);
  const db = getDb();
  const params = await searchParams;
  const scope = await resolveCoachScope(db, user.id, params);
  if (!scope) {
    return (
      <div className="flex flex-col gap-8">
        <PageHeading title="Progress" />
        <EmptySheet
          action={
            <Button asChild variant="outline">
              <Link href="/app/accounts">Link an account</Link>
            </Button>
          }
        >
          Progress appears once your games are analysed.
        </EmptySheet>
      </div>
    );
  }
  const now = await requestNow();
  const bar = await loadCoachBar(db, params, scope, {
    basePath: "/app/progress",
    formatSupported: false,
    color: "on",
    dates: true,
    compare: true,
    now,
  });
  const f = bar.filters;
  const [input, compared, trajectory, focus] = await Promise.all([
    loadProgress(db, user.id, scope, now, { from: f.from, to: f.to }, storedSide(f)),
    loadPeriodComparison(db, user.id, f, now, { accountId: scope.accountId }),
    loadTrajectory(db, user.id, scope),
    getFocusCard(db, user.id, scope),
  ]);
  const view = toProgressView(input);
  return (
    <>
      <LiveRefreshOn
        keys={[
          queryKeys.weekly(scope.accountId),
          queryKeys.report(scope.accountId),
          queryKeys.games(scope.accountId),
        ]}
      />
      <ProgressView
        view={view}
        scopeSwitch={
          <>
            {manyAccounts(scope) ? (
              <CoachScopeSwitch scope={scope} basePath="/app/progress" />
            ) : null}
            <FilterBar model={bar.model} />
            {f.color !== "both" ? (
              <p className="m-0 max-w-prose text-sm text-ink-soft">
                Colour applies to the comparison and the weekly trends. Before and after and the
                weakness timeline cover both colours: states are called on all your games.
              </p>
            ) : null}
          </>
        }
        compare={compared ? <CompareBlock view={compareView(compared)} /> : null}
        trajectory={
          trajectory ? (
            <TrajectorySection view={trajectoryView(trajectory, scope.timeClass)} />
          ) : null
        }
        focus={<FocusCard card={focus} />}
      />
    </>
  );
}
