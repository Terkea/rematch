import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { requireUser } from "../../../../../server/auth/session";
import { getReport } from "../../../../../server/coaching";
import { getDb } from "../../../../../server/db";
import { requestEventCursor } from "../../../../../server/events/request-cursor";
import {
  AREAS,
  type Area,
  getArea,
  getWeakness,
  KPIS,
  kpiTrends,
  resolveCoachScope,
} from "../../../../../server/profile";
import { AreaEmpty, type AreaPageModel, AreaView } from "./area-view";
import { scoreBasis } from "./basis";

// One skill area (M5b Task 8): its score, the coach's note, its measures, its weaknesses and
// their timeline, and up to three example positions from the newest report. Every read goes
// through T1's profile read model and T2's report read model as the signed-in user, so a foreign
// or malformed scope falls back to the user's default and nothing outside it is ever read.

export const dynamic = "force-dynamic";

type Params = Promise<{ area: string }>;
type Search = Promise<Record<string, string | string[] | undefined>>;

const isArea = (s: string): s is Area => (AREAS as readonly string[]).includes(s);

const TITLES: Record<Area, string> = {
  openings: "Openings",
  middlegame: "Middlegame",
  endgame: "Endgame",
  tactics: "Tactics",
  calculation: "Calculation",
  time: "Time",
  mindset: "Mindset",
};

export async function generateMetadata({ params }: { params: Params }): Promise<Metadata> {
  const { area } = await params;
  return { title: isArea(area) ? TITLES[area] : "Skill areas" };
}

export default async function AreaPage({
  params,
  searchParams,
}: {
  params: Params;
  searchParams: Search;
}) {
  const { area } = await params;
  if (!isArea(area)) notFound();
  const user = await requireUser();
  // Before any page data, so the event stream replays whatever lands after this read (T27).
  await requestEventCursor(user.id);
  const db = getDb();
  const scope = await resolveCoachScope(db, user.id, await searchParams);
  if (!scope) return <AreaEmpty area={area} />;

  // The area's own measures; its weakness rates are shown as weakness rows, not again here.
  const metrics = KPIS.filter((k) => k.area === area && !k.metric.startsWith("weakness:")).map(
    (k) => k.metric,
  );
  const [view, kpis, report] = await Promise.all([
    getArea(db, user.id, scope, area),
    kpiTrends(db, user.id, scope, metrics),
    getReport(db, user.id, { accountId: scope.accountId, timeClass: scope.timeClass }),
  ]);
  const weaknesses = view.weaknesses.filter((w) => w.area === area);
  const details = await Promise.all(
    weaknesses.map((w) => getWeakness(db, user.id, scope, w.seriesKey)),
  );
  const weekly = new Map(
    details.flatMap((d) => (d ? [[d.seriesKey, d.weekly.map((p) => p.rate?.value ?? null)]] : [])),
  );

  const model: AreaPageModel = {
    area,
    scope,
    score: view.score,
    basis: view.score ? scoreBasis(area, view.score.n, view.kpis) : null,
    areaKpis: view.kpis,
    kpis,
    weaknesses: weaknesses.map((w) => ({ summary: w, trend: weekly.get(w.seriesKey) ?? [] })),
    timeline: view.timeline,
    report,
    today: new Date().toISOString().slice(0, 10),
  };
  return <AreaView model={model} />;
}
