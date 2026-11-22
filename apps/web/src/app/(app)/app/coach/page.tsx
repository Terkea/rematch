import { Icon, InfoIcon, PlusIcon } from "@ci/ui/icons";
import { Button } from "@ci/ui/primitives";
import Link from "next/link";
import { CoachScopeSwitch } from "../../../../components/coach-scope-switch";
import { requireUser } from "../../../../server/auth/session";
import { chatAllowance } from "../../../../server/coach/allowance";
import { coachHref } from "../../../../server/coach/anchor";
import { listConversations } from "../../../../server/coach/threads";
import { getDb } from "../../../../server/db";
import { requestEventCursor } from "../../../../server/events/request-cursor";
import { resolveCoachScope } from "../../../../server/profile/scope";
import { EmptySheet, PageHeading } from "../../_shell/page-heading";
import { allowanceView } from "./allowance";
import { AllowanceLine } from "./allowance-line";
import { AI_DISCLOSURE_NOTE, EMPTY_THREADS } from "./copy";
import { NoScope } from "./no-scope";
import { ScopeRow } from "./scope-row";
import { ThreadList } from "./thread-list";

export const dynamic = "force-dynamic";
export const metadata = { title: { absolute: "Ask your coach" } };

type Params = Record<string, string | string[] | undefined>;

const LEDE = "Questions about your own games, answered from your analysis.";

/**
 * `/app/coach?account&tc`: the signed-in user's own conversations on the scope's account (R1),
 * the allowance and a way to start a new one. Threads are the user's (ADR 0070): another user of
 * the same account never sees them.
 */
export default async function CoachPage({ searchParams }: { searchParams: Promise<Params> }) {
  const user = await requireUser();
  // Before any page data, so the event stream replays whatever lands after this read (T27).
  await requestEventCursor(user.id);
  const db = getDb();
  const scope = await resolveCoachScope(db, user.id, await searchParams);
  if (!scope) {
    return (
      <>
        <PageHeading title="Ask your coach" lede={LEDE} />
        <NoScope />
      </>
    );
  }
  const [threads, allowance] = await Promise.all([
    listConversations(db, user.id, scope.accountId),
    chatAllowance(db, scope.accountId),
  ]);
  const newHref = coachHref(null, scope);
  return (
    <>
      <PageHeading title="Ask your coach" lede={LEDE} />
      <div className="flex flex-wrap items-center justify-between gap-4 [&>*:first-child]:min-w-0">
        <ScopeRow>
          <CoachScopeSwitch scope={scope} basePath="/app/coach" />
        </ScopeRow>
        <Button asChild>
          <Link href={newHref}>
            <Icon icon={PlusIcon} size={20} />
            New conversation
          </Link>
        </Button>
      </div>
      <section aria-labelledby="coach-threads" className="flex flex-col gap-3">
        <h2 id="coach-threads" className="sr-only">
          Your conversations
        </h2>
        {threads.length > 0 ? (
          <div data-surface="sheet" className="rounded-sheet bg-sheet py-1">
            <ThreadList threads={threads} />
          </div>
        ) : (
          <EmptySheet
            action={
              <Button asChild variant="outline">
                <Link href={newHref}>Ask your first question</Link>
              </Button>
            }
          >
            {EMPTY_THREADS}
          </EmptySheet>
        )}
      </section>
      <footer className="flex max-w-prose flex-col gap-2">
        <AllowanceLine allowance={allowanceView(allowance)} />
        <p className="m-0 flex items-start gap-2 font-sans text-sm text-ink-faint">
          <Icon icon={InfoIcon} size={16} className="mt-0.5 shrink-0" />
          <span>{AI_DISCLOSURE_NOTE}</span>
        </p>
      </footer>
    </>
  );
}
