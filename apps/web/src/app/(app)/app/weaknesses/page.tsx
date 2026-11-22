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
  const scope = await resolveCoachScope(db, user.id, await searchParams);
  if (!scope) {
    return <WeaknessesView scope={null} weaknesses={[]} trends={new Map()} difficulty={null} />;
  }
  const [weaknesses, trends, difficulty, benchmark] = await Promise.all([
    listWeaknesses(db, user.id, scope),
    weeklyRates(db, user.id, scope),
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
      />
    </>
  );
}
