import { Button } from "@ci/ui/primitives";
import Link from "next/link";
import { CoachScopeSwitch } from "../../../../components/coach-scope-switch";
import { LiveRefreshOn } from "../../../../components/live-refresh-on";
import { queryKeys } from "../../../../lib/query-keys";
import { requireUser } from "../../../../server/auth/session";
import { getDb } from "../../../../server/db";
import { requestEventCursor } from "../../../../server/events/request-cursor";
import { resolveCoachScope } from "../../../../server/profile";
import { EmptySheet, PageHeading } from "../../_shell/page-heading";
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
  const scope = await resolveCoachScope(db, user.id, await searchParams);
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
  const view = toProgressView(await loadProgress(db, user.id, scope));
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
        scopeSwitch={<CoachScopeSwitch scope={scope} basePath="/app/progress" />}
      />
    </>
  );
}
