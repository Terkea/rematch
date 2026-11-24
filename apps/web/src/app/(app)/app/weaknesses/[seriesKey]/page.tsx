import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { cache } from "react";
import { MakeFocusButton } from "../../../../../components/focus/make-focus-button";
import { LiveRefreshOn } from "../../../../../components/live-refresh-on";
import { practiseLinkFor } from "../../../../../components/practise-positions";
import { queryKeys } from "../../../../../lib/query-keys";
import { requireUser } from "../../../../../server/auth/session";
import { requestNow } from "../../../../../server/clock";
import { coachHref } from "../../../../../server/coach/anchor";
import { changeNotes } from "../../../../../server/coaching";
import { getDb } from "../../../../../server/db";
import { requestEventCursor } from "../../../../../server/events/request-cursor";
import type { DayRange } from "../../../../../server/filters/params";
import { focusOffer } from "../../../../../server/focus";
import { getBenchmark, getWeakness, resolveCoachScope } from "../../../../../server/profile";
import { reviewGameIds } from "../../../../../server/profile/game-ids";
import { FilterBar } from "../../_filters/filter-bar";
import { loadCoachBar } from "../../_filters/load";
import { withSide } from "../../_filters/side";
import { noteKeysInScope } from "../queries";
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
  const resolved = await resolveCoachScope(db, user.id, params);
  if (!resolved) notFound();
  // The shared bar (A4): colour and dates, as on the list. The colour reads that side's estimate
  // and weekly points (ADR 0082); the dates clip the weekly rate.
  const bar = await loadCoachBar(db, params, resolved, {
    basePath: `/app/weaknesses/${encodeURIComponent(key)}`,
    formatSupported: false,
    color: "on",
    dates: true,
    compare: false,
  });
  const scope = withSide(resolved, bar.filters);
  const range: DayRange = { from: bar.filters.from, to: bar.filters.to };
  const detail = await getWeakness(db, user.id, scope, key);
  if (!detail) notFound();
  const both = scope.side === "both";
  const [benchmark, white, black] = await Promise.all([
    getBenchmark(db, user.id, scope),
    both ? getWeakness(db, user.id, { ...scope, side: "white" }, key) : null,
    both ? getWeakness(db, user.id, { ...scope, side: "black" }, key) : null,
  ]);
  return { user, db, scope, detail, benchmark, bar, range, sides: both ? { white, black } : null };
});

/** The search params as a stable string, so `cache` sees one key per request. */
function queryOf(sp: Params): string {
  const q = new URLSearchParams();
  for (const name of ["account", "tc", "color", "from", "to"]) {
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
  const { user, db, scope, detail, benchmark, bar, range, sides } = await load(
    (await params).seriesKey,
    queryOf(await searchParams),
  );
  // Notes are account rows; only those written for this scope's time class belong here.
  const [allNotes, inScope, offer] = await Promise.all([
    changeNotes(db, user.id, scope.accountId, [detail.seriesKey]),
    noteKeysInScope(db, user.id, scope, detail.seriesKey),
    focusOffer(db, user.id, scope, detail),
  ]);
  const notes = allNotes.filter((n) => inScope.has(n.changeKey));
  const cited = [
    ...detail.examples.map((e) => e.gameId),
    ...notes.flatMap((n) =>
      (n.item?.segments ?? []).flatMap((s) =>
        s.kind === "move" || s.kind === "best" || s.kind === "pv" ? [s.gameId] : [],
      ),
    ),
  ];
  const reviewIds = await reviewGameIds(db, user.id, scope, cited);
  const practise = await practiseLinkFor(user.id, scope, detail.seriesKey);
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
        range={range}
        sides={sides}
        now={await requestNow()}
        filterBar={<FilterBar model={bar.model} />}
        focusAction={
          offer.current || offer.suggestion ? (
            <MakeFocusButton
              target={{
                accountId: scope.accountId,
                timeClass: scope.timeClass,
                seriesKey: detail.seriesKey,
                side: scope.side ?? "both",
              }}
              current={offer.current}
            />
          ) : null
        }
        practise={practise}
      />
    </>
  );
}
