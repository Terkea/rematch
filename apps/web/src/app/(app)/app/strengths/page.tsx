import { LiveRefreshOn } from "../../../../components/live-refresh-on";
import { queryKeys } from "../../../../lib/query-keys";
import { requireUser } from "../../../../server/auth/session";
import { getDb } from "../../../../server/db";
import { requestEventCursor } from "../../../../server/events/request-cursor";
import { resolveCoachScope } from "../../../../server/profile";
import { loadStrengths } from "../../../../server/profile/strengths";
import { FilterBar } from "../_filters/filter-bar";
import { loadCoachBar, manyAccounts } from "../_filters/load";
import { withSide } from "../_filters/side";
import { StrengthsView } from "./strengths-view";

export const dynamic = "force-dynamic";
export const metadata = { title: "Strengths" };

type Props = { searchParams: Promise<Record<string, string | string[] | undefined>> };

/**
 * `/app/strengths?account=&tc=&color=&from=&to=`: what the scope's games do well (the strengths
 * read model, which checks the scope belongs to the user). A foreign or malformed scope falls
 * back to the default, rapid first (R1, I8). The shared filter bar (audit A5) picks the colour
 * and the dates; the page says which of its reads the dates narrow.
 */
export default async function StrengthsPage({ searchParams }: Props) {
  const user = await requireUser();
  // Before any page data, so the event stream replays whatever lands after this read (T27).
  await requestEventCursor(user.id);
  const db = getDb();
  const params = await searchParams;
  const resolved = await resolveCoachScope(db, user.id, params);
  if (!resolved) return <StrengthsView scope={null} model={null} />;
  const bar = await loadCoachBar(db, params, resolved, {
    basePath: "/app/strengths",
    formatSupported: false,
    color: "on",
    dates: true,
    compare: false,
  });
  // The colour reads that side's stored rows and games (ADR 0082); Both also shows each colour.
  const scope = withSide(resolved, bar.filters);
  const model = await loadStrengths(db, user.id, scope, {
    from: bar.filters.from,
    to: bar.filters.to,
  });
  return (
    <>
      <LiveRefreshOn
        keys={[
          queryKeys.games(scope.accountId),
          queryKeys.notes(scope.accountId),
          queryKeys.report(scope.accountId),
        ]}
      />
      <StrengthsView
        scope={scope}
        model={model}
        filterBar={<FilterBar model={bar.model} />}
        accountSwitch={manyAccounts(scope)}
      />
    </>
  );
}
