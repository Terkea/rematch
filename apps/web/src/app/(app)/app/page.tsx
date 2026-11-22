import { cookies } from "next/headers";
import { requireUser } from "../../../server/auth/session";
import { getDb } from "../../../server/db";
import { getOverview } from "../../../server/overview";
import { OverviewSheet } from "./_overview/sheet";
import { knownTimeClass, parseTimeClass, REVEAL_COOKIE, toOverviewView } from "./_overview/view";

export const dynamic = "force-dynamic";
export const metadata = { title: "Overview" };

/**
 * The overview (`/app?tc=<timeClass>`): the signed-in player's analysed games. A server component;
 * only the trend is a client island, and it gets a narrow view model. An unknown or unanalysed
 * `tc` falls back to All.
 */
export default async function OverviewPage({
  searchParams,
}: {
  searchParams: Promise<{ tc?: string | string[] }>;
}) {
  const user = await requireUser();
  const db = getDb();
  const asked = knownTimeClass((await searchParams).tc);
  let overview = await getOverview(db, user.id, { timeClass: asked });
  const tc = parseTimeClass(
    asked ?? undefined,
    overview.timeClasses.map((c) => c.timeClass),
  );
  if (asked !== null && tc === null) overview = await getOverview(db, user.id, { timeClass: null });
  // The report reveal runs on the first view in a browser session only (DESIGN.md choreography 2).
  const reveal = !(await cookies()).has(REVEAL_COOKIE);
  return <OverviewSheet view={toOverviewView(overview, tc)} reveal={reveal} />;
}
