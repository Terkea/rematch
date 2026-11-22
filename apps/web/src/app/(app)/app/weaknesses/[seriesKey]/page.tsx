import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { cache } from "react";
import { LiveRefreshOn } from "../../../../../components/live-refresh-on";
import { queryKeys } from "../../../../../lib/query-keys";
import { requireUser } from "../../../../../server/auth/session";
import { coachHref } from "../../../../../server/coach/anchor";
import { changeNotes } from "../../../../../server/coaching";
import { getDb } from "../../../../../server/db";
import { getWeakness, resolveCoachScope } from "../../../../../server/profile";
import { reviewGameIds } from "../queries";
import { MAX_EXEMPLARS } from "./exemplars";
import { WeaknessView } from "./weakness-view";

export const dynamic = "force-dynamic";

type Params = Record<string, string | string[] | undefined>;
type Props = { params: Promise<{ seriesKey: string }>; searchParams: Promise<Params> };

const MAX_KEY = 200;

/** The URL-decoded series key, or null when it is malformed or too long. */
function decodeKey(raw: string): string | null {
  try {
    const key = decodeURIComponent(raw);
    return key.length > 0 && key.length <= MAX_KEY ? key : null;
  } catch {
    return null;
  }
}

/**
 * One read per request, shared by the metadata and the page. Scope and every not-found case are
 * T1's: an unknown key, another user's and a malformed one all give the same not-found.
 */
const load = cache(async (raw: string, query: string) => {
  const user = await requireUser();
  const db = getDb();
  const key = decodeKey(raw);
  if (key === null) notFound();
  const params = Object.fromEntries(new URLSearchParams(query));
  const scope = await resolveCoachScope(db, user.id, params);
  if (!scope) notFound();
  const detail = await getWeakness(db, user.id, scope, key);
  if (!detail) notFound();
  return { user, db, scope, detail };
});

/** The search params as a stable string, so `cache` sees one key per request. */
function queryOf(sp: Params): string {
  const q = new URLSearchParams();
  for (const name of ["account", "tc"]) {
    const v = sp[name];
    const first = Array.isArray(v) ? v[0] : v;
    if (typeof first === "string") q.set(name, first);
  }
  return q.toString();
}

export async function generateMetadata({ params, searchParams }: Props): Promise<Metadata> {
  const { detail } = await load((await params).seriesKey, queryOf(await searchParams));
  return { title: detail.label };
}

export default async function WeaknessPage({ params, searchParams }: Props) {
  const { user, db, scope, detail } = await load(
    (await params).seriesKey,
    queryOf(await searchParams),
  );
  const notes = await changeNotes(db, user.id, scope.accountId, [detail.seriesKey]);
  const cited = [
    ...(detail.exemplars?.items.slice(0, MAX_EXEMPLARS).map((e) => e.gameId) ?? []),
    ...notes.flatMap((n) =>
      (n.item?.segments ?? []).flatMap((s) =>
        s.kind === "move" || s.kind === "best" || s.kind === "pv" ? [s.gameId] : [],
      ),
    ),
  ];
  const reviewIds = await reviewGameIds(db, user.id, scope, cited);
  // Non-`W` series have no contract id, so the chat opens without an anchor.
  const askHref = coachHref(
    detail.contractId ? { kind: "weakness", weaknessId: detail.contractId } : null,
    scope,
  );
  return (
    <>
      <LiveRefreshOn keys={[queryKeys.notes(scope.accountId), queryKeys.games(scope.accountId)]} />
      <WeaknessView
        detail={detail}
        scope={scope}
        notes={notes}
        reviewIds={reviewIds}
        askHref={askHref}
      />
    </>
  );
}
