import { LiveRefreshOn } from "../../../../components/live-refresh-on";
import { queryKeys } from "../../../../lib/query-keys";
import { requireUser } from "../../../../server/auth/session";
import { getDb } from "../../../../server/db";
import { requestEventCursor } from "../../../../server/events/request-cursor";
import { resolveCoachScope } from "../../../../server/profile";
import { loadStrengths } from "../../../../server/profile/strengths";
import { StrengthsView } from "./strengths-view";

export const dynamic = "force-dynamic";
export const metadata = { title: "Strengths" };

type Props = { searchParams: Promise<Record<string, string | string[] | undefined>> };

/**
 * `/app/strengths?account=&tc=`: what the scope's games do well (the strengths read model, which
 * checks the scope belongs to the user). A foreign or malformed scope falls back to the default,
 * rapid first (R1, I8).
 */
export default async function StrengthsPage({ searchParams }: Props) {
  const user = await requireUser();
  // Before any page data, so the event stream replays whatever lands after this read (T27).
  await requestEventCursor(user.id);
  const db = getDb();
  const scope = await resolveCoachScope(db, user.id, await searchParams);
  if (!scope) return <StrengthsView scope={null} model={null} />;
  const model = await loadStrengths(db, user.id, scope);
  return (
    <>
      <LiveRefreshOn
        keys={[
          queryKeys.games(scope.accountId),
          queryKeys.notes(scope.accountId),
          queryKeys.report(scope.accountId),
        ]}
      />
      <StrengthsView scope={scope} model={model} />
    </>
  );
}
