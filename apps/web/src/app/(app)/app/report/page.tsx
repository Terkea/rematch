import { CoachState } from "@ci/ui/coach";
import { LiveRefreshOn } from "../../../../components/live-refresh-on";
import { queryKeys } from "../../../../lib/query-keys";
import { requireUser } from "../../../../server/auth/session";
import { requestEventCursor } from "../../../../server/events/request-cursor";
import { betaReportIntervalDays } from "../../../../server/coaching";
import { getDb } from "../../../../server/db";
import { resolveCoachScope } from "../../../../server/profile";
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
  const scope = await resolveCoachScope(db, user.id, await searchParams);
  if (!scope) {
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
  const data = await loadReport(db, user.id, scope, new Date(), betaReportIntervalDays());
  return (
    <>
      <LiveRefreshOn keys={[queryKeys.report(scope.accountId), queryKeys.notes(scope.accountId)]} />
      <ReportView data={data} />
    </>
  );
}
