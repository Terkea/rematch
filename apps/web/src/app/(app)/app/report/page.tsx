import { CoachState } from "@ci/ui/coach";
import { LiveRefreshOn } from "../../../../components/live-refresh-on";
import { queryKeys } from "../../../../lib/query-keys";
import { requireUser } from "../../../../server/auth/session";
import { betaReportIntervalDays } from "../../../../server/coaching";
import { getDb } from "../../../../server/db";
import { requestEventCursor } from "../../../../server/events/request-cursor";
import { resolveCoachScope } from "../../../../server/profile";
import { FilterBar } from "../_filters/filter-bar";
import { loadCoachBar, manyAccounts } from "../_filters/load";
import { withSide } from "../_filters/side";
import { loadReport } from "./load";
import { ReportView } from "./report-view";

export const dynamic = "force-dynamic";
export const metadata = { title: "Coaching report" };

/**
 * The coaching report (`/app/report?account=…&tc=…`, ADR 0073). Every read goes through the read
 * models, which check that the scope is the user's (a foreign or malformed choice falls back to
 * the default scope). `LiveRefreshOn` re-renders the page on each `report.section.ready`, so the
 * sections assemble in as the worker writes them. The rebuild interval is the beta plan's
 * (ruling G1), the same one the route enforces.
 */
export default async function ReportPage({
  searchParams,
}: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const user = await requireUser();
  // Before any page data, so the event stream replays whatever lands after this read (T27).
  await requestEventCursor(user.id);
  const db = getDb();
  const params = await searchParams;
  const resolved = await resolveCoachScope(db, user.id, params);
  if (!resolved) {
    return (
      <div className="flex flex-col gap-10">
        <h1 className="m-0 font-serif text-3xl font-semibold text-ink">Your coaching report</h1>
        <CoachState
          kind="building"
          titleAs="h2"
          title="Your coaching profile is still being built"
          body="Your first report starts on its own once your games are analysed and your profile is ready."
        />
      </div>
    );
  }
  const now = new Date();
  const bar = await loadCoachBar(db, params, resolved, {
    basePath: "/app/report",
    formatSupported: false,
    color: "coming",
    dates: true,
    compare: false,
    now,
  });
  // The stored estimates are Both only until ADR 0082; `withSide` carries the colour filter.
  const scope = withSide(resolved, bar.filters);
  const data = await loadReport(db, user.id, scope, now, betaReportIntervalDays(), {
    from: bar.filters.from,
    to: bar.filters.to,
  });
  return (
    <>
      <LiveRefreshOn keys={[queryKeys.report(scope.accountId), queryKeys.notes(scope.accountId)]} />
      <ReportView
        data={data}
        filterBar={<FilterBar model={bar.model} />}
        accountSwitch={manyAccounts(scope)}
      />
    </>
  );
}
