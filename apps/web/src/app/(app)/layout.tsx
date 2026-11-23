import type { ReactNode } from "react";
import { ActivityIndicator } from "../../components/activity-indicator";
import { TimeZoneCookie } from "../../components/time-zone-cookie";
import { RealtimeProvider } from "../../lib/realtime/realtime-provider";
import { requireUser } from "../../server/auth/session";
import { getDb } from "../../server/db";
import { hasDemoAccount } from "../../server/demo";
import { requestAccountIds, requestEventCursor } from "../../server/events/request-cursor";
import { Header } from "./_shell/header";

export const dynamic = "force-dynamic";

/**
 * The signed-in shell: the header, then one sheet per page (`template.tsx` turns it on route
 * changes). Every page below calls `requireUser()` too, so none relies on the layout alone.
 * The one event stream (M4 T23) wraps it all, starting at the request's event cursor, which
 * every page reads before its own data (`requestEventCursor`); the header shows its activity.
 */
export default async function AppLayout({ children }: { children: ReactNode }) {
  const user = await requireUser();
  const cursor = await requestEventCursor(user.id);
  const accountIds = await requestAccountIds(user.id);
  const demo = await hasDemoAccount(getDb(), user.id);
  return (
    <RealtimeProvider accountIds={accountIds} initialEventId={cursor}>
      <TimeZoneCookie />
      <div className="flex min-h-dvh flex-col bg-paper">
        <a
          href="#main"
          className="sr-only z-50 rounded-field bg-ink px-4 py-2 text-sm font-medium text-on-ink focus:not-sr-only focus:fixed focus:top-3 focus:left-3"
        >
          Skip to content
        </a>
        <Header email={user.email} name={user.name} activity={<ActivityIndicator />} demo={demo} />
        <main
          id="main"
          className="mx-auto flex w-full max-w-6xl flex-1 flex-col px-(--space-gutter) pt-6 pb-16 md:pt-10"
        >
          {children}
        </main>
      </div>
    </RealtimeProvider>
  );
}
