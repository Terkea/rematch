import { CoachScopeSwitch } from "../../../../../components/coach-scope-switch";
import { requireUser } from "../../../../../server/auth/session";
import { chatAllowance } from "../../../../../server/coach/allowance";
import { type ChatAnchor, decodeAnchor } from "../../../../../server/coach/anchor";
import { coachStatus } from "../../../../../server/coach/status";
import { getDb } from "../../../../../server/db";
import { requestEventCursor } from "../../../../../server/events/request-cursor";
import { resolveCoachScope } from "../../../../../server/profile/scope";
import { PageHeading } from "../../../_shell/page-heading";
import { AnchorCard, loadAnchorView } from "../[threadId]/anchor-card";
import { allowanceView } from "../allowance";
import { NoScope } from "../no-scope";
import { ScopeRow } from "../scope-row";
import { NewConversation } from "./new-view";

export const dynamic = "force-dynamic";
export const metadata = { title: "New conversation" };

type Params = Record<string, string | string[] | undefined>;

const first = (v: string | string[] | undefined) => (Array.isArray(v) ? v[0] : v) ?? null;

/**
 * `/app/coach/new?account&tc&anchor`: a new conversation in the scope, opened from an anchor. A
 * malformed anchor, or one this user cannot see (another account's game, a ply past the end),
 * is dropped silently: the question is then asked without it.
 */
export default async function NewCoachPage({ searchParams }: { searchParams: Promise<Params> }) {
  const user = await requireUser();
  // Before any page data, so the event stream replays whatever lands after this read (T27).
  await requestEventCursor(user.id);
  const db = getDb();
  const params = await searchParams;
  const scope = await resolveCoachScope(db, user.id, params);
  if (!scope) {
    return (
      <>
        <PageHeading title="New conversation" />
        <NoScope />
      </>
    );
  }
  const raw = first(params.anchor);
  const decoded = raw ? decodeAnchor(raw) : null;
  const [view, allowance, status] = await Promise.all([
    decoded ? loadAnchorView(db, user.id, scope.accountId, decoded) : Promise.resolve(null),
    chatAllowance(db, scope.accountId),
    coachStatus(db),
  ]);
  const anchor: ChatAnchor | null = view ? decoded : null;
  const q = new URLSearchParams({ account: scope.accountId, tc: scope.timeClass });
  return (
    <NewConversation
      scope={{ accountId: scope.accountId, timeClass: scope.timeClass }}
      anchor={anchor}
      allowance={allowanceView(allowance)}
      listHref={`/app/coach?${q.toString()}`}
      available={status === "available"}
      // An anchor belongs to one account's games: switching scope would drop it, so no switch.
      scopeSwitch={
        view ? null : (
          <ScopeRow>
            <CoachScopeSwitch scope={scope} basePath="/app/coach/new" />
          </ScopeRow>
        )
      }
    >
      {view ? <AnchorCard view={view} /> : null}
    </NewConversation>
  );
}
