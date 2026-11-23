import { LiveRefreshOn } from "../../../../components/live-refresh-on";
import { queryKeys } from "../../../../lib/query-keys";
import { requireUser } from "../../../../server/auth/session";
import { getDb } from "../../../../server/db";
import { requestEventCursor } from "../../../../server/events/request-cursor";
import { resolveCoachScope } from "../../../../server/profile";
import { requestTimeZone } from "../../../../server/time-zone";
import { isSourceKey } from "../../../../server/train/model";
import { loadTrainPage } from "../../../../server/train/store";
import { TrainView } from "./train-view";

export const dynamic = "force-dynamic";
export const metadata = { title: "Train" };

type Props = { searchParams: Promise<Record<string, string | string[] | undefined>> };

/**
 * `/app/train?account=&tc=&source=`: today's session of the player's own missed moments (ADR
 * 0090), rapid first (I8). `source` (a weakness key or `blunder`) keeps one source, as the
 * weakness page's "Practise these positions" asks; anything else is ignored.
 */
export default async function TrainPage({ searchParams }: Props) {
  const user = await requireUser();
  await requestEventCursor(user.id);
  const db = getDb();
  const params = await searchParams;
  const scope = await resolveCoachScope(db, user.id, params);
  if (!scope) return <TrainView scope={null} page={null} />;
  const raw = Array.isArray(params.source) ? params.source[0] : params.source;
  const source = isSourceKey(raw) ? raw : null;
  const page = await loadTrainPage(db, user.id, scope, {
    source,
    now: new Date(),
    timeZone: await requestTimeZone(),
  });
  return (
    <>
      <LiveRefreshOn keys={[queryKeys.games(scope.accountId)]} />
      <TrainView scope={scope} page={page} />
    </>
  );
}
