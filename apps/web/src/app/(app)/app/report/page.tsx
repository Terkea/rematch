import { CoachState } from "@ci/ui/coach";
import { LiveRefreshOn } from "../../../../components/live-refresh-on";
import { queryKeys } from "../../../../lib/query-keys";
import { requireUser } from "../../../../server/auth/session";
import { betaReportIntervalDays, changeNotes, getReport } from "../../../../server/coaching";
import { getDb } from "../../../../server/db";
import { getWeakness, resolveCoachScope } from "../../../../server/profile";
import { reviewGameIds } from "../../../../server/profile/game-ids";
import { reportGameIds, weaknessSeriesKey } from "./_sections/links";
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
  const now = new Date();
  const view = await getReport(db, user.id, scope, now, { intervalDays: betaReportIntervalDays() });
  const computed = view.payload?.computed;
  const changeKeys = computed
    ? [...computed.newThisPeriod, ...computed.fixedThisPeriod].map((c) => c.seriesKey)
    : [];
  const [notes, details] = await Promise.all([
    changeKeys.length > 0 ? changeNotes(db, user.id, scope.accountId, changeKeys) : [],
    Promise.all(
      (computed?.weaknesses ?? []).map((w) =>
        getWeakness(db, user.id, scope, weaknessSeriesKey(w)),
      ),
    ),
  ]);
  const cited = [
    ...(view.payload ? reportGameIds(view.payload) : []),
    ...reportGameIds({
      sections: { notes: notes.map((n) => n.item) },
      computed: { weaknesses: [], openings: [], strengths: [] },
    }),
  ];
  const gameIds = await reviewGameIds(db, user.id, scope, cited);
  const trends: Record<string, (number | null)[]> = {};
  for (const d of details) {
    if (d) trends[d.seriesKey] = d.weekly.map((w) => w.rate?.value ?? null);
  }
  return (
    <>
      <LiveRefreshOn keys={[queryKeys.report(scope.accountId), queryKeys.notes(scope.accountId)]} />
      <ReportView
        data={{ scope, view, notes, trends, gameIds: Object.fromEntries(gameIds), now }}
      />
    </>
  );
}
