import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { cache } from "react";
import { LiveRefreshOn } from "../../../../../components/live-refresh-on";
import { queryKeys } from "../../../../../lib/query-keys";
import { requireUser } from "../../../../../server/auth/session";
import { coachHref } from "../../../../../server/coach/anchor";
import { changeNotes } from "../../../../../server/coaching";
import { getDb } from "../../../../../server/db";
import { requestEventCursor } from "../../../../../server/events/request-cursor";
import { getBenchmark, getWeakness, resolveCoachScope } from "../../../../../server/profile";
import { reviewGameIds } from "../../../../../server/profile/game-ids";
import { noteKeysInScope } from "../queries";
import { MAX_EXEMPLARS } from "./exemplars";
import { WeaknessView } from "./weakness-view";

export const dynamic = "force-dynamic";

type Params = Record<string, string | string[] | undefined>;
type Props = { params: Promise<{ seriesKey: string }>; searchParams: Promise<Params> };

const MAX_KEY = 200;

/**
 * The URL-decoded series key, or null when it is malformed or too long. Next may hand the param
 * over already decoded, so it is decoded only while it still holds a percent escape (no series
 * key contains "%").
 */
function decodeKey(raw: string): string | null {
  try {
    const key = /%[0-9A-F]{2}/i.test(raw) ? decodeURIComponent(raw) : raw;
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
  // Before any page data, so the event stream replays whatever lands after this read (T27).
  await requestEventCursor(user.id);
  const db = getDb();
  const key = decodeKey(raw);
  if (key === null) notFound();
  const params = Object.fromEntries(new URLSearchParams(query));
  const scope = await resolveCoachScope(db, user.id, params);
  if (!scope) notFound();
  const detail = await getWeakness(db, user.id, scope, key);
  if (!detail) notFound();
  const benchmark = await getBenchmark(db, user.id, scope);
  return { user, db, scope, detail, benchmark };
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
  const { user, db, scope, detail, benchmark } = await load(
    (await params).seriesKey,
    queryOf(await searchParams),
  );
  // Notes are account rows; only those written for this scope's time class belong here.
  const [allNotes, inScope] = await Promise.all([
    changeNotes(db, user.id, scope.accountId, [detail.seriesKey]),
    noteKeysInScope(db, user.id, scope, detail.seriesKey),
  ]);
  const notes = allNotes.filter((n) => inScope.has(n.changeKey));
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
        benchmark={benchmark}
      />
    </>
  );
}
