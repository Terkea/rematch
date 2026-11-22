import { LiveRefreshOn } from "../../../../components/live-refresh-on";
import { queryKeys } from "../../../../lib/query-keys";
import { requireUser } from "../../../../server/auth/session";
import { getDb } from "../../../../server/db";
import { requestEventCursor } from "../../../../server/events/request-cursor";
import {
  difficultyStatus,
  getBenchmark,
  listWeaknesses,
  resolveCoachScope,
} from "../../../../server/profile";
import { FilterBar } from "../_filters/filter-bar";
import { loadCoachBar, manyAccounts } from "../_filters/load";
import { withSide } from "../_filters/side";
import { weeklyRates } from "./queries";
import { WeaknessesView } from "./weaknesses-view";

export const dynamic = "force-dynamic";
export const metadata = { title: "Weaknesses" };

type Props = { searchParams: Promise<Record<string, string | string[] | undefined>> };

/**
 * `/app/weaknesses?account=&tc=`: the scope's weaknesses (T1's read model, which checks the
 * scope belongs to the user). A foreign or malformed scope falls back to the default (R1).
 */
export default async function WeaknessesPage({ searchParams }: Props) {
  const user = await requireUser();
  // Before any page data, so the event stream replays whatever lands after this read (T27).
  await requestEventCursor(user.id);
  const db = getDb();
  const params = await searchParams;
  const resolved = await resolveCoachScope(db, user.id, params);
  if (!resolved) {
    return <WeaknessesView scope={null} weaknesses={[]} trends={new Map()} difficulty={null} />;
  }
  const bar = await loadCoachBar(db, params, resolved, {
    basePath: "/app/weaknesses",
    formatSupported: false,
    color: "coming",
    dates: true,
    compare: false,
  });
  // The stored estimates are Both only until ADR 0082; `withSide` carries the colour filter.
  const scope = withSide(resolved, bar.filters);
  const range = { from: bar.filters.from, to: bar.filters.to };
  const [weaknesses, trends, difficulty, benchmark] = await Promise.all([
    listWeaknesses(db, user.id, scope),
    weeklyRates(db, user.id, scope, range),
    difficultyStatus(db, user.id, scope),
    getBenchmark(db, user.id, scope),
  ]);
  return (
    <>
      <LiveRefreshOn keys={[queryKeys.games(scope.accountId), queryKeys.notes(scope.accountId)]} />
      <WeaknessesView
        scope={scope}
        weaknesses={weaknesses}
        trends={trends}
        difficulty={difficulty}
        benchmark={benchmark}
        filterBar={<FilterBar model={bar.model} />}
        accountSwitch={manyAccounts(scope)}
      />
    </>
  );
}
